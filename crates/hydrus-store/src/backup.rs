//! Database > backup (`_SetupBackupPath`, `_BackupDatabase`, the database's
//! `_Backup`): the chosen backup directory, when a backup was last made,
//! and making one: the database copied with SQLite's online backup, then
//! `client_files` mirrored (copying what differs in size or date, deleting
//! what the source lacks), as `HydrusPaths.MirrorTree` does.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::settings::Setting;
use crate::store::{DB_FILE_NAME, Store};

/// The backup directory (the reference's `backup_path`) and the last
/// backup's time (`last_backup_time`, seconds).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupSettings {
    pub path: Option<String>,
    pub last_backup: Option<i64>,
}

impl Setting for BackupSettings {
    const KEY: &'static str = "backup";
}

/// The one directory the store keeps its media in, if it keeps it all in
/// one (`client_files` for a new store; an imported client's own).
pub fn media_location(store: &Store) -> Result<Option<PathBuf>> {
    let storage = store.read(crate::storage::FileStorage::load)?;
    let mut paths = storage.locations().iter().map(|l| l.path.clone());
    let Some(first) = paths.next() else {
        return Ok(Some(store.dir().join("client_files")));
    };
    Ok(paths.all(|p| p == first).then_some(first))
}

/// Whether the store is simple enough for the in-client backup (the
/// reference's `all_locations_are_default`: media in one location).
pub fn locations_are_default(store: &Store) -> Result<bool> {
    Ok(media_location(store)?.is_some())
}

/// Copy `source` to `dest` unless it is there with the same size and
/// modified time (`MirrorFile`); whether it copied.
pub fn mirror_file(source: &Path, dest: &Path) -> std::io::Result<bool> {
    let from = std::fs::metadata(source)?;
    if let Ok(to) = std::fs::metadata(dest)
        && to.len() == from.len()
        && to.modified().ok() == from.modified().ok()
    {
        return Ok(false);
    }
    std::fs::copy(source, dest)?;
    if let Ok(modified) = from.modified() {
        std::fs::File::options()
            .write(true)
            .open(dest)?
            .set_modified(modified)?;
    }
    Ok(true)
}

