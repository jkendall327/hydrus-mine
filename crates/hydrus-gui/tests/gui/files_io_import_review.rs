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

use crate::common::widgets;

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
    // the removal replayed against the reference's
    // (`oracle/fixtures/import_review_remove.json`, from
    // `oracle/record_import_review_remove.py`)
    let recorded = hydrus_testkit::fixture_json("import_review_remove.json");
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let window = open(&ui, &bound);
    let work = tempfile::tempdir().unwrap();
    // the recorded files, given in the recorded order
    for name in recorded["files"].as_array().unwrap() {
        let path = place(work.path(), name.as_str().unwrap());
        window.invoke_path_entered(path.into());
        settle(&window);
    }
    let shown = |w: &ReviewImportsWindow| -> serde_json::Value {
        rows(w)
            .iter()
            .map(|r| {
                let name = Path::new(&r.1).file_name().unwrap().to_string_lossy();
                serde_json::json!([r.0, name])
            })
            .collect()
    };
    let steps = recorded["steps"].as_array().unwrap();
    assert_eq!(shown(&window), steps[0]["rows"]);
    assert!(
        !window.get_any_selected(),
        "nothing selected, nothing to remove"
    );

    // the recorded extended selection: click, ctrl-click, shift-click and
    // ctrl+shift-click, rows 0..3 (`selection_steps`)
    let selected = |w: &ReviewImportsWindow| -> Vec<usize> {
        rows(w)
            .iter()
            .enumerate()
            .filter(|(_, r)| r.2)
            .map(|(i, _)| i)
            .collect()
    };
    let mut differs = Vec::new();
    for step in recorded["selection_steps"].as_array().unwrap() {
        let name = step["do"].as_str().unwrap();
        let (kind, row) = name.rsplit_once(' ').unwrap();
        let row: i32 = row.parse().unwrap();
        match kind {
            "click" => window.invoke_row_clicked(row, false, false),
            "ctrl-click" => window.invoke_row_clicked(row, true, false),
            "shift-click" => window.invoke_row_clicked(row, false, true),
            _ => window.invoke_row_clicked(row, true, true),
        }
        let theirs: Vec<usize> = step["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| usize::try_from(r.as_i64().unwrap()).unwrap())
            .collect();
        if selected(&window) != theirs {
            differs.push((name.to_owned(), selected(&window), theirs));
        }
    }
    // (a shift-click after a ctrl-click that cleared the range's origin
    // keeps the other ctrl-selected row in the reference, not here:
    // DIFFERENCES.md)
    assert_eq!(
        differs,
        [("shift-click 1".to_owned(), vec![0, 1], vec![0, 1, 2])]
    );
    // the recorded selection: the second row
    window.invoke_row_clicked(1, false, false);
    assert_eq!(selected(&window), [1]);
    assert_eq!(shown(&window)[1], recorded["selected"]);
    assert!(window.get_any_selected());

    let win = window.window();
    widgets::lay_out(win, 900.0, 700.0);
    for step in &steps[1..] {
        let context = step["do"].to_string();
        assert!(!window.get_asking_remove());
        match step["do"].as_str().unwrap() {
            "delete key" => {
                // (the list has the keys' focus once a row is clicked: a
                // click is a pointer press the harness has no layout to
                // aim; Tab moves there)
                for _ in 0..3 {
                    if window.get_asking_remove() {
                        break;
                    }
                    widgets::key(win, slint::platform::Key::Delete);
                    if !window.get_asking_remove() {
                        widgets::key(win, "\t");
                    }
                }
            }
            _ => widgets::click(win, "remove files"),
        }
        // the reference's RemovePaths question, answered with its button
        let mut asked = Vec::new();
        for question in ["Remove all selected?"] {
            if widgets::shows(win, question) {
                asked.push(question);
            }
        }
        assert!(window.get_asking_remove(), "{context}");
        widgets::click(
            win,
            if step["answer"].as_bool().unwrap() {
                "yes"
            } else {
                "no"
            },
        );
        assert!(!window.get_asking_remove(), "{context}");
        assert!(!widgets::shows(win, "Remove all selected?"), "{context}");
        assert_eq!(serde_json::json!(asked), step["asked"], "{context}");
        assert_eq!(shown(&window), step["rows"], "{context}");
    }
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
