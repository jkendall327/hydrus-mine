//! Gallery URL generators and the domain manager's downloader definitions,
//! against `oracle/fixtures/gugs.json` (made by `oracle/dump_gugs.py`).

use serde_json::Value as Json;

use hydrus_core::url::{AnyGug, GugOptions, Gugs};
use hydrus_legacy::objects::parsers::{downloaders, gug};
use hydrus_legacy::serialisable::SerialisableObject;

fn options(case: &Json) -> GugOptions {
    GugOptions {
        percent_twenty_is_space: case["percent_twenty_is_space"].as_bool().unwrap(),
        collapse_leading_slashes: case["collapse_leading_slashes"].as_bool().unwrap(),
    }
}

fn outcome(result: Result<Vec<String>, hydrus_core::url::GugError>) -> Json {
    match result {
        Ok(urls) => serde_json::json!({ "urls": urls }),
        Err(e) => serde_json::json!({ "error": e.to_string() }),
    }
}

fn expected(case: &Json) -> Json {
    match case.get("urls") {
        Some(urls) => serde_json::json!({ "urls": urls }),
        None => serde_json::json!({ "error": case["error"] }),
    }
}

#[test]
fn gugs_make_urls_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("gugs.json");
    let mut failures = Vec::new();
    for (i, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["gug"].to_string()).unwrap();
        let g = gug(&object).unwrap();
        let gugs = Gugs {
            gugs: vec![g.clone()],
            keys_to_display: Vec::new(),
        };
        let got = outcome(gugs.gallery_urls(&g, case["query"].as_str().unwrap(), options(case)));
        if got != expected(case) {
            failures.push(format!(
                "case {i} ({:?}): expected {} got {got}",
                case["query"],
                expected(case)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn domain_managers_convert_and_nested_gugs_find_theirs() {
    let recorded = hydrus_testkit::fixture_json("gugs.json");
    let mut failures = Vec::new();
    for (i, manager) in recorded["managers"].as_array().unwrap().iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&manager["manager"].to_string()).unwrap();
        let d = downloaders(&object).unwrap();
        assert!(d.unconverted.is_empty(), "manager {i}: {:?}", d.unconverted);
        let pairs = |items: Vec<(&str, &str)>| {
            items
                .into_iter()
                .map(|(k, n)| serde_json::json!([k, n]))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            Json::from(pairs(
                d.gugs.gugs.iter().map(|g| (g.key(), g.name())).collect()
            )),
            manager["gugs"],
            "manager {i}"
        );
        let mut display = d.gugs.keys_to_display.clone();
        display.sort();
        assert_eq!(
            serde_json::json!(display),
            manager["keys_to_display"],
            "manager {i}"
        );
        assert_eq!(
            Json::from(pairs(
                d.parsers
                    .iter()
                    .map(|p| (p.key.as_str(), p.name.as_str()))
                    .collect()
            )),
            manager["parsers"],
            "manager {i}"
        );
        for run in manager["runs"].as_array().unwrap() {
            let nested = d
                .gugs
                .gugs
                .iter()
                .find(
                    |g| matches!(g, AnyGug::Nested(n) if n.name == run["nested"].as_str().unwrap()),
                )
                .unwrap();
            let got = outcome(d.gugs.gallery_urls(
                nested,
                run["query"].as_str().unwrap(),
                options(run),
            ));
            if got != expected(run) {
                failures.push(format!(
                    "manager {i} {}: expected {} got {got}",
                    run["nested"],
                    expected(run)
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} runs differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
