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

fn clipboard_import(log: &FileLogWindow) {
    log.invoke_log_menu(10.0, 10.0);
    choose(log, 0, "ADVANCED: import new sources");
    choose(log, 1, "from clipboard");
}

#[test]
fn clipboard_import_persists_deduplicates_and_handles_empty_or_missing_text() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    ui.invoke_open_file_log();
    let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    let text = Rc::new(RefCell::new(Ok(Some("\u{feff} https://clipboard.example/a b#frag\r\nhttps://clipboard.example/a%20b\nhttps://clipboard.example/日".to_owned()))));
    hydrus_gui::set_clipboard_reader({
        let text = text.clone();
        move || text.borrow().clone()
    });
    clipboard_import(&log);
    clipboard_import(&log);
    let persisted = store.read(|c| queues::file_seeds(c, queue)).unwrap();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].data, "https://clipboard.example/a%20b");
    assert_eq!(persisted[1].data, "https://clipboard.example/%E6%97%A5");
    assert!(
        persisted
            .iter()
            .all(|s| s.seed_type == SeedType::Url && s.status == SeedStatus::Unknown)
    );
    *text.borrow_mut() = Ok(Some(" \n\t".into()));
    clipboard_import(&log);
    assert!(log.get_asking());
    assert!(
        log.get_asking_message()
            .contains("Lines of URLs or file paths")
    );
    log.invoke_cancelled();
    assert!(!log.get_asking());
    assert_eq!(
        store.read(|c| queues::file_seeds(c, queue)).unwrap(),
        persisted
    );
    *text.borrow_mut() = Ok(None);
    clipboard_import(&log);
    assert_eq!(log.get_asking_title(), "Problem pasting!");
    log.invoke_chosen(0);
    *text.borrow_mut() = Err("synthetic clipboard access failure".into());
    clipboard_import(&log);
    assert_eq!(
        log.get_asking_message(),
        "synthetic clipboard access failure"
    );
    log.invoke_chosen(0);
    log.invoke_close_window();
    *text.borrow_mut() = Ok(Some("https://clipboard.example/stale".into()));
    log.invoke_log_menu(10.0, 10.0);
    log.invoke_menu_line_clicked(0, 0, 300.0, 100.0, 10.0);
    log.invoke_delete_pressed();
    log.invoke_chosen(0);
    assert_eq!(
        store.read(|c| queues::file_seeds(c, queue)).unwrap(),
        persisted
    );
    ui.invoke_open_file_log();
    let reopened = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(cells(&reopened).len(), 2);
    *text.borrow_mut() = Ok(Some(
        "/synthetic/a.jpg\n/synthetic/a.jpg\n/synthetic/b.jpg".into(),
    ));
    clipboard_import(&reopened);
    let seeds = store.read(|c| queues::file_seeds(c, queue)).unwrap();
    assert_eq!(seeds.len(), 4);
    assert!(seeds[2..].iter().all(|s| s.seed_type == SeedType::Path));
    reopened.invoke_close_window();
}

#[test]
fn selected_url_search_opens_a_local_or_search_and_reaches_matching_files() {
    use hydrus_core::pages::PageContent;
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.current.borrow().borrow_mut().refresh();
    let files = bound.current.borrow().borrow().files().clone();
    assert!(files.len() >= 2);
    let first = files[0];
    let second = files[1];
    let urls = vec![
        "https://clipboard.example/a".to_owned(),
        "https://clipboard.example/b".to_owned(),
    ];
    store
        .write_content({
            let urls = urls.clone();
            move |writer| {
                writer.add_urls(&[first], &urls[..1])?;
                writer.add_urls(&[second], &urls[1..])
            }
        })
        .unwrap();
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    hydrus_gui::set_clipboard_reader({
        let urls = urls.clone();
        move || Ok(Some(urls.join("\n")))
    });
    ui.invoke_open_file_log();
    let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    clipboard_import(&log);
    log.invoke_row_clicked(0, false, false);
    log.invoke_row_clicked(1, true, false);
    let pages_before = bound.pages.borrow().open_pages().len();
    log.invoke_row_menu(0, 20.0, 20.0);
    choose(&log, 0, "search for URLs");
    assert_eq!(bound.pages.borrow().open_pages().len(), pages_before + 1);
    {
        let pages = bound.pages.borrow();
        assert_eq!(pages.shown().name, "url search");
        let PageContent::Search { search, .. } = &pages.shown().content else {
            panic!("a search page");
        };
        assert_eq!(search.predicates, hydrus_gui::file_log::url_search(&urls));
        assert_eq!(
            search.location,
            hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec()
            ))
        );
    }
    let matches = bound.current.borrow().borrow().files().clone();
    assert!(
        matches.contains(&first) && matches.contains(&second),
        "both OR branches reach local files: {matches:?}"
    );
    bound.pages.borrow_mut().save(123).unwrap();
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(reopened.shown().name, "url search");
    assert_eq!(
        store.read(|c| queues::file_seeds(c, queue)).unwrap().len(),
        2
    );
    log.invoke_close_window();
}

