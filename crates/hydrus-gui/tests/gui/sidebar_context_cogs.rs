//! Real sidebar/default-collect menu callbacks reach the recorded media consumers.
use hydrus_core::{
    HashId, ServiceKey,
    pages::{PageCollect, PageSort, PageSortBy, SortSettings},
    search::context::TagContext,
};
use hydrus_gui::{
    Bound, ContextCogItem, ContextCogServices, MainWindow, OptionsWindow, Pages, SearchPage, bind,
    headless,
};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _, ModelRc};

fn context(value: &Value) -> TagContext {
    TagContext {
        service: ServiceKey::from_hex(value["service"].as_str().unwrap()).unwrap(),
        display_service: ServiceKey::from_hex(value["display_service"].as_str().unwrap()).unwrap(),
        include_current: value["include_current"].as_bool().unwrap(),
        include_pending: value["include_pending"].as_bool().unwrap(),
    }
}
fn sort(value: &Value) -> PageSort {
    PageSort {
        ascending: value["order"] == 0,
        tag_context: context(&value["tag_context"]),
        by: if value["type"] == "system" {
            PageSortBy::System(value["data"].as_i64().unwrap())
        } else {
            PageSortBy::Namespaces {
                namespaces: value["data"]["namespaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
                tag_display_type: value["data"]["tag_display_type"].as_i64().unwrap(),
            }
        },
    }
}
fn collect(value: &Value) -> PageCollect {
    PageCollect {
        namespaces: value["namespaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect(),
        ratings: value["ratings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| ServiceKey::from_hex(v.as_str().unwrap()).unwrap())
            .collect(),
        collect_unmatched: value["unmatched"].as_bool().unwrap(),
        tag_context: context(&value["tag_context"]),
    }
}
fn entries(items: &ModelRc<ContextCogItem>) -> Vec<Value> {
    (0..items.row_count())
        .map(|i| {
            let item = items.row_data(i).unwrap();
            json!({"check":item.label.to_string(),"checked":item.checked})
        })
        .collect()
}
fn service_entries(services: &ContextCogServices) -> Value {
    let mut result = Vec::new();
    for group in [&services.local, &services.repositories, &services.combined] {
        if group.row_count() == 0 {
            continue;
        }
        if !result.is_empty() {
            result.push(json!("---"));
        }
        result.extend(entries(group));
    }
    json!(result)
}
fn item_id(services: &ContextCogServices, name: &str) -> i32 {
    [&services.local, &services.repositories, &services.combined]
        .into_iter()
        .flat_map(|group| (0..group.row_count()).filter_map(|i| group.row_data(i)))
        .find(|item| item.label == name)
        .unwrap()
        .id
}
fn extra_id(items: &ModelRc<ContextCogItem>, label: &str) -> i32 {
    (0..items.row_count())
        .filter_map(|i| items.row_data(i))
        .find(|item| item.label == label)
        .unwrap()
        .id
}
fn media(store: &Store, page: &SearchPage) -> Value {
    let hashes = store
        .read(|conn| hydrus_store::master::hashes(conn, &page.files()))
        .unwrap();
    json!(
        page.results()
            .iter()
            .map(|id| match page.collection(*id) {
                Some(files) => json!(
                    files
                        .iter()
                        .map(|file| hashes[file].to_string())
                        .collect::<Vec<_>>()
                ),
                None => json!(hashes[id].to_string()),
            })
            .collect::<Vec<_>>()
    )
}
fn open_options(ui: &MainWindow, bound: &Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let index = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "file sort/collect")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(index).unwrap());
    window
}
fn collect_row(window: &OptionsWindow) -> i32 {
    let rows = window.get_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().kind == 11)
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn sidebar_actions_replay_all_service_display_and_unmatched_media_outputs() {
    let windows = headless::init();
    let source = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("sidebar_sort_collect_cogs.json");
    let files: Vec<HashId> = fixture["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| {
            store
                .read(|conn| {
                    hydrus_store::master::hash_id(conn, &hash.as_str().unwrap().parse().unwrap())
                })
                .unwrap()
                .unwrap()
        })
        .collect();
    let mut initial: SortSettings = store.read(settings::get).unwrap();
    initial.fallback_sort = sort(&fixture["fallback"]);
    initial.save_page_sort_on_change = false;
    initial.default_collect = PageCollect::default();
    initial.default_sort = sort(&fixture["sorts"][0]["sort"]);
    initial.default_sort.tag_context.service = TagContext::default().service;
    let initial_copy = initial.clone();
    store
        .write(move |tx| settings::set(tx.conn(), &initial_copy))
        .unwrap();
    let mut page = SearchPage::restored(
        store.clone(),
        FileSearchContext {
            location: LocationContext::single(ServiceKey::new(b"local files".to_vec())),
            tags: context(&fixture["search_context"]),
            ..FileSearchContext::default()
        },
        false,
        None,
        files,
    );
    if fixture["locked"].as_bool().unwrap() {
        page.lock_search();
    }
    assert_eq!(page.lock().is_some(), fixture["locked"].as_bool().unwrap());
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.show().unwrap();
    for state in fixture["states"].as_array().unwrap() {
        let by = match state["kind"].as_str().unwrap() {
            "filesize" => PageSortBy::System(0),
            "num_tags" => PageSortBy::System(9),
            _ => sort(&fixture["sorts"][4]["sort"]).by,
        };
        bound.current.borrow().borrow_mut().set_sort_type(by);
        ui.invoke_order_chosen(0);
        assert_eq!(
            ui.get_sort_cog_visible(),
            state["visible"].as_bool().unwrap()
        );
    }
    for case in fixture["sorts"].as_array().unwrap() {
        let expected = sort(&case["sort"]);
        let caller = bound.current.borrow().clone();
        caller.borrow_mut().set_collect(PageCollect::default());
        caller.borrow_mut().set_sort_type(expected.by.clone());
        ui.invoke_order_chosen(0);
        ui.invoke_context_cog_opened(false);
        let id = item_id(
            &ui.get_sort_cog_services(),
            case["service"].as_str().unwrap(),
        );
        ui.invoke_context_cog_chosen(false, id);
        if let PageSortBy::Namespaces {
            tag_display_type, ..
        } = expected.by
        {
            ui.invoke_context_cog_opened(false);
            let label = hydrus_gui_model::sort_cog::VIEWS
                .iter()
                .find(|(_, code)| *code == tag_display_type)
                .unwrap()
                .0;
            let id = extra_id(&ui.get_sort_cog_extra(), label);
            ui.invoke_context_cog_chosen(false, id);
        }
        assert_eq!(caller.borrow().sort(), &expected);
        assert_eq!(media(&store, &caller.borrow()), case["media"], "{case}");
        ui.invoke_context_cog_opened(false);
        assert_eq!(
            service_entries(&ui.get_sort_cog_services()),
            case["menu"][0]["entries"]
        );
        if case["kind"] == "namespaces" {
            assert_eq!(
                json!(entries(&ui.get_sort_cog_extra())),
                case["menu"][1]["entries"]
            );
        }
    }
    for case in fixture["collects"].as_array().unwrap() {
        let expected = collect(&case["collect"]);
        let caller = bound.current.borrow().clone();
        let mut before = expected.clone();
        before.tag_context.service = TagContext::default().service;
        before.collect_unmatched = !expected.collect_unmatched;
        caller.borrow_mut().set_collect(before);
        ui.invoke_context_cog_opened(true);
        let id = item_id(
            &ui.get_collect_cog_services(),
            case["service"].as_str().unwrap(),
        );
        ui.invoke_context_cog_chosen(true, id);
        ui.invoke_context_cog_opened(true);
        let id = extra_id(
            &ui.get_collect_cog_extra(),
            if expected.collect_unmatched {
                "collect into one group"
            } else {
                "leave separate"
            },
        );
        ui.invoke_context_cog_chosen(true, id);
        assert_eq!(caller.borrow().collect(), &expected);
        assert_eq!(caller.borrow().sort(), &sort(&case["sort"]));
        assert_eq!(media(&store, &caller.borrow()), case["media"], "{case}");
        ui.invoke_context_cog_opened(true);
        assert_eq!(
            service_entries(&ui.get_collect_cog_services()),
            case["menu"][0]["entries"]
        );
        assert_eq!(
            json!(entries(&ui.get_collect_cog_extra())),
            case["menu"][1]["entries"]
        );
    }
    // Popups are tied to the context that opened them, and ids cannot act on a
    // later popup or a hidden owner. Existing media assertions remain exact.
    ui.invoke_context_cog_opened(false);
    let stale = item_id(&ui.get_sort_cog_services(), "my tags");
    ui.invoke_context_cog_opened(false);
    let before = bound.current.borrow().borrow().sort().clone();
    ui.invoke_context_cog_chosen(false, stale);
    assert_eq!(bound.current.borrow().borrow().sort(), &before);
    ui.invoke_context_cog_opened(false);
    let stale = item_id(&ui.get_sort_cog_services(), "my tags");
    ui.hide().unwrap();
    ui.invoke_context_cog_chosen(false, stale);
    assert_eq!(bound.current.borrow().borrow().sort(), &before);
    ui.show().unwrap();
    let pixels = headless::render(&windows.get(0).unwrap(), 1060, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sidebar_context_cogs.png"),
        &pixels,
        1060,
        760,
    )
    .unwrap();
    ui.invoke_context_cog_opened(false);
    let stale = item_id(&ui.get_sort_cog_services(), "my tags");
    let old_page = bound.current.borrow().clone();
    let old_sort = old_page.borrow().sort().clone();
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    let new_page = bound.current.borrow().clone();
    let new_sort = new_page.borrow().sort().clone();
    ui.invoke_context_cog_chosen(false, stale);
    assert_eq!(old_page.borrow().sort(), &old_sort);
    assert_eq!(new_page.borrow().sort(), &new_sort);
    // Saving the sidebar sort follows the existing user preference and never
    // rewrites a search/collect tag context.
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
}

