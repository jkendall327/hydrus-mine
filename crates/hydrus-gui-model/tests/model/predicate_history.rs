//! Replay the actual Qt frame-global recency lists and visible-page toggles.
use hydrus_gui_model::main_menu::{Entry, Facts, menubar, shown};
use hydrus_gui_model::predicate_history::{History, Kind};
use hydrus_search::{Predicate, TextContext, enter_predicates, parse_api_search, predicate_text};
use serde_json::{Value, json};
use std::collections::HashMap;

fn decode(value: &Value) -> Predicate {
    let object =
        hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
            .unwrap();
    hydrus_legacy::objects::predicates::predicate(&object).unwrap()
}
fn tag(text: &str) -> Predicate {
    parse_api_search(&json!([text])).unwrap().remove(0)
}
fn labels(predicates: &[Predicate]) -> Vec<String> {
    predicates
        .iter()
        .map(|p| predicate_text(p, &TextContext::default()))
        .collect()
}
fn tree(entries: &[Entry]) -> Value {
    Value::Array(
        shown(entries)
            .iter()
            .map(|entry| match entry {
                Entry::Menu { label, entries, .. } => json!({"menu":label,"entries":tree(entries)}),
                Entry::Separator => json!("---"),
                _ => json!(entry.label()),
            })
            .collect(),
    )
}
#[test]
fn actual_histories_replay_toggles_recency_cross_page_or_and_clear() {
    let recording = hydrus_testkit::fixture_json("search_predicate_undo.json");
    let mut history = History::default();
    let mut pages = HashMap::from([("Undo A".to_owned(), Vec::new())]);
    let mut current = "Undo A".to_owned();
    let mut last_search_menu = json!([]);
    for event in recording["events"].as_array().unwrap() {
        let op = &event["operation"];
        if let Some(input) = op.get("enter") {
            let before = pages[&current].clone();
            let after = pages.get_mut(&current).unwrap();
            enter_predicates(after, &[decode(input)], &TextContext::default());
            history.record(&before, after);
        }
        if let Some(kind) = op.get("undo") {
            let kind = if kind == "addition" {
                Kind::Addition
            } else {
                Kind::Removal
            };
            let predicate = tag(op["predicate"].as_str().unwrap());
            if !current.is_empty() {
                assert!(history.take(kind, &predicate));
                let before = pages[&current].clone();
                let after = pages.get_mut(&current).unwrap();
                enter_predicates(after, &[predicate], &TextContext::default());
                history.record(&before, after);
            }
        }
        if let Some(input) = op.get("replace") {
            let before = pages[&current].clone();
            let after: Vec<_> = input.as_array().unwrap().iter().map(decode).collect();
            history.record(&before, &after);
            pages.insert(current.clone(), after);
        }
        if op.get("empty_notebook").is_some() {
            current.clear();
        }
        if op.get("new").is_some() {
            current = "Undo B".to_owned();
            pages.insert(current.clone(), vec![tag("undo:restored")]);
        }
        if op.get("close").is_some() {
            current = "Undo A".to_owned();
        }
        if op.get("restore").is_some() {
            current = "Undo B".to_owned();
        }
        if op["clear"] == true {
            history.clear();
        }
        assert_eq!(
            json!(labels(&history.added)),
            event["added"],
            "{}",
            event["label"]
        );
        assert_eq!(
            json!(labels(&history.removed)),
            event["removed"],
            "{}",
            event["label"]
        );
        for (name, predicates) in &pages {
            let mut search = labels(predicates);
            search.sort();
            assert_eq!(
                json!(search),
                event["searches"][name],
                "{}: {name}",
                event["label"]
            );
        }
        let labelled = |predicates: &[Predicate]| {
            predicates
                .iter()
                .map(|p| (p.clone(), predicate_text(p, &TextContext::default())))
                .collect()
        };
        let facts = Facts {
            search_added: labelled(&history.added),
            search_removed: labelled(&history.removed),
            ..Facts::default()
        };
        let menus = menubar(&facts);
        let undo = menus.iter().find(|m| m.label() == "&undo").unwrap();
        if history.added.is_empty() && history.removed.is_empty() {
            // With no closed pages/content/search undo, Qt disables the bar
            // entry and retains the last raw submenu object. Async loading
            // disables its child menus; those retained rows are not a usable
            // history menu. Keep the actual recorded rows checked separately.
            assert!(!undo.usable());
            assert_eq!(event["undo_enabled"], false);
            let mut retained = last_search_menu.clone();
            for entry in retained.as_array_mut().unwrap() {
                if entry.get("menu").is_some() {
                    entry["disabled"] = json!(true);
                }
            }
            assert_eq!(retained, event["menu"], "{}", event["label"]);
        } else {
            let Entry::Menu { entries, .. } = undo else {
                panic!()
            };
            let Entry::Menu { entries, .. } =
                entries.iter().find(|m| m.label() == "searching").unwrap()
            else {
                panic!()
            };
            assert!(undo.usable());
            assert_eq!(event["undo_enabled"], true);
            assert_eq!(tree(entries), event["menu"], "{}", event["label"]);
            last_search_menu = event["menu"].clone();
        }
    }
}
#[test]
fn refused_inputs_reordering_and_stale_history_commands_do_not_mutate_recency() {
    let alpha = tag("undo:alpha");
    let beta = tag("undo:beta");
    let mut history = History::default();
    history.record(&[], &[alpha.clone(), beta.clone()]);
    let kept = history.clone();
    history.record(
        &[alpha.clone(), beta.clone()],
        &[beta.clone(), alpha.clone()],
    );
    assert_eq!(history, kept);
    assert!(history.take(Kind::Addition, &alpha));
    assert!(!history.take(Kind::Addition, &alpha));
    assert_eq!(history.added, [beta]);
    assert!(!history.take(Kind::Removal, &alpha));
}