/// Make `dest` look exactly like `source` (`MirrorTree`), saying each
/// directory it works on, and stopping if `cancelled`.
pub fn mirror_tree(
    source: &Path,
    dest: &Path,
    say: &mut dyn FnMut(String),
    cancelled: &dyn Fn() -> bool,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut dirs = vec![(source.to_path_buf(), dest.to_path_buf())];
    while let Some((from, to)) = dirs.pop() {
        if cancelled() {
            return Ok(());
        }
        say(format!("Copying {}.", from.display()));
        std::fs::create_dir_all(&to)?;
        let mut surplus: std::collections::BTreeSet<std::ffi::OsString> = std::fs::read_dir(&to)?
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        for entry in std::fs::read_dir(&from)? {
            let entry = entry?;
            let name = entry.file_name();
            surplus.remove(&name);
            let (source, dest) = (entry.path(), to.join(&name));
            if entry.file_type()?.is_dir() {
                if dest.is_file() {
                    std::fs::remove_file(&dest)?;
                }
                dirs.push((source, dest));
            } else {
                if dest.is_dir() {
                    std::fs::remove_dir_all(&dest)?;
                }
                mirror_file(&source, &dest)?;
            }
        }
        for name in surplus {
            let path = to.join(name);
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

/// Back the store up to `dest` (`_Backup`): its database, then its media
/// directory; `say` gets the popup's text as it goes.
pub fn backup(
    store: &Store,
    dest: &Path,
    say: &mut dyn FnMut(String),
    cancelled: &dyn Fn() -> bool,
) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    say(format!("copying {DB_FILE_NAME}"));
    let partial = dest.join(format!("{DB_FILE_NAME}.partial"));
    let _ = std::fs::remove_file(&partial);
    store.read(|conn| {
        let mut copy = rusqlite::Connection::open(&partial)?;
        let backup = rusqlite::backup::Backup::new(conn, &mut copy)?;
        backup.run_to_completion(1024, Duration::ZERO, None)?;
        Ok(())
    })?;
    std::fs::rename(&partial, dest.join(DB_FILE_NAME))?;
    if let Some(media) = media_location(store)?
        && media.exists()
        && !cancelled()
    {
        mirror_tree(&media, &dest.join("client_files"), say, cancelled)?;
    }
    Ok(())
}

/// The file asking the next start to restore a backup (its path).
const RESTORE_REQUEST: &str = "restore_from_backup.txt";

/// Ask the next start to restore the backup at `from` over the store in
/// `dir`, whose media is in `media` (`RestoreDatabase` sets
/// `_restore_backup_path`, then exits to restart).
pub fn request_restore(dir: &Path, from: &Path, media: &Path) -> std::io::Result<()> {
    std::fs::write(
        dir.join(RESTORE_REQUEST),
        format!("{}\n{}", from.display(), media.display()),
    )
}

/// The backup a start was asked to restore and where its media goes, the
/// request taken away.
pub fn take_restore_request(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let request = dir.join(RESTORE_REQUEST);
    let text = std::fs::read_to_string(&request).ok()?;
    let _ = std::fs::remove_file(&request);
    let mut lines = text.lines();
    let from = PathBuf::from(lines.next()?);
    let media = lines
        .next()
        .map_or_else(|| dir.join("client_files"), PathBuf::from);
    Some((from, media))
}

/// Replace the (closed) store in `dir` with the backup at `from` (the
/// database's `_RestoreBackup`): its database copied over, `media`
/// mirrored from the backup's media.
pub fn restore(dir: &Path, from: &Path, media: &Path, say: &mut dyn FnMut(String)) -> Result<()> {
    let source = from.join(DB_FILE_NAME);
    if !source.is_file() {
        return Err(crate::StoreError::Invalid(format!(
            "There is no {DB_FILE_NAME} in \"{}\" to restore!",
            from.display()
        )));
    }
    for suffix in ["", "-wal", "-shm"] {
        let path = dir.join(format!("{DB_FILE_NAME}{suffix}"));
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    say(format!("restoring {DB_FILE_NAME}"));
    std::fs::copy(&source, dir.join(DB_FILE_NAME))?;
    let backed_up = from.join("client_files");
    if backed_up.is_dir() {
        mirror_tree(&backed_up, media, say, &|| false)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mirror_copies_changes_and_drops_surplus() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        std::fs::create_dir_all(a.path().join("f00")).unwrap();
        std::fs::write(a.path().join("f00/one"), b"1").unwrap();
        std::fs::write(b.path().join("stale"), b"x").unwrap();
        let mut said = Vec::new();
        mirror_tree(a.path(), b.path(), &mut |t| said.push(t), &|| false).unwrap();
        assert_eq!(std::fs::read(b.path().join("f00/one")).unwrap(), b"1");
        assert!(!b.path().join("stale").exists());
        assert!(!mirror_file(&a.path().join("f00/one"), &b.path().join("f00/one")).unwrap());
        assert!(!said.is_empty());
    }

    #[test]
    fn a_backup_restores_over_a_changed_store() {
        let (home, copy) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        {
            let store = Store::open(home.path()).unwrap();
            assert!(locations_are_default(&store).unwrap());
            store
                .write(|ctx| {
                    crate::settings::set(
                        ctx.conn(),
                        &BackupSettings {
                            path: Some("kept".into()),
                            last_backup: None,
                        },
                    )
                })
                .unwrap();
            backup(&store, copy.path(), &mut |_| {}, &|| false).unwrap();
            store
                .write(|ctx| crate::settings::set(ctx.conn(), &BackupSettings::default()))
                .unwrap();
        }
        let media = home.path().join("client_files");
        request_restore(home.path(), copy.path(), &media).unwrap();
        let (from, to) = take_restore_request(home.path()).unwrap();
        assert_eq!(to, media);
        assert!(take_restore_request(home.path()).is_none());
        restore(home.path(), &from, &to, &mut |_| {}).unwrap();
        let store = Store::open(home.path()).unwrap();
        let settings: BackupSettings = store.read(crate::settings::get).unwrap();
        assert_eq!(settings.path.as_deref(), Some("kept"));
    }
}
