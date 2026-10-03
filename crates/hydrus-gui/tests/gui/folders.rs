//! The manage import folders and manage export folders dialogs, from the
//! file menu, on a real store: adding, editing (with the edit dialogs'
//! checks) and deleting folders, "apply" writing them, and "cancel"
//! writing nothing. (Their rows and checks are tested against the
//! reference's in hydrus-gui-model's tests.)

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, FoldersWindow, MainWindow, Pages, bind, headless};
use hydrus_store::{import_folders, settings};

use crate::subscriptions::store;

/// file > import/export folders > `label`.
fn open(ui: &MainWindow, label: &str) {
    let titles = ui.get_menu_titles();
    let file = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "file")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(file).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let folders = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "import/export folders")
        .unwrap();
    ui.invoke_menu_line_hovered(0, i32::try_from(folders).unwrap(), 200.0, 100.0, 10.0);
    let lines = ui.get_menu_panes().row_data(1).unwrap().lines;
    let entry = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == label)
        .unwrap_or_else(|| panic!("{label}"));
    ui.invoke_menu_line_clicked(1, i32::try_from(entry).unwrap(), 0.0, 0.0, 0.0);
}

fn rows(list: &FoldersWindow) -> Vec<Vec<String>> {
    let rows = list.get_rows();
    (0..rows.row_count())
        .map(|r| {
            let row = rows.row_data(r).unwrap();
            (0..row.cells.row_count())
                .map(|c| row.cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

/// A screenshot of the third window made (after the main window and the
/// list), kept in the target directory.
fn shot(windows: &headless::Windows, name: &str, width: u32, height: u32) {
    let pixels = headless::render(&windows.get(2).unwrap(), width, height);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join(name), &pixels, width, height).unwrap();
}

fn import_list(bound: &Bound) -> FoldersWindow {
    bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .expect("the list opens")
        .clone_strong()
}

#[test]
fn import_folders_are_added_edited_and_written() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // (a directory of its own: the store's directory, inside the temporary
    // directory, can't be imported from)
    let watched = tempfile::tempdir().unwrap();
    let existing = watched.path().to_string_lossy().into_owned();

    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    assert_eq!(list.get_window_title(), "edit import folders");
    assert!(
        list.get_warning()
            .starts_with("WARNING: Import folders check")
    );
    assert!(rows(&list).is_empty());

    // "add": a new folder, refused without a path
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(edit.get_name(), "import folder");
    assert!(edit.get_check_regularly() && edit.get_search_subdirectories());
    let period: Vec<i32> = (0..edit.get_period().row_count())
        .map(|i| edit.get_period().row_data(i).unwrap().value)
        .collect();
    assert_eq!(period, [0, 1, 0], "an hour");
    assert_eq!(edit.get_actions().row_count(), 4);
    shot(&windows, "import_folder.png", 860, 760);
    edit.invoke_apply();
    assert!(edit.get_asking());
    assert_eq!(
        edit.get_asking_message(),
        "You must enter a path to import from!"
    );
    edit.invoke_chosen(0);
    assert!(!edit.get_asking());

    // a path, a move without a location (refused), then one that doesn't
    // exist (warned of, then given back)
    edit.set_path(existing.clone().into());
    let actions = edit.get_actions();
    edit.invoke_action_chosen(0, 2);
    edit.invoke_apply();
    assert_eq!(
        edit.get_asking_message(),
        "You must enter a path for your successful file move location!"
    );
    edit.invoke_chosen(0);
    edit.invoke_location_edited(0, "/nonexistent/place".into());
    assert_eq!(actions.row_data(0).unwrap().location, "/nonexistent/place");
    edit.invoke_period_edited(1, 2);
    edit.set_paused(true);
    edit.invoke_apply();
    assert!(edit.get_asking_message().starts_with(
        "The path you have entered for your successful file move location--\"/nonexistent/place\"--does not exist!"
    ));
    edit.invoke_chosen(0);
    assert!(bound.folders.import_edit.borrow().is_none());
    assert_eq!(
        rows(&list),
        [vec![
            "import folder".to_owned(),
            existing.clone(),
            "yes".into(),
            "2 hours".into()
        ]]
    );

    // another, named as the first: made unique
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path(existing.clone().into());
    edit.invoke_apply();
    let names: Vec<String> = rows(&list).into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(names, ["import folder", "import folder (1)"]);

    // "apply": written
    list.invoke_apply();
    assert!(bound.folders.import_list.borrow().is_none());
    let written = store.read(import_folders::import_folders).unwrap();
    assert_eq!(written.len(), 2);
    let first = written
        .iter()
        .find(|f| f.name() == "import folder")
        .unwrap();
    assert!(first.paused());
    assert_eq!(first.settings.period, 7200);
    assert_eq!(
        first.settings.actions.successful_and_new,
        hydrus_parse::folders::FolderAction::Move("/nonexistent/place".into())
    );

    // again: edit one, delete the other
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    assert_eq!(rows(&list).len(), 2);
    list.invoke_row_clicked(1, false, false);
    list.invoke_edit();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(edit.get_name(), "import folder (1)");
    edit.set_name("second".into());
    edit.invoke_apply();
    list.invoke_row_clicked(0, false, false);
    list.invoke_delete();
    assert_eq!(list.get_asking_message(), "Remove all selected?");
    list.invoke_chosen(0);
    assert_eq!(rows(&list).len(), 1);
    // "cancel": nothing written
    list.invoke_cancel();
    assert_eq!(store.read(import_folders::import_folders).unwrap().len(), 2);
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_delete();
    list.invoke_chosen(0);
    list.invoke_apply();
    let names: Vec<String> = store
        .read(import_folders::import_folders)
        .unwrap()
        .iter()
        .map(|f| f.name().to_owned())
        .collect();
    assert_eq!(names, ["import folder (1)"]);
}

#[test]
fn export_folders_are_added_edited_and_written() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    open(&ui, "manage export folders\u{2026}");
    let list = bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(list.get_window_title(), "edit export folders");
    list.invoke_add();
    let edit = bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(edit.get_name(), "export folder");
    assert_eq!(edit.get_phrase(), "{hash}");
    assert!(edit.get_run_regularly());
    // refused: no path, then a phrase that doesn't parse
    edit.invoke_apply();
    assert_eq!(
        edit.get_asking_message(),
        "You must enter a folder path to export to!"
    );
    edit.invoke_chosen(0);
    edit.set_path("/tmp/exported".into());
    edit.set_phrase("[series".into());
    edit.invoke_apply();
    assert!(
        edit.get_asking_message()
            .starts_with("Could not parse that export phrase!")
    );
    edit.invoke_chosen(0);
    edit.set_phrase("{hash} [series]".into());
    // a search typed in
    edit.set_typed("system:archive".into());
    edit.invoke_typed_accepted();
    edit.set_typed("samus aran".into());
    edit.invoke_typed_accepted();
    let predicates = edit.get_predicates();
    assert_eq!(predicates.row_count(), 2);
    shot(&windows, "export_folder.png", 760, 820);
    // deleting from the client warns, and "apply" asks
    edit.set_delete_from_client(true);
    edit.invoke_delete_toggled();
    assert!(
        edit.get_asking_message()
            .starts_with("This will delete the exported files")
    );
    edit.invoke_chosen(0);
    edit.invoke_apply();
    assert!(
        edit.get_asking_message()
            .starts_with("You have set this export folder to delete")
    );
    edit.invoke_chosen(1);
    assert!(bound.folders.export_edit.borrow().is_some(), "refused");
    // synchronising never deletes from the client
    edit.invoke_type_chosen(1);
    assert!(!edit.get_delete_from_client());
    edit.invoke_apply();
    assert!(bound.folders.export_edit.borrow().is_none());
    let shown = &rows(&list)[0];
    assert_eq!(shown[2], "synchronise");
    assert_eq!(shown[4], "1 day");
    list.invoke_apply();
    let settings::ExportFolders(written) = store.read(settings::get).unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].phrase, "{hash} [series]");
    assert_eq!(written[0].search.predicates.len(), 2);
    assert_eq!(
        written[0].export_type,
        hydrus_parse::folders::ExportType::Synchronise
    );
}
