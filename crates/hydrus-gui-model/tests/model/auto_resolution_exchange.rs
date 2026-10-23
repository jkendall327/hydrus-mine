//! The auto-resolution rules list's export and import: exports are the
//! reference's serialised form (compared with its own exports,
//! `oracle/fixtures/rules_exchange.json`) and import again, and the
//! reference's own serialised rules (its suggested rules) are read, alone or
//! in a list.
use hydrus_gui_model::auto_resolution_exchange::{
    added, export_text, import_text, refused_message,
};
use hydrus_store::duplicates::auto::suggested_rules;

/// What the encoder looks up, in a client with no services.
fn plain() -> hydrus_gui_model::auto_resolution_exchange::Context<'static> {
    hydrus_gui_model::auto_resolution_exchange::Context {
        scales: &|_| None,
        service_key: &|_| None,
        service_type: &|_| None,
        default_canvases: &[],
    }
}

#[test]
fn exported_rules_import_again() {
    let rules = suggested_rules();
    let text = export_text(&[(None, &rules[0]), (Some(7), &rules[1])], &plain()).unwrap();
    let imported = import_text(&text, &|_| None).unwrap();
    assert_eq!(imported.rules, rules[..2]);
    assert!(imported.refused.is_empty());
    assert_eq!(added(2), "2 objects added!");
    assert!(import_text("not a rule", &|_| None).is_err());
}

#[test]
fn the_reference_s_serialised_rules_import() {
    let stored: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../hydrus-store/src/duplicates/suggested_rules.json"
    ))
    .unwrap();
    let one = import_text(&stored[0].to_string(), &|_| None).unwrap();
    assert_eq!(one.rules, suggested_rules()[..1]);
    // a SerialisableList of two rules: [26, 3, [[2, rule], [2, rule]]]
    let list = serde_json::json!([26, 3, [[2, stored[0]], [2, stored[1]]]]);
    let two = import_text(&list.to_string(), &|_| None).unwrap();
    assert_eq!(two.rules, suggested_rules()[..2]);
    // something else entirely is refused, by type
    let other = import_text("[26, 3, [[0, 5]]]", &|_| None).unwrap();
    assert!(other.rules.is_empty());
    assert!(refused_message(&other.refused).ends_with("DuplicatesAutoResolutionRule"));
}

/// The serialised types of the reference's `PairComparator` subclasses, read
/// from its source: each class's `SERIALISABLE_TYPE` constant, looked up in
/// `HydrusSerialisable.py`.
fn reference_comparator_types() -> Vec<u16> {
    let comparators = include_str!(
        "../../../../hydrus/client/duplicates/ClientDuplicatesAutoResolutionComparators.py"
    );
    let constants = include_str!("../../../../hydrus/core/HydrusSerialisable.py");
    let mut class = "";
    let mut types = Vec::new();
    for line in comparators.lines() {
        if let Some(rest) = line.strip_prefix("class ") {
            class = rest;
        } else if class.starts_with("PairComparator")
            && let Some(name) = line
                .trim()
                .strip_prefix("SERIALISABLE_TYPE = HydrusSerialisable.")
        {
            let name = name.trim();
            let value = constants
                .lines()
                .find_map(|l| l.strip_prefix(name)?.trim_start().strip_prefix("= "))
                .unwrap_or_else(|| panic!("{name}"));
            types.push(value.trim().parse().unwrap());
        }
    }
    types.sort_unstable();
    types
}

#[test]
fn the_comparator_types_are_the_reference_s_pair_comparators() {
    let mut ours = hydrus_gui_model::auto_resolution_exchange::COMPARATOR_TYPES.to_vec();
    ours.sort_unstable();
    assert_eq!(ours, reference_comparator_types());
    assert_eq!(ours.len(), 7);
}

fn first_comparators(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    if let Some(array) = value.as_array() {
        if array.len() >= 2
            && array[0]
                .as_u64()
                .is_some_and(|t| reference_comparator_types().contains(&u16::try_from(t).unwrap()))
            && array[1].is_u64()
        {
            out.push(value.clone());
            return;
        }
        for item in array {
            first_comparators(item, out);
        }
    }
}

