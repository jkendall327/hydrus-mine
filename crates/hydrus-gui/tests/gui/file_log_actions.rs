//! The file log's context menus acting on a real queue: copying and opening sources, the whole
//! log's retry, delete, reverse and show-files entries. The menus' labels and counts are tested
//! against the reference in hydrus-gui-model; here each choice has its effect.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{
    Clip, FileLogWindow, MainWindow, Pages, bind, headless, set_clipper, set_launcher,
};
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

use crate::subscriptions::store;

fn urls(log: &FileLogWindow) -> Vec<String> {
    let rows = log.get_rows();
    (0..rows.row_count())
        .map(|r| {
            rows.row_data(r)
                .unwrap()
                .cells
                .row_data(1)
                .unwrap()
                .to_string()
        })
        .collect()
}
fn statuses(log: &FileLogWindow) -> Vec<String> {
    let rows = log.get_rows();
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
}
fn lines(log: &FileLogWindow, pane: usize) -> Vec<String> {
    let panes = log.get_menu_panes();
    let lines = panes.row_data(pane).unwrap().lines;
    (0..lines.row_count())
        .map(|l| lines.row_data(l).unwrap().label.to_string())
        .collect()
}
/// Choose the entry whose label starts with `prefix`, hovering any parent entry first.
fn choose(log: &FileLogWindow, pane: usize, prefix: &str) {
    if pane > 0 {
        let parent = lines(log, pane - 1)
            .iter()
            .position(|l| l.starts_with("export all sources"))
            .unwrap();
        log.invoke_menu_line_hovered(
            i32::try_from(pane - 1).unwrap(),
            i32::try_from(parent).unwrap(),
            300.0,
            100.0,
            10.0,
        );
    }
    let line = lines(log, pane)
        .iter()
        .position(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("{prefix} in {:?}", lines(log, pane)));
    let (p, l) = (i32::try_from(pane).unwrap(), i32::try_from(line).unwrap());
    log.invoke_menu_line_hovered(p, l, 300.0, 100.0, 10.0);
    log.invoke_menu_line_clicked(p, l, 300.0, 100.0, 10.0);
}

type Setup = (
    [tempfile::TempDir; 2],
    Rc<RefCell<Vec<Clip>>>,
    Rc<RefCell<Vec<String>>>,
    MainWindow,
    hydrus_gui::Bound,
    FileLogWindow,
);

fn setup(count: usize) -> Setup {
    let (dirs, store) = store();
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let launched: Rc<RefCell<Vec<String>>> = Rc::default();
    set_launcher({
        let launched = launched.clone();
        move |target| launched.borrow_mut().push(target.to_owned())
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
    let first_hash = store
        .read(|conn| {
            let id: hydrus_core::HashId = conn.query_row(
                "SELECT hash_id FROM files ORDER BY hash_id LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            Ok(hydrus_store::master::hash(conn, id)?.unwrap())
        })
        .unwrap();
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            let seeds: Vec<NewFileSeed> = (0..count)
                .map(|n| NewFileSeed {
                    seed_type: SeedType::Url,
                    data: format!("https://site.example/post/{n}"),
                    data_for_comparison: format!("https://site.example/post/{n}"),
                    source_time: None,
                    referral_url: None,
                    meta: FileSeedMeta::default(),
                })
                .collect();
            queues::add_file_seeds(conn, queue, &seeds, false, 0)?;
            let mut seeds = queues::file_seeds(conn, queue)?;
            seeds[0].status = SeedStatus::SuccessfulAndNew;
            seeds[0].meta.set_hash("sha256", first_hash.to_hex());
            seeds[1].status = SeedStatus::Error;
            seeds[1].note = "boom".into();
            seeds[2].status = SeedStatus::Vetoed;
            seeds[2].note = "veto: blacklisted!".into();
            for s in &seeds[..3] {
                queues::update_file_seed(conn, s)?;
            }
            Ok(())
        })
        .unwrap();
    ui.invoke_open_file_log();
    let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    (dirs, copied, launched, ui, bound, log)
}

// leaf: audit-network-file-log-copy
// leaf: audit-network-file-log-open
#[test]
fn selected_rows_copy_sources_and_notes_and_open_many_only_after_confirming() {
    let _windows = headless::init();
    let (_dirs, copied, launched, _ui, _bound, log) = setup(12);
    // Two rows with notes: sources joined by one newline, notes (only non-empty) by a blank line.
    log.invoke_row_clicked(1, false, false);
    log.invoke_row_clicked(2, true, false);
    log.invoke_row_menu(2, 50.0, 50.0);
    choose(&log, 0, "copy urls");
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text(
            "https://site.example/post/1\nhttps://site.example/post/2".into()
        ))
    );
    log.invoke_row_menu(2, 50.0, 50.0);
    choose(&log, 0, "copy notes");
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text("boom\n\nveto: blacklisted!".into()))
    );
    // Opening a few URLs launches each without asking.
    log.invoke_row_menu(2, 50.0, 50.0);
    choose(&log, 0, "open URLs");
    assert_eq!(
        *launched.borrow(),
        ["https://site.example/post/1", "https://site.example/post/2"]
    );
    launched.borrow_mut().clear();
    // More than ten asks first; "no" opens nothing, "yes" opens them all.
    log.invoke_row_clicked(0, false, false);
    log.invoke_row_clicked(11, false, true);
    log.invoke_row_menu(5, 50.0, 50.0);
    choose(&log, 0, "open URLs");
    assert!(log.get_asking());
    assert_eq!(
        log.get_asking_message(),
        "You have many objects selected--are you sure you want to open them all?"
    );
    log.invoke_chosen(1);
    assert!(launched.borrow().is_empty());
    log.invoke_row_menu(5, 50.0, 50.0);
    choose(&log, 0, "open URLs");
    log.invoke_chosen(0);
    assert_eq!(launched.borrow().len(), 12);
    // The whole log's "export all sources" copies every source line.
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 1, "to clipboard");
    let Some(Clip::Text(all)) = copied.borrow().last().cloned() else {
        panic!("no text copied");
    };
    assert_eq!(all.lines().count(), 12);
    assert!(all.starts_with("https://site.example/post/0\n"));
}

// leaf: audit-network-file-log-retry
// leaf: audit-network-file-log-delete
// leaf: audit-network-file-log-reverse
// leaf: audit-network-file-log-show
#[test]
fn whole_log_menu_retries_deletes_reverses_and_shows_files() {
    let _windows = headless::init();
    let (_dirs, _copied, _launched, _ui, bound, log) = setup(6);
    assert_eq!(statuses(&log)[..3], ["successful", "error", "ignored"]);
    // Retry the ignored: only they go back to unknown.
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "retry 1 ignored");
    assert_eq!(statuses(&log)[..3], ["successful", "error", ""]);
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "retry 1 failures");
    assert_eq!(statuses(&log)[..3], ["successful", "", ""]);
    // Reverse the import order.
    let before = urls(&log);
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "reverse import order");
    let mut reversed = before.clone();
    reversed.reverse();
    assert_eq!(urls(&log), reversed);
    // Show the successful files in a new page.
    let pages = bound.pages.borrow().open_pages().len();
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "show all files in a new page");
    assert_eq!(bound.pages.borrow().open_pages().len(), pages + 1);
    assert_eq!(bound.current.borrow().borrow().files().len(), 1);
    // Delete by status removes just that status; delete everything empties the log.
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "delete 1 'successful'");
    assert_eq!(urls(&log).len(), 5);
    assert!(!statuses(&log).contains(&"successful".to_owned()));
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "delete everything (5 items)");
    assert!(urls(&log).is_empty());
}
