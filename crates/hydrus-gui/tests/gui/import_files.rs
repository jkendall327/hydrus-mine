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

/// The open window's paths list's metadata column.
fn tagging_rows(dialog: &hydrus_gui::FilenameTaggingWindow) -> Vec<Vec<String>> {
    use slint::Model as _;
    let rows = dialog.get_rows();
    (0..rows.row_count())
        .map(|r| {
            let row = rows.row_data(r).unwrap();
            (0..row.cells.row_count())
                .map(|c| row.cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

#[test]
fn files_are_imported_with_the_tags_filename_tagging_gives_them() {
    use slint::Model as _;
    let (_dirs, store) = store();
    let windows = headless::init();
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
    let work = tempfile::tempdir().unwrap();
    let files = [
        place(work.path(), "bmp_24.bmp"),
        place(work.path(), "apng_rgba.png"),
    ];
    menu(&ui, "import files\u{2026}");
    window().invoke_path_entered(work.path().display().to_string().into());
    parsed(&window());
    window().invoke_add_tags();
    let dialog = bound
        .filename_tagging
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    // a tab per real tag service
    let services: Vec<String> = (0..dialog.get_services().row_count())
        .map(|i| dialog.get_services().row_data(i).unwrap().to_string())
        .collect();
    let mine = services.iter().position(|s| s == "my tags").unwrap();
    dialog.invoke_service_chosen(i32::try_from(mine).unwrap());
    // tags for all, the filename, and a number
    dialog.set_tags_all("Imported\nbatch:one".into());
    dialog.set_number_namespace("n".into());
    dialog.invoke_changed();
    dialog.invoke_misc_toggled(0, true);
    let rows = tagging_rows(&dialog);
    let order: Vec<String> = rows.iter().map(|r| r[1].clone()).collect();
    let stem = |p: &str| {
        std::path::Path::new(p)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    assert_eq!(
        rows[0][2],
        format!("batch:one, filename:{}, imported, n:1", stem(&order[0]))
    );
    // a tag for the second file alone
    dialog.invoke_row_clicked(1, false, false);
    assert!(dialog.get_has_selection());
    dialog.set_tags_selected("only this".into());
    dialog.invoke_changed();
    assert!(tagging_rows(&dialog)[1][2].contains("only this"));
    assert!(!tagging_rows(&dialog)[0][2].contains("only this"));
    // a bad regex is said to be bad, and left out
    dialog.set_regexes("[unclosed".into());
    dialog.invoke_changed();
    assert!(
        dialog
            .get_errors()
            .starts_with("That regex would not compile!")
    );
    let pixels = headless::render(&windows.get(2).unwrap(), 1000, 760);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("filename_tagging.png"), &pixels, 1000, 760).unwrap();

    // "apply": imported, each file with its tags for "my tags"
    dialog.invoke_apply();
    assert!(bound.filename_tagging.borrow().is_none());
    assert!(bound.review_imports.borrow().is_none());
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    let seeds = store.read(move |c| queues::file_seeds(c, queue)).unwrap();
    let my_tags = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let tags_of = |i: usize| -> Vec<String> {
        seeds[i]
            .meta
            .external_additional_tags
            .iter()
            .find(|(k, _)| *k == my_tags)
            .map(|(_, t)| t.iter().cloned().collect())
            .unwrap_or_default()
    };
    assert_eq!(seeds[0].data, order[0]);
    assert_eq!(
        tags_of(0),
        [
            "batch:one".to_owned(),
            format!("filename:{}", stem(&order[0])),
            "imported".to_owned(),
            "n:1".to_owned()
        ]
    );
    assert!(tags_of(1).contains(&"only this".to_owned()));
    let _ = files;
}
