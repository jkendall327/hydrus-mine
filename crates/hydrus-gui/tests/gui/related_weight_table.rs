//! Actual Qt table sorting/selection replay through an owned detached weight child.
use hydrus_gui::{RelatedWeightsWindow, headless, related_weights_window};
use hydrus_store::related_tags::Weights;
use slint::Model as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn snapshot(window: &RelatedWeightsWindow) -> (serde_json::Value, serde_json::Value) {
    let rows = window.get_rows();
    let display: Vec<Vec<String>> = rows
        .iter()
        .map(|row| row.cells.iter().map(|cell| cell.to_string()).collect())
        .collect();
    let selected: Vec<Vec<String>> = rows
        .iter()
        .filter(|row| row.selected)
        .map(|row| row.cells.iter().map(|cell| cell.to_string()).collect())
        .collect();
    (serde_json::json!(display), serde_json::json!(selected))
}
fn row_index(window: &RelatedWeightsWindow, label: &str) -> i32 {
    i32::try_from(
        window
            .get_rows()
            .iter()
            .position(|row| row.cells.row_data(0).unwrap() == label)
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn owned_weight_headers_and_table_switches_replay_qt_without_changing_cancelled_settings() {
    let _windows = headless::init();
    let f = hydrus_testkit::fixture_json("related_weight_table.json");
    let original = Weights {
        search: serde_json::from_value(f["initial"].clone()).unwrap(),
        result: serde_json::from_value(f["initial_result"].clone()).unwrap(),
    };
    let slot = Rc::new(RefCell::new(None));
    let parent = Rc::new(Cell::new(true));
    let accepted = Rc::new(RefCell::new(None));
    let window = related_weights_window::open(&slot, &original, parent.clone(), {
        let accepted = accepted.clone();
        Rc::new(move |weights| {
            accepted.borrow_mut().replace(weights);
        })
    })
    .unwrap();
    for event in f["events"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "initial" => {}
            "select both" => {
                window.invoke_clicked(row_index(&window, "'alpha' tags"), false, false);
                window.invoke_choose(true);
                window.invoke_clicked(row_index(&window, "'Zulu' tags"), false, false);
                window.invoke_choose(false);
            }
            "search weight ascending" => window.invoke_sort(1, true),
            "search weight descending" => window.invoke_sort(1, false),
            "add search replaces selection" => {
                window.invoke_action("add".into());
                window.set_namespace("bravo".into());
                window.invoke_action("accept-question".into());
                let before = snapshot(&window);
                window.invoke_sort(0, true);
                assert_eq!(
                    snapshot(&window),
                    before,
                    "a question freezes header sorting"
                );
                window.set_weight(200);
                window.invoke_action("accept-question".into());
            }
            "result slice descending" => {
                window.invoke_choose(true);
                window.invoke_sort(0, false);
                window.invoke_choose(false);
            }
            "search slice ascending" => window.invoke_sort(0, true),
            "search slice descending" => window.invoke_sort(0, false),
            action => panic!("unhandled recorded action: {action}"),
        }
        let (display, selected) = snapshot(&window);
        assert_eq!(display, event["search_display"], "{}", event["action"]);
        let expected_selected: Vec<_> = event["search_display"]
            .as_array()
            .unwrap()
            .iter()
            .zip(event["search"].as_array().unwrap())
            .filter_map(|(display, row)| {
                event["search_selected"]
                    .as_array()
                    .unwrap()
                    .contains(row)
                    .then_some(display.clone())
            })
            .collect();
        assert_eq!(selected, serde_json::json!(expected_selected));
        let sort = (window.get_sort_column(), window.get_ascending());
        window.invoke_choose(true);
        let (display, selected) = snapshot(&window);
        assert_eq!(display, event["result_display"]);
        let expected_selected: Vec<_> = event["result_display"]
            .as_array()
            .unwrap()
            .iter()
            .zip(event["result"].as_array().unwrap())
            .filter_map(|(display, row)| {
                event["result_selected"]
                    .as_array()
                    .unwrap()
                    .contains(row)
                    .then_some(display.clone())
            })
            .collect();
        assert_eq!(selected, serde_json::json!(expected_selected));
        window.invoke_choose(false);
        assert_eq!((window.get_sort_column(), window.get_ascending()), sort);
    }
    window.invoke_action("cancel".into());
    assert!(slot.borrow().is_none() && accepted.borrow().is_none());
    let retired = snapshot(&window);
    window.invoke_sort(1, true);
    assert_eq!(snapshot(&window), retired);
    let reopened = related_weights_window::open(&slot, &original, parent.clone(), {
        let accepted = accepted.clone();
        Rc::new(move |weights| {
            accepted.borrow_mut().replace(weights);
        })
    })
    .unwrap();
    assert_eq!(snapshot(&reopened).0, f["events"][0]["search_display"]);
    reopened.invoke_sort(1, false);
    reopened.invoke_action("apply".into());
    assert!(slot.borrow().is_none());
    let saved = accepted.borrow().as_ref().unwrap().clone();
    assert_eq!(serde_json::json!(saved.result), f["events"][0]["result"]);
    assert_eq!(
        saved
            .search
            .iter()
            .map(|(_, weight)| *weight)
            .collect::<Vec<_>>(),
        [400, 400, 200, 200, 100]
    );
    let stale = snapshot(&reopened);
    reopened.invoke_sort(0, true);
    assert_eq!(snapshot(&reopened), stale);
}
