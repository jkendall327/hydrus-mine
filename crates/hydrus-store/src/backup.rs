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
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "backup cancelled",
            ));
        }
        say(format!("Copying {}.", from.display()));
        std::fs::create_dir_all(&to)?;
        let mut surplus: std::collections::BTreeSet<std::ffi::OsString> = std::fs::read_dir(&to)?
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        for entry in std::fs::read_dir(&from)? {
            if cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "backup cancelled",
                ));
            }
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
            if cancelled() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "backup cancelled",
                ));
            }
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
    if cancelled() {
        return Err(
            std::io::Error::new(std::io::ErrorKind::Interrupted, "backup cancelled").into(),
        );
    }
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
    if cancelled() {
        return Err(
            std::io::Error::new(std::io::ErrorKind::Interrupted, "backup cancelled").into(),
        );
    }
    if let Some(media) = media_location(store)?
        && media.exists()
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
    let from = restore_source(dir, from)?;
    std::fs::write(
        dir.join(RESTORE_REQUEST),
        format!("{}\n{}", from.display(), media.display()),
    )
}

/// Read the requested backup and media destination without consuming the
/// request. Only a successful restore removes it.
pub fn restore_request(dir: &Path) -> std::io::Result<Option<(PathBuf, PathBuf)>> {
    let text = match std::fs::read_to_string(dir.join(RESTORE_REQUEST)) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut lines = text.lines();
    let from = lines
        .next()
        .filter(|line| !line.is_empty())
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "empty restore request")
        })?;
    let media = lines
        .next()
        .map_or_else(|| dir.join("client_files"), PathBuf::from);
    Ok(Some((PathBuf::from(from), media)))
}

/// Restore a pending backup, retaining the request on any failure. The
/// caller must hold the GUI lock before this or opening the store.
pub fn restore_pending(dir: &Path, say: &mut dyn FnMut(String)) -> Result<bool> {
    restore_pending_with_copy(dir, say, &mut |source, dest| std::fs::copy(source, dest))
}

fn restore_pending_with_copy(
    dir: &Path,
    say: &mut dyn FnMut(String),
    copy: &mut dyn FnMut(&Path, &Path) -> std::io::Result<u64>,
) -> Result<bool> {
    let Some((from, media)) = restore_request(dir)? else {
        return Ok(false);
    };
    restore_with_copy(dir, &from, &media, say, copy)?;
    std::fs::remove_file(dir.join(RESTORE_REQUEST))?;
    Ok(true)
}

fn restore_source(dir: &Path, from: &Path) -> std::io::Result<PathBuf> {
    let from = std::fs::canonicalize(from)?;
    if std::fs::canonicalize(dir)? == from {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the backup cannot be the store's own directory",
        ));
    }
    if !from.join(DB_FILE_NAME).is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "There is no {DB_FILE_NAME} in \"{}\" to restore!",
                from.display()
            ),
        ));
    }
    Ok(from)
}

/// Replace the closed store with a backup. An active serving process is
/// refused. Stage the entire database copy before touching the old database
/// or sidecars; media is mirrored before the staged database is installed.
/// The caller must also hold the GUI lock if used by a desktop client.
pub fn restore(dir: &Path, from: &Path, media: &Path, say: &mut dyn FnMut(String)) -> Result<()> {
    restore_with_copy(dir, from, media, say, &mut |source, dest| {
        std::fs::copy(source, dest)
    })
}

