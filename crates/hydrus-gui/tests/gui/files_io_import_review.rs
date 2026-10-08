//! The "review files to import" window's list: extended selection with
//! the keyboard's Delete asking before it removes (and numbering what is
//! left again), and pause and stop while it parses
//! (`ReviewLocalFileImports`, against `oracle/fixtures/local_import_dialog.json`
//! in hydrus-gui-model's tests for its rows and progress).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, Pages, ReviewImportsWindow, bind, headless};
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

fn place(dir: &Path, name: &str) -> String {
    let to = dir.join(name);
    std::fs::copy(hydrus_testkit::fixture_path(format!("media/{name}")), &to).unwrap();
    to.to_string_lossy().into_owned()
}

/// Run the window's timer for `for_` (it parses a little at a time).
fn tick(for_: Duration) {
    let end = std::time::Instant::now() + for_;
    while std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
}

fn settle(window: &ReviewImportsWindow) {
    for _ in 0..500 {
        tick(Duration::from_millis(20));
        if !window.get_working() {
            return;
        }
    }
    panic!("parsing never finished");
}

/// (index, path, selected) per row.
fn rows(window: &ReviewImportsWindow) -> Vec<(String, String, bool)> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .map(|i| {
            let row = rows.row_data(i).unwrap();
            (
                row.cells.row_data(0).unwrap().to_string(),
                row.cells.row_data(1).unwrap().to_string(),
                row.selected,
            )
        })
        .collect()
}

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> ReviewImportsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "import files\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    bound
        .review_imports
        .borrow()
        .as_ref()
        .map(|(w, _)| w.clone_strong())
        .expect("file > import files opens the window")
}

// leaf: audit-network-import-remove
#[test]
fn rows_are_selected_as_a_list_selects_and_removed_after_asking_then_numbered_again() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let window = open(&ui, &bound);
    let work = tempfile::tempdir().unwrap();
    for name in [
        "bmp_24.bmp",
        "apng_rgba.png",
        "gif_static.gif",
        "png_rgba.png",
    ] {
        if hydrus_testkit::fixture_path(format!("media/{name}")).exists() {
            place(work.path(), name);
        }
    }
    let placed = std::fs::read_dir(work.path()).unwrap().count();
    assert!(placed >= 3, "{placed}");
    window.invoke_path_entered(work.path().to_string_lossy().into_owned().into());
    settle(&window);
    let listed = rows(&window);
    assert_eq!(listed.len(), placed);
    let numbers: Vec<String> = listed.iter().map(|r| r.0.clone()).collect();
    assert_eq!(
        numbers,
        (1..=placed).map(|n| n.to_string()).collect::<Vec<_>>()
    );
    assert!(
        !window.get_any_selected(),
        "nothing selected, nothing to remove"
    );

    // click selects one; ctrl-click toggles; shift-click extends from the
    // anchor; ctrl+shift-click adds the range to what is selected
    let selected = |w: &ReviewImportsWindow| -> Vec<usize> {
        rows(w)
            .iter()
            .enumerate()
            .filter(|(_, r)| r.2)
            .map(|(i, _)| i)
            .collect()
    };
    window.invoke_row_clicked(0, false, false);
    assert_eq!(selected(&window), [0]);
    window.invoke_row_clicked(2, true, false);
    assert_eq!(selected(&window), [0, 2]);
    window.invoke_row_clicked(0, true, false);
    assert_eq!(selected(&window), [2]);
    window.invoke_row_clicked(1, false, true);
    assert_eq!(
        selected(&window),
        [0, 1],
        "from the anchor, the last clicked row"
    );
    window.invoke_row_clicked(0, false, false);
    window.invoke_row_clicked(placed as i32 - 1, false, true);
    assert_eq!(selected(&window), (0..placed).collect::<Vec<_>>());
    window.invoke_row_clicked(1, false, false);
    assert_eq!(selected(&window), [1]);
    assert!(window.get_any_selected());

    // "remove files" asks first; no leaves the list alone
    assert!(!window.get_asking_remove());
    // (the list has the keys' focus once a row is clicked: a click is
    // a pointer press the harness has no layout to aim; Tab moves there)
    for _ in 0..3 {
        if window.get_asking_remove() {
            break;
        }
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Delete.into(),
            });
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "\t".into() });
    }
    assert!(window.get_asking_remove(), "Delete asks");
    assert_eq!(rows(&window).len(), placed, "nothing removed until yes");
    window.set_asking_remove(false);
    let second = rows(&window)[1].1.clone();
    // yes: the selected row goes, the rest are numbered again
    window.invoke_remove_files();
    let left = rows(&window);
    assert_eq!(left.len(), placed - 1);
    assert!(left.iter().all(|r| r.1 != second));
    assert_eq!(
        left.iter().map(|r| r.0.clone()).collect::<Vec<_>>(),
        (1..placed).map(|n| n.to_string()).collect::<Vec<_>>()
    );
    assert!(!window.get_any_selected());
    window.invoke_cancel();
}

