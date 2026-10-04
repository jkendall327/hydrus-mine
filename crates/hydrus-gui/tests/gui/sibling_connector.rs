//! Options edits reach actual native storage and shared write-tag lists after reopening.
use super::tag_dialog_preferences::fixture;
use hydrus_core::{ServiceKey, tag_presentation::TagPresentation};
use hydrus_gui::{MainWindow, OptionsWindow, Pages, SearchPage, WriteTagsWindow, bind, headless};
use hydrus_store::settings;
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
use std::rc::Rc;

fn options(ui: &MainWindow) {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "file")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let row = ui
        .get_menu_panes()
        .row_data(0)
        .unwrap()
        .lines
        .iter()
        .position(|row| row.label.starts_with("options"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
}
fn edit(window: &OptionsWindow, text: &str) {
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "tag presentation")
        .unwrap();
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    let index = window
        .get_rows()
        .iter()
        .position(|row| row.label == "Sibling connecting string: ")
        .unwrap();
    window.invoke_text_edited(i32::try_from(index).unwrap(), text.into());
}
fn source(window: &WriteTagsWindow, tags: bool, label: &str) {
    window.invoke_domain_menu(tags, 0.0, 0.0);
    let rows = window.get_tag_menu_panes().row_data(0).unwrap().lines;
    let index = rows.iter().position(|row| row.label == label).unwrap();
    window.invoke_tag_menu_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
}
fn labels(rows: &Value) -> Vec<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .flat_map(|row| {
            row["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|label| label.as_str().unwrap().to_owned())
        })
        .collect()
}

#[test]
fn persisted_custom_empty_and_unicode_connectors_render_without_changing_tags() {
    let recorded = hydrus_testkit::fixture_json("sibling_connector.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    fixture::set_preferences(
        &store,
        &json!({"listbook":false,"parents":true,"expanded":true,"siblings":true}),
    );
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "connector corpus",
            None,
            files,
        )),
    );
    ui.invoke_select_all();
    let before: TagPresentation = store.read(settings::get).unwrap();
    options(&ui);
    let dialog = bound.options.borrow().as_ref().unwrap().clone_strong();
    edit(&dialog, recorded["options"]["applied"].as_str().unwrap());
    assert_eq!(
        store.read::<TagPresentation>(settings::get).unwrap(),
        before
    );
    dialog.invoke_cancel();
    assert_eq!(
        store
            .read::<TagPresentation>(settings::get)
            .unwrap()
            .sibling_connector,
        recorded["options"]["cancelled"]
    );
    let key = ServiceKey::from_hex(recorded["tag_service"].as_str().unwrap()).unwrap();
    let tag_label = store.snapshot().services.by_key(&key).unwrap().name.clone();
    let file_key = ServiceKey::from_hex(recorded["file_context"][0].as_str().unwrap()).unwrap();
    let file_label = store
        .snapshot()
        .services
        .by_key(&file_key)
        .unwrap()
        .name
        .clone();
    for case in recorded["cases"].as_array().unwrap() {
        options(&ui);
        let dialog = bound.options.borrow().as_ref().unwrap().clone_strong();
        let prior: TagPresentation = store.read(settings::get).unwrap();
        edit(&dialog, case["connector"].as_str().unwrap());
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), prior);
        dialog.invoke_apply();
        assert_eq!(
            store
                .read::<TagPresentation>(settings::get)
                .unwrap()
                .sibling_connector,
            case["connector"]
        );
        ui.invoke_manage_tags_selected();
        let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        let actual: Vec<String> = manage
            .get_tags()
            .iter()
            .filter(|row| row.text.starts_with("parity:connector"))
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(actual, labels(&case["storage"]));
        manage.invoke_cancel();
        let slot = hydrus_gui::write_tag_window::Slot::default();
        let child = hydrus_gui::write_tag_window::open(
            &store,
            key.clone(),
            &[],
            "edit tags",
            &slot,
            Rc::new(|_| {}),
            Rc::new(|| {}),
        )
        .unwrap();
        source(&child, true, &tag_label);
        source(&child, false, &file_label);
        child.invoke_edited(recorded["query"].as_str().unwrap().into());
        child.invoke_fetch();
        let actual: Vec<String> = child
            .get_suggestions()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(actual, labels(&case["write"]));
        assert_eq!(child.get_tags().row_count(), 0);
        child.invoke_cancel();
        assert!(slot.borrow().is_none());
        // A cancelled replacement connector cannot affect the next editor's labels.
        options(&ui);
        let dialog = bound.options.borrow().as_ref().unwrap().clone_strong();
        edit(&dialog, "unapplied replacement");
        dialog.invoke_cancel();
        ui.invoke_manage_tags_selected();
        let reopened = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        let actual: Vec<String> = reopened
            .get_tags()
            .iter()
            .filter(|row| row.text.starts_with("parity:connector"))
            .map(|row| row.text.to_string())
            .collect();
        assert_eq!(actual, labels(&case["storage"]));
        reopened.invoke_cancel();
    }
}
