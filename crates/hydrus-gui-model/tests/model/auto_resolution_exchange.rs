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
