//! The shared list's keys and pointer (`ui/list_table.slint`'s `ListTable`
//! and `ListKeys`), sent as real key and pointer events to real windows and
//! replayed against the reference's lists (`oracle/fixtures/list_keys.json`,
//! from `oracle/record_list_keys.py`): the "review files to import" list
//! for the keys that move and select, drag-selecting and Qt's own Ctrl+C;
//! the file log (the one list with both an activation and a copy-rows
//! callable) for Enter, Ctrl+C, double-click and the rows menu.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::Flickable;
use i_slint_core::window::WindowInner;
use serde_json::Value;
use slint::platform::{Key, PointerEventButton};
use slint::{ComponentHandle as _, LogicalPosition, Model as _, SharedString};

use hydrus_gui::{
    Clip, FileLogWindow, MainWindow, Pages, ReviewImportsWindow, bind, headless, set_clipper,
};
use hydrus_store::Store;
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

use crate::common::widgets::{self, Mods};

// --- real events (the shared helpers, `common/widgets.rs`) --------------

fn mods(ctrl: bool, shift: bool) -> Mods {
    Mods {
        ctrl,
        shift,
        ..Mods::NONE
    }
}

fn key(win: &slint::Window, text: SharedString, ctrl: bool, shift: bool) {
    widgets::key_with(win, text, mods(ctrl, shift));
    widgets::settle();
}

fn click(win: &slint::Window, at: LogicalPosition, ctrl: bool, shift: bool) {
    widgets::click_at_with(win, at, PointerEventButton::Left, mods(ctrl, shift));
}

/// A key the recording names, as Slint's text.
fn key_text(name: &str) -> SharedString {
    match name {
        "down" => Key::DownArrow.into(),
        "up" => Key::UpArrow.into(),
        "home" => Key::Home.into(),
        "end" => Key::End.into(),
        "pagedown" => Key::PageDown.into(),
        "pageup" => Key::PageUp.into(),
        "insert" => Key::Insert.into(),
        "enter" | "keypad enter" => Key::Return.into(),
        "space" => " ".into(),
        other => other.into(),
    }
}

/// The list's scrolled viewport (its top and height in the window): the
/// tallest flickable in the window.
fn viewport(win: &slint::Window) -> (f32, f32) {
    let inner = WindowInner::from_pub(win);
    let root = ItemRc::new_root(inner.component());
    let mut best = (0.0, 0.0);
    root.visit_descendants(|item| {
        if item.downcast::<Flickable>().is_some() {
            let height = item.geometry().size.height;
            let top = item
                .map_to_window(i_slint_core::lengths::LogicalPoint::default())
                .y;
            if height > best.1 {
                best = (top, height);
            }
        }
        std::ops::ControlFlow::<()>::Continue(())
    });
    best
}

/// Whether the list's keys (`ListKeys`, a focus scope of no size) have
/// the keyboard.
fn list_has_focus(win: &slint::Window) -> bool {
    let inner = WindowInner::from_pub(win);
    let focused = inner.focus_item.borrow().upgrade();
    focused.is_some_and(|item| {
        item.downcast::<i_slint_core::items::FocusScope>().is_some()
            && item.geometry().size.width == 0.0
            && item.geometry().size.height == 0.0
    })
}

/// Tab to the list, as a user gives it the keyboard without a click (the
/// recording focuses the list so).
fn tab_to_list(win: &slint::Window) {
    for _ in 0..10 {
        if list_has_focus(win) {
            return;
        }
        widgets::key(win, "\t");
        widgets::settle();
    }
    assert!(list_has_focus(win), "Tab reaches the list");
}

// --- the "review files to import" list ---------------------------------

fn import_store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    crate::subscriptions::store()
}

