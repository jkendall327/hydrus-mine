//! URL classes making referral URLs and next gallery pages, against
//! `oracle/fixtures/url_classes.json` (made by `oracle/dump_url_classes.py`).

use serde_json::{Value as Json, json};

use hydrus_legacy::objects::domain::url_class;
use hydrus_legacy::serialisable::SerialisableObject;

/// The reference names an arbitrary query parameter (a stale loop variable)
/// when a required one is missing; we name the missing one (see
/// DIFFERENCES.md).
fn same_flesh_out_error(got: &Json, expected: &Json) -> bool {
    let prefix = "Could not flesh out query--no default for ";
    let is_it = |v: &Json| {
        v["next_page_error"]
            .as_str()
            .is_some_and(|e| e.starts_with(prefix))
    };
    is_it(got) && is_it(expected)
}

#[test]
fn url_classes_page_and_refer_like_the_reference() {
    let recorded = hydrus_testkit::fixture_json("url_classes.json");
    let mut failures = Vec::new();
    for (i, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let object = SerialisableObject::from_tuple_str(&case["url_class"].to_string()).unwrap();
        let class = url_class(&object).unwrap();
        let collapse = case["collapse_leading_slashes"].as_bool().unwrap();
        assert_eq!(
            json!(class.can_generate_next_gallery_page()),
            case["can_generate_next_gallery_page"],
            "case {i}"
        );
        for r in case["results"].as_array().unwrap() {
            let url = r["url"].as_str().unwrap();
            let next = match class.next_gallery_page(url, collapse) {
                Ok(next) => json!({ "next_page": next }),
                Err(e) => json!({ "next_page_error": e.to_string() }),
            };
            let expected = match r.get("next_page") {
                Some(next) => json!({ "next_page": next }),
                None => json!({ "next_page_error": r["next_page_error"] }),
            };
            if next != expected && !same_flesh_out_error(&next, &expected) {
                failures.push(format!("case {i} {url}: expected {expected} got {next}"));
            }
            for (key, given) in [
                ("referral", None),
                (
                    "referral_given",
                    Some("https://example.com/where/it/was/found"),
                ),
            ] {
                if let Some(expected) = r.get(key) {
                    let got = Json::from(class.referral_url(url, given, collapse));
                    if &got != expected {
                        failures.push(format!(
                            "case {i} {url} {key}: expected {expected} got {got}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
