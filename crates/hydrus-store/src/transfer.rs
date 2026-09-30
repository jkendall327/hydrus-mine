//! Bringing an imported install's media files under the new store.
//!
//! After the database import, the native store's storage locations still
//! point into the reference install's `client_files`. A transfer gives the
//! new store its own copy of the tree, laid out identically (ADR-2):
//!
//! - **hardlink** (the default): new directory entries for the same files.
//!   No extra space, instant, and the two installs are independent: deleting
//!   a file in one leaves the other's link. Needs the same filesystem.
//! - **copy**: an independent copy. Needs the space.
//! - **move**: takes the files away from the reference install, which is
//!   left without its media. Fast on one filesystem.
//! - **in place**: keep using the reference install's files. The store then
//!   never deletes media from disk, since the reference install still needs
//!   them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};
use crate::settings::{self, Setting};

/// How to bring media files across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferMode {
    #[default]
    Hardlink,
    Copy,
    Move,
    InPlace,
}

/// Who owns the media files on disk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaOwnership {
    /// Set when the files belong to another install (an in-place import):
    /// the store must never delete them.
    pub shared_with: Option<PathBuf>,
}

impl Setting for MediaOwnership {
    const KEY: &'static str = "media_ownership";
}

/// What a transfer did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TransferReport {
    pub files: u64,
    pub bytes: u64,
    /// The store's media directory afterwards.
    pub destination: PathBuf,
}

/// One storage subfolder (e.g. `f3a`, or `f3a/b` at granularity 3).
#[derive(Debug, Clone)]
struct Subfolder {
    /// Relative to the location, e.g. `f3a/b`.
    relative: PathBuf,
    location: PathBuf,
}

fn subfolders(conn: &Connection) -> Result<Vec<Subfolder>> {
    let mut stmt = conn.prepare(
        "SELECT s.prefix, l.path FROM storage_subfolders s JOIN storage_locations l USING (location_id)
         ORDER BY s.prefix",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (prefix, location) = row?;
        // 'f3ab' -> 'f3a/b': the first folder keeps the kind letter and two hex digits
        let split = prefix.len().min(3);
        let mut relative = PathBuf::from(&prefix[..split]);
        let mut rest = &prefix[split..];
        while !rest.is_empty() {
            let take = rest.len().min(2);
            relative.push(&rest[..take]);
            rest = &rest[take..];
        }
        out.push(Subfolder {
            relative,
            location: PathBuf::from(location),
        });
    }
    Ok(out)
}

/// The regular files directly in `dir` (subfolders at deeper granularity are
/// separate [`Subfolder`]s).
fn files_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    out.push(entry.path());
                }
            }
        }
        // a subfolder with no files yet may not exist
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(out)
}

fn is_cross_device(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::CrossesDevices
}

