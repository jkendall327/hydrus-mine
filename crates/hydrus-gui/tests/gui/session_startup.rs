//! Startup selector and real boot consumption, replayed against the reference.
use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless, page_chooser::NewPage};
use hydrus_search::FileSearchContext;
use hydrus_store::{
    Store, queues, sessions,
    settings::{self, GuiSessionSettings},
};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn source(rows: &serde_json::Value) -> Vec<Page> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| Page {
            key: PageKey::random(),
            name: row["name"].as_str().unwrap().into(),
            content: if row["children"].is_array() {
                PageContent::Pages(source(&row["children"]))
            } else {
                PageContent::Search {
                    search: FileSearchContext::default(),
                    synchronised: false,
                    sort: None,
                    lock: None,
                    collect: None,
                }
            },
        })
        .collect()
}
fn tree(pages: &[Page]) -> serde_json::Value {
    serde_json::json!(
        pages
            .iter()
            .map(|page| match &page.content {
                PageContent::Pages(children) =>
                    serde_json::json!({"name":page.name,"children":tree(children)}),
                _ => serde_json::json!({"name":page.name}),
            })
            .collect::<Vec<_>>()
    )
}

fn startup_option(ui: &MainWindow, bound: &hydrus_gui::Bound) -> (OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 200.0, 100.0, 10.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let page = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui sessions")
        .unwrap();
    let page = i32::try_from(page).unwrap();
    window.set_page(page);
    window.invoke_page_chosen(page);
    let rows = window.get_rows();
    let row = (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().label == "Default session on startup: ")
        .unwrap();
    (window, i32::try_from(row).unwrap())
}

// leaf: audit-options-gui-sessions-sessions-default-session-on-startup
#[test]
fn startup_replays_real_blank_missing_last_and_named_trees_and_initial_schedule() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("session_startup.json");
    let named = Session {
        name: "startup work".into(),
        pages: source(&fixture["named_tree"]),
    };
    store
        .write(move |ctx| hydrus_store::session_backups::save(ctx.conn(), &named, 1_000))
        .unwrap();
    for step in fixture["steps"].as_array().unwrap() {
        let last = Session {
            name: sessions::LAST_SESSION.into(),
            pages: source(&fixture["last_tree"]),
        };
        let config = GuiSessionSettings {
            startup: step["startup"].as_str().map(str::to_owned),
            ..GuiSessionSettings::default()
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &last, 1)?;
                settings::set(ctx.conn(), &config)
            })
            .unwrap();
        let pages = Pages::open_startup(store.clone()).unwrap();
        assert_eq!(tree(&pages.session().pages), step["tree"]);
        assert_eq!(pages.shown().name, step["shown"].as_str().unwrap());
        let ui = MainWindow::new().unwrap();
        let start = hydrus_core::TimestampMs::now().0;
        let bound = bind(&ui, pages);
        let end = hydrus_core::TimestampMs::now().0;
        let delay = step["calls"][1]["delay"].as_i64().unwrap() * 1_000;
        let next = bound.session_autosave.next().unwrap();
        assert!((start + delay..=end + delay).contains(&next));
        (bound.sync)();
        let reopened = Pages::open(store.clone()).unwrap();
        assert_eq!(tree(&reopened.session().pages), step["tree"]);
    }
}

// leaf: audit-options-gui-sessions-sessions-default-session-on-startup
#[test]
fn startup_dropdown_cancels_invalid_choice_and_applies_frozen_named_session_selection() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("session_startup.json");
    let named = Session {
        name: "startup work".into(),
        pages: source(&fixture["named_tree"]),
    };
    store
        .write(move |ctx| hydrus_store::session_backups::save(ctx.conn(), &named, 1_000))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let before: GuiSessionSettings = store.read(settings::get).unwrap();
    let (window, row) = startup_option(&ui, &bound);
    let data = window
        .get_rows()
        .row_data(usize::try_from(row).unwrap())
        .unwrap();
    let labels: Vec<_> = (0..data.items.row_count())
        .map(|i| data.items.row_data(i).unwrap().to_string())
        .collect();
    assert_eq!(labels[0], "just a blank page");
    assert!(labels.iter().any(|name| name == sessions::LAST_SESSION));
    window.invoke_choice_chosen(row, 0);
    window.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<GuiSessionSettings>).unwrap(),
        before
    );
    let (window, row) = startup_option(&ui, &bound);
    window.invoke_choice_chosen(row, 999);
    window.invoke_apply();
    assert_eq!(
        store.read(settings::get::<GuiSessionSettings>).unwrap(),
        before
    );
    let (window, row) = startup_option(&ui, &bound);
    let selected = i32::try_from(
        labels
            .iter()
            .position(|label| label == "startup work")
            .unwrap(),
    )
    .unwrap();
    // Dropdown choices stay frozen through unrelated named-session insertion.
    let unrelated = Session {
        name: "aaa after opening".into(),
        pages: vec![],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &unrelated, 2))
        .unwrap();
    window.invoke_choice_chosen(row, selected);
    window.invoke_apply();
    assert_eq!(
        store
            .read(settings::get::<GuiSessionSettings>)
            .unwrap()
            .startup
            .as_deref(),
        Some("startup work")
    );
    let reopened = Pages::open_startup(store.clone()).unwrap();
    assert_eq!(tree(&reopened.session().pages), fixture["named_tree"]);
}

