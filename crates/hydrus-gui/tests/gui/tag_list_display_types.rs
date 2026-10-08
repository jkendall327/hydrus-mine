//! Actual Options, newly opened tag-list modes and retained opening defaults.
use super::tag_dialog_preferences::fixture;
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::tag_presentation::{TagDisplayType, TagPresentation};
use hydrus_gui::{MainWindow, MediaViewer, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, settings, tag_display::TagDisplayFilters};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

const LABELS: [&str; 2] = [
    "Tag display type for new page sidebar taglists: ",
    "Tag display type for new media viewer taglists: ",
];
fn options(ui: &MainWindow) {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "file")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let at = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label.starts_with("options"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
}
fn edit(window: &OptionsWindow, mode: TagDisplayType, recorded: &Value) {
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag presentation")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    for (label, field) in LABELS.into_iter().zip(["sidebar", "viewer"]) {
        let row = window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap();
        let option = window.get_rows().row_data(row).unwrap();
        assert_eq!(option.kind, 5);
        assert_eq!(
            json!(
                option
                    .items
                    .iter()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>()
            ),
            json!(
                recorded["options"]["choices"][field]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|choice| choice[0].as_str().unwrap())
                    .collect::<Vec<_>>()
            )
        );
        window.invoke_choice_chosen(
            i32::try_from(row).unwrap(),
            i32::try_from(mode.choice()).unwrap(),
        );
    }
}
fn rows<'a>(values: impl Iterator<Item = &'a str>) -> Value {
    json!(
        values
            .filter(|row| row.contains("mode "))
            .collect::<Vec<_>>()
    )
}
fn expected(case: &Value, list: &str) -> Value {
    json!(
        case["rows"][list]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|term| term["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row.as_str().unwrap()))
            .collect::<Vec<_>>()
    )
}
fn configure(store: &Store, recorded: &Value) {
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .to_hex();
    let recorded = recorded.clone();
    store
        .write(move |ctx| {
            let mut presentation: TagPresentation = settings::get(ctx.conn())?;
            presentation.show_namespaces = false;
            presentation.replace_underscores = true;
            settings::set(ctx.conn(), &presentation)?;
            let mut filters = TagDisplayFilters::default();
            for (single, tag) in [(false, "selection"), (true, "single")] {
                let mut filter = TagFilter::default();
                filter.set_rule(
                    recorded["filters"][tag].as_str().unwrap(),
                    FilterRule::Blacklist,
                );
                if single {
                    filters.single_media.insert(service.clone(), filter);
                } else {
                    filters.selection_list.insert(service.clone(), filter);
                }
            }
            settings::set(ctx.conn(), &filters)
        })
        .unwrap();
}
// leaf: audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-media-viewer-taglists
// leaf: audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-page-sidebar-taglists
#[test]
fn options_apply_cancel_new_sidebar_and_viewer_replay_raw_display_and_filters() {
    let recorded = hydrus_testkit::fixture_json("tag_list_display_types.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    configure(&store, &recorded);
    let windows = headless::init();
    for case in recorded["cases"].as_array().unwrap().iter().take(4) {
        let mode = TagDisplayType::from_code(case["mode"].as_i64().unwrap()).unwrap();
        assert_eq!(case["rows"], case["after_default_change"]);
        let main_adapter = windows.count();
        let ui = MainWindow::new().unwrap();
        let bound = bind(
            &ui,
            Pages::single(SearchPage::fixed(
                store.clone(),
                "mode corpus",
                None,
                files.clone(),
            )),
        );
        let original_page_mode = bound.current.borrow().borrow().tag_display_type();
        let before: TagPresentation = store.read(settings::get).unwrap();
        options(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(&window, mode, &recorded);
        assert_eq!(
            store.read::<TagPresentation>(settings::get).unwrap(),
            before
        );
        window.invoke_cancel();
        assert_eq!(
            store.read::<TagPresentation>(settings::get).unwrap(),
            before
        );
        options(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(&window, mode, &recorded);
        window.invoke_apply();
        assert_eq!(
            bound.current.borrow().borrow().tag_display_type(),
            original_page_mode
        );
        let saved: TagPresentation = store.read(settings::get).unwrap();
        assert_eq!(
            (saved.sidebar_display_type, saved.viewer_display_type),
            (mode, mode)
        );
        let page = SearchPage::fixed(store.clone(), "reopened mode corpus", None, files.clone());
        assert_eq!(page.tag_display_type(), mode);
        assert_eq!(rows(page.tag_rows().into_iter()), expected(case, "sidebar"));
        *bound.current.borrow().borrow_mut() = page;
        ui.invoke_refresh_page();
        let tags: Vec<String> = ui
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(
            rows(tags.iter().map(String::as_str)),
            expected(case, "sidebar")
        );
        let viewer_model = MediaViewer::new(store.clone(), files.clone(), 0).unwrap();
        assert_eq!(viewer_model.tag_display_type(), mode);
        let viewer_adapter = windows.count();
        ui.invoke_thumbnail_activated(0);
        let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        let tags: Vec<String> = viewer
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(
            rows(tags.iter().map(String::as_str)),
            expected(case, "viewer")
        );
        let next = TagDisplayType::from_code((mode.code() + 1) % 4).unwrap();
        options(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(&window, next, &recorded);
        window.invoke_apply();
        assert_eq!(bound.current.borrow().borrow().tag_display_type(), mode);
        assert_eq!(viewer_model.tag_display_type(), mode);
        let tags: Vec<String> = viewer
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(
            rows(tags.iter().map(String::as_str)),
            expected(case, "viewer")
        );
        assert_eq!(SearchPage::new(store.clone()).tag_display_type(), next);
        assert_eq!(
            MediaViewer::new(store.clone(), files.clone(), 0)
                .unwrap()
                .tag_display_type(),
            next
        );
        // Cancel after a saved change preserves the next opening mode.
        options(&ui);
        let window = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(&window, mode, &recorded);
        window.invoke_cancel();
        assert_eq!(SearchPage::new(store.clone()).tag_display_type(), next);
        let pixels = headless::render(&windows.get(main_adapter).unwrap(), 1200, 900);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
        let pixels = headless::render(&windows.get(viewer_adapter).unwrap(), 1000, 760);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
        viewer.invoke_close_requested();
        ui.invoke_thumbnail_activated(0);
        let reopened = bound.viewer.borrow().as_ref().unwrap().clone_strong();
        let next_case = recorded["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["mode"] == next.code())
            .unwrap();
        let tags: Vec<String> = reopened
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(
            rows(tags.iter().map(String::as_str)),
            expected(next_case, "viewer")
        );
        reopened.invoke_close_requested();
    }
}

// leaf: audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-media-viewer-taglists
// leaf: audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-page-sidebar-taglists
#[test]
fn sidebar_and_viewer_opening_modes_are_independent_after_gui_apply() {
    let recorded = hydrus_testkit::fixture_json("tag_list_display_types.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    configure(&store, &recorded);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "different modes",
            None,
            files.clone(),
        )),
    );
    options(&ui);
    let window = bound.options.borrow().as_ref().unwrap().clone_strong();
    edit(&window, TagDisplayType::Storage, &recorded);
    let viewer_row = window
        .get_rows()
        .iter()
        .position(|row| row.label == LABELS[1])
        .unwrap();
    window.invoke_choice_chosen(
        i32::try_from(viewer_row).unwrap(),
        i32::try_from(TagDisplayType::SelectionList.choice()).unwrap(),
    );
    window.invoke_apply();
    let page = SearchPage::fixed(store.clone(), "new different modes", None, files.clone());
    assert_eq!(page.tag_display_type(), TagDisplayType::Storage);
    assert_eq!(
        rows(page.tag_rows().into_iter()),
        expected(&recorded["cases"][3], "sidebar")
    );
    *bound.current.borrow().borrow_mut() = page;
    ui.invoke_refresh_page();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let tags: Vec<String> = viewer
        .get_tags()
        .iter()
        .map(|row| row.text.to_string())
        .collect();
    assert_eq!(
        rows(tags.iter().map(String::as_str)),
        expected(&recorded["cases"][0], "viewer")
    );
    viewer.invoke_close_requested();
}
