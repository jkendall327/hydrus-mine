//! How paths reach the "review files to import" window, replayed against the
//! reference's (`oracle/fixtures/import_paths.json`, from
//! `oracle/record_import_paths.py`): "add files" and "add folder" through the
//! system's pickers (a stand-in that answers as the recording's scripted
//! dialogs did, and says what it was asked), each also cancelled, and files
//! dropped on the window and on the main window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, Pick, ReviewImportsWindow, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Run the window's timer until it has parsed everything it was given.
fn settle(window: &ReviewImportsWindow) {
    for _ in 0..1000 {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
        if !window.get_working() {
            // (and once more, for the text its last parse sets)
            slint::platform::update_timers_and_animations();
            return;
        }
    }
    panic!("parsing never finished");
}

fn rows(window: &ReviewImportsWindow) -> Vec<Vec<String>> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .map(|i| {
            let cells = rows.row_data(i).unwrap().cells;
            (0..cells.row_count())
                .map(|c| cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

fn strings(value: &serde_json::Value) -> Vec<Vec<String>> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_owned())
                .collect()
        })
        .collect()
}

/// The pickers, drops and the main window's drop, step by step as recorded:
/// what each picker was asked (its title, and whether it takes several
/// existing files or one folder), and the window's rows and progress text
/// once parsing settles. A main-window drop while the window is open joins
/// its list, where the reference opens a second window (DIFFERENCES.md): the
/// new window's rows are compared as they follow the first's.
// leaf: audit-network-import-paths
#[test]
fn the_pickers_and_drops_give_the_window_the_reference_s_paths() {
    let recorded = hydrus_testkit::fixture_json("import_paths.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let work = tempfile::tempdir().unwrap();
    let folder = work.path().join("in");
    copy_tree(&hydrus_testkit::fixture_path("import_folder"), &folder);
    let folder_text = folder.display().to_string();
    let real = |p: &str| PathBuf::from(p.replace("<folder>", &folder_text));
    let shown = |p: &str| p.replace(&folder_text, "<folder>");

    // the stand-in pickers: they answer with the step's paths, or nothing (cancelled)
    let answer: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let asked: Rc<RefCell<Vec<(Pick, String)>>> = Rc::default();
    hydrus_gui::set_picker({
        let (answer, asked) = (answer.clone(), asked.clone());
        move |kind, title| {
            asked.borrow_mut().push((kind, title.to_owned()));
            std::mem::take(&mut *answer.borrow_mut())
        }
    });
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let window = || {
        bound
            .review_imports
            .borrow()
            .as_ref()
            .map(|(w, _)| w.clone_strong())
            .expect("the window is open")
    };

    let mut seen = 0;
    for step in recorded["steps"].as_array().unwrap() {
        let name = step["step"].as_str().unwrap();
        let given: Vec<PathBuf> = step["given"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| real(p.as_str().unwrap()))
            .collect();
        *answer.borrow_mut() = given.clone();
        asked.borrow_mut().clear();
        let given: Vec<String> = given.iter().map(|p| p.display().to_string()).collect();
        match name {
            "file > import files" => {
                ui.invoke_menu_title_pressed(0, 20.0, 22.0);
                let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
                let at = (0..lines.row_count())
                    .position(|i| lines.row_data(i).unwrap().label == "import files\u{2026}")
                    .unwrap();
                ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
            }
            n if n.starts_with("add files") => window().invoke_add_files(),
            n if n.starts_with("add folder") => window().invoke_add_folder(),
            // (winit hands over dropped files only, so the recording's web URL has no
            // counterpart to drop: the window is given the file)
            "dropped on the window, with a web URL" => {
                assert!(hydrus_gui::deliver_dropped_files(window().window(), given));
            }
            "dropped on the main window" => (bound.drop_files)(given),
            other => panic!("{other}"),
        }
        settle(&window());

        // what the pickers were asked
        let wanted: Vec<(Pick, String)> = step["asked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| {
                let kind = match (a[0].as_str().unwrap(), a.get(2).and_then(|m| m.as_str())) {
                    ("file dialog", Some("ExistingFiles")) => Pick::Files,
                    ("pick directory", None) => Pick::Folder,
                    other => panic!("{other:?}"),
                };
                (kind, a[1].as_str().unwrap().to_owned())
            })
            .collect();
        assert_eq!(*asked.borrow(), wanted, "{name}");

        // the rows and progress
        let windows = step["windows"].as_array().unwrap();
        let mut wanted_rows = strings(&windows[0]["rows"]);
        let mut wanted_progress = windows[0]["progress"].as_str();
        if let Some(second) = windows.get(1) {
            // (the reference's second window: here its rows follow the first's, numbered on;
            // the progress of the two lists together was not recorded)
            for mut row in strings(&second["rows"]) {
                row[0] = (wanted_rows.len() + 1).to_string();
                wanted_rows.push(row);
            }
            wanted_progress = None;
        }
        let got: Vec<Vec<String>> = rows(&window())
            .into_iter()
            .map(|r| r.iter().map(|c| shown(c)).collect())
            .collect();
        assert_eq!(got, wanted_rows, "{name}");
        if let Some(progress) = wanted_progress {
            assert_eq!(window().get_progress(), progress, "{name}");
        }
        seen += 1;
    }
    assert_eq!(seen, 7);
}
