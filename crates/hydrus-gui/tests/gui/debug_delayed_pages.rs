//! Actual menu snapshots → owned deadlines → selected real query pages.
use hydrus_core::{ServiceKey, pages::PageContent, service::builtin_keys};
use hydrus_gui::{Bound, MainWindow, Pages, bind, headless};
use hydrus_gui_model::page_chooser::NewPage;
use hydrus_search::{LocationContext, TagContext};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::Cell, rc::Rc, time::Duration};

fn location(key: &[u8]) -> LocationContext {
    LocationContext::single(ServiceKey::new(key.to_vec()))
}
fn defaults(store: &Store, file: &[u8], tags: &[u8]) {
    let local_location = location(file);
    let tag_service = ServiceKey::new(tags.to_vec());
    store
        .write(move |ctx| {
            let mut value: settings::SearchDefaults = settings::get(ctx.conn())?;
            value.local_location = local_location;
            value.tag_service = tag_service;
            settings::set(ctx.conn(), &value)
        })
        .unwrap();
}
fn clock(bound: &Bound) -> Rc<Cell<Duration>> {
    let now = Rc::new(Cell::new(Duration::ZERO));
    bound.debug_long_popup.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));
    now
}
fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let rows = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = rows.iter().position(|row| row.label == label).unwrap();
    assert!(rows.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0.0, 0.0, 0.0);
}
fn menu(ui: &MainWindow) {
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    choose(ui, 0, "debug");
    choose(ui, 1, "gui actions");
}
fn launch(ui: &MainWindow) {
    menu(ui);
    choose(ui, 2, "make a new page in five seconds");
}
fn searched(store: &Store, file: &[u8], tags: &[u8]) -> Vec<hydrus_core::HashId> {
    let search = hydrus_search::FileSearchContext {
        location: location(file),
        tags: TagContext::new(ServiceKey::new(tags.to_vec()), true, true),
        predicates: vec![hydrus_search::Predicate::System(
            hydrus_search::SystemPredicate::Everything,
        )],
    };
    store
        .read(|conn| {
            Ok(hydrus_search::search_files(
                conn,
                &store.snapshot(),
                &search,
                hydrus_search::FileSort {
                    by: hydrus_search::SortBy::ImportTime,
                    order: hydrus_search::SortOrder::Descending,
                },
                &hydrus_search::Clock::system(),
            )
            .unwrap())
        })
        .unwrap()
}

