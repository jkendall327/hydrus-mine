//! A middle press on a search page's collect control clears collecting, as
//! the reference's (`oracle/record_collect_middle_click.py`: a real page's
//! `MediaCollectControl` pressed with a real middle button): no namespaces
//! or ratings, files that match none collected or not as before, the tag
//! context back to its default, every file shown again. A left press only
//! opens the choices.

use slint::platform::PointerEventButton;
use slint::{ComponentHandle as _, Model as _};

use super::common::widgets;
use super::media_support::*;

// leaf: interaction-collect-control-middle-click
#[test]
fn a_middle_press_on_the_collect_control_stops_collecting() {
    let recorded = hydrus_testkit::fixture_json("collect_middle_click.json");
    let fixture = start();
    let page = fixture.bound.current.borrow().clone();
    assert_eq!(
        page.borrow().files().len() as u64,
        recorded["files"].as_u64().unwrap(),
        "the page's files are the recording's"
    );
    let ui = &fixture.ui;
    widgets::lay_out(ui.window(), 1200.0, 800.0);
    let series = {
        let rows = ui.get_collect_choices();
        (0..rows.row_count())
            .position(|i| rows.row_data(i).unwrap().name == "series")
            .map(|i| i32::try_from(i).unwrap())
            .unwrap()
    };
    for case in recorded["cases"].as_array().unwrap() {
        let what = format!("{case}");
        let before = &case["before"];
        let after = &case["after"];
        // collect by series, unmatched files as the case has them
        if page.borrow().collect().collects() {
            ui.invoke_collect_toggled(series, false);
        }
        ui.invoke_collect_unmatched_chosen(before["collect"]["unmatched"].as_bool().unwrap());
        ui.invoke_collect_toggled(series, true);
        ui.set_collect_open(false);
        assert_eq!(
            ui.get_collect_label(),
            before["label"].as_str().unwrap(),
            "{what}"
        );
        assert_eq!(
            page.borrow().results().len() as u64,
            before["media"].as_u64().unwrap(),
            "{what}: collected"
        );
        let at = widgets::position(ui.window(), before["label"].as_str().unwrap(), 0);
        let button = match case["button"].as_str().unwrap() {
            "middle" => PointerEventButton::Middle,
            _ => PointerEventButton::Left,
        };
        widgets::click_at(ui.window(), at, button);
        let collect = page.borrow().collect().clone();
        assert_eq!(
            collect.namespaces,
            after["collect"]["namespaces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{what}"
        );
        assert!(collect.ratings.is_empty(), "{what}");
        assert_eq!(
            collect.collect_unmatched,
            after["collect"]["unmatched"].as_bool().unwrap(),
            "{what}"
        );
        assert_eq!(
            ui.get_collect_label(),
            after["label"].as_str().unwrap(),
            "{what}"
        );
        assert_eq!(
            page.borrow().results().len() as u64,
            after["media"].as_u64().unwrap(),
            "{what}: shown"
        );
        if case["button"] == "middle" {
            // (the tag context back to the default: all known tags, current
            // and pending)
            let context = &after["collect"]["tag_context"];
            assert_eq!(
                collect.tag_context,
                hydrus_core::search::context::TagContext::default(),
                "{what}"
            );
            assert!(context["include_current"].as_bool().unwrap());
            assert!(context["include_pending"].as_bool().unwrap());
            assert!(!ui.get_collect_open(), "{what}: the choices stay shut");
        } else {
            assert!(ui.get_collect_open(), "{what}: the choices open");
        }
    }
}