// leaf: audit-options-gui-sessions-sessions-default-session-on-startup
#[test]
fn named_startup_restores_ordered_media_selection_and_independent_importer_snapshot() {
    let (_dirs, store) = store();
    let pages = Pages::open(store.clone()).unwrap();
    let original = pages.shown().key;
    store
        .write(move |ctx| {
            sessions::set_page_files(
                ctx.conn(),
                &original,
                &[hydrus_core::HashId(2), hydrus_core::HashId(1)],
            )?;
            sessions::set_page_selected(ctx.conn(), &original, &[hydrus_core::HashId(1)])
        })
        .unwrap();
    // Reopen the source to consume the persisted ordered files.
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.new_page(&NewPage::Urls).unwrap();
    let source_key = pages.shown().key;
    let source_queue = match &pages.shown().content {
        PageContent::Downloader {
            kind: DownloaderKind::Urls,
            queues,
            ..
        } => queues[0],
        _ => unreachable!(),
    };
    store
        .write(move |ctx| {
            queues::set_paused(ctx.conn(), source_queue, Some(true), Some(true))?;
            queues::add_file_seeds(
                ctx.conn(),
                source_queue,
                &[queues::NewFileSeed {
                    seed_type: queues::SeedType::Url,
                    data: "https://files.example/startup.jpg".into(),
                    data_for_comparison: "https://files.example/startup.jpg".into(),
                    source_time: None,
                    referral_url: None,
                    meta: queues::FileSeedMeta::default(),
                }],
                false,
                1,
            )?;
            Ok(())
        })
        .unwrap();
    pages.save_session_at_ms("startup work", 1_000).unwrap();
    let config = GuiSessionSettings {
        startup: Some("startup work".into()),
        ..GuiSessionSettings::default()
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &config))
        .unwrap();
    let mut boot = Pages::open_startup(store.clone()).unwrap();
    assert!(
        store
            .read(|conn| queues::queue(conn, source_queue))
            .unwrap()
            .is_none()
    );
    let first = boot.shown().key;
    assert_ne!(first, original);
    assert_eq!(
        store
            .read(|conn| sessions::page_files(conn, &first))
            .unwrap(),
        [hydrus_core::HashId(2), hydrus_core::HashId(1)]
    );
    assert_eq!(
        store
            .read(|conn| sessions::page_selected(conn, &first))
            .unwrap(),
        [hydrus_core::HashId(1)]
    );
    let importer = boot
        .session()
        .all_pages()
        .into_iter()
        .find(|page| {
            matches!(
                page.content,
                PageContent::Downloader {
                    kind: DownloaderKind::Urls,
                    ..
                }
            )
        })
        .unwrap();
    assert_ne!(importer.key, source_key);
    let PageContent::Downloader { queues, .. } = &importer.content else {
        unreachable!()
    };
    let fresh = queues[0];
    assert_ne!(fresh, source_queue);
    let restored = store
        .read(|conn| queues::queue(conn, fresh))
        .unwrap()
        .unwrap();
    assert!(restored.files_paused && restored.gallery_paused);
    assert_eq!(restored.page_key, Some(importer.key.0.to_vec()));
    assert_eq!(
        store.read(|conn| queues::file_seeds(conn, fresh)).unwrap()[0].data,
        "https://files.example/startup.jpg"
    );
    boot.sync(2).unwrap();
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(reopened.session().pages, boot.session().pages);
}

