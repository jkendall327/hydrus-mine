//! The display/search window's autocomplete checkboxes put through the
//! reference's recording (`oracle/fixtures/tag_display.json`,
//! `autocomplete_steps`) as a user would: the service chosen, a box ticked,
//! and the others read back, to see the reference's interlocks.

use hydrus_gui::{MainWindow, Pages, SearchPage, TagDisplayWindow, bind, headless};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound) -> TagDisplayWindow {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let row = pane
        .lines
        .iter()
        .position(|r| r.label.starts_with("display/search"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
    bound.tag_display.borrow().as_ref().unwrap().clone_strong()
}

// leaf: audit-media-tag-display-namespace
#[test]
fn the_autocomplete_boxes_interlock_as_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("tag_display.json");
    let _windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store)));
    let window = open(&ui, &bound);
    let service = recorded["services"][0].as_str().unwrap();
    let at = window
        .get_services()
        .iter()
        .position(|s| s == service)
        .unwrap();
    window.invoke_service_chosen(i32::try_from(at).unwrap());
    let steps = recorded["autocomplete_steps"].as_array().unwrap();
    for (i, step) in steps.iter().enumerate() {
        match step["step"].as_str().unwrap() {
            "default" => {}
            "search namespaces" => window.invoke_rule_changed(0, true),
            "any namespace" => window.invoke_rule_changed(1, true),
            "manual fetch, always autocomplete" => {
                window.set_fetch_automatically(false);
                window.invoke_options_changed();
            }
            other => panic!("{other}"),
        }
        let v = &step["value"];
        let context = format!("step {i}: {}", step["step"]);
        assert_eq!(
            window.get_search_namespaces(),
            v[4].as_bool().unwrap(),
            "{context}"
        );
        assert_eq!(
            window.get_any_namespace(),
            v[5].as_bool().unwrap(),
            "{context}"
        );
        assert_eq!(
            window.get_bare_fetch(),
            v[6].as_bool().unwrap(),
            "{context}"
        );
        assert_eq!(
            window.get_namespace_fetch(),
            v[7].as_bool().unwrap(),
            "{context}"
        );
        assert_eq!(window.get_fetch_all(), v[8].as_bool().unwrap(), "{context}");
        assert_eq!(
            window.get_fetch_automatically(),
            v[9].as_bool().unwrap(),
            "{context}"
        );
        match v[10].as_i64() {
            Some(threshold) => {
                assert_eq!(i64::from(window.get_threshold()), threshold, "{context}");
            }
            // fetching by hand, the character threshold means nothing (the
            // reference's is None); the window keeps its last number but
            // fetching automatically is off
            None => assert!(!window.get_fetch_automatically(), "{context}"),
        }
    }
    window.invoke_cancel();
}