fn png_import(log: &FileLogWindow) {
    log.invoke_log_menu(10.0, 10.0);
    choose(log, 0, "ADVANCED: import new sources");
    choose(log, 1, "from png");
}
fn png_export(log: &FileLogWindow) {
    log.invoke_log_menu(10.0, 10.0);
    choose(log, 0, "export all sources");
    choose(log, 1, "to png");
}

#[test]
fn source_png_dialogs_cancel_validate_import_export_and_close_with_the_log() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    ui.invoke_open_file_log();
    let log = bound.file_log.borrow().as_ref().unwrap().clone_strong();
    let picked = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_picker({
        let picked = picked.clone();
        move |_, title| {
            assert_eq!(title, "select the png with the sources");
            picked.borrow().clone()
        }
    });
    png_import(&log);
    assert_eq!(cells(&log).len(), 0, "cancelled picker changes nothing");
    *picked.borrow_mut() = vec![hydrus_testkit::fixture_path("file_log_sources.png")];
    png_import(&log);
    png_import(&log);
    assert_eq!(cells(&log).len(), 2, "imported Qt carrier deduplicates");
    let persisted = store.read(|c| queues::file_seeds(c, queue)).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let malformed = temp.path().join("bad.png");
    std::fs::write(&malformed, b"not a PNG").unwrap();
    *picked.borrow_mut() = vec![malformed];
    png_import(&log);
    assert_eq!(log.get_asking_title(), "Could not import!");
    log.invoke_cancelled();
    assert_eq!(
        store.read(|c| queues::file_seeds(c, queue)).unwrap(),
        persisted
    );
    png_export(&log);
    let export = hydrus_gui::png_export_window::last().unwrap();
    assert_eq!(export.get_window_title(), "export to png");
    assert_eq!(export.get_png_width(), 512);
    assert!(!export.get_can_export());
    assert!(log.get_busy());
    let output = temp.path().join("shared_sources");
    export.set_path(output.to_string_lossy().as_ref().into());
    export.set_png_title("".into());
    export.invoke_action("update".into());
    assert!(!export.get_can_export());
    export.invoke_action("export".into());
    assert!(!output.with_extension("png").exists());
    assert!(export.get_error().contains("set a title"));
    export.set_png_title("Synthetic source list".into());
    export.set_description("Shared source lines".into());
    export.set_png_width(256);
    export.invoke_action("update".into());
    assert!(export.get_can_export());
    export.invoke_action("export".into());
    assert!(export.get_done());
    let expected = persisted
        .iter()
        .map(|s| s.data.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = std::fs::read(output.with_extension("png")).unwrap();
    assert_eq!(
        hydrus_downloader_exchange::text_png::decode(&bytes).unwrap(),
        expected
    );
    let directory = store
        .read(hydrus_store::settings::get::<hydrus_gui_model::png_export::Directory>)
        .unwrap();
    assert_eq!(directory.0.as_deref(), temp.path().to_str());
    export.invoke_action("close".into());
    assert!(!log.get_busy());
    png_export(&log);
    let next = hydrus_gui::png_export_window::last().unwrap();
    assert!(next.get_path().starts_with(temp.path().to_str().unwrap()));
    let stale = temp.path().join("stale.png");
    next.set_path(stale.to_string_lossy().as_ref().into());
    log.invoke_close_window();
    assert!(!next.window().is_visible());
    next.invoke_action("export".into());
    assert!(!stale.exists());
}