#[test]
fn default_collect_cog_stages_cancels_reopens_and_rejects_retired_menu_actions() {
    let _windows = headless::init();
    let source = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("sidebar_sort_collect_cogs.json");
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    for case in fixture["options"].as_array().unwrap() {
        let mut base: SortSettings = store.read(settings::get).unwrap();
        let mut draft = collect(&case["draft"]);
        draft.tag_context.service = TagContext::default().service;
        base.default_collect = draft;
        let before = base.clone();
        store
            .write(move |tx| settings::set(tx.conn(), &base))
            .unwrap();
        let window = open_options(&ui, &bound);
        let row = collect_row(&window);
        window.invoke_collect_cog_opened(row);
        let id = item_id(
            &window.get_collect_cog_services(),
            case["service"].as_str().unwrap(),
        );
        window.invoke_collect_cog_chosen(row, id);
        assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
        window.invoke_collect_cog_opened(row);
        assert_eq!(
            service_entries(&window.get_collect_cog_services()),
            case["menu"][0]["entries"]
        );
        window.invoke_apply();
        assert_eq!(
            store
                .read(settings::get::<SortSettings>)
                .unwrap()
                .default_collect,
            collect(&case["saved"])
        );
        assert_eq!(
            SearchPage::new(store.clone()).collect(),
            &collect(&case["reopened"])
        );
        let reopened = open_options(&ui, &bound);
        let row = collect_row(&reopened);
        reopened.invoke_collect_cog_opened(row);
        assert_eq!(
            service_entries(&reopened.get_collect_cog_services()),
            case["menu"][0]["entries"]
        );
        reopened.invoke_cancel();
    }
    let before: SortSettings = store.read(settings::get).unwrap();
    assert_eq!(before.default_collect, collect(&fixture["cancel_before"]));
    let abandoned = open_options(&ui, &bound);
    let row = collect_row(&abandoned);
    abandoned.invoke_collect_cog_opened(row);
    let stale = item_id(&abandoned.get_collect_cog_services(), "downloader tags");
    abandoned.invoke_collect_cog_chosen(row, stale);
    abandoned.invoke_collect_cog_opened(row);
    let stale = item_id(&abandoned.get_collect_cog_services(), "my tags");
    abandoned.invoke_cancel();
    let successor = open_options(&ui, &bound);
    abandoned.invoke_collect_cog_chosen(row, stale);
    abandoned.invoke_apply();
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
    let row = collect_row(&successor);
    successor.invoke_collect_cog_opened(row);
    assert_eq!(
        service_entries(&successor.get_collect_cog_services()),
        fixture["options"][1]["menu"][0]["entries"]
    );
    // A retained action from the former page cannot edit a rebuilt control.
    let stale = item_id(&successor.get_collect_cog_services(), "my tags");
    successor.invoke_page_chosen(0);
    successor.invoke_collect_cog_chosen(row, stale);
    successor.invoke_cancel();
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
}