// leaf: audit-options-gui-sessions-sessions-default-session-on-startup
#[test]
fn bad_shutdown_recovery_replays_real_yes_no_close_timeout_and_blank_bypasses() {
    use std::{cell::RefCell, rc::Rc};
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("session_startup.json");
    let named = Session {
        name: "startup work".into(),
        pages: source(&fixture["named_tree"]),
    };
    store
        .write(move |ctx| hydrus_store::session_backups::save(ctx.conn(), &named, 1_000))
        .unwrap();
    for step in fixture["recovery_steps"].as_array().unwrap() {
        let config = GuiSessionSettings {
            startup: step["startup"].as_str().map(str::to_owned),
            ..GuiSessionSettings::default()
        };
        let last = Session {
            name: sessions::LAST_SESSION.into(),
            pages: source(&fixture["last_tree"]),
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &last, 1)?;
                settings::set(ctx.conn(), &config)
            })
            .unwrap();
        let results = Rc::new(RefCell::new(Vec::new()));
        let ready = Rc::new({
            let results = results.clone();
            move |pages: hydrus_store::Result<Pages>| results.borrow_mut().push(pages.unwrap())
        });
        let start = hydrus_core::TimestampMs::now().0;
        let recovery = hydrus_gui::session_startup::prepare(store.clone(), true, ready).unwrap();
        if let Some(question) = step["questions"].as_array().unwrap().first() {
            assert!(results.borrow().is_empty());
            let recovery = recovery.unwrap();
            let window = recovery.window();
            assert_eq!(
                window.get_window_title(),
                question["title"].as_str().unwrap()
            );
            assert_eq!(window.get_message(), question["message"].as_str().unwrap());
            assert_eq!(
                window.get_yes_label(),
                question["yes_label"].as_str().unwrap()
            );
            assert_eq!(
                window.get_no_label(),
                question["no_label"].as_str().unwrap()
            );
            let delay = i64::try_from(question["auto_yes_time"].as_u64().unwrap()).unwrap() * 1_000;
            assert!(
                (start + delay..=hydrus_core::TimestampMs::now().0 + delay)
                    .contains(&recovery.deadline())
            );
            match step["answer"].as_str().unwrap() {
                "yes" => window.invoke_answered(true),
                "no" => window.invoke_answered(false),
                "close" => window
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested),
                "timeout" => {
                    recovery.poll_at(recovery.deadline() - 1);
                    assert!(results.borrow().is_empty());
                    recovery.poll_at(recovery.deadline());
                }
                answer => panic!("unexpected recovery answer {answer}"),
            }
            assert_eq!(results.borrow().len(), 1);
            // The expired timer or a stale answer cannot resolve startup again.
            recovery.poll_at(recovery.deadline() + 1);
            window.invoke_answered(false);
            assert_eq!(results.borrow().len(), 1);
        } else {
            assert!(recovery.is_none());
        }
        let mut pages = results.borrow_mut().pop().unwrap();
        assert_eq!(tree(&pages.session().pages), step["tree"]);
        assert_eq!(pages.shown().name, step["shown"].as_str().unwrap());
        assert_eq!(
            store
                .read(settings::get::<GuiSessionSettings>)
                .unwrap()
                .startup
                .as_deref(),
            step["startup"].as_str()
        );
        pages.sync(2).unwrap();
        let reopened = Pages::open(store.clone()).unwrap();
        assert_eq!(tree(&reopened.session().pages), step["tree"]);
    }
}

// leaf: audit-options-gui-sessions-sessions-default-session-on-startup
#[test]
fn recovery_keeps_displayed_name_if_preferences_change_and_clean_startup_skips_question() {
    use std::{cell::RefCell, rc::Rc};
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("session_startup.json");
    let named = Session {
        name: "startup work".into(),
        pages: source(&fixture["named_tree"]),
    };
    store
        .write(move |ctx| {
            hydrus_store::session_backups::save(ctx.conn(), &named, 1_000)?;
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    startup: Some("startup work".into()),
                    ..GuiSessionSettings::default()
                },
            )
        })
        .unwrap();
    let loaded = Rc::new(RefCell::new(None));
    let ready = Rc::new({
        let loaded = loaded.clone();
        move |pages: hydrus_store::Result<Pages>| *loaded.borrow_mut() = Some(pages.unwrap())
    });
    let recovery = hydrus_gui::session_startup::prepare(store.clone(), true, ready)
        .unwrap()
        .unwrap();
    assert!(recovery.window().get_yes_label().contains("startup work"));
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &GuiSessionSettings {
                    startup: None,
                    ..GuiSessionSettings::default()
                },
            )
        })
        .unwrap();
    recovery.window().invoke_answered(true);
    assert_eq!(
        tree(&loaded.borrow().as_ref().unwrap().session().pages),
        fixture["named_tree"]
    );
    let clean = Rc::new(RefCell::new(None));
    let ready = Rc::new({
        let clean = clean.clone();
        move |pages: hydrus_store::Result<Pages>| *clean.borrow_mut() = Some(pages.unwrap())
    });
    assert!(
        hydrus_gui::session_startup::prepare(store.clone(), false, ready)
            .unwrap()
            .is_none()
    );
    assert_eq!(clean.borrow().as_ref().unwrap().shown().name, "files");
}

// leaf: audit-options-gui-sessions-sessions-default-session-on-startup
#[test]
fn running_marker_survives_interrupted_boot_and_clears_only_explicit_clean_finish() {
    use hydrus_gui::session_startup::Run;
    let directory = tempfile::tempdir().unwrap();
    {
        let first = Run::begin(directory.path()).unwrap();
        assert!(!first.bad());
        // An interrupted process does not run explicit clean shutdown.
    }
    let recovered = Run::begin(directory.path()).unwrap();
    assert!(recovered.bad());
    recovered.finish().unwrap();
    let clean = Run::begin(directory.path()).unwrap();
    assert!(!clean.bad());
    clean.finish().unwrap();
    let final_run = Run::begin(directory.path()).unwrap();
    assert!(!final_run.bad());
    final_run.finish().unwrap();
    assert!(Run::begin(&directory.path().join("absent")).is_err());
}
