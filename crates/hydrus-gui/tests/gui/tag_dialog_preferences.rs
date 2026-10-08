//! Native options transactions, rendered service navigators and storage list decoration.
#[path = "../../../hydrus-gui-model/tests/support/tag_dialog_preferences.rs"]
pub(super) mod fixture;
use hydrus_gui::{MainWindow, ManageTagsWindow, OptionsWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{settings, tag_editing::TagEditingSettings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

const CONTROLS: [(&str, &str); 4] = [
    (
        "listbook",
        "Use listbook instead of tabbed notebook for tag service panels: ",
    ),
    (
        "parents",
        "Show parent info by default on edit/write taglists: ",
    ),
    (
        "expanded",
        "Show parents expanded by default on edit/write taglists: ",
    ),
    (
        "siblings",
        "Show sibling info by default on edit/write taglists: ",
    ),
];

fn menu(ui: &MainWindow, title: &str, item: &str) {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == title)
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let entries = ui.get_menu_panes().row_data(0).unwrap().lines;
    let row = entries
        .iter()
        .position(|row| row.label.starts_with(item))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
}
fn edit(window: &OptionsWindow, values: &Value) {
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag editing")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    for (key, label) in CONTROLS {
        let index = window
            .get_rows()
            .iter()
            .position(|row| row.label == label)
            .unwrap();
        window.invoke_check_toggled(
            i32::try_from(index).unwrap(),
            values[key].as_bool().unwrap(),
        );
    }
}
fn preferences(store: &hydrus_store::Store) -> Value {
    fixture::preferences(&store.read::<TagEditingSettings>(settings::get).unwrap())
}
fn rows(window: &ManageTagsWindow) -> Value {
    let mut rows = Vec::<Value>::new();
    let mut in_recorded_tag = false;
    for row in window.get_tags().iter() {
        let text = row.text.to_string();
        if text.starts_with("parity:") {
            let tag = text.split(" (").next().unwrap();
            rows.push(json!({"tag":tag,"rows":[text]}));
            in_recorded_tag = true;
        } else if text.starts_with("    ") && in_recorded_tag {
            rows.last_mut().unwrap()["rows"]
                .as_array_mut()
                .unwrap()
                .push(json!(text));
        } else {
            in_recorded_tag = false;
        }
    }
    fixture::canonical(json!(rows))
}

/// The reference asked "what would you like to do?" (some of the files had
/// the tag): its message and buttons, then the recorded choice (the first, add).
fn choose_add(window: &hydrus_gui::ManageTagsWindow, asked: &serde_json::Value) {
    assert_eq!(window.get_tag_menu_question_title().as_str(), asked["title"].as_str().unwrap());
    assert_eq!(window.get_tag_menu_question().as_str(), asked["message"].as_str().unwrap());
    assert_eq!(window.get_tag_menu_yes_label().as_str(), asked["choices"][0].as_str().unwrap());
    assert_eq!(window.get_tag_menu_no_label().as_str(), asked["choices"][1].as_str().unwrap());
    assert_eq!(asked["chosen"], asked["choices"][0]);
    window.invoke_tag_menu_answered(true);
}