fn restore_with_copy(
    dir: &Path,
    from: &Path,
    media: &Path,
    say: &mut dyn FnMut(String),
    copy: &mut dyn FnMut(&Path, &Path) -> std::io::Result<u64>,
) -> Result<()> {
    let from = restore_source(dir, from)?;
    let _serving = crate::store::lock_serving(dir)?.ok_or_else(|| {
        crate::StoreError::Invalid("stop the serving process before restoring a backup".into())
    })?;
    say(format!("restoring {DB_FILE_NAME}"));
    let staged = tempfile::NamedTempFile::new_in(dir)?;
    copy(&from.join(DB_FILE_NAME), staged.path())?;
    staged.as_file().sync_all()?;
    let backed_up = from.join("client_files");
    if backed_up.is_dir() {
        mirror_tree(&backed_up, media, say, &|| false)?;
    }
    for suffix in ["-wal", "-shm"] {
        let path = dir.join(format!("{DB_FILE_NAME}{suffix}"));
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    staged
        .persist(dir.join(DB_FILE_NAME))
        .map_err(|error| error.error)?;
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
    fn cancelled_backup_does_not_report_success_or_touch_destination() {
        let home = tempfile::tempdir().unwrap();
        let copy = tempfile::tempdir().unwrap();
        let store = Store::open(home.path()).unwrap();
        let sentinel = copy.path().join(DB_FILE_NAME);
        std::fs::write(&sentinel, b"previous backup").unwrap();
        let result = backup(&store, copy.path(), &mut |_| {}, &|| true);
        assert!(
            matches!(result, Err(crate::StoreError::Io(e)) if e.kind() == std::io::ErrorKind::Interrupted)
        );
        assert_eq!(std::fs::read(sentinel).unwrap(), b"previous backup");
    }

    #[test]
    fn cancellation_during_mirroring_preserves_uncopied_and_surplus_files() {
        let source = tempfile::tempdir().unwrap();
        let dest = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("new"), b"new").unwrap();
        std::fs::write(dest.path().join("old"), b"old").unwrap();
        let cancelled = std::cell::Cell::new(false);
        let result = mirror_tree(
            source.path(),
            dest.path(),
            &mut |_| cancelled.set(true),
            &|| cancelled.get(),
        );
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(std::fs::read(dest.path().join("old")).unwrap(), b"old");
        assert!(!dest.path().join("new").exists());
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
        let (from, to) = restore_request(home.path()).unwrap().unwrap();
        assert_eq!(from, std::fs::canonicalize(copy.path()).unwrap());
        assert_eq!(to, media);
        assert!(restore_request(home.path()).unwrap().is_some());
        assert!(restore_pending(home.path(), &mut |_| {}).unwrap());
        assert!(restore_request(home.path()).unwrap().is_none());
        let store = Store::open(home.path()).unwrap();
        let settings: BackupSettings = store.read(crate::settings::get).unwrap();
        assert_eq!(settings.path.as_deref(), Some("kept"));
    }

    #[test]
    fn restore_rejects_the_store_itself_before_writing_a_request_or_database() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(DB_FILE_NAME), b"original").unwrap();
        let media = home.path().join("client_files");
        assert_eq!(
            request_restore(home.path(), &home.path().join("."), &media)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
        assert!(!home.path().join(RESTORE_REQUEST).exists());
        assert!(restore(home.path(), home.path(), &media, &mut |_| {}).is_err());
        assert_eq!(
            std::fs::read(home.path().join(DB_FILE_NAME)).unwrap(),
            b"original"
        );
    }

    #[cfg(unix)]
    #[test]
    fn restore_rejects_a_symlink_to_the_store() {
        let home = tempfile::tempdir().unwrap();
        let aliases = tempfile::tempdir().unwrap();
        let alias = aliases.path().join("same-store");
        std::os::unix::fs::symlink(home.path(), &alias).unwrap();
        std::fs::write(home.path().join(DB_FILE_NAME), b"original").unwrap();
        let media = home.path().join("client_files");
        assert!(request_restore(home.path(), &alias, &media).is_err());
        assert!(restore(home.path(), &alias, &media, &mut |_| {}).is_err());
        assert_eq!(
            std::fs::read(home.path().join(DB_FILE_NAME)).unwrap(),
            b"original"
        );
    }

    #[test]
    fn a_failed_restore_copy_preserves_the_database_sidecars_media_and_request() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let media = home.path().join("client_files");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("kept"), b"media").unwrap();
        for suffix in ["", "-wal", "-shm"] {
            std::fs::write(home.path().join(format!("{DB_FILE_NAME}{suffix}")), suffix).unwrap();
        }
        std::fs::write(source.path().join(DB_FILE_NAME), b"replacement").unwrap();
        request_restore(home.path(), source.path(), &media).unwrap();
        let request = std::fs::read(home.path().join(RESTORE_REQUEST)).unwrap();
        let result = restore_pending_with_copy(home.path(), &mut |_| {}, &mut |_, dest| {
            std::fs::write(dest, b"partial")?;
            Err(std::io::Error::other("injected source copy failure"))
        });
        assert!(result.is_err());
        for suffix in ["", "-wal", "-shm"] {
            assert_eq!(
                std::fs::read(home.path().join(format!("{DB_FILE_NAME}{suffix}"))).unwrap(),
                suffix.as_bytes()
            );
        }
        assert_eq!(std::fs::read(media.join("kept")).unwrap(), b"media");
        assert_eq!(
            std::fs::read(home.path().join(RESTORE_REQUEST)).unwrap(),
            request
        );
        assert_eq!(
            std::fs::read_dir(home.path()).unwrap().count(),
            6,
            "failed staging leaves no temporary copy behind"
        );
        // A retry consumes the same retained request only after success.
        assert!(restore_pending(home.path(), &mut |_| {}).unwrap());
        assert_eq!(
            std::fs::read(home.path().join(DB_FILE_NAME)).unwrap(),
            b"replacement"
        );
        assert!(!home.path().join(RESTORE_REQUEST).exists());
        assert!(!home.path().join(format!("{DB_FILE_NAME}-wal")).exists());
        assert!(!home.path().join(format!("{DB_FILE_NAME}-shm")).exists());
    }

    #[test]
    fn a_missing_backup_retains_the_request_and_original_database() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let media = home.path().join("client_files");
        std::fs::write(home.path().join(DB_FILE_NAME), b"original").unwrap();
        std::fs::write(source.path().join(DB_FILE_NAME), b"replacement").unwrap();
        request_restore(home.path(), source.path(), &media).unwrap();
        std::fs::remove_file(source.path().join(DB_FILE_NAME)).unwrap();
        assert!(restore_pending(home.path(), &mut |_| {}).is_err());
        assert_eq!(
            std::fs::read(home.path().join(DB_FILE_NAME)).unwrap(),
            b"original"
        );
        assert!(restore_request(home.path()).unwrap().is_some());
    }

    #[test]
    fn an_active_server_prevents_restore_without_consuming_the_request() {
        let home = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let media = home.path().join("client_files");
        std::fs::write(home.path().join(DB_FILE_NAME), b"original").unwrap();
        std::fs::write(source.path().join(DB_FILE_NAME), b"replacement").unwrap();
        request_restore(home.path(), source.path(), &media).unwrap();
        let _serving = crate::store::lock_serving(home.path()).unwrap().unwrap();
        let error = restore_pending(home.path(), &mut |_| {}).unwrap_err();
        assert!(error.to_string().contains("stop the serving process"));
        assert_eq!(
            std::fs::read(home.path().join(DB_FILE_NAME)).unwrap(),
            b"original"
        );
        assert!(restore_request(home.path()).unwrap().is_some());
    }
}
