//! The display/search window's autocomplete checkboxes put through the
//! reference's recording (`oracle/fixtures/tag_display.json`,
//! `autocomplete_steps`) as a user would: the service chosen, a box clicked,
//! and the others read back (ticked, and enabled), to see the reference's
//! interlocks.

use hydrus_gui::{MainWindow, Pages, SearchPage, TagDisplayWindow, bind, headless};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};

use crate::common::widgets;

const SEARCH: &str = "Search namespaces with normal input";
const ANY: &str = "Unnamespaced input gives (any namespace) wildcard results";
const BARE: &str = "Allow namespace:";
const STAR: &str = "Allow namespace:*";
const ALL: &str = "Allow *";
const AUTOMATIC: &str = "Fetch results as you type";

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
    let win = window.window();
    widgets::lay_out(win, 900.0, 900.0);
    let steps = recorded["autocomplete_steps"].as_array().unwrap();
    for (i, step) in steps.iter().enumerate() {
        match step["step"].as_str().unwrap() {
            "default" => {}
            "search namespaces" => widgets::click(win, SEARCH),
            "any namespace" => widgets::click(win, ANY),
            "manual fetch, always autocomplete" => {
                // (0 is the window's "always autocomplete", the reference's
                // None)
                window.set_threshold(0);
                widgets::click(win, AUTOMATIC);
            }
            other => panic!("{other}"),
        }
        let v = &step["value"];
        let context = format!("step {i}: {}", step["step"]);
        let ticked: Vec<bool> = [SEARCH, ANY, BARE, STAR, ALL, AUTOMATIC]
            .iter()
            .map(|label| widgets::checked(win, label))
            .collect();
        let theirs: Vec<bool> = (4..=9).map(|n| v[n].as_bool().unwrap()).collect();
        assert_eq!(ticked, theirs, "{context}");
        assert_eq!(
            [widgets::enabled(win, BARE), widgets::enabled(win, STAR)],
            [
                step["namespace_enabled"][0].as_bool().unwrap(),
                step["namespace_enabled"][1].as_bool().unwrap()
            ],
            "{context}"
        );
        // (and what the window holds)
        assert_eq!(window.get_search_namespaces(), theirs[0], "{context}");
        assert_eq!(window.get_any_namespace(), theirs[1], "{context}");
        assert_eq!(window.get_bare_fetch(), theirs[2], "{context}");
        assert_eq!(window.get_namespace_fetch(), theirs[3], "{context}");
        assert_eq!(window.get_fetch_all(), theirs[4], "{context}");
        assert_eq!(window.get_fetch_automatically(), theirs[5], "{context}");
        match v[10].as_i64() {
            Some(threshold) => {
                assert_eq!(i64::from(window.get_threshold()), threshold, "{context}");
            }
            None => assert_eq!(window.get_threshold(), 0, "{context}"),
        }
    }
    window.invoke_cancel();
}
