//! Actual Options owner callbacks preserve Cancel, Apply and store reopen.
use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_store::settings::{self, DuplicateColourSettings};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn options(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|line| line.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    bound.options.borrow().as_ref().unwrap().clone_strong()
}
fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    let window = options(ui, bound);
    let page = window
        .get_pages()
        .iter()
        .position(|page| page.text == "duplicates")
        .unwrap();
    window.set_page(i32::try_from(page).unwrap());
    window.invoke_page_chosen(i32::try_from(page).unwrap());
    window
}
fn values(settings: &DuplicateColourSettings) -> Value {
    json!([
        settings.intensity_a,
        settings.intensity_b,
        settings.checkerboard
    ])
}
fn edit(window: &OptionsWindow, fixture: &Value, input: &Value) {
    for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
        let row = i32::try_from(
            window
                .get_rows()
                .iter()
                .position(|row| row.label == label.as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        if index == 2 {
            window.invoke_check_toggled(row, input[index].as_bool().unwrap());
        } else {
            window.invoke_none_toggled(row, input[index].is_null());
            if let Some(number) = input[index].as_i64() {
                window.invoke_number_edited(row, i32::try_from(number).unwrap());
            }
        }
    }
}
#[test]
fn options_replay_cancel_retired_apply_bounds_and_persisted_reopen() {
    let fixture = hydrus_testkit::fixture_json("duplicate_colours.json");
    let (_directories, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(hydrus_gui::SearchPage::new(store.clone())),
    );
    assert_eq!(
        values(
            &store
                .read(settings::get::<DuplicateColourSettings>)
                .unwrap()
        ),
        fixture["initial"]
    );
    let cancelled = options(&ui, &bound);
    cancelled.invoke_cancel();
    assert_eq!(
        values(
            &store
                .read(settings::get::<DuplicateColourSettings>)
                .unwrap()
        ),
        fixture["initial"]
    );
    let unvisited = options(&ui, &bound);
    unvisited.invoke_apply();
    assert_eq!(
        values(
            &store
                .read(settings::get::<DuplicateColourSettings>)
                .unwrap()
        ),
        fixture["initial_displayed"],
        "Apply normalises the eagerly created spin box even on another page"
    );
    for event in fixture["options"].as_array().unwrap() {
        let before: DuplicateColourSettings = store.read(settings::get).unwrap();
        let cancelled = open(&ui, &bound);
        edit(&cancelled, &fixture, &event["input"]);
        cancelled.invoke_cancel();
        let accepted = open(&ui, &bound);
        cancelled.invoke_apply();
        assert_eq!(
            store
                .read(settings::get::<DuplicateColourSettings>)
                .unwrap(),
            before
        );
        assert!(
            bound.options.borrow().is_some(),
            "retired owner cannot close successor"
        );
        edit(&accepted, &fixture, &event["input"]);
        accepted.invoke_apply();
        let saved: DuplicateColourSettings = hydrus_store::Store::open(store.dir())
            .unwrap()
            .read(settings::get)
            .unwrap();
        assert_eq!(values(&saved), event["saved"]);
        let reopened = open(&ui, &bound);
        for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
            let row = reopened
                .get_rows()
                .iter()
                .find(|row| row.label == label.as_str().unwrap())
                .unwrap();
            if index == 2 {
                assert_eq!(json!(row.checked), event["reopened"][index]);
            } else if event["reopened"][index].is_null() {
                assert!(row.is_none);
            } else {
                assert!(!row.is_none);
                assert_eq!(json!(row.number), event["reopened"][index]);
            }
        }
        reopened.invoke_cancel();
    }
    ui.hide().unwrap();
    drop(bound);
    drop(ui);
    drop(windows);
}
