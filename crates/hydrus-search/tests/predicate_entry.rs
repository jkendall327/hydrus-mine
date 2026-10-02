//! Predicates entered into a search as the reference's list of a search's
//! predicates takes them (`oracle/fixtures/predicate_entry.json`, recorded
//! on the `basic` fixture by `oracle/record_predicate_entry.py`): each
//! predicate's inverse, and what entering predicates in turn leaves in a
//! search, in the order the list shows them.

use serde_json::Value;

use hydrus_core::{ServiceKey, ServiceType};
use hydrus_search::entry::{enter_predicates, is_incdec};
use hydrus_search::{
    NamedService, Predicate, SystemPredicate, TextContext, parse_api_search, predicate_text,
};

fn fixture() -> Value {
    hydrus_testkit::fixture_json("predicate_entry.json")
}

/// The `basic` fixture's rating services.
fn context() -> TextContext {
    let service = |name: &str, service_type, stars| NamedService {
        key: ServiceKey::new(name.as_bytes().to_vec()),
        name: name.to_owned(),
        service_type,
        stars,
    };
    TextContext {
        services: vec![
            service("favourites", ServiceType::LocalRatingLike, None),
            service("stars", ServiceType::LocalRatingNumerical, Some((5, false))),
            service("counter", ServiceType::LocalRatingIncDec, None),
        ],
        ..TextContext::default()
    }
}

/// A predicate as the recording typed it: "OR:a|b" an OR of a and b,
/// "system:local" and "system:not local" as such, and
/// "namespace:*anything*" (or "-namespace:*anything*") a namespace
/// predicate, as the recorder makes them; the rest as the Client API's
/// search parses it.
fn parse(text: &str) -> Predicate {
    if let Some(parts) = text.strip_prefix("OR:") {
        return Predicate::Or(parts.split('|').map(parse).collect());
    }
    match text {
        "system:local" => return Predicate::System(SystemPredicate::Local),
        "system:not local" => return Predicate::System(SystemPredicate::NotLocal),
        _ => {}
    }
    let (inclusive, bare) = match text.strip_prefix('-') {
        Some(bare) => (false, bare),
        None => (true, text),
    };
    if let Some(namespace) = bare.strip_suffix(":*anything*") {
        return Predicate::Namespace {
            namespace: namespace.to_owned(),
            inclusive,
        };
    }
    let mut parsed =
        parse_api_search(&serde_json::json!([text])).unwrap_or_else(|e| panic!("{text}: {e}"));
    assert_eq!(parsed.len(), 1, "{text}");
    parsed.remove(0)
}

fn texts(search: &[Predicate], context: &TextContext) -> Vec<String> {
    search.iter().map(|p| predicate_text(p, context)).collect()
}

fn strings(json: &Value) -> Vec<String> {
    json.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn each_predicate_s_inverse_is_the_reference_s() {
    let context = context();
    let counts = |service: &_| is_incdec(service, &context);
    let mut checked = 0;
    for case in fixture()["inverses"].as_array().unwrap() {
        // (what the reference's parser can't read)
        if case.get("error").is_some() {
            continue;
        }
        let predicate = parse(case["typed"].as_str().unwrap());
        assert_eq!(predicate_text(&predicate, &context), case["text"], "{case}");
        let inverse = predicate
            .inverse(&counts)
            .map(|p| predicate_text(&p, &context));
        assert_eq!(inverse.as_deref(), case["inverse"].as_str(), "{case}");
        checked += 1;
    }
    assert!(checked > 50, "{checked}");
}

#[test]
fn entering_predicates_leaves_what_the_reference_s_list_does() {
    let context = context();
    for scenario in fixture()["scenarios"].as_array().unwrap() {
        // (set as they are, unsorted, as the list sets them)
        let mut search: Vec<Predicate> = strings(&scenario["start"])
            .iter()
            .map(|t| parse(t))
            .collect();
        assert_eq!(
            texts(&search, &context),
            strings(&scenario["started"]),
            "{scenario}"
        );
        for (entered, shown) in strings(&scenario["enter"])
            .iter()
            .zip(scenario["shown"].as_array().unwrap())
        {
            enter_predicates(&mut search, &[parse(entered)], &context);
            assert_eq!(
                texts(&search, &context),
                strings(shown),
                "{entered} into {scenario}"
            );
        }
    }
}

#[test]
fn the_order_is_by_the_stored_text_whatever_is_shown() {
    // (the reference sorts by the text it would copy, not as tags are
    // shown: with namespaces hidden, "series:metroid" shows "metroid" but
    // still sorts after "character:link")
    let plain = context();
    let hidden = TextContext {
        presentation: Some(hydrus_core::tag_presentation::TagPresentation {
            show_namespaces: false,
            ..Default::default()
        }),
        ..context()
    };
    let mut search = Vec::new();
    for entered in ["series:metroid", "character:zelda", "blue eyes"] {
        enter_predicates(&mut search, &[parse(entered)], &hidden);
    }
    assert_eq!(
        texts(&search, &plain),
        ["blue eyes", "character:zelda", "series:metroid"]
    );
    assert_eq!(texts(&search, &hidden), ["blue eyes", "zelda", "metroid"]);
}
