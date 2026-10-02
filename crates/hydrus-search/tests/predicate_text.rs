//! Predicates are written as the reference writes them: against the text
//! `oracle/fixtures/system_predicates.json` recorded for each predicate the
//! reference parsed (decoded from the form it stored), and for each Client
//! API search.

use std::collections::BTreeSet;

use serde_json::Value;

use hydrus_core::{ServiceKey, ServiceType};
use hydrus_search::{
    NamedService, TextContext, ViewCanvas, parse_api_search, parse_system_predicate, predicate_text,
};

fn fixture() -> Value {
    hydrus_testkit::fixture_json("system_predicates.json")
}

/// The oracle's stub services and options.
fn context(fixture: &Value) -> TextContext {
    TextContext {
        services: fixture["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| NamedService {
                key: ServiceKey::new(hex::decode(s["key"].as_str().unwrap()).unwrap()),
                name: s["name"].as_str().unwrap().to_owned(),
                service_type: ServiceType::from_code(
                    u8::try_from(s["type"].as_u64().unwrap()).unwrap(),
                )
                .unwrap(),
                stars: s["num_stars"]
                    .as_u64()
                    .map(|n| (n, s["allow_zero"].as_bool().unwrap())),
            })
            .collect(),
        default_canvases: fixture["default_view_canvases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| match c.as_str().unwrap() {
                "media" => ViewCanvas::MediaViewer,
                "preview" => ViewCanvas::Preview,
                "client api" => ViewCanvas::ClientApi,
                other => panic!("unknown canvas {other}"),
            })
            .collect(),
        presentation: None,
    }
}

#[test]
fn predicates_are_written_as_the_reference_writes_them() {
    let fixture = fixture();
    let context = context(&fixture);
    let scales = |key: &ServiceKey| {
        context
            .services
            .iter()
            .find(|s| s.key == *key)
            .and_then(|s| s.stars)
    };
    let mut report = String::new();
    let (mut checked, mut undecodable) = (0, 0);
    for case in fixture["system_predicates"].as_array().unwrap() {
        if case["ok"] != true {
            continue;
        }
        let predicate = match case.get("serialised") {
            Some(serialised) => {
                let stored = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
                    &serialised.to_string(),
                )
                .unwrap();
                match hydrus_legacy::objects::predicates::predicate_with_scales(&stored, &scales) {
                    Ok(p) => p,
                    Err(e) => {
                        // values we cannot hold, whether typed or stored: a
                        // hash of the wrong length, a number past 64 bits, a
                        // negative age (dateparser's "in 2 days")
                        let e = e.to_string();
                        let input = case["input"].as_str().unwrap();
                        let too_large = input
                            .split(|c: char| !c.is_ascii_digit())
                            .any(|run| run.len() >= 20);
                        if e.contains("bad hash")
                            || e.contains("negative time part")
                            || (too_large
                                && (e.contains("is not a whole number we can hold")
                                    || e.contains("found float")))
                        {
                            undecodable += 1;
                        } else {
                            report.push_str(&format!("{input}\n    could not decode: {e}\n"));
                        }
                        continue;
                    }
                }
            }
            // URL class predicates were not stored (the oracle's classes are stubs)
            None => hydrus_search::Predicate::System(
                parse_system_predicate(case["input"].as_str().unwrap()).unwrap(),
            ),
        };
        checked += 1;
        let ours = predicate_text(&predicate, &context);
        let expected = case["text"].as_str().unwrap();
        let expected = if expected.starts_with("error:") {
            // the reference cannot write `≠` here
            assert!(expected.contains("o_text"), "{expected}");
            "system:num file relationships - has not 5 alternates"
        } else {
            expected
        };
        if ours != expected {
            report.push_str(&format!(
                "{}\n    reference: {expected}\n    ours:      {ours}\n",
                case["input"]
            ));
        }
        assert_eq!(case["text_for_user"], case["text"]);
    }
    println!("{checked} written, {undecodable} not decodable");
    assert!(
        report.is_empty(),
        "predicates written differently:\n{report}"
    );
    assert!(checked > 4500);
}

#[test]
fn api_searches_are_written_as_the_reference_writes_them() {
    let fixture = fixture();
    let context = context(&fixture);
    let mut checked = 0;
    for case in fixture["api_searches"].as_array().unwrap() {
        if case["ok"] != true {
            continue;
        }
        let Ok(predicates) = parse_api_search(&case["tags"]) else {
            continue;
        };
        checked += 1;
        let ours: BTreeSet<String> = predicates
            .iter()
            .map(|p| predicate_text(p, &context))
            .collect();
        let expected: BTreeSet<String> = case["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["text"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(ours, expected, "{}", case["tags"]);
    }
    assert!(checked > 30, "{checked}");
}

/// The predicates the reference's system predicate editors make
/// (`oracle/record_system_predicate_editors.py`), as it stores them, are
/// read and written as its own: among them those its parser has no words
/// for ("≠" ratios, durations within an amount either side).
#[test]
fn predicates_the_reference_s_editors_make_are_read_and_written_as_its_own() {
    let fixture = hydrus_testkit::fixture_json("system_predicate_editors.json");
    let context = context(&fixture);
    let scales = |key: &ServiceKey| {
        context
            .services
            .iter()
            .find(|s| s.key == *key)
            .and_then(|s| s.stars)
    };
    let mut report = String::new();
    let mut checked = 0;
    for case in fixture["stored"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        let stored = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
            &case["serialised"].to_string(),
        )
        .unwrap();
        match hydrus_legacy::objects::predicates::predicate_with_scales(&stored, &scales) {
            Ok(predicate) => {
                let ours = predicate_text(&predicate, &context);
                // (a size in terabytes, which the reference can't write)
                let unwritable = text.starts_with("error:") && ours.ends_with("TB");
                if ours != text && !unwritable {
                    report.push_str(&format!("{text}\n    written {ours}\n"));
                }
                checked += 1;
            }
            Err(e) => report.push_str(&format!("{text}\n    could not decode: {e}\n")),
        }
    }
    assert!(report.is_empty(), "{report}");
    assert!(checked > 200, "{checked}");
}
