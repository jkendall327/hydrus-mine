//! A duplicates page's preparation tab against the reference's, recorded
//! by `oracle/record_duplicates_preparation.py`: what it says of the
//! similar files search for many sets of searched files, search distances
//! and the "hide when caught up" option, and its distance button's labels.

use std::collections::BTreeMap;

use serde_json::json;

use hydrus_gui_model::duplicates_page::{distance_label, preparation};

#[test]
fn the_preparation_tab_says_what_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("duplicates_preparation.json");
    for case in recorded["cases"].as_array().unwrap() {
        let searched: BTreeMap<Option<u32>, usize> = case["counts"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(d, n)| {
                let d: i64 = d.parse().unwrap();
                (u32::try_from(d).ok(), n.as_u64().unwrap() as usize)
            })
            .collect();
        let distance = case["distance"].as_u64().unwrap() as u32;
        let ours = preparation(&searched, distance, case["hide"].as_bool().unwrap());
        let ours = json!({
            "eligible": ours.eligible,
            "searched": ours.searched,
            "gauge": [ours.gauge.0, ours.gauge.1],
            "can_start": ours.can_start,
            "page_name": ours.page_name,
        });
        for key in ["eligible", "searched", "gauge", "can_start", "page_name"] {
            assert_eq!(ours[key], case[key], "{key} of {case}");
        }
    }
    for (distance, label) in recorded["distance_labels"].as_object().unwrap() {
        assert_eq!(distance_label(distance.parse().unwrap()), label);
    }
}

#[test]
fn the_auto_resolution_rules_rows_are_the_references() {
    use hydrus_gui_model::duplicates_page::{RULE_COLUMNS, rule_progress, rule_status};
    use hydrus_store::duplicates::auto::{OperationMode, PairStatus};

    let recorded = hydrus_testkit::fixture_json("auto_resolution_rows.json");
    assert_eq!(recorded["columns"], json!(RULE_COLUMNS));
    for row in recorded["rows"].as_array().unwrap() {
        let counts: BTreeMap<PairStatus, u64> = row["counts"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(s, n)| {
                (
                    PairStatus::from_code(s.parse().unwrap()).unwrap(),
                    n.as_u64().unwrap(),
                )
            })
            .collect();
        let mut rule = crate::duplicates_page::a_rule();
        rule.paused = row["paused"].as_bool().unwrap();
        match row["max_pending"].as_u64() {
            None => rule.mode = OperationMode::FullyAutomatic,
            Some(most) => {
                rule.mode = OperationMode::SemiAutomatic;
                rule.max_pending_pairs = (most > 0).then_some(most as u32);
            }
        }
        assert_eq!(rule_progress(&counts), row["progress"], "{row}");
        assert_eq!(
            rule_status(&rule, &counts, row["can_work"].as_bool().unwrap()),
            row["status"],
            "{row}"
        );
    }
}

/// A rule, its settings to be set.
pub(crate) fn a_rule() -> hydrus_store::duplicates::auto::Rule {
    use hydrus_store::duplicates::auto::{OperationMode, Rule, RuleAction};
    Rule {
        name: "a rule".into(),
        paused: false,
        mode: OperationMode::FullyAutomatic,
        max_pending_pairs: None,
        search: hydrus_core::duplicates::DuplicatesSearch {
            search_1: hydrus_core::search::context::FileSearchContext::default(),
            search_2: hydrus_core::search::context::FileSearchContext::default(),
            kind: hydrus_core::duplicates::PairSearchKind::OneFileMatchesOneSearch,
            pixel_duplicates: hydrus_core::duplicates::PixelDuplicates::Allowed,
            max_hamming_distance: 8,
        },
        comparators: Vec::new(),
        action: RuleAction::Better,
        delete_a: false,
        delete_b: true,
        custom_merge: None,
    }
}