#[test]
fn menu_snapshot_overlapping_hidden_delivery_current_notebook_and_real_query() {
    let fixture = hydrus_testkit::fixture_json("debug_delayed_pages.json");
    assert_eq!(
        fixture["menu_path"],
        serde_json::json!([
            "help",
            "debug",
            "gui actions",
            "make a new page in five seconds"
        ])
    );
    assert_eq!(fixture["schedule"].as_array().unwrap().len(), 2);
    for item in fixture["schedule"].as_array().unwrap() {
        assert_eq!(item["delay_seconds"], 5);
        assert_eq!(item["topic"], "new_page_query");
    }
    assert_eq!(fixture["trace"][2]["visible"], false);
    assert_eq!(fixture["trace"][2]["published"], 1);
    assert_eq!(fixture["trace"][4]["minimized"], true);
    assert_eq!(fixture["trace"][4]["published"], 2);
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    defaults(&store, builtin_keys::MY_FILES, builtin_keys::COMBINED_TAG);
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    // Representative menu surfaces only: the action-result consumers are untested here.
    // Keyboard submenu placement uses the real menu geometry published by Slint.
    let adapter = windows.get(0).unwrap();
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|title| title.label == "help")
        .unwrap();
    for (group, filename) in [
        ("gui actions", "help-debug-gui-actions-menu-native.png"),
        ("data actions", "help-debug-data-actions-menu-native.png"),
        ("profiling", "help-debug-profiling-menu-native.png"),
    ] {
        ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
        let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
        let debug = lines.iter().position(|line| line.label == "debug").unwrap();
        let steps = lines
            .iter()
            .take(debug + 1)
            .filter(|line| line.kind != 2)
            .count();
        let _ = headless::render(&adapter, 1400, 900);
        for _ in 0..steps {
            ui.invoke_menu_key(slint::platform::Key::DownArrow.into(), false);
        }
        ui.invoke_menu_key(slint::platform::Key::RightArrow.into(), false);
        let index = ui
            .get_menu_panes()
            .row_data(1)
            .unwrap()
            .lines
            .iter()
            .position(|line| line.label == group)
            .unwrap();
        let _ = headless::render(&adapter, 1400, 900);
        for _ in 0..index {
            ui.invoke_menu_key(slint::platform::Key::DownArrow.into(), false);
        }
        ui.invoke_menu_key(slint::platform::Key::RightArrow.into(), false);
        let pixels = headless::render(&adapter, 1400, 900);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(filename),
            &pixels,
            1400,
            900,
        )
        .unwrap();
        ui.invoke_menu_dismissed();
    }
    let now = clock(&bound);
    let source = bound.pages.borrow().shown().key;
    let initial = bound.pages.borrow().page_count();
    menu(&ui);
    defaults(&store, builtin_keys::TRASH, builtin_keys::COMBINED_TAG);
    choose(&ui, 2, "make a new page in five seconds");
    assert_eq!(bound.pages.borrow().page_count(), initial);
    now.set(Duration::from_secs(1));
    menu(&ui);
    defaults(&store, builtin_keys::MY_FILES, b"downloader tags");
    choose(&ui, 2, "make a new page in five seconds");
    bound.pages.borrow_mut().new_page(&NewPage::Pages).unwrap();
    let nested = bound.pages.borrow().tabs()[1].parent.unwrap();
    let before = bound.pages.borrow().page_count();
    assert_eq!(bound.debug_long_popup.pending_pages(), 2);
    ui.hide().unwrap();
    bound
        .debug_long_popup
        .start_delayed_page(location(builtin_keys::TRASH));
    assert_eq!(
        bound.debug_long_popup.pending_pages(),
        2,
        "hidden Main refuses new work"
    );
    now.set(Duration::from_millis(4999));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), before);
    now.set(Duration::from_secs(5));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), before + 1);
    assert_eq!(bound.pages.borrow().tabs()[1].parent, Some(nested));
    assert!(
        !ui.window().is_visible(),
        "delivery does not raise hidden Main"
    );
    assert_eq!(
        bound.current.borrow().borrow().location(),
        &location(builtin_keys::MY_FILES)
    );
    assert_eq!(
        bound
            .current
            .borrow()
            .borrow()
            .tag_context()
            .service
            .as_bytes(),
        b"downloader tags"
    );
    assert!(bound.current.borrow().borrow().results().is_empty());
    let borrowed_pages = bound.pages.borrow();
    let PageContent::Search { search, .. } = &borrowed_pages.shown().content else {
        panic!("expected query");
    };
    assert!(search.predicates.is_empty());
    assert_eq!(
        search
            .location
            .current()
            .iter()
            .map(|key| hex::encode(key.as_bytes()))
            .collect::<Vec<_>>(),
        vec![
            fixture["pages"][0]["location"]["current"][0]
                .as_str()
                .unwrap()
                .to_owned()
        ]
    );
    drop(borrowed_pages);
    assert!(bound.pages.borrow_mut().show(&source));
    now.set(Duration::from_millis(5999));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), before + 1);
    now.set(Duration::from_secs(6));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), before + 2);
    assert_eq!(bound.pages.borrow().tabs().len(), 1);
    assert_eq!(
        bound.current.borrow().borrow().location(),
        &location(builtin_keys::TRASH)
    );
    assert_eq!(bound.debug_long_popup.pending_pages(), 0);
    assert!(!bound.debug_long_popup.timer_running());
    ui.show().unwrap();
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let expected = searched(&store, builtin_keys::TRASH, b"downloader tags");
    assert!(
        !expected.is_empty(),
        "fixture exercises a real query result"
    );
    let mut actual = bound.current.borrow().borrow().results().to_vec();
    let mut expected = expected;
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "deferred page reaches the actual Store query"
    );
    let drawn = windows.get(0).unwrap();
    headless::render(&drawn, 1000, 700);
    bound.rows.wait();
    let pixels = headless::render(&drawn, 1000, 700);
    assert!(bound.rows.cached() > 0, "actual result thumbnails decode");
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("debug_delayed_pages_native.png"),
        &pixels,
        1000,
        700,
    )
    .unwrap();
}

#[test]
fn pages_share_popup_deadlines_without_dropping_pruning_or_publication() {
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound);
    bound.debug_long_popup.start();
    bound.debug_long_popup.start_delayed_popup();
    launch(&ui);
    let count = bound.pages.borrow().page_count();
    assert_eq!(bound.debug_long_popup.pending_updates(), 124);
    ui.invoke_popup_dismiss(0);
    ui.invoke_popup_dismiss(0);
    bound.debug_long_popup.tick();
    assert_eq!(bound.debug_long_popup.pending_updates(), 0);
    assert_eq!(bound.debug_long_popup.pending_pages(), 1);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 1);
    assert!(bound.debug_long_popup.timer_running());
    now.set(Duration::from_secs(5));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), count + 1);
    assert_eq!(ui.get_popups().row_count(), 1);
    assert_eq!(bound.debug_long_popup.pending_delayed_popups(), 0);
    assert!(!bound.debug_long_popup.timer_running());
}

