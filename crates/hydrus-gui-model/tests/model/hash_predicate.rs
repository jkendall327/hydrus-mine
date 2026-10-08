//! Replay the real Qt hash groups, cleanup messages and typed reconstruction.
use hydrus_core::search::predicate::{FileHashes, Predicate, SystemPredicate};
use hydrus_gui_model::predicate_editors::{
    Blank, Context, Editor, Field, Pressed, defaults::CustomDefaults,
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
    let strings = raw["hashes"].as_array().unwrap();
    let hashes = match raw["type"].as_str().unwrap() {
        "md5" => FileHashes::Md5(
            strings
                .iter()
                .map(|s| s.as_str().unwrap().parse().unwrap())
                .collect(),
        ),
        "sha1" => FileHashes::Sha1(
            strings
                .iter()
                .map(|s| s.as_str().unwrap().parse().unwrap())
                .collect(),
        ),
        "sha256" => FileHashes::Sha256(
            strings
                .iter()
                .map(|s| s.as_str().unwrap().parse().unwrap())
                .collect(),
        ),
        "sha512" => FileHashes::Sha512(
            strings
                .iter()
                .map(|s| s.as_str().unwrap().parse().unwrap())
                .collect(),
        ),
        _ => panic!("type"),
    };
    Predicate::System(SystemPredicate::Hash {
        hashes,
        inclusive: raw["inclusive"].as_bool().unwrap(),
    })
}
fn snapshot(panel: &hydrus_gui_model::predicate_editors::Panel, raw: &Value, context: &Context) {
    let Field::Lines { text, .. } = &panel.fields[2] else {
        panic!("lines")
    };
    assert_eq!(text, raw["text"].as_str().unwrap());
    assert_eq!(panel.chosen(1) == 0, raw["inclusive"].as_bool().unwrap());
    let Field::Choice { options, chosen } = &panel.fields[5] else {
        panic!("type")
    };
    assert_eq!(options[*chosen], raw["type"].as_str().unwrap());
    if let Some(error) = raw.get("error") {
        assert_eq!(
            panel.predicates(context),
            Err(error.as_str().unwrap().to_owned())
        );
    } else {
        assert_eq!(
            panel.predicates(context).unwrap(),
            vec![predicate(&raw["predicate"])]
        );
    }
}
// leaf: audit-options-predicate-hash-hash-clean
// leaf: audit-options-predicate-hash-hash-hashes
#[test]
fn cleanup_and_explicit_values_match_real_qt() {
    let fixture = hydrus_testkit::fixture_json("hash_predicate.json");
    let context = context();
    let mut editor = Editor::new(Blank::Hash, &context);
    let panel = &mut editor.pages[0].panels[0];
    snapshot(panel, &fixture["default"], &context);
    for case in fixture["cleanup"].as_array().unwrap() {
        panel.choose(1, 0);
        panel.choose(5, 3);
        panel.set_text(2, case["before"]["text"].as_str().unwrap());
        snapshot(panel, &case["before"], &context);
        let button = if case["action"] == "normal" { 3 } else { 4 };
        let answer = panel.press(button);
        if button == 4 {
            assert_eq!(
                answer,
                Pressed::Confirm(case["questions"][0].as_str().unwrap().to_owned())
            );
            if case["yes"].as_bool().unwrap() {
                assert_eq!(panel.press_confirmed(button), Pressed::Done);
            }
        } else if let Some(warning) = case["warnings"].as_array().unwrap().first() {
            assert_eq!(
                answer,
                Pressed::Warning(warning.as_str().unwrap().to_owned())
            );
        } else {
            assert_eq!(answer, Pressed::Done);
        }
        snapshot(panel, &case["after"], &context);
    }
    for case in fixture["queries"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["reopened"].as_array().unwrap())
    {
        let expected = predicate(&case["predicate"]);
        // Explicit supplied input must win over a conflicting saved hash default.
        let defaults = CustomDefaults {
            predicates: vec![predicate(&fixture["default"]["predicate"])],
        };
        panel.initialise(Some(&expected), &defaults, &context);
        assert_eq!(panel.predicates(&context).unwrap(), vec![expected]);
        assert_eq!(panel.chosen(1) == 0, case["inclusive"].as_bool().unwrap());
        let Field::Lines { text, .. } = &panel.fields[2] else {
            panic!("lines")
        };
        let mut lines = case["predicate"]["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap())
            .collect::<Vec<_>>();
        lines.sort_unstable();
        assert_eq!(*text, lines.join("\n"));
    }
}
