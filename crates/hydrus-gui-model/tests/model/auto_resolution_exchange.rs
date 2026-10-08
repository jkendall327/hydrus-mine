//! The auto-resolution rules list's export and import: hydrus-rs JSON round
//! trips, and the reference's own serialised rules (its suggested rules)
//! are read, alone or in a list.
use hydrus_gui_model::auto_resolution_exchange::{
    added, export_text, import_text, refused_message,
};
use hydrus_store::duplicates::auto::suggested_rules;

#[test]
fn exported_rules_import_again() {
    let rules = suggested_rules();
    let text = export_text(&rules[..2]);
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

fn first_comparators(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    if let Some(array) = value.as_array() {
        if array.len() >= 2
            && array[0]
                .as_u64()
                .is_some_and(|t| [130, 131, 137, 138, 140, 141, 152].contains(&t))
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

// leaf: audit-media-rules-exchange
#[test]
fn comparators_export_and_import_alone_in_lists_and_from_the_reference() {
    use hydrus_gui_model::auto_resolution_exchange::{
        COMPARATOR_TYPE, export_comparators_text, import_comparators_text, refused_message_for,
    };
    let rules = suggested_rules();
    let comparators: Vec<_> = rules.iter().flat_map(|r| r.comparators.clone()).collect();
    assert!(comparators.len() >= 2);
    let text = export_comparators_text(&comparators[..2]);
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
            "The imported objects included these types:\n\nserialisable type 128\n\nWhereas this control only allows:\n\nPairComparator"
        )
    );
}
