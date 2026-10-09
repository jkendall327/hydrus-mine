//! File > import/export folders' "check import folder now" and "run
//! export folder now" (`_CheckImportFolder`, `_RunExportFolder`): one named
//! folder, or all of them, flagged to run at the next scheduler pass.

use hydrus_store::Store;

/// What the client says when the folders are asked to run while they are
/// paused under the file menu (they are flagged all the same).
pub const IMPORT_FOLDERS_PAUSED: &str = "Import folders are currently paused under the 'file' menu. Please unpause them and try this again.";
pub const EXPORT_FOLDERS_PAUSED: &str = "Export folders are currently paused under the 'file' menu. Please unpause them and try this again.";

/// Flag the import folder `name` (all: none) to be checked now, and
/// unpause it (`ImportFolder.CheckNow`). The text to show, if the import
/// folders are paused under the file menu.
pub fn check_import_folders(
    store: &Store,
    name: Option<String>,
) -> hydrus_store::Result<Option<&'static str>> {
    store.write(move |ctx| {
        let conn = ctx.conn();
        let paused: hydrus_store::settings::FolderSettings = hydrus_store::settings::get(conn)?;
        for folder in hydrus_store::import_folders::import_folders(conn)? {
            if name.as_deref().is_none_or(|n| n == folder.name()) {
                let mut settings = folder.settings.clone();
                settings.check_now = true;
                hydrus_store::import_folders::set_settings(conn, folder.id(), &settings)?;
                hydrus_store::queues::set_paused(conn, folder.id(), Some(false), None)?;
            }
        }
        Ok(paused.pause_import_folders.then_some(IMPORT_FOLDERS_PAUSED))
    })
}

/// Flag the export folder `name` (all: none) to run now. The text to show,
/// if the export folders are paused under the file menu.
pub fn run_export_folders(
    store: &Store,
    name: Option<String>,
) -> hydrus_store::Result<Option<&'static str>> {
    store.write(move |ctx| {
        let paused: hydrus_store::settings::FolderSettings =
            hydrus_store::settings::get(ctx.conn())?;
        let mut folders: hydrus_store::settings::ExportFolders =
            hydrus_store::settings::get(ctx.conn())?;
        for folder in &mut folders.0 {
            if name.as_deref().is_none_or(|n| n == folder.name) {
                folder.run_now = true;
            }
        }
        hydrus_store::settings::set(ctx.conn(), &folders)?;
        Ok(paused.pause_export_folders.then_some(EXPORT_FOLDERS_PAUSED))
    })
}
