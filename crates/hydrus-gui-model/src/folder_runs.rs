//! File > import/export folders' "check import folder now" and "run
//! export folder now" (`_CheckImportFolder`, `_RunExportFolder`): one named
//! folder, or all of them, flagged to run at the next scheduler pass.

use hydrus_store::Store;

/// Flag the import folder `name` (all: none) to be checked now.
pub fn check_import_folders(store: &Store, name: Option<String>) -> hydrus_store::Result<()> {
    store.write(move |ctx| {
        let conn = ctx.conn();
        for folder in hydrus_store::import_folders::import_folders(conn)? {
            if name.as_deref().is_none_or(|n| n == folder.name()) {
                let mut settings = folder.settings.clone();
                settings.check_now = true;
                hydrus_store::import_folders::set_settings(conn, folder.id(), &settings)?;
            }
        }
        Ok(())
    })
}

/// Flag the export folder `name` (all: none) to run now.
pub fn run_export_folders(store: &Store, name: Option<String>) -> hydrus_store::Result<()> {
    store.write(move |ctx| {
        let mut folders: hydrus_store::settings::ExportFolders =
            hydrus_store::settings::get(ctx.conn())?;
        for folder in &mut folders.0 {
            if name.as_deref().is_none_or(|n| n == folder.name) {
                folder.run_now = true;
            }
        }
        hydrus_store::settings::set(ctx.conn(), &folders)
    })
}