/// Give the store at `db_path` its own media under `destination`, and point
/// its storage locations there. On failure, whatever was created under
/// `destination` is removed and the store still points at the source.
pub fn transfer_media(
    db_path: &Path,
    destination: &Path,
    mode: TransferMode,
) -> Result<TransferReport> {
    let conn = Connection::open(db_path)?;
    if mode == TransferMode::InPlace {
        let source = conn
            .query_row(
                "SELECT path FROM storage_locations ORDER BY location_id LIMIT 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .map(PathBuf::from)
            .unwrap_or_default();
        settings::set(
            &conn,
            &MediaOwnership {
                shared_with: Some(source.clone()),
            },
        )?;
        return Ok(TransferReport {
            destination: source,
            ..TransferReport::default()
        });
    }
    if destination.exists() {
        return Err(StoreError::Invalid(format!(
            "{} already exists; choose a new directory for the media",
            destination.display()
        )));
    }
    std::fs::create_dir_all(destination)?;
    let folders = subfolders(&conn)?;
    let result = match mode {
        TransferMode::Move => move_folders(&folders, destination),
        _ => link_or_copy(&folders, destination, mode),
    };
    let report = match result {
        Ok(report) => report,
        Err(e) => {
            if mode != TransferMode::Move {
                let _ = std::fs::remove_dir_all(destination);
            }
            return Err(e);
        }
    };
    // one location holding every subfolder
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM storage_locations", [])?;
    tx.execute(
        "INSERT INTO storage_locations (location_id, path, ideal_weight, max_bytes, is_thumbnail_override)
         VALUES (1, ?1, 1, NULL, 0)",
        [destination.to_string_lossy()],
    )?;
    tx.execute(
        "UPDATE OR REPLACE storage_subfolders SET location_id = 1",
        [],
    )?;
    settings::set(&tx, &MediaOwnership::default())?;
    tx.commit()?;
    Ok(report)
}

fn link_or_copy(
    folders: &[Subfolder],
    destination: &Path,
    mode: TransferMode,
) -> Result<TransferReport> {
    use rayon::prelude::*;
    let files = AtomicU64::new(0);
    let bytes = AtomicU64::new(0);
    folders.par_iter().try_for_each(|folder| -> Result<()> {
        let target = destination.join(&folder.relative);
        std::fs::create_dir_all(&target)?;
        for source in files_in(&folder.location.join(&folder.relative))? {
            let name = source.file_name().expect("read_dir entries have names");
            let dest = target.join(name);
            let size = match mode {
                TransferMode::Hardlink => {
                    std::fs::hard_link(&source, &dest).map_err(|e| {
                        if is_cross_device(&e) {
                            StoreError::Invalid(format!(
                                "{} is on a different filesystem from {}, so it can't be hardlinked; \
                                 copy the files instead, or choose a media directory on the same filesystem",
                                source.display(),
                                destination.display()
                            ))
                        } else {
                            e.into()
                        }
                    })?;
                    std::fs::metadata(&dest)?.len()
                }
                _ => std::fs::copy(&source, &dest)?,
            };
            files.fetch_add(1, Ordering::Relaxed);
            bytes.fetch_add(size, Ordering::Relaxed);
        }
        Ok(())
    })?;
    Ok(TransferReport {
        files: files.into_inner(),
        bytes: bytes.into_inner(),
        destination: destination.to_path_buf(),
    })
}

/// Move whole subfolders; undo the moves made so far if one fails.
fn move_folders(folders: &[Subfolder], destination: &Path) -> Result<TransferReport> {
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut report = TransferReport {
        destination: destination.to_path_buf(),
        ..TransferReport::default()
    };
    let result = (|| -> Result<()> {
        // deepest first, so 'f3a/b' moves before 'f3a' would swallow it
        let mut ordered: Vec<&Subfolder> = folders.iter().collect();
        ordered.sort_by_key(|f| std::cmp::Reverse(f.relative.components().count()));
        for folder in ordered {
            let source = folder.location.join(&folder.relative);
            let target = destination.join(&folder.relative);
            let files = files_in(&source)?;
            if files.is_empty() {
                continue;
            }
            for file in &files {
                report.bytes += std::fs::metadata(file)?.len();
            }
            report.files += files.len() as u64;
            std::fs::create_dir_all(&target)?;
            for file in files {
                let dest = target.join(file.file_name().expect("read_dir entries have names"));
                std::fs::rename(&file, &dest).map_err(|e| {
                    if is_cross_device(&e) {
                        StoreError::Invalid(format!(
                            "{} is on a different filesystem from {}; copy the files instead",
                            file.display(),
                            destination.display()
                        ))
                    } else {
                        StoreError::from(e)
                    }
                })?;
                done.push((file, dest));
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        for (source, dest) in done.into_iter().rev() {
            let _ = std::fs::rename(&dest, &source);
        }
        return Err(e);
    }
    Ok(report)
}

/// Transfer a fixture's media and check the store can still find every file.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::tests::import_basic;

    fn file_count(dir: &Path) -> usize {
        walk(dir).len()
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    #[test]
    fn hardlink_copy_and_move_give_the_store_its_own_media() {
        for mode in [
            TransferMode::Hardlink,
            TransferMode::Copy,
            TransferMode::Move,
        ] {
            let (source, dest_dir, db) = import_basic();
            let source_files = source.path().join("client_files");
            let before = file_count(&source_files);
            assert!(before > 0);
            let media = dest_dir.path().join("media");
            let report = transfer_media(&db, &media, mode).unwrap();
            assert_eq!(report.files as usize, before, "{mode:?}");
            assert_eq!(file_count(&media), before, "{mode:?}");
            let expected_left = if mode == TransferMode::Move {
                0
            } else {
                before
            };
            assert_eq!(file_count(&source_files), expected_left, "{mode:?}");

            // every file the store knows is found under the new location
            let conn = Connection::open(&db).unwrap();
            let storage = crate::storage::FileStorage::load(&conn).unwrap();
            assert_eq!(storage.locations().len(), 1);
            let mut stmt = conn
                .prepare("SELECT h.sha256, f.mime FROM files f JOIN hashes h USING (hash_id)")
                .unwrap();
            let rows: Vec<(Vec<u8>, u8)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            let mut found = 0;
            for (hash, mime) in rows {
                let hash = hydrus_core::Sha256::from_slice(&hash).unwrap();
                let Some(mime) = hydrus_core::Mime::from_code(mime) else {
                    continue;
                };
                if storage.file_path(&hash, mime).unwrap().is_file() {
                    found += 1;
                }
            }
            assert!(found > 0, "{mode:?}");
            let ownership: MediaOwnership = settings::get(&conn).unwrap();
            assert_eq!(ownership.shared_with, None);
        }
    }

    #[test]
    fn in_place_marks_the_media_as_shared() {
        let (source, _dest_dir, db) = import_basic();
        transfer_media(&db, Path::new("/nonexistent"), TransferMode::InPlace).unwrap();
        let conn = Connection::open(&db).unwrap();
        let ownership: MediaOwnership = settings::get(&conn).unwrap();
        assert_eq!(
            ownership.shared_with,
            Some(source.path().join("client_files"))
        );
    }

    #[test]
    fn refuses_an_existing_destination() {
        let (_source, dest_dir, db) = import_basic();
        assert!(transfer_media(&db, dest_dir.path(), TransferMode::Copy).is_err());
    }
}
