//! file > import files, and files dropped on the window: the "review files
//! to import" window with the paths given, parsing them as it goes; and
//! "import now" opening an import page with its good files, in order, each
//! with its modified time, and whether to delete them.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, ReviewImportsWindow, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues::{self, LocalImport};

const OLD_MTIME: i64 = 1_600_000_000;

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

/// A copy of a fixture file in `dir`, last modified long ago.
fn place(dir: &Path, name: &str) -> String {
    let to = dir.join(name);
    std::fs::copy(hydrus_testkit::fixture_path(format!("media/{name}")), &to).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&to)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(OLD_MTIME as u64))
        .unwrap();
    to.to_string_lossy().into_owned()
}

/// file > `label`
fn menu(ui: &MainWindow, label: &str) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == label)
        .unwrap_or_else(|| panic!("file > {label}"));
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
}

/// Let the window parse (its timer runs) until it is done.
fn parsed(window: &ReviewImportsWindow) {
    for _ in 0..500 {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
        if !window.get_working() && window.get_rows().row_count() > 0 {
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

#[test]
fn files_given_are_reviewed_then_imported() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = || {
        bound
            .review_imports
            .borrow()
            .as_ref()
            .map(|(w, _)| w.clone_strong())
            .expect("the window is open")
    };

    // file > import files…: the window, waiting for paths
    menu(&ui, "import files\u{2026}");
    assert_eq!(window().get_progress(), "waiting for paths to parse.");
    assert!(!window().get_can_import());
    assert!(!window().get_working());

    // a folder typed in (as a file manager copies it, in quotes): parsed
    let work = tempfile::tempdir().unwrap();
    let files = [
        place(work.path(), "bmp_24.bmp"),
        place(work.path(), "apng_rgba.png"),
        place(work.path(), "gif_static.gif"),
    ];
    std::fs::write(work.path().join("readme.md"), b"not a file to import").unwrap();
    window().invoke_path_entered(format!("  \"{}\" ", work.path().display()).into());
    parsed(&window());
    let shown = rows(&window());
    assert_eq!(shown.len(), 4, "{shown:?}");
    assert_eq!(
        window().get_progress(),
        "4 files parsed - 3 good | 1 bad: 1 had unsupported file types."
    );
    assert!(window().get_can_import());
    // (the unsupported file taken out, asked first)
    let readme = shown
        .iter()
        .position(|r| r[1].ends_with("readme.md"))
        .unwrap();
    assert!(!window().get_any_selected());
    window().invoke_row_clicked(i32::try_from(readme).unwrap(), false, false);
    assert!(window().get_any_selected());
    window().invoke_remove_files();
    assert_eq!(rows(&window()).len(), 3);
    assert!(!window().get_any_selected());
    // deleting the originals, and import now: an import page of the three,
    // in the list's order, the window closed
    let order: Vec<String> = rows(&window()).into_iter().map(|r| r[1].clone()).collect();
    let mut sorted_files = files.to_vec();
    sorted_files.sort();
    let mut sorted_order = order.clone();
    sorted_order.sort();
    assert_eq!(sorted_order, sorted_files);
    window().set_delete_after_success(true);
    window().invoke_delete_toggled(true);
    window().invoke_import_now();
    assert!(bound.review_imports.borrow().is_none(), "closed");
    assert_eq!(bound.pages.borrow().shown().name, "import");
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    let made = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert_eq!(
        LocalImport::of(&made),
        Some(LocalImport {
            delete_after_success: true
        })
    );
    let seeds: Vec<(String, Option<i64>)> = store
        .read(move |c| queues::file_seeds(c, queue))
        .unwrap()
        .into_iter()
        .map(|s| (s.data, s.source_time))
        .collect();
    let expected: Vec<(String, Option<i64>)> =
        order.into_iter().map(|p| (p, Some(OLD_MTIME))).collect();
    assert_eq!(seeds, expected);

    // files dropped on the window: the window again, with them
    (bound.drop_files)(vec![files[0].clone()]);
    parsed(&window());
    assert_eq!(rows(&window()).len(), 1);
    assert!(!window().get_delete_after_success(), "a fresh window");
    // and dropped again while it is open: added to it
    (bound.drop_files)(vec![files[1].clone()]);
    parsed(&window());
    assert_eq!(rows(&window()).len(), 2);
    // escape closes it, importing nothing
    window().invoke_cancel();
    assert!(bound.review_imports.borrow().is_none());
}
