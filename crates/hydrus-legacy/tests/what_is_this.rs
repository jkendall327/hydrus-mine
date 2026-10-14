//! What "what is this object?" says of each payload the reference was given
//! (`oracle/fixtures/what_is_this_object.json`, recorded by
//! `oracle/record_what_is_this_object.py` through the window's own
//! `_ImportPayloads`), and its class names.
use hydrus_legacy::serialisable::{class_name, what_is_this};

fn recording() -> serde_json::Value {
    hydrus_testkit::fixture_json("what_is_this_object.json")
}

/// The window's text for some payloads: `N items`, then each result.
fn window_text(payloads: &[String]) -> String {
    let results: Vec<String> = payloads.iter().map(|p| what_is_this(p)).collect();
    format!("{} items\n\n{}", payloads.len(), results.join("\n\n"))
}

fn payloads(case: &serde_json::Value) -> Vec<String> {
    case["payloads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn every_class_name_is_the_reference_s() {
    for class in recording()["types"].as_array().unwrap() {
        let code = u16::try_from(class["type"].as_u64().unwrap()).unwrap();
        assert_eq!(class_name(code), class["name"].as_str(), "type {code}");
    }
}

#[test]
fn constructed_objects_are_described_as_the_reference_describes_them() {
    let recording = recording();
    let objects = recording["objects"].as_object().unwrap();
    assert!(objects.len() >= 9);
    for (name, case) in objects {
        assert_eq!(
            window_text(&payloads(case)),
            case["description"].as_str().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn what_is_not_an_object_is_an_error_worded_as_python_words_it() {
    let recording = recording();
    let errors = recording["errors"].as_object().unwrap();
    assert!(errors.len() >= 25);
    for (name, case) in errors {
        assert_eq!(
            window_text(&payloads(case)),
            case["description"].as_str().unwrap(),
            "{name}"
        );
    }
}