#[test]
fn cancel_exit_preserves_query_but_accepted_exit_rebind_and_owner_drop_retire_requests() {
    let (_dirs, store) = super::subscriptions::store();
    store
        .write(|ctx| {
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = clock(&bound);
    launch(&ui);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert_eq!(bound.debug_long_popup.pending_pages(), 1);
    let count = bound.pages.borrow().page_count();
    now.set(Duration::from_secs(5));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), count + 1);
    launch(&ui);
    let old = bound.debug_long_popup.clone();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(old.pending_pages(), 0);
    assert!(!old.timer_running());
    let old_count = bound.pages.borrow().page_count();
    now.set(Duration::from_secs(30));
    old.tick();
    old.start_delayed_page(location(builtin_keys::MY_FILES));
    assert_eq!(bound.pages.borrow().page_count(), old_count);
    let next = clock(&successor);
    launch(&ui);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert_eq!(successor.debug_long_popup.pending_pages(), 0);
    let count = successor.pages.borrow().page_count();
    ui.show().unwrap();
    next.set(Duration::from_secs(30));
    successor.debug_long_popup.tick();
    launch(&ui);
    assert_eq!(successor.pages.borrow().page_count(), count);
    assert_eq!(successor.debug_long_popup.pending_pages(), 0);
    let fresh = bind(&ui, Pages::open(store.clone()).unwrap());
    let next = clock(&fresh);
    launch(&ui);
    let retained = fresh.debug_long_popup.clone();
    let pages = fresh.pages.clone();
    let count = pages.borrow().page_count();
    drop(fresh);
    next.set(Duration::from_secs(30));
    retained.tick();
    assert_eq!(retained.pending_pages(), 0);
    assert_eq!(pages.borrow().page_count(), count);
    let last = bind(&ui, Pages::open(store).unwrap());
    let next = clock(&last);
    launch(&ui);
    ui.hide().unwrap();
    let weak = ui.as_weak();
    drop(ui);
    assert!(
        weak.upgrade().is_none(),
        "page delivery owns only Weak MainWindow"
    );
    next.set(Duration::from_secs(30));
    last.debug_long_popup.tick();
    assert_eq!(last.debug_long_popup.pending_pages(), 0);
    assert!(!last.debug_long_popup.timer_running());
}

#[test]
fn background_delivery_preserves_the_open_choosers_frozen_notebook_and_anchor() {
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let now = clock(&bound);
    let source = bound.pages.borrow().shown().key;
    launch(&ui);
    bound.pages.borrow_mut().new_page(&NewPage::Pages).unwrap();
    let delivery_notebook = bound.pages.borrow().tabs()[1].parent.unwrap();
    // The actual chooser captures root + before source, despite current nested.
    ui.invoke_tab_new_page_requested("".into(), source.to_hex().into());
    assert!(ui.get_chooser_labels().row_count() > 0);
    let before = bound.pages.borrow().page_count();
    now.set(Duration::from_secs(5));
    bound.debug_long_popup.tick();
    assert_eq!(bound.pages.borrow().page_count(), before + 1);
    assert_eq!(
        bound.pages.borrow().tabs()[1].parent,
        Some(delivery_notebook)
    );
    assert!(
        ui.get_chooser_labels().row_count() > 0,
        "delivery leaves the chooser live"
    );
    // Choose Special → page of pages through the real retained modal callbacks.
    ui.invoke_chooser_pressed(6);
    ui.invoke_chooser_pressed(8);
    let pages = bound.pages.borrow();
    assert_eq!(pages.page_count(), before + 3);
    let root = &pages.session().pages;
    assert_eq!(
        root[1].key, source,
        "chooser still inserts before its captured source"
    );
    assert_eq!(root[2].key, delivery_notebook);
    assert!(matches!(root[0].content, PageContent::Pages(_)));
    let PageContent::Pages(children) = &root[2].content else {
        panic!("delivery notebook");
    };
    assert_eq!(
        children.len(),
        2,
        "chooser did not silently insert into the delivery notebook"
    );
    assert_eq!(pages.tabs()[1].parent, Some(root[0].key));
    assert_eq!(ui.get_chooser_labels().row_count(), 0);
}
