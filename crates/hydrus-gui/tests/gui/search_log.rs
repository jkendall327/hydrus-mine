//! A downloader page's search log window, opened from its "search" box:
//! its rows and status, and its menus' actions on the store: copying a
//! page's URL, skipping it, trying it again (after it), restarting the
//! failed search, and deleting the failed entries (asking first). The
//! menus' entries are tested against the reference's in hydrus-gui-model.

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Clip, FileLogWindow, MainWindow, Pages, bind, headless, set_clipper};
use hydrus_store::queues::{self, GallerySeedMeta, NewGallerySeed, SeedStatus};

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
fn the_search_log_lists_a_queues_pages_and_acts_on_them() {
    let (_dirs, store) = store();
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
    let page = |n: usize| NewGallerySeed {
        url: format!("https://site.example/gallery?page={n}"),
        can_generate_more_pages: true,
        referral_url: None,
        meta: GallerySeedMeta::default(),
    };
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            queues::add_gallery_seeds(conn, queue, &[page(1), page(2), page(3)], None, 0)?;
            let mut seeds = queues::gallery_seeds(conn, queue)?;
            seeds[0].status = SeedStatus::SuccessfulAndNew;
            seeds[0].note = "found 3 new urls".into();
            seeds[2].status = SeedStatus::Error;
            seeds[2].note = "404".into();
            for s in [&seeds[0], &seeds[2]] {
                queues::update_gallery_seed(conn, s)?;
            }
            Ok(())
        })
        .unwrap();

    ui.invoke_open_search_log();
    let log = bound
        .file_log
        .borrow()
        .as_ref()
        .expect("it opens")
        .clone_strong();
    assert_eq!(log.get_window_title(), "search log");
    assert_eq!(log.get_status(), "1 successful, 1 failed, 1 pending");
    assert_eq!(log.get_columns().row_count(), 6);
    let shown = cells(&log);
    assert_eq!(
        shown[0][..3],
        ["0", "https://site.example/gallery?page=1", "successful"]
    );
    assert_eq!(shown[0][5], "found 3 new urls");
    assert_eq!(shown[1][2], "");
    assert_eq!(shown[2][2], "error");

    // the second page: copy its URL, then skip it
    log.invoke_row_menu(1, 50.0, 50.0);
    choose(&log, 0, "copy url");
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text("https://site.example/gallery?page=2".into()))
    );
    log.invoke_row_menu(1, 50.0, 50.0);
    choose(&log, 0, "skip");
    assert_eq!(cells(&log)[1][2], "skipped");

    // the first, read again: a new page after it
    log.invoke_row_clicked(0, false, false);
    log.invoke_row_menu(0, 50.0, 50.0);
    choose(&log, 0, "try again (just this one page)");
    let shown = cells(&log);
    assert_eq!(shown.len(), 4);
    assert_eq!(shown[1][1], "https://site.example/gallery?page=1");
    assert_eq!(shown[1][2], "");

    // a failed last page: the search restarted from it
    log.invoke_row_clicked(3, false, false);
    log.invoke_log_menu(10.0, 10.0);
    choose(&log, 0, "restart and resume failed search");
    let seeds = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
    assert_eq!(seeds.len(), 5);
    assert_eq!(seeds[4].url, "https://site.example/gallery?page=3");
    assert!(seeds[4].meta.force_next_page_url_generation);

    // deleting the failed entries asks first
    log.invoke_log_menu(10.0, 10.0);
    choose(
        &log,
        0,
        "delete 1 'failed' gallery log entries from the log",
    );
    assert!(log.get_asking());
    assert!(
        log.get_asking_message()
            .starts_with("Are you sure you want to delete all the error search log entries?")
    );
    log.invoke_chosen(0);
    assert_eq!(cells(&log).len(), 4);
    assert!(!log.get_asking());

    // (a screenshot, kept in the target directory, with a menu open)
    log.invoke_row_menu(0, 120.0, 90.0);
    let pixels = headless::render(&windows.get(1).unwrap(), 980, 560);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("search_log.png"), &pixels, 980, 560).unwrap();
    log.invoke_menu_dismissed();

    log.invoke_close_window();
    assert!(bound.file_log.borrow().is_none());
}

fn exchange(log: &FileLogWindow, group: &str, action: &str) {
    log.invoke_log_menu(10.0, 10.0);
    choose(log, 0, group);
    choose(log, 1, action);
}

