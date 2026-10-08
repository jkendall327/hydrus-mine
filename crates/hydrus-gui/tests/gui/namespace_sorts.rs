//! Real namespace queue prompts, parent staging and media sort/collect consumers.
use hydrus_core::{
    HashId,
    pages::{PageSortBy, SortSettings},
};
use hydrus_gui::{
    MainWindow, NamespaceSortsWindow, OptionsWindow, Pages, SearchPage, bind, headless,
};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;
pub(super) fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
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
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let i = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(i).unwrap(), 0.0, 0.0, 0.0);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    page(&window, "file sort/collect");
    window
}
fn page(window: &OptionsWindow, name: &str) {
    let pages = window.get_pages();
    let i = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == name)
        .unwrap();
    window.invoke_page_chosen(i32::try_from(i).unwrap());
}
fn advanced(window: &OptionsWindow, on: bool) {
    page(window, "advanced");
    let rows = window.get_rows();
    let i = (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().label == "Advanced mode: ")
        .unwrap();
    window.invoke_check_toggled(i32::try_from(i).unwrap(), on);
    page(window, "file sort/collect");
}
fn child(window: &OptionsWindow) -> NamespaceSortsWindow {
    window.invoke_namespace_sorts_clicked();
    hydrus_gui::namespace_sorts_window::last_opened().unwrap()
}
fn labels(window: &NamespaceSortsWindow) -> Vec<String> {
    let rows = window.get_rows();
    (0..rows.row_count())
        .map(|i| {
            rows.row_data(i)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}
fn add(window: &NamespaceSortsWindow, text: &str) {
    window.invoke_action("add".into());
    assert!(window.get_asking_text());
    window.set_text(text.into());
    window.invoke_text_entered();
}

// leaf: audit-options-nested-namespace-sort-parse
#[test]
fn queue_replays_real_questions_selection_cancel_and_parent_apply_isolation() {
    let windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("namespace_sorts.json");
    let (_dirs, store) = store();
    let before = store.read(settings::get::<SortSettings>).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let options = open(&ui, &bound);
    let mut window = child(&options);
    let mut was_advanced = false;
    assert_eq!(
        labels(&window),
        fixture["initial"]["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    for step in fixture["steps"].as_array().unwrap() {
        let on = step["advanced"].as_bool().unwrap();
        if on != was_advanced {
            window.invoke_apply();
            advanced(&options, on);
            window = child(&options);
            was_advanced = on;
        }
        if let Some(indices) = step["selection"].as_array() {
            for (n, i) in indices.iter().enumerate() {
                window.invoke_clicked(i32::try_from(i.as_u64().unwrap()).unwrap(), n != 0, false);
            }
        }
        let action = match step["action"].as_str().unwrap() {
            "_Add" => "add",
            "_Edit" => "edit",
            "_Up" => "up",
            "_Down" => "down",
            "_Delete" => "delete",
            _ => unreachable!(),
        };
        window.invoke_action(action.into());
        match action {
            "add" | "edit" => {
                assert_eq!(
                    window.get_message(),
                    step["calls"][0]["message"].as_str().unwrap()
                );
                assert_eq!(
                    window.get_text(),
                    step["calls"][0]["default"].as_str().unwrap()
                );
                if let Some(text) = step["text"].as_str() {
                    window.set_text(text.into());
                    window.invoke_text_entered();
                    if window.get_choosing_view() {
                        assert_eq!(
                            window.get_message(),
                            step["calls"][1]["message"].as_str().unwrap()
                        );
                        assert_eq!(
                            window.get_window_title(),
                            step["calls"][1]["title"].as_str().unwrap()
                        );
                        if let Some(i) = step["view"].as_u64() {
                            window.invoke_view_chosen(i32::try_from(i).unwrap());
                        } else {
                            window.invoke_cancel_prompt();
                        }
                    }
                } else {
                    window.invoke_cancel_prompt();
                }
            }
            "delete" => {
                assert_eq!(
                    window.get_message(),
                    step["calls"][0]["message"].as_str().unwrap()
                );
                window.invoke_answer(step["accepted"].as_bool().unwrap());
            }
            _ => {}
        }
        assert_eq!(
            labels(&window),
            step["state"]["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{step}"
        );
        let rows = window.get_rows();
        assert_eq!(
            (0..rows.row_count())
                .filter(|&i| rows.row_data(i).unwrap().selected)
                .collect::<Vec<_>>(),
            step["state"]["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
    }
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 760, 580);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("namespace_sorts.png"),
        &pixels,
        760,
        580,
    )
    .unwrap();
    options.invoke_apply();
    assert!(
        bound.options.borrow().is_some(),
        "parent Apply is blocked while queue is open"
    );
    window.invoke_apply();
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
    options.invoke_cancel();
    window.invoke_action("add".into());
    assert!(
        !window.get_asking_text(),
        "retained closed callbacks cannot reopen a prompt"
    );
    let reopened = open(&ui, &bound);
    let fresh = child(&reopened);
    assert_eq!(
        labels(&fresh),
        fixture["initial"]["labels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    fresh.invoke_action("add".into());
    reopened.invoke_cancel();
    fresh.invoke_text_entered();
    assert_eq!(store.read(settings::get::<SortSettings>).unwrap(), before);
}

#[test]
fn applied_scheme_reopens_and_drives_real_ordered_media_and_new_collect_namespace() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("media_collect.json");
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
    let location = LocationContext::single(
        hydrus_core::ServiceKey::from_hex(fixture["service_key"].as_str().unwrap()).unwrap(),
    );
    let opened = SearchPage::restored(
        store.clone(),
        FileSearchContext {
            location,
            ..FileSearchContext::default()
        },
        false,
        None,
        files,
    );
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(opened));
    let options = open(&ui, &bound);
    let window = child(&options);
    for i in 0..window.get_rows().row_count() {
        window.invoke_clicked(i32::try_from(i).unwrap(), i != 0, false);
    }
    window.invoke_action("delete".into());
    window.invoke_answer(true);
    assert!(labels(&window).is_empty());
    add(&window, "series-creator-title-volume-chapter-page");
    add(&window, "studio");
    window.invoke_apply();
    options.invoke_apply();
    let sorts = store.read(settings::get::<SortSettings>).unwrap();
    assert_eq!(sorts.namespace_sorts.len(), 2);
    assert_eq!(
        sorts.namespace_sorts[1].by,
        PageSortBy::Namespaces {
            namespaces: vec!["studio".into()],
            tag_display_type: 1
        }
    );
    let choices = ui.get_collect_choices();
    let names = (0..choices.row_count())
        .map(|i| choices.row_data(i).unwrap().name.to_string())
        .collect::<Vec<_>>();
    assert!(
        names.contains(&"studio".into()),
        "added namespace reaches the actual collect control"
    );
    let series = i32::try_from(names.iter().position(|name| name == "series").unwrap()).unwrap();
    ui.invoke_collect_toggled(series, true);
    let names = ui.get_sort_names();
    let index = (0..names.row_count())
        .find(|&i| names.row_data(i).unwrap() == "tags: series-creator-title-volume-chapter-page")
        .unwrap();
    ui.invoke_sort_chosen(i32::try_from(index).unwrap());
    ui.invoke_order_chosen(0);
    let expected = &fixture["cases"][0]["sorts"][54];
    let page = bound.current.borrow().clone();
    let page = page.borrow();
    assert_eq!(page.sort(), &sorts.namespace_sorts[0]);
    let hashes = store
        .read(|conn| hydrus_store::master::hashes(conn, &page.files()))
        .unwrap();
    let actual = page
        .results()
        .iter()
        .map(|&item| match page.collection(item) {
            Some(files) => serde_json::Value::Array(
                files
                    .iter()
                    .map(|id| serde_json::Value::String(hashes[id].to_string()))
                    .collect(),
            ),
            None => serde_json::Value::String(hashes[&item].to_string()),
        })
        .collect::<Vec<_>>();
    assert_eq!(serde_json::json!(actual), expected["media"]);
    drop(page);
    let reopened = open(&ui, &bound);
    let fresh = child(&reopened);
    assert_eq!(
        labels(&fresh),
        vec!["series-creator-title-volume-chapter-page", "studio"]
    );
    fresh.invoke_cancel();
    reopened.invoke_cancel();
}