#[test]
fn options_apply_cancel_reopen_and_rendered_tag_service_topologies() {
    let recorded = hydrus_testkit::fixture_json("tag_dialog_preferences.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "recorded tagging corpus",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    let before = preferences(&store);
    menu(&ui, "file", "options");
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    edit(&options, &recorded["options"]["applied"]);
    assert_eq!(preferences(&store), before);
    options.invoke_cancel();
    assert_eq!(preferences(&store), recorded["options"]["cancelled"]);
    for index in [0, 5, 8, 15] {
        let case = &recorded["cases"][index];
        menu(&ui, "file", "options");
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        let prior = preferences(&store);
        edit(&options, &case["preferences"]);
        assert_eq!(preferences(&store), prior);
        options.invoke_apply();
        assert_eq!(preferences(&store), case["preferences"]);
        let rendered = windows.count();
        ui.invoke_manage_tags_selected();
        let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        let listbook = case["topology"]["manage"] == "ListBook";
        assert_eq!(manage.get_use_listbook(), listbook);
        let pixels = headless::render(&windows.get(rendered).unwrap(), 720, 680);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
        // Query the actual laid-out right page's position after software rendering.
        assert_eq!(manage.get_service_content_offset() > 100.0, listbook);
        assert_eq!(rows(&manage), fixture::canonical(case["rows"].clone()));
        let names: Vec<String> = manage
            .get_service_names()
            .iter()
            .map(|name| name.to_string())
            .collect();
        assert_eq!(json!(names), case["services"]);
        manage.invoke_text_edited("parity:caller autocomplete draft".into());
        let mine = manage.get_service_index();
        let other = manage
            .get_service_names()
            .iter()
            .position(|name| name == "downloader tags")
            .unwrap();
        manage.invoke_service_chosen(i32::try_from(other).unwrap());
        manage.invoke_service_chosen(mine);
        assert_eq!(manage.get_text(), "parity:caller autocomplete draft");
        manage.invoke_cancel();
        for kind in ["siblings", "parents"] {
            let rendered = windows.count();
            menu(&ui, "tags", kind);
            let relationship = bound
                .tag_relationships
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            headless::render(&windows.get(rendered).unwrap(), 1000, 800);
            assert_eq!(relationship.get_use_listbook(), listbook);
            assert_eq!(relationship.get_service_content_offset() > 100.0, listbook);
            relationship.invoke_cancel();
        }
        // Options cancellation after a persisted choice leaves the next dialog's topology intact.
        menu(&ui, "file", "options");
        let options = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(
            &options,
            &recorded["cases"][if listbook { 0 } else { 15 }]["preferences"],
        );
        options.invoke_cancel();
        ui.invoke_manage_tags_selected();
        let reopened = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        assert_eq!(reopened.get_use_listbook(), listbook);
        assert_eq!(rows(&reopened), fixture::canonical(case["rows"].clone()));
        reopened.invoke_cancel();
    }
}

#[test]
fn expanded_parent_rows_keep_colour_and_activate_their_originating_tag() {
    let recorded = hydrus_testkit::fixture_json("tag_dialog_preferences.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    fixture::set_preferences(&store, &recorded["cases"][15]["preferences"]);
    store
        .write(|ctx| {
            let mut colours: hydrus_core::tag_presentation::NamespaceColours =
                settings::get(ctx.conn())?;
            colours.colours.retain(|(namespace, _)| {
                !matches!(namespace.as_deref(), Some("category" | "series"))
            });
            colours.colours.push((Some("category".into()), [3, 5, 7]));
            colours.colours.push((Some("series".into()), [11, 13, 17]));
            settings::set(ctx.conn(), &colours)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "recorded tagging corpus",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let tags = manage.get_tags();
    let child = tags
        .iter()
        .position(|row| row.text.starts_with("parity:amber old ("))
        .unwrap();
    assert_eq!(
        tags.row_data(child + 1).unwrap().text,
        "    category:colour"
    );
    assert_eq!(
        tags.row_data(child + 1).unwrap().colour,
        slint::Color::from_rgb_u8(3, 5, 7)
    );
    assert_eq!(tags.row_data(child + 2).unwrap().text, "    series:root");
    assert_eq!(
        tags.row_data(child + 2).unwrap().colour,
        slint::Color::from_rgb_u8(11, 13, 17)
    );
    manage.invoke_tag_activated(i32::try_from(child + 1).unwrap());
    choose_add(&manage, &recorded["cases"][15]["parent_activation"]["asked"][0]);
    manage.invoke_cancel();
    ui.invoke_manage_tags_selected();
    let reopened = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        rows(&reopened),
        fixture::canonical(recorded["cases"][15]["rows"].clone())
    );
    let child = reopened
        .get_tags()
        .iter()
        .position(|row| row.text.starts_with("parity:amber old ("))
        .unwrap();
    reopened.invoke_tag_activated(i32::try_from(child + 1).unwrap());
    choose_add(&reopened, &recorded["cases"][15]["parent_activation"]["asked"][0]);
    reopened.invoke_apply();
    ui.invoke_manage_tags_selected();
    let persisted = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        rows(&persisted),
        fixture::canonical(
            recorded["cases"][15]["parent_activation"]["rows_after_activation"].clone()
        )
    );
    assert!(
        !persisted
            .get_tags()
            .iter()
            .any(|row| row.text.starts_with("category:colour (")
                || row.text.starts_with("series:root ("))
    );
    persisted.invoke_cancel();
    // A callback retained after close cannot stage or commit any new mapping.
    persisted.invoke_tag_activated(i32::try_from(child + 1).unwrap());
    persisted.invoke_apply();
    assert!(bound.manage_tags.borrow().is_none());
}
