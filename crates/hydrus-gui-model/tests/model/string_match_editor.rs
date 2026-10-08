//! The string match editor replayed against the reference's
//! (`oracle/fixtures/string_match_editor.json`, from
//! `oracle/record_string_match_editor.py`): the type, the rows shown, the
//! fields (a fixed text or regex taking the example when its type is
//! chosen), whether the example matches and why not, and what "ok" gives.

use hydrus_core::url::strings::{FlexibleMatch, MatchKind, PyRegex, StringMatch};
use hydrus_gui_model::string_editors::{CHARACTER_SETS, MATCH_TYPES, MatchEditor};
use serde_json::{Value, json};

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

fn check(editor: &MatchEditor, state: &Value, context: &str) {
    assert_eq!(MATCH_TYPES[editor.match_type], state["type"], "{context}");
    let shown = editor.shown();
    assert_eq!(
        json!({"fixed": shown.fixed, "regex": shown.regex, "flexible": shown.flexible,
            "limits": shown.limits}),
        state["shown"],
        "{context}"
    );
    assert_eq!(editor.fixed, state["fixed"], "{context}");
    assert_eq!(editor.regex, state["regex"], "{context}");
    let flexible = CHARACTER_SETS
        .iter()
        .find(|(f, _)| *f == editor.flexible)
        .unwrap()
        .1;
    assert_eq!(flexible, state["flexible"], "{context}");
    assert_eq!(json!(editor.min_chars), state["min"], "{context}");
    assert_eq!(json!(editor.max_chars), state["max"], "{context}");
    assert_eq!(editor.example, state["example"], "{context}");
    let recorded = state["test"].as_str().unwrap();
    match editor.test_result() {
        None => assert_eq!(recorded, "", "{context}"),
        // (a regex that won't compile: Python's words there, ours here)
        Some((text, false)) if recorded.contains("That regex did not work!") => {
            assert!(text.starts_with("Example does not match - That regex did not work! "));
        }
        Some((text, valid)) => {
            assert_eq!(text, recorded, "{context}");
            let expected = if valid {
                "HydrusValid"
            } else {
                "HydrusInvalid"
            };
            assert_eq!(state["valid"], expected, "{context}");
        }
    }
    match editor.value() {
        Ok(value) => assert_eq!(value, from_tuple(&state["value"]), "{context}"),
        Err(veto) => assert_eq!(veto, state["veto"], "{context}"),
    }
}

// leaf: audit-network-matcher-fields
#[test]
fn the_string_match_editor_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("string_match_editor.json");
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let start = &case["start"];
        let start = from_tuple(&json!([start[0], start[1], start[2], start[3], start[4]]));
        let mut editor = MatchEditor::new(&start);
        let steps = case["steps"].as_array().unwrap();
        check(&editor, &steps[0]["state"], &format!("case {c} start"));
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("case {c} step {s}: {}", step["do"]);
            let (kind, v) = (step["do"][0].as_str().unwrap(), &step["do"][1]);
            match kind {
                "type" => editor.set_type(MATCH_TYPES.iter().position(|t| t == v).unwrap()),
                "fixed" => editor.fixed = v.as_str().unwrap().to_owned(),
                "regex" => editor.regex = v.as_str().unwrap().to_owned(),
                "flexible" => {
                    editor.flexible = CHARACTER_SETS.iter().find(|(_, l)| l == v).unwrap().0;
                }
                "min" => editor.min_chars = count(v),
                "max" => editor.max_chars = count(v),
                "example" => editor.example = v.as_str().unwrap().to_owned(),
                other => panic!("{other}"),
            }
            check(&editor, &step["state"], &context);
        }
    }
}
