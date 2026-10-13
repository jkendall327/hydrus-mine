pub mod main_menu;
pub mod menus;
pub mod widgets;

/// Recordings that expect trash to prune a view opt into that saved policy.
/// The reference's false default otherwise retains page and viewer rows.
pub fn remove_trashed_from_view(store: &hydrus_store::Store) {
    store
        .write(|ctx| {
            let mut preferences: hydrus_store::settings::FileViewRemoval =
                hydrus_store::settings::get(ctx.conn())?;
            preferences.trashed = true;
            hydrus_store::settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
}

/// An explicit combined-local-media page for recordings that span every local
/// domain. GUI opening defaults are user settings and can select one domain.
pub fn all_local_page(store: std::sync::Arc<hydrus_store::Store>) -> hydrus_gui::SearchPage {
    store
        .write_and_refresh(|ctx| {
            let mut defaults: hydrus_store::settings::SearchDefaults =
                hydrus_store::settings::get(ctx.conn())?;
            defaults.local_location = hydrus_search::LocationContext::default();
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    hydrus_gui::SearchPage::new(store)
}

/// Runs the export-folder worker until no folder is left flagged to run now,
/// collecting every run. A folder whose activity lock is briefly unavailable
/// is skipped by design ("a harmless skipped run") and runs on the next pass;
/// in a test binary another test's child process can hold an inherited copy
/// of the lock for a moment, so one pass may skip a folder.
pub fn export_folders_until_done(
    store: &std::sync::Arc<hydrus_store::Store>,
) -> Vec<(String, hydrus_download::export::ExportRun)> {
    let mut all = Vec::new();
    for _ in 0..20 {
        all.extend(hydrus_download::export::work_export_folders(store).unwrap());
        let folders: hydrus_store::settings::ExportFolders =
            store.read(hydrus_store::settings::get).unwrap();
        if folders.0.iter().all(|f| !f.run_now) {
            return all;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("export folders still flagged to run after 20 passes: {all:?}");
}

/// The "Delete files?" dialog a thumbnail panel or the viewer has open, if any.
pub fn delete_dialog(bound: &hydrus_gui::Bound) -> Option<hydrus_gui::DeleteFilesWindow> {
    let viewer = bound
        .viewer_deletion
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    viewer.or_else(|| {
        bound
            .delete_files
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    })
}

/// What the open "Delete files?" dialog asks: its actions' labels, one per
/// line (a single action is shown as its label alone); empty if none is open.
pub fn delete_question(bound: &hydrus_gui::Bound) -> String {
    use slint::Model as _;
    delete_dialog(bound)
        .map(|dialog| {
            dialog
                .get_actions()
                .iter()
                .map(|label| label.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Accept (`true`) or cancel the open "Delete files?" dialog.
pub fn answer_delete(bound: &hydrus_gui::Bound, accept: bool) {
    let dialog = delete_dialog(bound).expect("a Delete files? dialog is open");
    if accept {
        dialog.invoke_accept_deletion();
    } else {
        dialog.invoke_cancel();
    }
}
