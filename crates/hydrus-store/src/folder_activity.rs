//! Crash-safe coordination between folder managers and folder workers.
//!
//! An edit request is a transient pause, held even while waiting for work to
//! finish. An activity lease prevents reading a draft until that work has
//! committed, and prevents new work until the manager closes. Persisted pause
//! preferences are never overwritten, including changes made by another client.

use std::fs::File;
use std::path::{Path, PathBuf};

use crate::settings::{self, FolderSettings};
use crate::{Result, Store};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Import,
    Export,
}

impl Kind {
    fn request_file(self) -> &'static str {
        match self {
            Self::Import => "import-folders-edit.lock",
            Self::Export => "export-folders-edit.lock",
        }
    }

    fn activity_file(self) -> &'static str {
        match self {
            Self::Import => "import-folders-work.lock",
            Self::Export => "export-folders-work.lock",
        }
    }

    pub fn wait_text(self) -> &'static str {
        match self {
            Self::Import => "Waiting for import folders to finish.",
            Self::Export => "Waiting for export folders to finish.",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Import => "edit import folders",
            Self::Export => "edit export folders",
        }
    }
}

/// Whether an editor holds a pause request. Stale files after a crash are inert:
/// the OS lock, rather than the existence or contents of the file, is the marker.
pub fn edit_requested(dir: &Path, kind: Kind) -> std::io::Result<bool> {
    Ok(crate::store::lock(dir, kind.request_file())?.is_none())
}

/// Current effective pause, including live user changes and a manager request.
pub fn paused(store: &Store, kind: Kind) -> Result<bool> {
    if edit_requested(store.dir(), kind)? {
        return Ok(true);
    }
    let settings: FolderSettings = store.read(settings::get)?;
    Ok(match kind {
        Kind::Import => settings.pause_import_folders,
        Kind::Export => settings.pause_export_folders,
    })
}

/// Exclusive work permission. Hold this through the worker's final store write.
#[derive(Debug)]
pub struct Activity {
    _lock: File,
}

impl Activity {
    /// A concurrent worker or pending manager makes this a harmless skipped run.
    pub fn acquire(dir: &Path, kind: Kind) -> std::io::Result<Option<Self>> {
        if edit_requested(dir, kind)? {
            return Ok(None);
        }
        let Some(lock) = crate::store::lock(dir, kind.activity_file())? else {
            return Ok(None);
        };
        // Close the race with a manager requesting a pause during acquisition.
        if edit_requested(dir, kind)? {
            return Ok(None);
        }
        Ok(Some(Self { _lock: lock }))
    }
}

/// A manager's lifetime: request pause first, poll without blocking the UI,
/// then hold activity permission while reading and editing its private draft.
#[derive(Debug)]
pub struct Edit {
    dir: PathBuf,
    kind: Kind,
    _request: File,
    activity: Option<File>,
}

impl Edit {
    pub fn request(dir: &Path, kind: Kind) -> std::io::Result<Option<Self>> {
        Ok(
            crate::store::lock(dir, kind.request_file())?.map(|request| Self {
                dir: dir.to_owned(),
                kind,
                _request: request,
                activity: None,
            }),
        )
    }

    /// True only after the active worker's lease has been released.
    pub fn try_ready(&mut self) -> std::io::Result<bool> {
        if self.activity.is_none() {
            self.activity = crate::store::lock(&self.dir, self.kind.activity_file())?;
        }
        Ok(self.activity.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::{Activity, Edit, Kind, edit_requested};

    #[test]
    fn request_interrupts_work_waits_for_release_and_rejects_new_work() {
        let dir = tempfile::tempdir().unwrap();
        for kind in [Kind::Import, Kind::Export] {
            let worker = Activity::acquire(dir.path(), kind).unwrap().unwrap();
            let mut editor = Edit::request(dir.path(), kind).unwrap().unwrap();
            assert!(edit_requested(dir.path(), kind).unwrap());
            assert!(!editor.try_ready().unwrap());
            assert!(Edit::request(dir.path(), kind).unwrap().is_none());
            assert!(Activity::acquire(dir.path(), kind).unwrap().is_none());
            drop(worker);
            assert!(editor.try_ready().unwrap());
            assert!(editor.try_ready().unwrap());
            assert!(Activity::acquire(dir.path(), kind).unwrap().is_none());
            drop(editor);
            assert!(!edit_requested(dir.path(), kind).unwrap());
            assert!(Activity::acquire(dir.path(), kind).unwrap().is_some());
        }
    }

    #[test]
    fn abandoned_request_and_old_marker_files_do_not_pause_another_kind() {
        let dir = tempfile::tempdir().unwrap();
        let editor = Edit::request(dir.path(), Kind::Import).unwrap().unwrap();
        assert!(
            Activity::acquire(dir.path(), Kind::Export)
                .unwrap()
                .is_some()
        );
        drop(editor);
        // Dropping the owner leaves files behind, just as process death does.
        assert!(dir.path().join(Kind::Import.request_file()).exists());
        assert!(!edit_requested(dir.path(), Kind::Import).unwrap());
        let mut reopened = Edit::request(dir.path(), Kind::Import).unwrap().unwrap();
        assert!(reopened.try_ready().unwrap());
    }

    #[test]
    fn transient_pause_preserves_original_and_live_user_pause_preferences() {
        use crate::settings::{self, FolderSettings};
        let dir = tempfile::tempdir().unwrap();
        let store = crate::Store::open(dir.path()).unwrap();
        for kind in [Kind::Import, Kind::Export] {
            for originally_paused in [false, true] {
                let initial = FolderSettings {
                    pause_import_folders: originally_paused,
                    pause_export_folders: originally_paused,
                    ..FolderSettings::default()
                };
                store
                    .write(move |ctx| settings::set(ctx.conn(), &initial))
                    .unwrap();
                let editor = Edit::request(dir.path(), kind).unwrap().unwrap();
                assert!(super::paused(&store, kind).unwrap());
                let unchanged: FolderSettings = store.read(settings::get).unwrap();
                assert_eq!(unchanged.pause_import_folders, originally_paused);
                assert_eq!(unchanged.pause_export_folders, originally_paused);
                let changed = FolderSettings {
                    pause_import_folders: !originally_paused,
                    pause_export_folders: !originally_paused,
                    ..FolderSettings::default()
                };
                store
                    .write(move |ctx| settings::set(ctx.conn(), &changed))
                    .unwrap();
                assert!(super::paused(&store, kind).unwrap());
                drop(editor);
                assert_eq!(super::paused(&store, kind).unwrap(), !originally_paused);
            }
        }
    }
}