#[test]
fn exchange_confirmations_png_objects_and_owner_cancel_reach_the_persisted_queue() {
    use hydrus_gui::{png_export_window, search_log_import_window};
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    store
        .write(move |ctx| {
            queues::add_gallery_seeds(
                ctx.conn(),
                queue,
                &[NewGallerySeed {
                    url: "https://gallery-exchange.example/a".into(),
                    can_generate_more_pages: true,
                    referral_url: None,
                    meta: GallerySeedMeta::default(),
                }],
                None,
                123,
            )?;
            let mut s = queues::gallery_seeds(ctx.conn(), queue)?.remove(0);
            s.status = SeedStatus::Error;
            s.note = "old failure".into();
            queues::update_gallery_seed(ctx.conn(), &s)?;
            queues::take_nudges(ctx.conn()).map(|_| ())
        })
        .unwrap();
    let before = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
    ui.invoke_open_search_log();
    let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    let pasted = Rc::new(RefCell::new(Ok(Some(
        "https://gallery-exchange.example/a\nhttps://gallery-exchange.example/b".to_owned(),
    ))));
    hydrus_gui::set_clipboard_reader({
        let pasted = pasted.clone();
        move || pasted.borrow().clone()
    });
    exchange(&log, "ADVANCED: import new urls", "from clipboard");
    let question = search_log_import_window::last().unwrap();
    let fixture = hydrus_testkit::fixture_json("search_log_exchange.json");
    assert_eq!(
        question.get_message().as_str(),
        fixture["questions"][0]["text"].as_str().unwrap()
    );
    assert!(log.get_busy());
    assert_eq!(
        store.read(|c| queues::gallery_seeds(c, queue)).unwrap(),
        before
    );
    question.invoke_chosen(1); // Reference duplicate No imports nothing.
    assert_eq!(
        store.read(|c| queues::gallery_seeds(c, queue)).unwrap(),
        before
    );
    exchange(&log, "ADVANCED: import new urls", "from clipboard");
    let question = search_log_import_window::last().unwrap();
    question.invoke_chosen(0);
    assert_eq!(
        question.get_message(),
        hydrus_gui::search_log::CONTINUE_QUESTION
    );
    assert_eq!(
        store.read(|c| queues::gallery_seeds(c, queue)).unwrap(),
        before
    );
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 650, 260);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("search_log_import.png"),
        &pixels,
        650,
        260,
    )
    .unwrap();
    question.invoke_chosen(0);
    let saved = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0], before[0]);
    assert!(!saved[1].can_generate_more_pages);
    assert!(!saved[1].meta.run_token.is_empty());
    assert!(store.read(queues::any_nudged).unwrap());
    assert_eq!(cells(&log).len(), 2);
    *pasted.borrow_mut() = Ok(Some("https://gallery-exchange.example/c".into()));
    exchange(&log, "ADVANCED: import new urls", "from clipboard");
    let cancelled = search_log_import_window::last().unwrap();
    cancelled.invoke_cancelled();
    cancelled.invoke_chosen(1);
    assert_eq!(
        store.read(|c| queues::gallery_seeds(c, queue)).unwrap(),
        saved
    );
    exchange(&log, "ADVANCED: import new urls", "from clipboard");
    search_log_import_window::last().unwrap().invoke_chosen(1);
    assert!(store.read(|c| queues::gallery_seeds(c, queue)).unwrap()[2].can_generate_more_pages);
    let copied = Rc::new(RefCell::new(Vec::new()));
    set_clipper({
        let copied = copied.clone();
        move |c| copied.borrow_mut().push(c.clone())
    });
    log.invoke_row_clicked(0, false, false);
    exchange(
        &log,
        "advanced",
        "export selected page objects to clipboard",
    );
    let persisted = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
    assert_eq!(
        copied.borrow().last(),
        Some(&Clip::Text(
            hydrus_gui::search_log::export_objects(&[&persisted[0]]).unwrap()
        ))
    );
    let picked = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_picker({
        let picked = picked.clone();
        move |_, title| {
            assert_eq!(title, "select the png with the urls");
            picked.borrow().clone()
        }
    });
    exchange(&log, "ADVANCED: import new urls", "from png");
    assert_eq!(
        store.read(|c| queues::gallery_seeds(c, queue)).unwrap(),
        persisted
    );
    *picked.borrow_mut() = vec![hydrus_testkit::fixture_path("search_log_urls.png")];
    exchange(&log, "ADVANCED: import new urls", "from png");
    search_log_import_window::last().unwrap().invoke_chosen(1);
    let imported = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
    assert_eq!(imported.len(), 4);
    assert!(imported[3].can_generate_more_pages);
    assert_eq!(imported[3].url, fixture["png"]["payload"].as_str().unwrap());
    let tmp = tempfile::tempdir().unwrap();
    let bad = tmp.path().join("bad.png");
    std::fs::write(&bad, b"invalid").unwrap();
    *picked.borrow_mut() = vec![bad];
    exchange(&log, "ADVANCED: import new urls", "from png");
    let error = search_log_import_window::last().unwrap();
    assert_eq!(error.get_window_title(), "Could not import!");
    error.invoke_chosen(0);
    *pasted.borrow_mut() = Err("synthetic clipboard access failure".into());
    exchange(&log, "ADVANCED: import new urls", "from clipboard");
    let error = search_log_import_window::last().unwrap();
    assert_eq!(
        error.get_window_title(),
        "Problem importing from clipboard!"
    );
    error.invoke_cancelled();
    *pasted.borrow_mut() = Ok(Some("not a valid URL".into()));
    exchange(&log, "ADVANCED: import new urls", "from clipboard");
    search_log_import_window::last().unwrap().invoke_chosen(0);
    let all = store.read(|c| queues::gallery_seeds(c, queue)).unwrap();
    assert_eq!(all[4].url, "not a valid URL");
    exchange(&log, "export all urls", "to png");
    let export = png_export_window::last().unwrap();
    let output = tmp.path().join("gallery");
    export.set_path(output.to_string_lossy().as_ref().into());
    export.set_png_title("Synthetic gallery URLs".into());
    export.set_png_width(256);
    export.invoke_action("export".into());
    let text = hydrus_downloader_exchange::text_png::decode(
        &std::fs::read(output.with_extension("png")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        text,
        all.iter()
            .map(|s| s.url.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    log.invoke_close_window();
    export.invoke_action("export".into());
    assert!(bound.file_log.borrow().is_none());
    ui.invoke_open_search_log();
    let reopened = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(cells(&reopened).len(), 5);
    *pasted.borrow_mut() = Ok(Some("https://gallery-exchange.example/closed".into()));
    exchange(&reopened, "ADVANCED: import new urls", "from clipboard");
    let stale = search_log_import_window::last().unwrap();
    reopened.invoke_close_window();
    stale.invoke_chosen(1);
    assert_eq!(
        store.read(|c| queues::gallery_seeds(c, queue)).unwrap(),
        all
    );
}