#[test]
fn comparators_export_and_import_alone_in_lists_and_from_the_reference() {
    use hydrus_gui_model::auto_resolution_exchange::{
        COMPARATOR_TYPE, export_comparators_text, import_comparators_text, refused_message_for,
    };
    let rules = suggested_rules();
    let comparators: Vec<_> = rules.iter().flat_map(|r| r.comparators.clone()).collect();
    assert!(comparators.len() >= 2);
    let text = export_comparators_text(&comparators[..2], &plain()).unwrap();
    let back = import_comparators_text(&text, &|_| None).unwrap();
    assert_eq!(back.comparators, comparators[..2]);
    assert!(import_comparators_text("nonsense", &|_| None).is_err());
    // the reference's own comparators, singly and in a list
    let stored: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../hydrus-store/src/duplicates/suggested_rules.json"
    ))
    .unwrap();
    let mut objects = Vec::new();
    for rule in &stored {
        first_comparators(rule, &mut objects);
    }
    assert!(!objects.is_empty());
    let one = import_comparators_text(&objects[0].to_string(), &|_| None).unwrap();
    assert_eq!(one.comparators.len(), 1);
    assert!(one.refused.is_empty());
    let list = serde_json::json!([
        26,
        3,
        objects
            .iter()
            .map(|o| serde_json::json!([2, o]))
            .collect::<Vec<_>>()
    ]);
    let all = import_comparators_text(&list.to_string(), &|_| None).unwrap();
    assert_eq!(all.comparators.len(), objects.len());
    // rules are not comparators: refused, with the reference's wording
    let rule = import_comparators_text(&stored[0].to_string(), &|_| None).unwrap();
    assert!(rule.comparators.is_empty());
    assert_eq!(
        refused_message_for(&rule.refused, COMPARATOR_TYPE),
        format!(
            "The imported objects included these types:\n\nDuplicatesAutoResolutionRule\n\nWhereas this control only allows:\n\nPairComparator"
        )
    );
}

/// The basic fixture's numerical rating service "stars" (5 stars, zero
/// allowed), as `oracle/make_fixture_db.py` makes it.
fn fixture_scales(key: &hydrus_core::ServiceKey) -> Option<(u64, bool)> {
    (key.to_hex() == "57a2".repeat(16)).then_some((5, true))
}

/// What the encoder looks up in the basic fixture: its "stars" scale.
fn fixture() -> hydrus_gui_model::auto_resolution_exchange::Context<'static> {
    hydrus_gui_model::auto_resolution_exchange::Context {
        scales: &fixture_scales,
        service_key: &|_| None,
        service_type: &|_| None,
        default_canvases: &[],
    }
}

#[test]
fn rules_and_comparators_read_from_the_reference_export_as_it_does() {
    use hydrus_gui_model::auto_resolution_exchange::{
        export_comparators_text, import_comparators_text,
    };
    use hydrus_testkit::serialised::normalised_rule_export as normal;
    let rec = hydrus_testkit::fixture_json("rules_exchange.json");
    for export in rec["exports"].as_array().unwrap() {
        let text = export["text"].as_str().unwrap();
        let rules = import_text(text, &fixture_scales)
            .unwrap_or_else(|e| panic!("{}: {e}", export["names"]))
            .rules;
        let names: Vec<&str> = rules.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(serde_json::json!(names), export["selected_in_order"]);
        let ours = export_text(
            &rules.iter().map(|r| (None, r)).collect::<Vec<_>>(),
            &fixture(),
        )
        .unwrap();
        assert_eq!(normal(&ours), normal(text), "{names:?}");
    }
    for export in rec["comparators"].as_array().unwrap() {
        let text = export["text"].as_str().unwrap();
        let comparators = import_comparators_text(text, &fixture_scales)
            .unwrap()
            .comparators;
        let ours = export_comparators_text(&comparators, &fixture()).unwrap();
        assert_eq!(normal(&ours), normal(text), "{}", export["rows"]);
    }
}
