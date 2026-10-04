//! Staged owned sort cogs, cancellation/reopen, and actual page sorting.
use hydrus_core::{
    HashId,
    pages::{PageSortBy, SortSettings},
    search::context::TagContext,
};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;
fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let source = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([source, native], store)
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let i = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(i).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let p = (0..pages.row_count())
        .find(|&p| pages.row_data(p).unwrap().text == "file sort/collect")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(p).unwrap());
    window
}
fn row(window: &OptionsWindow, role: &str) -> i32 {
    let label = if role == "default" {
        "Default file sort: "
    } else {
        "Secondary file sort (when primary gives two equal values): "
    };
    let rows = window.get_rows();
    i32::try_from(
        (0..rows.row_count())
            .find(|&i| rows.row_data(i).unwrap().label == label)
            .unwrap(),
    )
    .unwrap()
}
fn service(window: &OptionsWindow, row: i32, name: &str) {
    window.invoke_sort_cog_open(row);
    window.invoke_sort_cog_group_chosen(row, 0);
    let items = window.get_sort_cog_items();
    let i = (0..items.row_count())
        .find(|&i| items.row_data(i).unwrap().label == name)
        .unwrap();
    window.invoke_sort_cog_choice(row, i32::try_from(i).unwrap());
}
#[test]
fn owned_cogs_stage_cancel_reopen_and_feed_independent_default_and_fallback_keys() {
    let windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("sort_cogs.json");
    let original: SortSettings = store.read(settings::get).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let window = open(&ui, &bound);
    let default = row(&window, "default");
    let choices = window
        .get_rows()
        .row_data(usize::try_from(default).unwrap())
        .unwrap()
        .items;
    let namespace = (0..choices.row_count())
        .find(|&i| choices.row_data(i).unwrap() == "tags: series-creator-title-volume-chapter-page")
        .unwrap();
    window.invoke_sort_chosen(default, i32::try_from(namespace).unwrap());
    service(&window, default, "my tags");
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), original);
    window.invoke_sort_cog_open(default);
    window.invoke_sort_cog_group_chosen(default, 0);
    window.invoke_sort_cog_cancel();
    window.invoke_sort_cog_choice(default, 1);
    window.invoke_sort_cog_open(default);
    window.invoke_sort_cog_group_chosen(default, 0);
    let pages = window.get_pages();
    let advanced = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "advanced")
        .unwrap();
    let sorts = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "file sort/collect")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(advanced).unwrap());
    window.invoke_page_chosen(i32::try_from(sorts).unwrap());
    window.invoke_sort_cog_choice(default, 0);
    window.invoke_sort_cog_open(default);
    window.invoke_sort_cog_group_chosen(default, 0);
    let items = window.get_sort_cog_items();
    assert!(
        (0..items.row_count()).any(|i| {
            let item = items.row_data(i).unwrap();
            item.label == "my tags" && item.checked
        }),
        "detached menu actions cannot edit a rebuilt sort control"
    );
    window.invoke_sort_cog_cancel();
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 860, 740);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sort_cog_options.png"),
        &pixels,
        860,
        740,
    )
    .unwrap();
    window.invoke_cancel();
    window.invoke_sort_cog_open(default);
    window.invoke_sort_cog_group_chosen(default, 0);
    window.invoke_sort_cog_choice(default, 1);
    window.invoke_apply();
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), original);
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
    let hashes = store
        .read(|conn| hydrus_store::master::hashes(conn, &files))
        .unwrap();
    let location = LocationContext::single(
        hydrus_core::ServiceKey::from_hex(fixture["location"].as_str().unwrap()).unwrap(),
    );
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["collected"] == false && case["sort"]["order"] == 0)
    {
        let role = case["role"].as_str().unwrap();
        let by = if case["kind"] == "num_tags" {
            PageSortBy::System(case["sort"]["data"].as_i64().unwrap())
        } else {
            PageSortBy::Namespaces {
                namespaces: case["sort"]["data"]["namespaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
                tag_display_type: 1,
            }
        };
        let mut base = original.clone();
        let target = if role == "default" {
            &mut base.default_sort
        } else {
            base.default_sort.by = PageSortBy::System(case["primary"]["data"].as_i64().unwrap());
            base.default_sort.ascending = true;
            &mut base.fallback_sort
        };
        target.by = by;
        target.ascending = true;
        target.tag_context = TagContext {
            include_current: false,
            include_pending: false,
            ..TagContext::default()
        };
        let before = base.clone();
        store
            .write(move |tx| settings::set(tx.conn(), &base))
            .unwrap();
        let window = open(&ui, &bound);
        let target_row = row(&window, role);
        service(&window, target_row, case["service_name"].as_str().unwrap());
        if case["kind"] == "namespaces" {
            window.invoke_sort_cog_open(target_row);
            window.invoke_sort_cog_group_chosen(target_row, 1);
            let view = [1, 3, 2]
                .iter()
                .position(|&view| Some(view) == case["sort"]["data"]["tag_display_type"].as_i64())
                .unwrap();
            window.invoke_sort_cog_choice(target_row, i32::try_from(view).unwrap());
            window.invoke_sort_cog_open(target_row);
            window.invoke_sort_cog_group_chosen(target_row, 1);
            let views = window.get_sort_cog_items();
            let actual = (0..views.row_count())
                .map(|i| {
                    let entry = views.row_data(i).unwrap();
                    serde_json::json!({"check":entry.label.to_string(),"checked":entry.checked})
                })
                .collect::<Vec<_>>();
            assert_eq!(serde_json::json!(actual), case["menu"][1]["entries"]);
            window.invoke_sort_cog_cancel();
        }
        window.invoke_sort_cog_open(target_row);
        window.invoke_sort_cog_group_chosen(target_row, 0);
        let items = window.get_sort_cog_items();
        let actual: Vec<_> = (0..items.row_count())
            .map(|i| {
                let item = items.row_data(i).unwrap();
                if item.separator {
                    serde_json::json!("---")
                } else {
                    serde_json::json!({"check":item.label.to_string(),"checked":item.checked})
                }
            })
            .collect();
        assert_eq!(serde_json::json!(actual), case["menu"][0]["entries"]);
        window.invoke_sort_cog_choice(target_row, 999);
        window.invoke_sort_cog_cancel();
        assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
        window.invoke_apply();
        let saved: SortSettings = store.read(settings::get).unwrap();
        let chosen = if role == "default" {
            &saved.default_sort
        } else {
            &saved.fallback_sort
        };
        assert_eq!(
            chosen.tag_context.service.to_hex(),
            case["sort"]["tag_context"]["service"].as_str().unwrap()
        );
        assert!(!chosen.tag_context.include_current && !chosen.tag_context.include_pending);
        if let PageSortBy::Namespaces {
            tag_display_type, ..
        } = chosen.by
        {
            assert_eq!(
                tag_display_type,
                case["sort"]["data"]["tag_display_type"].as_i64().unwrap()
            );
        }
        assert_eq!(
            chosen.tag_context.display_service,
            TagContext::default().display_service
        );
        let untouched = if role == "default" {
            &saved.fallback_sort
        } else {
            &saved.default_sort
        };
        let expected_untouched = if role == "default" {
            &before.fallback_sort
        } else {
            &before.default_sort
        };
        assert_eq!(untouched, expected_untouched);
        let reopened = open(&ui, &bound);
        let reopened_row = row(&reopened, role);
        reopened.invoke_sort_cog_open(reopened_row);
        reopened.invoke_sort_cog_group_chosen(reopened_row, 0);
        assert_eq!(
            reopened.get_sort_cog_items().row_data(0).unwrap().label,
            "downloader tags"
        );
        reopened.invoke_cancel();
        let search = FileSearchContext {
            location: location.clone(),
            tags: TagContext::new(
                hydrus_core::ServiceKey::new(b"local tags".to_vec()),
                false,
                false,
            ),
            ..FileSearchContext::default()
        };
        let page = SearchPage::restored(store.clone(), search, false, None, files.clone());
        let consumer = MainWindow::new().unwrap();
        let consumer_bound = bind(&consumer, Pages::single(page));
        consumer.invoke_order_chosen(0);
        assert_eq!(
            consumer_bound
                .current
                .borrow()
                .borrow()
                .results()
                .iter()
                .map(|id| hashes[id].to_string())
                .collect::<Vec<_>>(),
            case["media"]
                .as_array()
                .unwrap()
                .iter()
                .map(|hash| hash.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{case}"
        );
    }
}