/// The window, opened as the user does (file > import files), with the
/// recorded files given to it in the recorded order.
fn open_imports(
    ui: &MainWindow,
    bound: &hydrus_gui::Bound,
    work: &Path,
    names: &[String],
) -> (ReviewImportsWindow, Vec<String>) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "import files\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let window = bound
        .review_imports
        .borrow()
        .as_ref()
        .map(|(w, _)| w.clone_strong())
        .expect("file > import files opens the window");
    let media = [
        "bmp_24.bmp",
        "apng_rgba.png",
        "gif_static.gif",
        "png_rgba.png",
        "bmp_8bit.bmp",
        "bmp_gray.bmp",
    ];
    let mut paths = Vec::new();
    for name in names {
        let to = work.join(name);
        if !to.exists() {
            let source = media.iter().find(|m| name.ends_with(*m)).unwrap();
            std::fs::copy(hydrus_testkit::fixture_path(format!("media/{source}")), &to).unwrap();
        }
        let path = to.to_string_lossy().into_owned();
        window.invoke_path_entered(path.clone().into());
        paths.push(path);
    }
    for _ in 0..1000 {
        std::thread::sleep(Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
        if !window.get_working() && window.get_rows().row_count() == names.len() {
            break;
        }
    }
    assert_eq!(window.get_rows().row_count(), names.len());
    (window, paths)
}

fn import_selected(window: &ReviewImportsWindow) -> Vec<u64> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .filter(|&i| rows.row_data(i).unwrap().selected)
        .map(|i| u64::try_from(i).unwrap())
        .collect()
}

fn numbers(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_u64().unwrap())
        .collect()
}

