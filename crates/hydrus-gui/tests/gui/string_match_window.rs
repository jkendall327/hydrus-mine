//! The string match editor window put through the reference's recording
//! (`oracle/fixtures/string_match_editor.json`), as a user would: the window's
//! controls set, the window told it changed, and what it shows read back;
//! then "apply" for what it gives or why it won't.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use hydrus_core::url::strings::{FlexibleMatch, MatchKind, PyRegex, StringMatch};
use hydrus_gui::{StringStepWindow, headless, string_processor_window};
use hydrus_gui_model::string_editors::{CHARACTER_SETS, MATCH_TYPES};
use hydrus_store::Store;
use serde_json::{Value, json};
use slint::ComponentHandle as _;

fn count(value: &Value) -> Option<usize> {
    value.as_u64().map(|n| usize::try_from(n).unwrap())
}

/// A match from the reference's `ToTuple`.
fn from_tuple(tuple: &Value) -> StringMatch {
    let kind = match tuple[0].as_i64().unwrap() {
        0 => MatchKind::Fixed(tuple[1].as_str().unwrap().to_owned()),
        1 => MatchKind::Flexible(FlexibleMatch::from_code(tuple[1].as_i64().unwrap()).unwrap()),
        2 => MatchKind::Regex(PyRegex::new(tuple[1].as_str().unwrap())),
        _ => MatchKind::Any,
    };
    StringMatch {
        kind,
        min_chars: count(&tuple[2]),
        max_chars: count(&tuple[3]),
        example: tuple[4].as_str().unwrap().to_owned(),
    }
}

/// An editor open on `start`, and where the applied match lands.
fn open(
    store: &Arc<Store>,
    slots: &string_processor_window::Slots,
    start: &StringMatch,
) -> (StringStepWindow, Rc<RefCell<Option<StringMatch>>>) {
    let applied = Rc::new(RefCell::new(None));
    string_processor_window::open_match(store, start, slots, {
        let applied = applied.clone();
        Rc::new(move |m| *applied.borrow_mut() = Some(m))
    });
    let window = slots.step.borrow().as_ref().unwrap().clone_strong();
    (window, applied)
}

/// Do what the recording did to the reference's editor, to ours.
fn act(window: &StringStepWindow, step: &Value) {
    let (kind, v) = (step["do"][0].as_str().unwrap(), &step["do"][1]);
    let at = |n: usize| i32::try_from(n).unwrap();
    match kind {
        "type" => window.set_match_type(at(MATCH_TYPES.iter().position(|t| t == v).unwrap())),
        "fixed" => window.set_fixed(v.as_str().unwrap().into()),
        "regex" => window.set_match_regex(v.as_str().unwrap().into()),
        "flexible" => {
            window.set_character_set(at(CHARACTER_SETS.iter().position(|(_, l)| l == v).unwrap()));
        }
        "min" => {
            window.set_min_on(!v.is_null());
            if let Some(n) = count(v) {
                window.set_min_chars(at(n));
            }
        }
        "max" => {
            window.set_max_on(!v.is_null());
            if let Some(n) = count(v) {
                window.set_max_chars(at(n));
            }
        }
        "example" => window.set_match_example(v.as_str().unwrap().into()),
        other => panic!("{other}"),
    }
    window.invoke_changed();
}

fn check(window: &StringStepWindow, state: &Value, context: &str) {
    assert_eq!(
        MATCH_TYPES[usize::try_from(window.get_match_type()).unwrap()],
        state["type"],
        "{context}"
    );
    assert_eq!(
        json!({"fixed": window.get_show_fixed(), "regex": window.get_show_match_regex(),
            "flexible": window.get_show_character_set(), "limits": window.get_show_limits()}),
        state["shown"],
        "{context}"
    );
    assert_eq!(
        window.get_fixed(),
        state["fixed"].as_str().unwrap(),
        "{context}"
    );
    assert_eq!(
        window.get_match_regex(),
        state["regex"].as_str().unwrap(),
        "{context}"
    );
    let set = CHARACTER_SETS[usize::try_from(window.get_character_set()).unwrap()].1;
    assert_eq!(set, state["flexible"], "{context}");
    assert_eq!(
        window.get_min_on().then(|| window.get_min_chars()),
        state["min"].as_i64().map(|n| i32::try_from(n).unwrap()),
        "{context}"
    );
    assert_eq!(
        window.get_max_on().then(|| window.get_max_chars()),
        state["max"].as_i64().map(|n| i32::try_from(n).unwrap()),
        "{context}"
    );
    assert_eq!(
        window.get_match_example(),
        state["example"].as_str().unwrap(),
        "{context}"
    );
    let recorded = state["test"].as_str().unwrap();
    let text = window.get_test_result();
    if recorded.is_empty() {
        assert_eq!(text, "", "{context}");
    } else if recorded.contains("That regex did not work!") {
        // (Python's words for a regex that won't compile, ours here)
        assert!(
            text.starts_with("Example does not match - That regex did not work! "),
            "{context}: {text}"
        );
        assert!(!window.get_test_ok(), "{context}");
    } else {
        assert_eq!(text, recorded, "{context}");
        assert_eq!(
            window.get_test_ok(),
            state["valid"] == "HydrusValid",
            "{context}"
        );
    }
}

// leaf: audit-network-matcher-fields
#[test]
fn the_string_match_window_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("string_match_editor.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let (mut states, mut vetoes) = (0, 0);
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let start = &case["start"];
        let start = from_tuple(&json!([start[0], start[1], start[2], start[3], start[4]]));
        let steps = case["steps"].as_array().unwrap();
        for upto in 0..steps.len() {
            // one window, every step to here, what it shows; and a second to
            // here, "apply", what it gives or why not
            let slots = string_processor_window::Slots::default();
            let (window, _) = open(&store, &slots, &start);
            for step in steps.iter().take(upto + 1).skip(1) {
                act(&window, step);
            }
            let state = &steps[upto]["state"];
            let context = format!("case {c} step {upto}: {}", steps[upto]["do"]);
            check(&window, state, &context);
            states += 1;
            window.invoke_cancel();

            let slots = string_processor_window::Slots::default();
            let (window, applied) = open(&store, &slots, &start);
            for step in steps.iter().take(upto + 1).skip(1) {
                act(&window, step);
            }
            window.invoke_apply();
            match state["veto"].as_str() {
                Some(veto) => {
                    vetoes += 1;
                    assert_eq!(window.get_veto(), veto, "{context}");
                    assert!(applied.borrow().is_none(), "{context}");
                    window.invoke_cancel();
                }
                None => assert_eq!(
                    applied.borrow().clone(),
                    Some(from_tuple(&state["value"])),
                    "{context}"
                ),
            }
        }
    }
    assert_eq!(states, 42, "every recorded state");
    assert!(vetoes > 0, "the recorded refusals");
}
