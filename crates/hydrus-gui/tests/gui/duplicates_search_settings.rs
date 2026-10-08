//! The potential duplicates search's settings read by the search itself:
//! each search type, pixel dupe preference, maximum distance and pair of file
//! searches is set on the duplicates page's filtering tab, and the count it
//! shows is what the reference finds for the same settings on the same
//! database (`oracle/record_potential_duplicates_search.py`).

use std::time::{Duration, Instant};

use hydrus_core::numbers::human_int;

use crate::auto_resolution_review::opened;

fn await_count(ui: &hydrus_gui::MainWindow, expected: &str) {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        slint::platform::update_timers_and_animations();
        let now = ui.get_duplicates_filtering().count.to_string();
        if now == expected {
            return;
        }
        assert!(
            Instant::now() < until,
            "the count never said {expected:?}; it says {now:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

// leaf: audit-media-duplicate-search-kind, audit-media-duplicate-search-pixels, audit-media-duplicate-search-predicates
#[test]
fn every_search_setting_finds_the_pairs_the_reference_finds() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("potential_duplicates_search.json");
    let results = recorded["results"].as_array().unwrap();
    assert_eq!(results.len(), 54);
    for r in results {
        let o = opened();
        let ui = &o.ui;
        let at = format!("{r}");
        // the kind and the pixel preference by the order of their choices,
        // as the reference lists them
        ui.invoke_duplicates_filtering_action("kind".into(), r["kind"].as_i64().unwrap() as i32);
        ui.invoke_duplicates_filtering_action("pixel".into(), r["pixel"].as_i64().unwrap() as i32);
        ui.invoke_duplicates_filtering_action(
            "distance".into(),
            r["distance"].as_i64().unwrap() as i32,
        );
        for (which, input) in [("1", "one"), ("2", "two")] {
            for line in r[input].as_array().unwrap() {
                if which == "1" {
                    ui.set_duplicates_filter_input_1(line.as_str().unwrap().into());
                } else {
                    ui.set_duplicates_filter_input_2(line.as_str().unwrap().into());
                }
                ui.invoke_duplicates_filtering_action(format!("add {which}").into(), 0);
            }
        }
        let expected = format!(
            "{} pairs searched; {} match",
            human_int(r["space"].as_u64().unwrap()),
            human_int(r["matches"].as_u64().unwrap())
        );
        // (a search with no pairs is worded another way)
        let expected = if r["space"] == 0 {
            "no potential pairs in this file domain!".to_owned()
        } else {
            expected
        };
        let settled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            await_count(ui, &expected);
        }));
        assert!(settled.is_ok(), "{at}");
    }
}