/// Whether the recorded sequence uses a key whose effect depends on the
/// list's height.
fn pages(sequence: &Value) -> bool {
    sequence["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["what"] == "pagedown" || s["what"] == "pageup")
}

// leaf: interaction-list-keys-navigate, interaction-list-pointer
#[test]
fn the_keys_move_and_select_as_the_reference_s_list() {
    // every recorded sequence, from a fresh window: arrows, Home/End and
    // PageUp/PageDown (plain, shift, ctrl, ctrl+shift), Space and
    // Ctrl+Space, Ctrl+A, then shift ranges from where they left the
    // anchor, and the clicks and drags between
    let recorded = hydrus_testkit::fixture_json("list_keys.json");
    let names: Vec<String> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    // a page is what Qt's tree view moves by: the row a viewport below
    // (or above) the current one (403 px of 15 px rows: 26)
    let page = &recorded["page"];
    let theirs_page =
        page["viewport_height"].as_u64().unwrap() / page["row_height"].as_u64().unwrap();
    assert_eq!(theirs_page, 26);
    let (_dirs, store) = import_store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let work = tempfile::tempdir().unwrap();
    let mut replayed = 0;
    for sequence in recorded["sequences"].as_array().unwrap() {
        let name = sequence["name"].as_str().unwrap();
        let (window, paths) = open_imports(&ui, &bound, work.path(), &names);
        let win = window.window();
        // tall enough for every row, or a viewport of 26 rows (of 22 px)
        // for the page keys, as the recording's
        win.set_size(slint::LogicalSize::new(900.0, 1100.0));
        widgets::settle();
        if pages(sequence) {
            let (_, height) = viewport(win);
            let want = 26.0 * 22.0 + 11.0;
            win.set_size(slint::LogicalSize::new(900.0, 1100.0 - (height - want)));
            widgets::settle();
            let (_, height) = viewport(win);
            assert_eq!(
                (height / 22.0).floor() as i32,
                26,
                "{name}: the list is a page of 26 rows"
            );
        }
        let row_at = |row: u64| widgets::position(win, &paths[usize::try_from(row).unwrap()], 0);
        // (the reference's list has the keyboard from the start)
        tab_to_list(win);
        for (i, step) in sequence["steps"].as_array().unwrap().iter().enumerate() {
            let context = format!("{name} step {i}");
            let (ctrl, shift) = (
                step["ctrl"].as_bool().unwrap(),
                step["shift"].as_bool().unwrap(),
            );
            match step["kind"].as_str().unwrap() {
                "click" => click(win, row_at(step["what"].as_u64().unwrap()), ctrl, shift),
                "drag" => {
                    // (the recorded drags hold no modifier)
                    assert!(!ctrl && !shift, "{context}");
                    let ends = numbers(&step["what"]);
                    widgets::drag(
                        win,
                        row_at(ends[0]),
                        row_at(ends[1]),
                        PointerEventButton::Left,
                    );
                }
                "key" => key(win, key_text(step["what"].as_str().unwrap()), ctrl, shift),
                other => panic!("{other}"),
            }
            assert_eq!(
                import_selected(&window),
                numbers(&step["selected"]),
                "{context}"
            );
            // the current row is brought into view
            if pages(sequence) {
                let current = step["current"].as_u64().unwrap();
                let (top, height) = viewport(win);
                let y = row_at(current).y;
                assert!(
                    y > top && y < top + height,
                    "{context}: row {current} in view"
                );
            }
        }
        window.invoke_cancel();
        widgets::settle();
        replayed += 1;
    }
    assert!(replayed >= 20);
}

// leaf: interaction-list-pointer
#[test]
fn clicks_with_ctrl_and_shift_and_qt_s_copy_on_the_import_list() {
    // the clicks of `list_selection.json` (on this same list in the
    // reference) as real pointer presses with ctrl and shift held
    let clicks = hydrus_testkit::fixture_json("list_selection.json");
    let recorded = hydrus_testkit::fixture_json("list_keys.json");
    let names: Vec<String> = clicks["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_owned())
        .collect();
    let (_dirs, store) = import_store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let work = tempfile::tempdir().unwrap();
    let (window, paths) = open_imports(&ui, &bound, work.path(), &names);
    let win = window.window();
    win.set_size(slint::LogicalSize::new(900.0, 700.0));
    widgets::settle();
    for sequence in clicks["sequences"].as_array().unwrap() {
        for (i, step) in sequence["steps"].as_array().unwrap().iter().enumerate() {
            let row = usize::try_from(step["row"].as_u64().unwrap()).unwrap();
            click(
                win,
                widgets::position(win, &paths[row], 0),
                step["ctrl"].as_bool().unwrap(),
                step["shift"].as_bool().unwrap(),
            );
            assert_eq!(
                import_selected(&window),
                numbers(&step["selected"]),
                "{} step {i}",
                sequence["name"]
            );
        }
    }
    // Ctrl+C on a list that copies nothing itself: Qt's tree view copies
    // the current cell (the first column of the row last clicked)
    let theirs = &recorded["no_copy_callable"];
    headless::set_clipboard_text("before");
    click(win, widgets::position(win, &paths[3], 0), false, false);
    click(win, widgets::position(win, &paths[5], 0), false, true);
    assert_eq!(import_selected(&window), numbers(&theirs["selected"]));
    key(win, "c".into(), true, false);
    assert_eq!(
        headless::clipboard_text().as_deref(),
        theirs["clipboard"].as_str()
    );
    // and Delete there asks to remove them, as the window's button does
    key(win, Key::Delete.into(), false, false);
    assert!(window.get_asking_remove());
    widgets::click(win, "no");
    window.invoke_cancel();
}

// --- the file log ------------------------------------------------------

type Log = (
    [tempfile::TempDir; 2],
    Arc<Store>,
    Rc<RefCell<Vec<Clip>>>,
    MainWindow,
    hydrus_gui::Bound,
);

/// A downloader page whose file log holds the recorded seeds.
fn log_page(recorded: &Value) -> Log {
    let (dirs, store) = crate::subscriptions::store();
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    let seeds: Vec<(String, Option<String>)> = recorded["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["url"].as_str().unwrap().to_owned(),
                s["hash"].as_str().map(str::to_owned),
            )
        })
        .collect();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let new: Vec<NewFileSeed> = seeds
                .iter()
                .map(|(url, _)| NewFileSeed {
                    seed_type: SeedType::Url,
                    data: url.clone(),
                    data_for_comparison: url.clone(),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                })
                .collect();
            queues::add_file_seeds(conn, queue, &new, false, 0)?;
            let mut stored = queues::file_seeds(conn, queue)?;
            for (seed, (_, hash)) in stored.iter_mut().zip(&seeds) {
                if let Some(hash) = hash {
                    seed.status = SeedStatus::SuccessfulAndNew;
                    seed.meta.set_hash("sha256", hash.clone());
                    queues::update_file_seed(conn, seed)?;
                }
            }
            Ok(())
        })
        .unwrap();
    (dirs, store, copied, ui, bound)
}

