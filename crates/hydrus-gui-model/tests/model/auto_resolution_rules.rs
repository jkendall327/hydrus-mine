//! The "edit rules" list and the rule editor's comparator summaries
//! against the reference's, recorded by
//! `oracle/record_auto_resolution_summaries.py`: the reference's suggested
//! rules and rules using every comparator, each decoded from the stored
//! form the reference gives it.

use serde_json::{Value as Json, json};

use hydrus_gui_model::auto_resolution_rules::{
    can_determine_better, comparator_summary, row, rule_can_determine_better,
};
use hydrus_legacy::objects::auto_resolution::AutoResolutionRule;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_search::TextContext;

fn recorded() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../oracle/fixtures/auto_resolution_summaries.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn rules_rows_and_comparators_are_the_references() {
    let context = TextContext::default();
    for case in recorded()["rules"].as_array().unwrap() {
        let stored = SerialisableObject::from_tuple_str(&case["stored"].to_string()).unwrap();
        let legacy = AutoResolutionRule::from_object(&stored).unwrap();
        let rule = hydrus_store::import::auto_resolution_rule(&legacy, &|_| None).unwrap();
        let progress = case["row"][4].as_str().unwrap();
        assert_eq!(
            json!(row(&rule, progress, &context)),
            case["row"],
            "{}",
            rule.name
        );
        // (each comparator, in the rule's order)
        let ours: Vec<Json> = rule
            .comparators
            .iter()
            .map(|c| json!([comparator_summary(c, &context), can_determine_better(c)]))
            .collect();
        assert_eq!(Json::Array(ours), case["comparators"], "{}", rule.name);
        assert_eq!(
            json!(rule_can_determine_better(&rule.comparators)),
            case["can_determine_better"],
            "{}",
            rule.name
        );
    }
}
