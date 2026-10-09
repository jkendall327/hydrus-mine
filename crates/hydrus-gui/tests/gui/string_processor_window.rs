//! The string processor window, and the step windows it opens (splitter,
//! joiner, slicer, sorter), put through the reference's recording
//! (`oracle/fixtures/string_processor_editor.json`) as a user would: rows and
//! example tabs clicked, the example typed, "add" asking which type and
//! opening a step window, that window's controls set, "apply". What the windows
//! show is read back at every step, and the processor "apply" gives is the
//! recording's last value. Match, tag filter and converter steps are stood in
//! for by the recording itself (their editors are replayed by their own tests).

use std::{cell::RefCell, rc::Rc};

use hydrus_core::url::strings::{ProcessingStep, StringProcessor};
use hydrus_gui::{StringStepWindow, headless, string_processor_window};
use hydrus_gui_model::string_editors::{ADD_CHOICES, ADD_TITLE, SORT_TYPES, STEP_TITLE};
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_store::Store;
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn processor(value: &Value) -> StringProcessor {
    string_processor(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap()
}

fn strings(value: &Value) -> Vec<String> {
    serde_json::from_value(value.clone()).unwrap()
}

fn column(rows: &slint::ModelRc<hydrus_gui::TableRow>) -> Vec<String> {
    rows.iter()
        .map(|r| r.cells.row_data(0).unwrap().to_string())
        .collect()
}

fn check(window: &hydrus_gui::StringProcessorWindow, state: &Value, context: &str) {
    let rows = window.get_steps();
    assert_eq!(column(&rows), strings(&state["rows"]), "{context}");
    let selected: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.selected.then_some(i))
        .collect();
    assert_eq!(json!(selected), state["selected"], "{context}");
    assert_eq!(
        column(&window.get_processed()),
        strings(&state["processed"]),
        "{context}"
    );
    assert_eq!(
        window.get_example(),
        state["example"].as_str().unwrap(),
        "{context}"
    );
    let tabs: Vec<String> = window.get_tabs().iter().map(|t| t.to_string()).collect();
    let recorded: Vec<String> = state["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t[0].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(tabs, recorded, "{context}");
    // the tab shown lists its strings
    if let Some(tab) = state["tabs"]
        .as_array()
        .unwrap()
        .get(usize::try_from(window.get_tab_index()).unwrap())
    {
        assert_eq!(
            column(&window.get_tab_rows()),
            strings(&tab[1]),
            "{context}"
        );
    }
}

/// A step window's state as the recording wrote the model's.
fn step_state(window: &StringStepWindow) -> Value {
    let kind = window.get_kind();
    let rows = |m: slint::ModelRc<hydrus_gui::TableRow>| column(&m);
    let opt = |on: bool, n: i32| if on { json!(n) } else { Value::Null };
    match kind {
        0 => json!({"separator": window.get_separator().to_string(),
            "max_splits": opt(window.get_max_splits_on(), window.get_max_splits()),
            "example": window.get_split_example().to_string(), "results": rows(window.get_results()),
            "invalid": window.get_invalid()}),
        1 => json!({"joiner": window.get_joiner().to_string(),
            "tuple": opt(window.get_tuple_on(), window.get_tuple_size()),
            "summary": window.get_summary().to_string(), "texts": rows(window.get_texts()),
            "results": rows(window.get_results()), "invalid": window.get_invalid()}),
        2 => json!({"select": if window.get_select_type() == 0 { "single" } else { "range" },
            "single": window.get_single(),
            "start": opt(window.get_start_on(), window.get_start()),
            "end": opt(window.get_end_on(), window.get_end()),
            "summary": window.get_summary().to_string(), "texts": rows(window.get_texts()),
            "results": rows(window.get_results())}),
        3 => json!({"sort_type": SORT_TYPES[usize::try_from(window.get_sort_type()).unwrap()].1,
            "asc": window.get_ascending(),
            "regex": if window.get_regex_on() { json!(window.get_regex().to_string()) } else { Value::Null },
            "texts": rows(window.get_texts()), "results": rows(window.get_results())}),
        other => panic!("kind {other}"),
    }
}

/// The reference's regex errors are Python's own words; ours say why too.
fn same_state(ours: &Value, recorded: &Value, context: &str) {
    let mut ours = ours.clone();
    if let (Some(results), Some(recorded)) = (
        ours.get_mut("results").and_then(Value::as_array_mut),
        recorded["results"].as_array(),
    ) && let ([ours_error], [Value::String(theirs)]) =
        (results.as_mut_slice(), recorded.as_slice())
        && theirs.starts_with("Error: missing")
    {
        assert!(
            ours_error.as_str().unwrap().starts_with("Error: "),
            "{context}"
        );
        *ours_error = Value::String(theirs.clone());
    }
    assert_eq!(&ours, recorded, "{context}");
}

fn edit(window: &StringStepWindow, edits: &serde_json::Map<String, Value>) {
    let n = |v: &Value| i32::try_from(v.as_i64().unwrap()).unwrap();
    for (key, v) in edits {
        match (window.get_kind(), key.as_str()) {
            (0, "separator") => window.set_separator(v.as_str().unwrap().into()),
            (0, "max_splits") => {
                window.set_max_splits_on(!v.is_null());
                if !v.is_null() {
                    window.set_max_splits(n(v));
                }
            }
            (1, "joiner") => window.set_joiner(v.as_str().unwrap().into()),
            (1, "tuple") => {
                window.set_tuple_on(!v.is_null());
                if !v.is_null() {
                    window.set_tuple_size(n(v));
                }
            }
            (2, "select") => window.set_select_type(i32::from(v != "single")),
            (2, "single") => window.set_single(n(v)),
            (2, "start") => {
                window.set_start_on(!v.is_null());
                if !v.is_null() {
                    window.set_start(n(v));
                }
            }
            (2, "end") => {
                window.set_end_on(!v.is_null());
                if !v.is_null() {
                    window.set_end(n(v));
                }
            }
            (3, "sort_type") => window.set_sort_type(
                i32::try_from(SORT_TYPES.iter().position(|(_, name)| name == v).unwrap()).unwrap(),
            ),
            (3, "asc") => window.set_ascending(v.as_bool().unwrap()),
            (3, "regex") => {
                window.set_regex_on(!v.is_null());
                if let Some(regex) = v.as_str() {
                    window.set_regex(regex.into());
                }
            }
            (kind, key) => panic!("no field {key} on kind {kind}"),
        }
        window.invoke_changed();
    }
}

fn replay_case(c: usize) {
    let recorded = hydrus_testkit::fixture_json("string_processor_editor.json");
    let case = &recorded["cases"][c];
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = headless::init();
    let slots = string_processor_window::Slots::default();
    let applied = Rc::new(RefCell::new(None));
    let window = string_processor_window::open(
        &store,
        &processor(&case["processor"]),
        strings(&case["texts"]),
        &slots,
        {
            let applied = applied.clone();
            Rc::new(move |value| *applied.borrow_mut() = Some(value))
        },
    )
    .unwrap();
    let choices: Vec<Value> = ADD_CHOICES
        .iter()
        .map(|(_, label, description)| json!([label, description]))
        .collect();
    let steps = case["steps"].as_array().unwrap();
    check(&window, &steps[0]["state"], &format!("case {c} start"));
    for (s, step) in steps.iter().enumerate().skip(1) {
        let context = format!("case {c} step {s}: {}", step["do"]);
        let action = step["do"].as_array().unwrap();
        let said = step["said"].as_array().unwrap();
        let opened = step["opened"].as_array().unwrap();
        // a step window (or a stand-in's window) just opened: put the
        // recording's edits to it, then "apply" or cancel
        let step_window = |edits: &Value| {
            let step_window = slots
                .step
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            let Some(step_window) = step_window else {
                // a converter: stood in for, given back
                let converter = slots.converter.borrow().as_ref().unwrap().clone_strong();
                if edits.is_null() {
                    converter.invoke_cancel();
                } else {
                    converter.invoke_apply();
                }
                return;
            };
            if step_window.get_kind() >= 4 {
                // a match or tag filter: stood in for, given back
                if edits.is_null() {
                    step_window.invoke_cancel();
                } else {
                    step_window.invoke_apply();
                }
                return;
            }
            assert_eq!(step_window.get_window_title(), STEP_TITLE, "{context}");
            let record = &opened[0];
            same_state(&step_state(&step_window), &record["opened"], &context);
            let Some(edits) = edits.as_object() else {
                step_window.invoke_cancel();
                return;
            };
            edit(&step_window, edits);
            same_state(&step_state(&step_window), &record["edited"], &context);
            step_window.invoke_apply();
            match record.get("veto").and_then(Value::as_str) {
                Some(veto) => {
                    assert_eq!(step_window.get_veto(), veto, "{context}");
                    step_window.invoke_cancel();
                }
                None => assert!(slots.step.borrow().is_none(), "{context}"),
            }
        };
        match action[0].as_str().unwrap() {
            "click" => window.invoke_row_clicked(
                i32::try_from(action[1].as_u64().unwrap()).unwrap(),
                action[2].as_bool().unwrap(),
                false,
            ),
            "text" => {
                window.invoke_text_clicked(i32::try_from(action[1].as_u64().unwrap()).unwrap());
            }
            "example" => {
                window.set_example(action[1].as_str().unwrap().into());
                window.invoke_example_edited();
            }
            "up" => window.invoke_up(),
            "down" => window.invoke_down(),
            "delete" => {
                window.invoke_delete();
                if window.get_asking() {
                    assert_eq!(
                        said,
                        &[json!({"asked": window.get_asking_message().to_string()})],
                        "{context}"
                    );
                    window.invoke_chosen(0);
                } else {
                    assert!(said.is_empty(), "{context}");
                }
            }
            "add" => {
                window.invoke_add();
                assert!(window.get_asking(), "{context}");
                let asked: Vec<Value> = window
                    .get_asking_choices()
                    .iter()
                    .zip(&ADD_CHOICES)
                    .map(|(label, (_, _, description))| json!([label.to_string(), description]))
                    .collect();
                assert_eq!(window.get_asking_title(), ADD_TITLE, "{context}");
                assert_eq!(
                    said[0],
                    json!({"select": ADD_TITLE, "choices": choices}),
                    "{context}"
                );
                assert_eq!(asked, choices, "{context}");
                match action[1].as_str() {
                    Some(label) => {
                        assert_eq!(said[1], json!({"dialog": STEP_TITLE}), "{context}");
                        let at = ADD_CHOICES
                            .iter()
                            .position(|(_, l, _)| *l == label)
                            .unwrap();
                        window.invoke_chosen(i32::try_from(at).unwrap());
                        step_window(&action[2]);
                    }
                    None => window.invoke_cancelled(),
                }
            }
            "edit" => {
                assert_eq!(said, &[json!({"dialog": STEP_TITLE})], "{context}");
                window.invoke_edit();
                step_window(&action[1]);
            }
            other => panic!("{other}"),
        }
        check(&window, &step["state"], &context);
    }
    window.invoke_apply();
    let last = processor(&steps[steps.len() - 1]["state"]["value"]);
    // (a converter applied from the processor takes the processor's example:
    // docs/rust/DIFFERENCES.md, "The string processor editor")
    let same_examples = |mut p: StringProcessor| {
        for step in &mut p.steps {
            if let ProcessingStep::Convert(converter) = step {
                converter.example.clear();
            }
        }
        p
    };
    let given = applied.borrow().clone().map(same_examples);
    assert_eq!(given, Some(same_examples(last)), "case {c}");
}

// leaf: audit-network-processor-order
// leaf: audit-network-processor-results
// leaf: string-joiner
// leaf: string-slicer
// leaf: string-sorter
// leaf: string-splitter
#[test]
fn the_string_processor_window_works_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("string_processor_editor.json");
    for c in 0..recorded["cases"].as_array().unwrap().len() {
        std::thread::spawn(move || replay_case(c))
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e));
    }
}
