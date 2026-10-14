//! The manage import folders and manage export folders dialogs, from the
//! file menu, on a real store: adding, editing (with the edit dialogs'
//! checks) and deleting folders, "apply" writing them, and "cancel"
//! writing nothing. (Their rows and checks are tested against the
//! reference's in hydrus-gui-model's tests.)

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, FoldersWindow, MainWindow, Pages, bind, headless};
use hydrus_store::{import_folders, settings};

use crate::subscriptions::store;

/// A 1x1 png: an importable file by content.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// file > import/export folders > `label`.
pub(crate) fn open(ui: &MainWindow, label: &str) {
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

pub(crate) fn import_list(bound: &Bound) -> FoldersWindow {
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
    // its cached paths' file log
    assert!(edit.get_has_file_log());
    edit.invoke_file_log();
    let log = bound.folders.log.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(log.get_window_title(), "file log");
    assert_eq!(log.get_rows().row_count(), 0);
    log.invoke_close_window();
    assert!(bound.folders.log.borrow().is_none());
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

// leaf: audit-network-tagging-example
#[test]
fn an_import_folders_filename_tagging_is_added_edited_and_deleted() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();
    // the example is the first importable file by content (`_PathChanged`):
    // a text file and a one-byte ".jpg" are passed over
    std::fs::write(watched.path().join("notes.txt"), b"a sidecar").unwrap();
    std::fs::write(watched.path().join("bad.jpg"), b"x").unwrap();
    let example = watched.path().join("pic 1.png");
    std::fs::write(&example, PNG).unwrap();
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path(watched.path().to_string_lossy().into_owned().into());
    let choices: Vec<String> = (0..edit.get_tagging_choices().row_count())
        .map(|i| edit.get_tagging_choices().row_data(i).unwrap().to_string())
        .collect();
    let mine = choices.iter().position(|c| c == "my tags").unwrap();
    edit.set_tagging_choice(i32::try_from(mine).unwrap());
    edit.invoke_tagging_add();
    let dialog = || {
        bound
            .folders
            .filename_tagging
            .borrow()
            .as_ref()
            .expect("it opens")
            .clone_strong()
    };
    // `oracle/fixtures/import_folder_example.json`, from
    // `oracle/record_import_folder_example.py`: the real editor's dialog
    // on this same folder
    let recorded = hydrus_testkit::fixture_json("import_folder_example.json");
    let opened = &recorded["dialogs"][0];
    assert_eq!(
        dialog().get_window_title(),
        opened["title"].as_str().unwrap()
    );
    assert!(dialog().get_options_mode());
    assert_eq!(dialog().get_example(), example.to_string_lossy().as_ref());
    assert_eq!(
        example.file_name().unwrap().to_string_lossy(),
        opened["example_path"].as_str().unwrap()
    );
    assert_eq!(
        dialog().get_example_tags(),
        opened["output_as_opened"].as_str().unwrap()
    );
    dialog().set_tags_all("Imported".into());
    dialog().invoke_changed();
    dialog().invoke_misc_toggled(0, true);
    assert_eq!(
        dialog().get_example_tags(),
        opened["output_edited"].as_str().unwrap()
    );
    dialog().invoke_apply();
    assert!(bound.folders.filename_tagging.borrow().is_none());
    let listed = |w: &hydrus_gui::ImportFolderWindow| -> Vec<String> {
        (0..w.get_filename_tagging().row_count())
            .map(|i| w.get_filename_tagging().row_data(i).unwrap().to_string())
            .collect()
    };
    assert_eq!(serde_json::json!(listed(&edit)), recorded["rows"][0]);
    // the same service again: refused
    edit.invoke_tagging_add();
    assert_eq!(
        edit.get_asking_message(),
        recorded["warnings"][0].as_str().unwrap()
    );
    edit.invoke_chosen(0);
    // edited: what it had
    edit.invoke_tagging_edit(0);
    assert_eq!(dialog().get_tags_all(), "imported");
    dialog().invoke_cancel();
    // written with the folder
    edit.invoke_apply();
    list.invoke_apply();
    let folder = store
        .read(import_folders::import_folders)
        .unwrap()
        .remove(0);
    let (_, tagging) = &folder.settings.filename_tagging[0];
    assert_eq!(tagging.add_filename.as_deref(), Some("filename"));
    assert!(tagging.tags_for_all.contains("imported"));
    // and deleted
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.invoke_tagging_delete(0);
    assert!(listed(&edit).is_empty());
}

// leaf: audit-network-import-folder-tagging
#[test]
fn the_filename_tagging_list_and_its_example_path_follow_the_references_recorded_steps() {
    // oracle/record_import_folder_tagging.py: the reference's editor, its
    // service chooser and tagging dialog scripted
    let recorded = hydrus_testkit::fixture_json("import_folder_tagging.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let work = tempfile::tempdir().unwrap();
    // a folder like the recorder's first: a text file, a one-byte jpeg, a
    // png, a folder and a gif
    let make_folder = |files: &[(&str, &str)]| {
        let path = tempfile::tempdir_in(work.path()).unwrap().keep();
        for (name, kind) in files {
            let full = path.join(name);
            match *kind {
                "dir" => std::fs::create_dir(&full).unwrap(),
                "one byte" => std::fs::write(&full, b"x").unwrap(),
                "text" => std::fs::write(&full, b"just words").unwrap(),
                image => {
                    std::fs::copy(
                        hydrus_testkit::fixture_path(format!("auto_resolution/{image}")),
                        &full,
                    )
                    .unwrap();
                }
            }
        }
        path
    };
    let folder = make_folder(&[
        ("a.txt", "text"),
        ("b.jpg", "one byte"),
        ("c.png", "p03_a.png"),
        ("d", "dir"),
        ("e.gif", "p00_h.gif"),
    ]);
    open(&ui, "manage import folders\u{2026}");
    let list = import_list(&bound);
    list.invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.set_path(folder.to_string_lossy().into_owned().into());
    // the services to choose from are the reference's tag services
    let choices: Vec<String> = (0..edit.get_tagging_choices().row_count())
        .map(|i| edit.get_tagging_choices().row_data(i).unwrap().to_string())
        .collect();
    let mut theirs: Vec<String> = recorded["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect();
    let mut ours = choices.clone();
    ours.sort();
    theirs.sort();
    assert_eq!(ours, theirs);
    let listed = |w: &hydrus_gui::ImportFolderWindow| -> Vec<Vec<String>> {
        (0..w.get_filename_tagging().row_count())
            .map(|i| vec![w.get_filename_tagging().row_data(i).unwrap().to_string()])
            .collect()
    };
    assert_eq!(listed(&edit), Vec::<Vec<String>>::new());
    let dialog = || {
        bound
            .folders
            .filename_tagging
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
    };
    let pick = |name: &str| i32::try_from(choices.iter().position(|c| c == name).unwrap()).unwrap();
    let services: Vec<String> = recorded["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect();
    let example_is_recorded = |example: &str, recorded_names: &serde_json::Value| {
        let names: Vec<&str> = recorded_names
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        match std::path::Path::new(example).file_name() {
            Some(name) => names.contains(&name.to_string_lossy().as_ref()),
            None => names.is_empty(),
        }
    };
    let first_examples = &recorded["examples"][0]["example_paths"];
    for step in recorded["steps"].as_array().unwrap() {
        let name = step["name"].as_str().unwrap();
        let warnings = step["warnings"].as_array().unwrap();
        let dialogs = step["dialogs"].as_array().unwrap();
        match name {
            "add the first service" | "add it again" | "add the second service" => {
                let service = if name == "add the second service" {
                    &services[1]
                } else {
                    &services[0]
                };
                edit.set_tagging_choice(pick(service));
                edit.invoke_tagging_add();
            }
            "edit the first" => edit.invoke_tagging_edit(0),
            "delete the first" => edit.invoke_tagging_delete(0),
            // (native chooses the service in the box beside the button)
            "cancel the service choice" => continue,
            other => panic!("{other}"),
        }
        // the warning, as the reference's words
        match warnings.first() {
            Some(warning) => {
                assert_eq!(
                    edit.get_asking_message().as_str(),
                    warning.as_str().unwrap(),
                    "{name}"
                );
                edit.invoke_chosen(0);
            }
            None => assert!(edit.get_asking_message().is_empty(), "{name}"),
        }
        // the tagging dialog, with the example path it was given
        match (dialogs.first(), dialog()) {
            (Some(theirs), Some(open)) => {
                assert_eq!(
                    open.get_window_title(),
                    theirs["title"].as_str().unwrap(),
                    "{name}"
                );
                // (the reference lists a folder's files in the file system's
                // order and takes the first importable; the folder here has two,
                // so either of them is the example, as in the recording's set)
                assert!(
                    example_is_recorded(&open.get_example(), first_examples),
                    "{name}: {}",
                    open.get_example()
                );
                // the dialog is for the recorded service (the recorder made
                // each service's key the hex of its name)
                let key = theirs["service"].as_str().unwrap();
                let recorded_name: String = (0..key.len())
                    .step_by(2)
                    .map(|i| char::from(u8::from_str_radix(&key[i..i + 2], 16).unwrap()))
                    .collect();
                let services = open.get_services();
                let index = usize::try_from(open.get_service_index()).unwrap();
                assert_eq!(
                    services.row_data(index).unwrap().as_str(),
                    recorded_name,
                    "{name}"
                );
                open.invoke_apply();
            }
            (None, None) => {}
            (theirs, ours) => panic!("{name}: {theirs:?} {}", ours.is_some()),
        }
        // the list after the step, as the reference's (service names, sorted)
        let mut shown = listed(&edit);
        shown.sort();
        let mut theirs: Vec<Vec<String>> = step["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                r.as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap().to_owned())
                    .collect()
            })
            .collect();
        theirs.sort();
        assert_eq!(shown, theirs, "{name}");
    }
    // the example path for other folders: the first importable file by content
    for example in recorded["examples"].as_array().unwrap() {
        let name = example["name"].as_str().unwrap();
        if name.starts_with("twenty-six") {
            // (the example is the first importable of the directory's first
            // 25 entries in the file system's order, here and in the
            // recording alike, so which of 27 entries make the cut is not
            // the same on two file systems; DIFFERENCES.md)
            continue;
        }
        let files: Vec<(String, &str)> = example["listing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                let n = n.as_str().unwrap();
                let kind = match std::path::Path::new(n).extension().and_then(|e| e.to_str()) {
                    Some("txt") => "text",
                    Some("jpg") => "one byte",
                    Some("png") => "p03_a.png",
                    Some("gif") => "p00_h.gif",
                    _ => "dir",
                };
                (n.to_owned(), kind)
            })
            .collect();
        let refs: Vec<(&str, &str)> = files.iter().map(|(n, k)| (n.as_str(), *k)).collect();
        let path = make_folder(&refs);
        edit.set_path(path.to_string_lossy().into_owned().into());
        if listed(&edit).is_empty() {
            edit.set_tagging_choice(pick(&services[0]));
            edit.invoke_tagging_add();
            dialog().expect("it opens").invoke_cancel();
            edit.invoke_tagging_add();
            edit.invoke_chosen(0);
        }
        edit.invoke_tagging_edit(0);
        let open = dialog().expect("the dialog opens");
        assert!(
            example_is_recorded(&open.get_example(), &example["example_paths"]),
            "{name}: {}",
            open.get_example()
        );
        open.invoke_cancel();
    }
    list.invoke_cancel();
}

/// "browse" beside a folder path, and beside a moved file outcome's
/// location, asks the system's folder picker (here, a stand-in).
#[test]
fn folder_paths_are_picked_with_browse() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    hydrus_gui::set_picker(|kind, title| {
        assert_eq!(
            (kind, title),
            (hydrus_gui::Pick::Folder, "Select directory")
        );
        vec!["/picked/folder".into()]
    });
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    open(&ui, "manage import folders\u{2026}");
    import_list(&bound).invoke_add();
    let edit = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.invoke_browse_path();
    assert_eq!(edit.get_path(), "/picked/folder");
    // a row whose action is "move"
    let mut row = edit.get_actions().row_data(0).unwrap();
    row.action = 2;
    edit.get_actions().set_row_data(0, row);
    edit.invoke_browse_location(0);
    assert_eq!(
        edit.get_actions().row_data(0).unwrap().location,
        "/picked/folder"
    );

    open(&ui, "manage export folders\u{2026}");
    bound
        .folders
        .export_list
        .borrow()
        .as_ref()
        .unwrap()
        .invoke_add();
    let edit = bound
        .folders
        .export_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    edit.invoke_browse_path();
    assert_eq!(edit.get_path(), "/picked/folder");
}