// leaf: audit-network-import-pause-stop
#[test]
fn pausing_holds_the_paths_still_to_parse_and_stop_drops_them_but_keeps_the_parsed() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let window = open(&ui, &bound);
    let work = tempfile::tempdir().unwrap();
    let first = place(work.path(), "bmp_24.bmp");
    let second = place(work.path(), "gif_static.gif");
    let third = place(work.path(), "apng_rgba.png");

    // paused before anything is parsed: the path waits, "import" still off
    window.invoke_path_entered(first.clone().into());
    assert!(window.get_working());
    window.invoke_pause_play();
    assert!(window.get_paused());
    tick(Duration::from_millis(300));
    assert!(rows(&window).is_empty(), "paused, nothing parsed");
    assert_eq!(window.get_progress(), "0/1 files parsed.");
    assert!(window.get_working(), "its pause and stop buttons stay live");
    // resumed: it parses
    window.invoke_pause_play();
    assert!(!window.get_paused());
    settle(&window);
    assert_eq!(rows(&window).len(), 1);
    assert!(window.get_can_import());

    // paths added then stopped before they are parsed: dropped, and the
    // parsed row stays
    window.invoke_path_entered(second.clone().into());
    window.invoke_path_entered(third.into());
    assert!(window.get_working());
    window.invoke_stop();
    assert!(!window.get_working());
    tick(Duration::from_millis(300));
    let kept = rows(&window);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].1, first);
    assert_eq!(window.get_progress(), "1 files parsed.");
    // and the window is usable again afterwards
    window.invoke_path_entered(second.into());
    settle(&window);
    assert_eq!(rows(&window).len(), 2);
    window.invoke_cancel();
}

// leaf: audit-network-tagging-number
#[test]
fn files_are_numbered_from_a_base_by_a_step_and_the_numbers_are_cleaned_and_kept_apart_from_the_selected_tags()
 {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let window = open(&ui, &bound);
    let work = tempfile::tempdir().unwrap();
    for name in ["bmp_24.bmp", "gif_static.gif", "apng_rgba.png"] {
        place(work.path(), name);
    }
    window.invoke_path_entered(work.path().to_string_lossy().into_owned().into());
    settle(&window);
    window.invoke_add_tags();
    let dialog = bound
        .filename_tagging
        .borrow()
        .as_ref()
        .expect("add tags/urls with the import opens the dialog")
        .clone_strong();
    let services: Vec<String> = (0..dialog.get_services().row_count())
        .map(|i| dialog.get_services().row_data(i).unwrap().to_string())
        .collect();
    let mine = services.iter().position(|s| s == "my tags").unwrap();
    dialog.invoke_service_chosen(i32::try_from(mine).unwrap());
    let tags = |d: &hydrus_gui::FilenameTaggingWindow| -> Vec<String> {
        let rows = d.get_rows();
        (0..rows.row_count())
            .map(|r| {
                rows.row_data(r)
                    .unwrap()
                    .cells
                    .row_data(2)
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    // no namespace: no numbers
    dialog.set_number_base(5);
    dialog.set_number_step(3);
    dialog.invoke_changed();
    assert!(
        tags(&dialog).iter().all(String::is_empty),
        "{:?}",
        tags(&dialog)
    );

    // a namespace (cleaned as a tag is): from 5, in threes, in the list's order
    dialog.set_number_namespace("  Page ".into());
    dialog.invoke_changed();
    assert_eq!(tags(&dialog), ["page:5", "page:8", "page:11"]);
    // a tag for the middle file alone survives a new base and step
    dialog.invoke_row_clicked(1, false, false);
    dialog.set_tags_selected("Only This".into());
    dialog.invoke_changed();
    assert_eq!(tags(&dialog)[1], "only this, page:8");
    dialog.set_number_base(1);
    dialog.set_number_step(1);
    dialog.invoke_changed();
    assert_eq!(tags(&dialog), ["page:1", "only this, page:2", "page:3"]);
    // the namespace cleared: the numbers go, the selected tag stays
    dialog.set_number_namespace("".into());
    dialog.invoke_changed();
    assert_eq!(tags(&dialog), ["", "only this", ""]);
}
