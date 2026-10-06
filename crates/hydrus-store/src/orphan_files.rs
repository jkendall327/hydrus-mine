//! Database > file maintenance > "clear orphan files" (the files manager's
//! `ClearOrphans`): every file and thumbnail in storage that local file
//! storage doesn't hold (`_IsAnOrphan`), moved somewhere or deleted.

use std::path::{Path, PathBuf};

use crate::error::{Result, StoreError};
use crate::storage::{FileStorage, prefix_dir};

/// What a scan found and did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub files_reviewed: usize,
    pub thumbnails_reviewed: usize,
    pub orphan_files: Vec<PathBuf>,
    pub orphan_thumbnails: Vec<PathBuf>,
}

/// Whether a stored file named `name` (a hash, maybe with an extension)
/// is one local file storage has; anything else is an orphan.
fn is_kept(conn: &rusqlite::Connection, local: hydrus_core::ServiceId, name: &str) -> Result<bool> {
    let Some(hex) = name.get(..64) else {
        return Ok(false);
    };
    let Ok(bytes) = hex::decode(hex) else {
        return Ok(false);
    };
    Ok(conn
        .query_row(
            "SELECT 1 FROM hashes h JOIN file_domain_current c USING (hash_id)
             WHERE h.sha256 = ? AND c.service_id = ?",
            rusqlite::params![bytes, local],
            |_| Ok(()),
        )
        .map(|()| true)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            e => Err(e),
        })?)
}

/// Find the orphans (`say` takes progress; stops if `cancelled`).
pub fn scan(
    store: &crate::Store,
    say: &mut dyn FnMut(String, Option<String>),
    cancelled: &dyn Fn() -> bool,
) -> Result<Outcome> {
    let storage = store.read(FileStorage::load)?;
    let local = crate::content::DomainRoles::new(&store.snapshot().services)?.local_file_storage;
    let mut folders: Vec<(String, PathBuf)> = storage
        .locations()
        .iter()
        .flat_map(|l| l.prefixes.iter().map(move |p| (p.clone(), l.path.clone())))
        .collect();
    folders.sort();
    let mut outcome = Outcome::default();
    for (prefix, base) in folders {
        if cancelled() {
            return Err(StoreError::Invalid("cancelled".into()));
        }
        say(format!("checking {prefix}"), None);
        let dir = prefix_dir(&base, &prefix);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let files = prefix.starts_with('f');
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let kept = store.read(|conn| is_kept(conn, local, &name))?;
            let (reviewed, orphans, what) = if files {
                (
                    &mut outcome.files_reviewed,
                    &mut outcome.orphan_files,
                    "files",
                )
            } else {
                (
                    &mut outcome.thumbnails_reviewed,
                    &mut outcome.orphan_thumbnails,
                    "thumbnails",
                )
            };
            if !kept {
                orphans.push(entry.path());
            }
            if (*reviewed).is_multiple_of(100) {
                let found = orphans.len();
                say(
                    format!("checking {prefix}"),
                    Some(format!(
                        "reviewed {} {what}, found {} orphans",
                        hydrus_core::numbers::human_int(*reviewed as u64),
                        hydrus_core::numbers::human_int(found as u64)
                    )),
                );
            }
            *reviewed += 1;
        }
    }
    Ok(outcome)
}

/// A free name for `path` (`AppendPathUntilNoConflicts`): `name (1).ext`.
fn free_name(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (1..=u32::MAX)
        .map(|i| path.with_file_name(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or_else(|| path.to_path_buf())
}

/// Clear what a scan found: moved under `to` (thumbnails into its
/// `thumbnails` folder), or deleted; refused for media another install
/// owns.
pub fn clear(
    store: &crate::Store,
    outcome: &Outcome,
    to: Option<&Path>,
    say: &mut dyn FnMut(String),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    let ownership: crate::transfer::MediaOwnership = store.read(crate::settings::get)?;
    if let Some(owner) = ownership.shared_with {
        return Err(StoreError::Invalid(format!(
            "This client's files belong to the install at \"{}\", so it will not move or delete any of them.",
            owner.display()
        )));
    }
    for (paths, what) in [
        (&outcome.orphan_files, "files"),
        (&outcome.orphan_thumbnails, "thumbnails"),
    ] {
        if paths.is_empty() {
            continue;
        }
        let dest_dir = to.map(|to| {
            if what == "files" {
                to.to_path_buf()
            } else {
                to.join("thumbnails")
            }
        });
        if let Some(dir) = &dest_dir {
            std::fs::create_dir_all(dir)?;
        }
        for (i, path) in paths.iter().enumerate() {
            if cancelled() {
                return Ok(());
            }
            if let Some(dir) = &dest_dir {
                say(format!(
                    "moving orphan {what}: {}",
                    hydrus_core::numbers::value_range(i as u64 + 1, paths.len() as u64)
                ));
                let dest = free_name(&dir.join(path.file_name().unwrap_or_default()));
                if std::fs::rename(path, &dest).is_err() {
                    std::fs::copy(path, &dest)?;
                    std::fs::remove_file(path)?;
                }
            } else {
                say(format!(
                    "deleting orphan {what}: {}",
                    hydrus_core::numbers::value_range(i as u64 + 1, paths.len() as u64)
                ));
                std::fs::remove_file(path)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strays_are_found_and_moved_and_kept_files_stay() {
        let (home, away) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let store = crate::Store::open(home.path()).unwrap();
        let media = home.path().join("client_files");
        let stray = prefix_dir(&media, "fab").join(format!("{}.png", "ab".repeat(32)));
        let junk = prefix_dir(&media, "t00").join("Thumbs.db");
        for path in [&stray, &junk] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        let found = scan(&store, &mut |_, _| {}, &|| false).unwrap();
        assert_eq!(found.orphan_files, std::slice::from_ref(&stray));
        assert_eq!(found.orphan_thumbnails, std::slice::from_ref(&junk));
        clear(&store, &found, Some(away.path()), &mut |_| {}, &|| false).unwrap();
        assert!(!stray.exists());
        assert!(away.path().join(stray.file_name().unwrap()).is_file());
        assert!(away.path().join("thumbnails/Thumbs.db").is_file());
    }
}
