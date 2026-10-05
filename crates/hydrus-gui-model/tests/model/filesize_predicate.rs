//! Actual Qt filesize choices, bounded amounts and explicit separated units.
use hydrus_core::search::{
    number::Comparison,
    predicate::{Predicate, SizeUnit, SystemPredicate},
};
use hydrus_gui_model::predicate_editors::{
    Blank, Context, Editor, Field, defaults::CustomDefaults,
};
use hydrus_search::CivilDateTime;
use serde_json::Value;

fn context() -> Context {
    Context {
        file_services: vec![],
        tag_services: vec![],
        url_classes: vec![],
        rating_services: vec![],
        today: CivilDateTime::new(2026, 10, 5, 0, 0).unwrap(),
    }
}
fn predicate(raw: &Value) -> Predicate {
    let op = match raw[0].as_str().unwrap() {
        "<" => Comparison::Less,
        "≈" => Comparison::Approx,
        "=" => Comparison::Equal,
        "≠" => Comparison::NotEqual,
        ">" => Comparison::Greater,
        _ => panic!("operator"),
    };
    let unit = match raw[2].as_u64().unwrap() {
        1 => SizeUnit::Bytes,
        1024 => SizeUnit::Kilobytes,
        1_048_576 => SizeUnit::Megabytes,
        1_073_741_824 => SizeUnit::Gigabytes,
        1_099_511_627_776 => SizeUnit::Terabytes,
        _ => panic!("unit"),
    };
    Predicate::System(SystemPredicate::FileSize {
        op,
        size: raw[1].as_u64().unwrap(),
        unit,
    })
}
#[test]
fn choices_bounds_and_explicit_reopening_match_qt() {
    let fixture = hydrus_testkit::fixture_json("filesize_predicate.json");
    let context = context();
    let mut editor = Editor::new(Blank::Filesize, &context);
    let panel = &mut editor.pages[0].panels[0];
    assert_eq!(
        panel.predicates(&context).unwrap(),
        vec![predicate(&fixture["default"]["value"])]
    );
    let Field::Choice { options, .. } = &panel.fields[1] else {
        panic!("comparison")
    };
    assert_eq!(serde_json::json!(options), fixture["operators"]);
    let Field::Number { min, max, .. } = &panel.fields[2] else {
        panic!("amount")
    };
    assert_eq!(
        (*min, *max),
        (
            fixture["minimum"].as_i64().unwrap(),
            fixture["maximum"].as_i64().unwrap()
        )
    );
    for case in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["boundary_queries"].as_array().unwrap())
    {
        let op = fixture["operators"]
            .as_array()
            .unwrap()
            .iter()
            .position(|op| op == &case["value"][0])
            .unwrap();
        let unit = fixture["units"]
            .as_array()
            .unwrap()
            .iter()
            .position(|unit| unit[1] == case["value"][2])
            .unwrap();
        panel.choose(1, op);
        panel.set_number(2, case["amount"].as_i64().unwrap());
        panel.choose(3, unit);
        assert_eq!(
            panel.predicates(&context).unwrap(),
            vec![predicate(&case["value"])]
        );
    }
    for case in fixture["bounds"].as_array().unwrap() {
        panel.choose(1, 4);
        panel.choose(3, 1);
        panel.set_number(2, case["input"].as_i64().unwrap());
        assert_eq!(
            panel.predicates(&context).unwrap(),
            vec![predicate(&case["result"]["value"])]
        );
    }
    let defaults = CustomDefaults {
        predicates: vec![
            Predicate::System(SystemPredicate::Limit(37)),
            predicate(&fixture["cases"][0]["value"]),
        ],
    };
    for case in fixture["reopened"].as_array().unwrap() {
        panel.initialise(Some(&predicate(&case["input"])), &defaults, &context);
        assert_eq!(
            panel.predicates(&context).unwrap(),
            vec![predicate(&case["result"]["value"])]
        );
    }
    let before = panel.predicates(&context).unwrap();
    panel.choose(1, usize::MAX);
    panel.choose(3, usize::MAX);
    assert_eq!(panel.predicates(&context).unwrap(), before);
}
