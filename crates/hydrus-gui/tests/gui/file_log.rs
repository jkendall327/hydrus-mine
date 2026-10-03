//! A URL downloader page's file log window, opened from its "imports"
//! box: its rows and status, its right-click and whole log menus (their
//! entries are tested against the reference's in hydrus-gui-model's
//! tests), and their actions on the store: skipping, retrying, deleting
//! (asking first), copying sources, and showing the files in a new page.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{Clip, FileLogWindow, MainWindow, Pages, bind, headless, set_clipper};
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, SeedStatus, SeedType};

use crate::subscriptions::store;

fn cells(log: &FileLogWindow) -> Vec<Vec<String>> {
    let rows = log.get_rows();
    (0..rows.row_count())
        .map(|r| {
            let row = rows.row_data(r).unwrap();
            (0..row.cells.row_count())
                .map(|c| row.cells.row_data(c).unwrap().to_string())
                .collect()
        })
        .collect()
}

/// The open menus' lines, by pane.
fn menu(log: &FileLogWindow) -> Vec<Vec<String>> {
    let panes = log.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|l| lines.row_data(l).unwrap().label.to_string())
                .collect()
        })
        .collect()
}

/// Choose the entry labelled `label` in pane `pane`.
fn choose(log: &FileLogWindow, pane: usize, label: &str) {
    let line = menu(log)[pane]
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", menu(log)));
    let (p, l) = (i32::try_from(pane).unwrap(), i32::try_from(line).unwrap());
    log.invoke_menu_line_hovered(p, l, 300.0, 100.0, 10.0);
    log.invoke_menu_line_clicked(p, l, 300.0, 100.0, 10.0);
}

#[test]
fn the_file_log_lists_a_queues_files_and_acts_on_them() {
    let (_dirs, store) = store();
    let first: (HashId, hydrus_core::Sha256) = store
        .read(|conn| {
            let id: HashId = conn.query_row(
                "SELECT hash_id FROM files ORDER BY hash_id LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            Ok((id, hydrus_store::master::hash(conn, id)?.unwrap()))
        })
        .unwrap();
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // download, then urls
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
    let seed = |n: usize| NewFileSeed {
        seed_type: SeedType::Url,
        data: format!("https://site.example/post/{n}"),
        data_for_comparison: format!("https://site.example/post/{n}"),
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    };
    let hash = first.1;
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            queues::add_file_seeds(conn, queue, &[seed(1), seed(2), seed(3)], false, 0)?;
            let mut seeds = queues::file_seeds(conn, queue)?;
            seeds[0].status = SeedStatus::SuccessfulAndNew;
            seeds[0].meta.set_hash("sha256", hash.to_hex());
            seeds[1].status = SeedStatus::Error;
            seeds[1].note = "404\nmore".into();
            for s in &seeds[..2] {
                queues::update_file_seed(conn, s)?;
            }
            Ok(())
        })
        .unwrap();

    ui.invoke_open_file_log();
    let log = bound
        .file_log
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    assert_eq!(log.get_window_title(), "file log");
    assert_eq!(log.get_status(), "1 successful, 1 failed");
    let shown = cells(&log);
    assert_eq!(shown.len(), 3);
    assert_eq!(
        (
            shown[0][0].as_str(),
            shown[0][1].as_str(),
            shown[0][2].as_str()
        ),
        ("0", "https://site.example/post/1", "successful")
    );
    assert_eq!(
        (shown[1][2].as_str(), shown[1][6].as_str()),
        ("error", "404")
    );
    assert_eq!(shown[2][5], "unknown");

    // the whole log's menu: retry the failure
    log.invoke_log_menu(10.0, 10.0);
    assert_eq!(menu(&log)[0][0], "retry 1 failures");
    choose(&log, 0, "retry 1 failures");
    assert!(log.get_menu_panes().row_count() == 0, "it closes");
    assert_eq!(log.get_status(), "1 successful");

    // a right click on the third row: copy its URL, then skip it
    log.invoke_row_menu(2, 50.0, 50.0);
    assert!(log.get_rows().row_data(2).unwrap().selected);
    assert!(menu(&log)[0].contains(&"copy urls".to_owned()));
    choose(&log, 0, "copy urls");
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text("https://site.example/post/3".into()))
    );
    log.invoke_row_menu(2, 50.0, 50.0);
    choose(&log, 0, "skip");
    assert_eq!(cells(&log)[2][2], "skipped");

    // deleting the second asks first
    log.invoke_row_clicked(1, false, false);
    log.invoke_delete_pressed();
    assert_eq!(
        log.get_asking_message(),
        "Are you sure you want to delete the 1 selected entries?"
    );
    log.invoke_chosen(0);
    assert_eq!(cells(&log).len(), 2);

    // the first's file, in a new page
    let pages_before = bound.pages.borrow().open_pages().len();
    log.invoke_row_menu(0, 50.0, 50.0);
    choose(&log, 0, "open selected files in a new page");
    assert_eq!(bound.pages.borrow().open_pages().len(), pages_before + 1);
    let shown_files = bound.current.borrow().borrow().files().clone();
    assert_eq!(shown_files, [first.0]);

    // (a screenshot, kept in the target directory, with a menu open)
    log.invoke_row_menu(0, 120.0, 90.0);
    let pixels = headless::render(&windows.get(1).unwrap(), 980, 560);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("file_log.png"), &pixels, 980, 560).unwrap();
    log.invoke_menu_dismissed();

    log.invoke_close_window();
    assert!(bound.file_log.borrow().is_none());
}