fn log_selected(log: &FileLogWindow) -> Vec<u64> {
    let rows = log.get_rows();
    (0..rows.row_count())
        .filter(|&i| rows.row_data(i).unwrap().selected)
        .map(|i| u64::try_from(i).unwrap())
        .collect()
}

/// The first entries of the open menu, as the recording keeps them.
fn menu_firsts(log: &FileLogWindow) -> Vec<String> {
    let panes = log.get_menu_panes();
    if panes.row_count() == 0 {
        return Vec::new();
    }
    let lines = panes.row_data(0).unwrap().lines;
    (0..lines.row_count())
        .map(|l| lines.row_data(l).unwrap().label.to_string())
        .filter(|l| !l.is_empty())
        .take(3)
        .collect()
}

// leaf: interaction-list-keys-activate-copy, interaction-list-pointer
#[test]
fn enter_activates_and_ctrl_c_copies_the_file_log_s_selection_as_the_reference() {
    // Enter (and the keypad's) opens a page of the selected rows' files,
    // as a double-click does; Ctrl+C (and Ctrl+Insert) copies the selected
    // rows' sources; with nothing selected, Enter does nothing and Ctrl+C
    // falls to Qt's copy of the current cell; a double-click activates and
    // a right-click shows the rows menu, on the rows it selected
    let fixture = hydrus_testkit::fixture_json("list_keys.json");
    let recorded = &fixture["log"];
    let _windows = headless::init();
    let (_dirs, store, copied, ui, bound) = log_page(recorded);
    let hex = |id: &hydrus_core::HashId| -> String {
        store
            .read(|c| hydrus_store::master::hash(c, *id))
            .unwrap()
            .unwrap()
            .to_hex()
    };
    // (the downloader page, whose log it is: the pages Enter opens are
    // shown over it)
    let (level, index) = bound.pages.borrow().shown_position();
    for sequence in recorded["sequences"].as_array().unwrap() {
        let name = sequence["name"].as_str().unwrap();
        ui.invoke_tab_chosen(i32::try_from(level).unwrap(), i32::try_from(index).unwrap());
        ui.invoke_open_file_log();
        let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
        let win = log.window();
        win.set_size(slint::LogicalSize::new(980.0, 560.0));
        widgets::settle();
        let urls: Vec<String> = recorded["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["url"].as_str().unwrap().to_owned())
            .collect();
        let row_at = |row: u64| widgets::position(win, &urls[usize::try_from(row).unwrap()], 0);
        for (i, step) in sequence["steps"].as_array().unwrap().iter().enumerate() {
            let context = format!("{name} step {i}");
            let (ctrl, shift) = (
                step["ctrl"].as_bool().unwrap(),
                step["shift"].as_bool().unwrap(),
            );
            copied.borrow_mut().clear();
            headless::set_clipboard_text("before");
            let pages_before = bound.pages.borrow().open_pages().len();
            match step["kind"].as_str().unwrap() {
                "click" => click(win, row_at(step["what"].as_u64().unwrap()), ctrl, shift),
                "double" => widgets::double_click_at(
                    win,
                    row_at(step["what"].as_u64().unwrap()),
                    PointerEventButton::Left,
                    mods(ctrl, shift),
                ),
                "right" => widgets::click_at_with(
                    win,
                    row_at(step["what"].as_u64().unwrap()),
                    PointerEventButton::Right,
                    mods(ctrl, shift),
                ),
                "key" => key(win, key_text(step["what"].as_str().unwrap()), ctrl, shift),
                other => panic!("{other}"),
            }
            assert_eq!(log_selected(&log), numbers(&step["selected"]), "{context}");
            // a page of files opened, or not
            let opened: Vec<String> = step["pubs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["topic"] == "new_page_query")
                .flat_map(|p| p["hashes"].as_array().unwrap().iter())
                .map(|h| h.as_str().unwrap().to_owned())
                .collect();
            let pages_now = bound.pages.borrow().open_pages().len();
            if opened.is_empty() {
                assert_eq!(pages_now, pages_before, "{context}: no page");
            } else {
                assert_eq!(pages_now, pages_before + 1, "{context}: a page");
                let mut ours: Vec<String> = bound
                    .current
                    .borrow()
                    .borrow()
                    .files()
                    .iter()
                    .map(hex)
                    .collect();
                let mut theirs = opened.clone();
                ours.sort();
                theirs.sort();
                assert_eq!(ours, theirs, "{context}");
            }
            // the clipboard: the window's copy, or the list's own
            let theirs_copied: Vec<Clip> = step["pubs"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["topic"] == "clipboard")
                .map(|p| Clip::Text(p["args"][1].as_str().unwrap().to_owned()))
                .collect();
            assert_eq!(*copied.borrow(), theirs_copied, "{context}");
            assert_eq!(
                headless::clipboard_text().as_deref(),
                step["clipboard"].as_str(),
                "{context}: the clipboard"
            );
            // a right-click's menu, on the rows it selected
            let menus = step["menus"].as_array().unwrap();
            if let Some(menu) = menus.first() {
                let theirs: Vec<String> = menu
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e.as_str().unwrap().to_owned())
                    .collect();
                assert_eq!(menu_firsts(&log), theirs, "{context}: the menu");
                log.invoke_menu_dismissed();
            }
        }
        log.invoke_close_window();
        widgets::settle();
    }
}

// --- lists that do not select through `ListSelection` ------------------

#[test]
fn ctrl_a_and_ctrl_arrows_leave_a_one_row_list_alone() {
    // the shortcut set editor keeps the one row clicked (as the window's
    // own index, not a `ListSelection`): the shared list's Ctrl+A and
    // ctrl+arrows are not rows to it, and OK still applies the set
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(hydrus_gui::SearchPage::new(store)));
    let parent = crate::options_shortcut_sets::options(&ui, &bound);
    parent.invoke_shortcut_set_action("add".into());
    let set = hydrus_gui::shortcut_windows::last_set().unwrap();
    crate::options_shortcut_sets::add_key(&set, "q");
    crate::options_shortcut_sets::add_key(&set, "w");
    let win = set.window();
    win.set_size(slint::LogicalSize::new(900.0, 600.0));
    widgets::settle();
    let first = set
        .get_rows()
        .row_data(0)
        .unwrap()
        .cells
        .row_data(0)
        .unwrap();
    click(win, widgets::position(win, &first, 0), false, false);
    assert_eq!(set.get_selected(), 0);
    key(win, "a".into(), true, false);
    key(win, Key::DownArrow.into(), true, false);
    assert_eq!(set.get_selected(), 0, "Ctrl+A and Ctrl+Down are not rows");
    // (Ctrl+Down made the second row current; Space selects it)
    key(win, " ".into(), false, false);
    assert_eq!(set.get_selected(), 1);
    widgets::click(win, "ok");
    assert_eq!(parent.get_shortcut_custom_rows().row_count(), 1);
}
