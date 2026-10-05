//! Actual fleshed-out system values retain their originating Shift activation.
use hydrus_gui_model::{predicate_history::History, search_or::Construction};
use hydrus_search::{Predicate, TextContext, enter_predicates, parse_api_search, predicate_text};
use serde_json::{Value, json};

fn decode(value: &Value) -> Predicate {
    let stored =
        hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
            .unwrap();
    hydrus_legacy::objects::predicates::predicate(&stored).unwrap()
}
fn predicates(value: &Value) -> Vec<Predicate> {
    value.as_array().unwrap().iter().map(decode).collect()
}
fn assert_stage(draft: &Construction, search: &[Predicate], actual: &Value, case: &Value) {
    assert_eq!(search, predicates(&actual["predicates"]), "{case}");
    let expected_draft = actual["draft"]
        .as_array()
        .map(|values| values.iter().map(decode).collect::<Vec<_>>());
    assert_eq!(draft.terms(), expected_draft.as_deref(), "{case}");
}
#[test]
fn actual_system_activation_replays_shift_cancel_drafts_and_outer_acceptance() {
    let recording = hydrus_testkit::fixture_json("system_or_activation.json");
    let text = TextContext::default();
    let alpha = parse_api_search(&json!([recording["tag"]])).unwrap();
    for case in recording["cases"].as_array().unwrap() {
        let mut draft = Construction::default();
        let mut search = Vec::new();
        let mut history = History::default();
        if case["seed_draft"] == true {
            assert!(draft.broadcast(alpha.clone(), true, &text).is_empty());
        }
        assert_stage(&draft, &search, &case["before"], case);
        if case["accepted"] == true {
            let committed = draft.broadcast(
                predicates(&case["system_predicates"]),
                case["shift"].as_bool().unwrap(),
                &text,
            );
            enter_predicates(&mut search, &committed, &text);
        }
        assert_stage(&draft, &search, &case["after_system"], case);
        if case["owner"] == "main" {
            history.record(&[], &search);
            assert_stage(&draft, &search, &case["after_parent"], case);
        } else {
            if case["outer_accepted"] != true {
                search.clear();
            }
            history.record(&[], &search);
            assert_stage(
                &Construction::default(),
                &search,
                &case["after_parent"],
                case,
            );
        }
        let added: Vec<_> = history
            .added
            .iter()
            .map(|p| predicate_text(p, &text))
            .collect();
        assert_eq!(json!(added), case["after_parent"]["added"], "{case}");
        assert!(history.removed.is_empty());
        assert_eq!(case["after_parent"]["removed"], json!([]));
    }
}