#[test]
fn actual_tag_display_menu_changes_modes_captures_checks_and_rejects_retired_view_actions() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};
    use std::rc::Rc;

    fn key(
        ui: &MainWindow,
        drawn: &slint::platform::software_renderer::MinimalSoftwareWindow,
        value: Key,
    ) {
        let text: slint::SharedString = value.into();
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
        headless::render(drawn, 1060, 760);
    }
    fn cog_click(
        ui: &MainWindow,
        drawn: &slint::platform::software_renderer::MinimalSoftwareWindow,
        button: PointerEventButton,
    ) {
        headless::render(drawn, 1060, 760);
        assert!(ui.get_sort_cog_visible());
        let frame = ui.get_sort_order_frame();
        assert!(
            [frame.x, frame.y, frame.w, frame.h]
                .iter()
                .all(|value| value.is_finite())
                && frame.w > 0.0
                && frame.h > 0.0
        );
        // main.slint places the fixed 30px cog after order-choice with 4px
        // spacing. Use the actual adjacent control's exposed absolute frame.
        let position =
            slint::LogicalPosition::new(frame.x + frame.w + 4.0 + 15.0, frame.y + frame.h / 2.0);
        assert!(position.x > 0.0 && position.x < 1060.0);
        assert!(position.y > 0.0 && position.y < 760.0);
        ui.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        ui.window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        ui.window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
        headless::render(drawn, 1060, 760);
    }
    fn open_cog(
        ui: &MainWindow,
        drawn: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ) {
        let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window());
        assert!(inner.active_popups().is_empty());
        let previous_ids: Vec<_> = ui.get_sort_cog_extra().iter().map(|item| item.id).collect();
        cog_click(ui, drawn, PointerEventButton::Left);
        assert_eq!(inner.active_popups().len(), 1, "actual sort cog root");
        let fresh_ids: Vec<_> = ui.get_sort_cog_extra().iter().map(|item| item.id).collect();
        assert_eq!(fresh_ids.len(), 3);
        assert_ne!(
            fresh_ids, previous_ids,
            "real sort-button opening refreshes ids"
        );
    }
    fn open_display(
        ui: &MainWindow,
        drawn: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ) {
        open_cog(ui, drawn);
        // The real root has tag service first and ADVANCED: tag display type
        // second. PopupMenuImpl starts unhighlighted and skips separators.
        key(ui, drawn, Key::DownArrow);
        key(ui, drawn, Key::DownArrow);
        key(ui, drawn, Key::RightArrow);
        let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window());
        assert_eq!(inner.active_popups().len(), 2, "actual display submenu");
    }
    fn close_display(
        ui: &MainWindow,
        drawn: &slint::platform::software_renderer::MinimalSoftwareWindow,
    ) {
        let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window());
        assert_eq!(inner.active_popups().len(), 2);
        key(ui, drawn, Key::Escape);
        assert_eq!(inner.active_popups().len(), 1, "submenu Escape keeps root");
        key(ui, drawn, Key::Escape);
        assert!(inner.active_popups().is_empty());
    }
    fn capture(
        ui: &MainWindow,
        drawn: &slint::platform::software_renderer::MinimalSoftwareWindow,
        windows: &headless::Windows,
        adapter_count: usize,
        popup_count: usize,
        name: &str,
    ) {
        let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window());
        assert_eq!(inner.active_popups().len(), popup_count);
        // MinimalSoftwareWindow embeds real Slint popups in this composite.
        // Fail if transport creates another adapter; never export a closed
        // parent as if it were the defining root/submenu image.
        assert_eq!(windows.count(), adapter_count);
        let pixels = headless::render(drawn, 1060, 760);
        assert_eq!(inner.active_popups().len(), popup_count);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
            &pixels,
            1060,
            760,
        )
        .unwrap();
    }

    let windows = headless::init();
    let source = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("sidebar_sort_collect_cogs.json");
    // One service exercises real widget activation for all three recorded
    // modes. The original test still replays all 16 sort and 8 collect cases.
    let cases: Vec<_> = [1, 3, 2]
        .into_iter()
        .map(|mode| {
            fixture["sorts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| {
                    case["kind"] == "namespaces"
                        && case["service"] == "my tags"
                        && case["sort"]["data"]["tag_display_type"] == mode
                })
                .unwrap()
        })
        .collect();
    let files: Vec<HashId> = fixture["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hash| {
            store
                .read(|conn| {
                    hydrus_store::master::hash_id(conn, &hash.as_str().unwrap().parse().unwrap())
                })
                .unwrap()
                .unwrap()
        })
        .collect();
    let mut initial: SortSettings = store.read(settings::get).unwrap();
    initial.fallback_sort = sort(&fixture["fallback"]);
    initial.save_page_sort_on_change = false;
    initial.default_collect = PageCollect::default();
    initial.default_sort = sort(&cases[2]["sort"]);
    let saved = initial.clone();
    store
        .write(move |tx| settings::set(tx.conn(), &saved))
        .unwrap();
    let mut page = SearchPage::restored(
        store.clone(),
        FileSearchContext {
            location: LocationContext::single(ServiceKey::new(b"local files".to_vec())),
            tags: context(&fixture["search_context"]),
            ..FileSearchContext::default()
        },
        false,
        None,
        files,
    );
    page.lock_search();
    assert_eq!(page.lock().is_some(), fixture["locked"].as_bool().unwrap());
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(page));
    ui.show().unwrap();
    let drawn = windows.get(0).unwrap();
    let adapter_count = windows.count();
    let caller = bound.current.borrow().clone();
    let search = caller.borrow().tag_context().clone();
    let original_collect = caller.borrow().collect().clone();
    assert_eq!(caller.borrow().sort(), &initial.default_sort);
    assert_eq!(search, context(&fixture["search_context"]));
    let inner = slint::private_unstable_api::re_exports::WindowInner::from_pub(ui.window());
    cog_click(&ui, &drawn, PointerEventButton::Right);
    assert!(
        inner.active_popups().is_empty(),
        "Qt right-click opens no cog menu"
    );
    assert_eq!(caller.borrow().sort(), &initial.default_sort);
    assert_eq!(caller.borrow().tag_context(), &search);
    assert_eq!(caller.borrow().collect(), &original_collect);
    assert_eq!(media(&store, &caller.borrow()), cases[2]["media"]);
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
    open_cog(&ui, &drawn);
    assert_eq!(
        service_entries(&ui.get_sort_cog_services()),
        cases[2]["menu"][0]["entries"]
    );
    assert_eq!(
        json!(entries(&ui.get_sort_cog_extra())),
        cases[2]["menu"][1]["entries"]
    );
    capture(
        &ui,
        &drawn,
        &windows,
        adapter_count,
        1,
        "sidebar-sort-cog-root.png",
    );
    key(&ui, &drawn, Key::Escape);
    assert!(inner.active_popups().is_empty());

    for (index, case) in cases.iter().enumerate() {
        let expected = sort(&case["sort"]);
        assert_ne!(
            caller.borrow().sort().by,
            expected.by,
            "every real activation changes mode"
        );
        open_display(&ui, &drawn);
        for _ in 0..=index {
            key(&ui, &drawn, Key::DownArrow);
        }
        key(&ui, &drawn, Key::Return);
        assert!(
            inner.active_popups().is_empty(),
            "actual activation closes the menu chain"
        );
        assert_eq!(caller.borrow().sort(), &expected);
        assert_eq!(caller.borrow().tag_context(), &search);
        assert_eq!(caller.borrow().collect(), &original_collect);
        // The recorded fixture has the same order for these three display
        // modes. This checks exact consumer routing, not differentiated filters.
        assert_eq!(media(&store, &caller.borrow()), case["media"]);
        assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
        open_display(&ui, &drawn);
        assert_eq!(
            service_entries(&ui.get_sort_cog_services()),
            case["menu"][0]["entries"]
        );
        assert_eq!(
            json!(entries(&ui.get_sort_cog_extra())),
            case["menu"][1]["entries"]
        );
        capture(
            &ui,
            &drawn,
            &windows,
            adapter_count,
            2,
            &format!(
                "sidebar-tag-display-{}.png",
                case["sort"]["data"]["tag_display_type"].as_i64().unwrap()
            ),
        );
        close_display(&ui, &drawn);
        assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
    }

    // View ids have the same per-opening and captured-page admission as service
    // ids, but exercise those boundaries on the selected leaf itself.
    open_display(&ui, &drawn);
    let stale = extra_id(&ui.get_sort_cog_extra(), "display tags");
    close_display(&ui, &drawn);
    open_display(&ui, &drawn);
    ui.invoke_context_cog_chosen(false, stale);
    assert_eq!(caller.borrow().sort(), &sort(&cases[2]["sort"]));
    assert_eq!(caller.borrow().tag_context(), &search);
    assert_eq!(caller.borrow().collect(), &original_collect);
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
    let hidden_action = extra_id(&ui.get_sort_cog_extra(), "display tags");
    ui.hide().unwrap();
    ui.invoke_context_cog_chosen(false, hidden_action);
    assert_eq!(caller.borrow().sort(), &sort(&cases[2]["sort"]));
    assert_eq!(caller.borrow().tag_context(), &search);
    assert_eq!(caller.borrow().collect(), &original_collect);
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
    ui.show().unwrap();
    // This boundary tests hidden action admission, not automatic popup teardown.
    // Clear any embedded popup retained across temporary hide before new input.
    inner.close_all_popups();
    open_display(&ui, &drawn);
    let old_page_action = extra_id(&ui.get_sort_cog_extra(), "display tags");
    bound.pages.borrow_mut().new_search_page();
    ui.invoke_tab_chosen(0, 1);
    let successor = bound.current.borrow().clone();
    let successor_sort = successor.borrow().sort().clone();
    let successor_search = successor.borrow().tag_context().clone();
    let successor_collect = successor.borrow().collect().clone();
    ui.invoke_context_cog_chosen(false, old_page_action);
    assert_eq!(caller.borrow().sort(), &sort(&cases[2]["sort"]));
    assert_eq!(caller.borrow().tag_context(), &search);
    assert_eq!(caller.borrow().collect(), &original_collect);
    assert_eq!(successor.borrow().sort(), &successor_sort);
    assert_eq!(successor.borrow().tag_context(), &successor_search);
    assert_eq!(successor.borrow().collect(), &successor_collect);
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
    inner.close_all_popups();
    ui.invoke_tab_chosen(0, 0);
    assert!(Rc::ptr_eq(&bound.current.borrow(), &caller));

    // One real View activation covers the true save-default policy as well.
    let mut enabled = initial.clone();
    enabled.save_page_sort_on_change = true;
    let saved = enabled.clone();
    store
        .write(move |tx| settings::set(tx.conn(), &saved))
        .unwrap();
    open_display(&ui, &drawn);
    key(&ui, &drawn, Key::DownArrow);
    key(&ui, &drawn, Key::DownArrow);
    key(&ui, &drawn, Key::Return);
    assert!(inner.active_popups().is_empty());
    let expected = sort(&cases[1]["sort"]);
    assert_eq!(caller.borrow().sort(), &expected);
    assert_eq!(caller.borrow().tag_context(), &search);
    assert_eq!(caller.borrow().collect(), &original_collect);
    assert_eq!(media(&store, &caller.borrow()), cases[1]["media"]);
    enabled.default_sort = expected;
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), enabled);
    let new_page = SearchPage::new(store.clone());
    assert_eq!(new_page.sort(), &enabled.default_sort);
    assert_eq!(new_page.collect(), &enabled.default_collect);
    let saved = initial.clone();
    store
        .write(move |tx| settings::set(tx.conn(), &saved))
        .unwrap();
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), initial);
}
