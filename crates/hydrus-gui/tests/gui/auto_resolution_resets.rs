//! The auto-resolution tab's reset buttons against the reference's, replaying
//! `oracle/fixtures/auto_resolution_resets.json`
//! (`oracle/record_auto_resolution_resets.py`: the real panel on the database
//! the reference's auto-resolution run left, buttons pressed with rules
//! selected or none, the question answered yes or no): the question asked and
//! every rule's pair counts by status after each step.

use std::collections::BTreeMap;

use serde_json::Value;
use slint::Model as _;

use crate::auto_resolution_review::opened;
use hydrus_store::Store;
use hydrus_store::duplicates::auto;

/// Each rule's pairs by status, as the recording has them.
fn counts(store: &Store) -> BTreeMap<String, BTreeMap<String, i64>> {
    let rules = store.read(auto::rules).unwrap();
    rules
        .into_iter()
        .map(|(id, rule)| {
            let by_status = store.read(move |c| auto::counts(c, id)).unwrap();
            (
                rule.name,
                by_status
                    .into_iter()
                    .filter(|(_, n)| *n > 0)
                    .map(|(s, n)| (s.code().to_string(), i64::try_from(n).unwrap()))
                    .collect(),
            )
        })
        .collect()
}

fn recorded_counts(value: &Value) -> BTreeMap<String, BTreeMap<String, i64>> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, by_status)| {
            (
                name.clone(),
                by_status
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(s, n)| (s.clone(), n.as_i64().unwrap()))
                    .collect(),
            )
        })
        .collect()
}

// leaf: audit-media-rule-sidebar-resets
#[test]
fn the_reset_buttons_ask_and_move_pairs_as_the_reference_does() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_resets.json");
    let o = opened();
    let (ui, store) = (&o.ui, &o.store);
    // the same database to begin with
    assert_eq!(
        counts(store),
        recorded_counts(&recorded["start"]),
        "the starting pair counts"
    );

    let names = |ui: &hydrus_gui::MainWindow| -> Vec<String> {
        let rows = ui.get_duplicates_rules();
        (0..rows.row_count())
            .map(|r| {
                rows.row_data(r)
                    .unwrap()
                    .cells
                    .row_data(0)
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    for step in recorded["steps"].as_array().unwrap() {
        let button = step["button"].as_str().unwrap();
        let action = match button {
            "reset_search" => "reset search",
            "reset_test" => "reset test",
            "reset_denied" => "reset denied",
            // the reference's regen numbers and resync buttons for the rules are
            // not ported (docs/rust/DIFFERENCES.md); the counts stay as they were
            _ => continue,
        };
        // the selection: the named rules, or none
        let listed = names(ui);
        if let Some(selected) = step["selected"].as_array() {
            // (clear what is selected, then select each named rule)
            for (i, _) in listed.iter().enumerate() {
                let rows = ui.get_duplicates_rules();
                if rows.row_data(i).unwrap().selected {
                    ui.invoke_duplicates_action("rule".into(), i as i32, true, false);
                }
            }
            for name in selected {
                let at = listed
                    .iter()
                    .position(|n| n == name.as_str().unwrap())
                    .unwrap();
                ui.invoke_duplicates_action("rule".into(), at as i32, true, false);
            }
        } else {
            for (i, _) in listed.iter().enumerate() {
                let rows = ui.get_duplicates_rules();
                if rows.row_data(i).unwrap().selected {
                    ui.invoke_duplicates_action("rule".into(), i as i32, true, false);
                }
            }
        }
        ui.invoke_duplicates_action(action.into(), 0, false, false);
        let asked = step["asked"].as_array().unwrap();
        assert_eq!(asked.len(), 1, "{step}");
        let question = ui.get_duplicates().asking_message.to_string();
        assert_eq!(
            question,
            asked[0]["message"].as_str().unwrap(),
            "{button} {}",
            step["selected"]
        );
        if step["answer"] == "yes" {
            ui.invoke_duplicates_action("chosen".into(), 0, false, false);
        } else {
            ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
        }
        assert_eq!(
            counts(store),
            recorded_counts(&step["counts"]),
            "after {button} on {} answered {}",
            step["selected"],
            step["answer"]
        );
    }
}
