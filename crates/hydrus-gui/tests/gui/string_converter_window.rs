//! The string converter window, and the conversion window it opens, put
//! through the reference's recordings
//! (`oracle/fixtures/string_converter_editor.json` and `string_date_editor.json`)
//! as a user would: rows clicked, the example typed, "add" and "edit" opening
//! the conversion window, its controls set, its question answered. What the
//! windows show is read back at every step and the converter "apply" gives is
//! compared with the recording's last value.

use std::{cell::RefCell, rc::Rc};

use hydrus_core::url::strings::{Conversion, ProcessingStep, StringConverter};
use hydrus_gui::{ConversionWindow, StringConverterWindow, headless, string_processor_window};
use hydrus_gui_model::string_editors::{CONVERSION_TYPES, ENCODINGS, HASH_FUNCTIONS, TIMEZONES};
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn converter(value: &Value) -> StringConverter {
    let processor =
        string_processor(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap();
    let ProcessingStep::Convert(converter) = &processor.steps[0] else {
        panic!("{processor:?}")
    };
    converter.clone()
}

/// Whether a conversion's result can't be checked whole: random text, or a
/// date (whose error hydrus-rs words its own way).
fn random_or_date(conversion: &Conversion) -> bool {
    matches!(conversion, Conversion::AppendRandom { population, count } if !population.is_empty() && *count > 0)
        || matches!(
            conversion,
            Conversion::Unsupported { .. }
                | Conversion::DateDecode { .. }
                | Conversion::DateEncode { .. }
                | Conversion::DateParse
        )
}

fn before_reason(text: &str) -> Option<&str> {
    let at = text.find("\": ")?;
    text.starts_with("ERROR: Could not apply ")
        .then(|| &text[..at])
}

fn same_result(ours: &str, theirs: &str, loose: bool, context: &str) {
    if !loose {
        assert_eq!(ours, theirs, "{context}");
    } else if let Some(theirs) = before_reason(theirs) {
        assert_eq!(before_reason(ours), Some(theirs), "{context}");
    } else {
        assert_eq!(
            ours.chars().count(),
            theirs.chars().count(),
            "{context}: {ours} {theirs}"
        );
    }
}

fn label(model: &slint::ModelRc<slint::SharedString>, i: i32) -> String {
    model
        .row_data(usize::try_from(i).unwrap())
        .unwrap()
        .to_string()
}

/// What the conversion window shows, against what the reference's showed.
fn check_conversion(window: &ConversionWindow, state: &Value, value: &Conversion, context: &str) {
    assert_eq!(
        label(&window.get_types(), window.get_kind()),
        state["type"],
        "{context}"
    );
    assert_eq!(
        CONVERSION_TYPES[usize::try_from(window.get_kind()).unwrap()].1,
        state["type"],
        "{context}"
    );
    let shown = &state["shown"];
    let note = |label: slint::SharedString| (!label.is_empty()).then(|| label.to_string());
    assert_eq!(
        note(window.get_text_label()),
        shown["text"].as_str().map(str::to_owned),
        "{context}"
    );
    assert_eq!(
        note(window.get_number_label()),
        shown["number"].as_str().map(str::to_owned),
        "{context}"
    );
    assert_eq!(window.get_show_encoding(), shown["encoding"], "{context}");
    assert_eq!(window.get_show_decoding(), shown["decoding"], "{context}");
    assert_eq!(window.get_show_regex(), shown["regex"], "{context}");
    assert_eq!(window.get_show_date_link(), shown["date_link"], "{context}");
    assert_eq!(
        window.get_show_timezone_decode(),
        shown["timezone_decode"],
        "{context}"
    );
    assert_eq!(
        window.get_show_timezone_offset(),
        shown["timezone_offset"],
        "{context}"
    );
    assert_eq!(
        window.get_show_timezone_encode(),
        shown["timezone_encode"],
        "{context}"
    );
    assert_eq!(window.get_show_hash(), shown["hash"], "{context}");
    assert_eq!(
        !window.get_dateparser_note().is_empty(),
        shown["dateparser"],
        "{context}"
    );
    assert_eq!(
        window.get_text(),
        state["text"].as_str().unwrap(),
        "{context}"
    );
    assert_eq!(json!(window.get_number()), state["number"], "{context}");
    assert_eq!(
        label(&window.get_encodings(), window.get_encoding()),
        state["encoding"],
        "{context}"
    );
    assert_eq!(
        label(&window.get_encodings(), window.get_decoding()),
        state["decoding"],
        "{context}"
    );
    assert_eq!(
        window.get_pattern(),
        state["pattern"].as_str().unwrap(),
        "{context}"
    );
    assert_eq!(
        window.get_replacement(),
        state["replacement"].as_str().unwrap(),
        "{context}"
    );
    let zone = |i: i32| TIMEZONES[usize::try_from(i).unwrap()];
    assert_eq!(
        zone(window.get_timezone_decode()),
        state["timezone_decode"],
        "{context}"
    );
    assert_eq!(
        json!(window.get_timezone_offset()),
        state["offset"],
        "{context}"
    );
    assert_eq!(
        zone(window.get_timezone_encode()),
        state["timezone_encode"],
        "{context}"
    );
    assert_eq!(
        label(&window.get_hashes(), window.get_hash()),
        state["hash"],
        "{context}"
    );
    assert_eq!(
        window.get_example(),
        state["example"].as_str().unwrap(),
        "{context}"
    );
    let recorded = state["result"].as_str().unwrap();
    same_result(
        &window.get_result(),
        recorded,
        random_or_date(value)
            && (recorded.starts_with("ERROR:") || matches!(value, Conversion::AppendRandom { .. })),
        context,
    );
}

/// Set one control of the conversion window as the recording set the
/// reference's, and tell the window.
fn edit(window: &ConversionWindow, key: &str, v: &Value) {
    let at = |n: usize| i32::try_from(n).unwrap();
    let index = |choices: &[&str], v: &Value| at(choices.iter().position(|c| c == v).unwrap());
    match key {
        "type" => window.set_kind(index(&CONVERSION_TYPES.map(|(_, l)| l), v)),
        "text" => window.set_text(v.as_str().unwrap().into()),
        "number" => window.set_number(i32::try_from(v.as_i64().unwrap()).unwrap()),
        "encoding" => window.set_encoding(index(&ENCODINGS.map(|(_, l)| l), v)),
        "decoding" => window.set_decoding(index(&ENCODINGS.map(|(_, l)| l), v)),
        "pattern" => window.set_pattern(v.as_str().unwrap().into()),
        "replacement" => window.set_replacement(v.as_str().unwrap().into()),
        "timezone_decode" => window.set_timezone_decode(index(&TIMEZONES, v)),
        "offset" => window.set_timezone_offset(i32::try_from(v.as_i64().unwrap()).unwrap()),
        "timezone_encode" => window.set_timezone_encode(index(&TIMEZONES, v)),
        "hash" => window.set_hash(index(&HASH_FUNCTIONS.map(|(_, l)| l), v)),
        "ok" => return,
        other => panic!("{other}"),
    }
    window.invoke_changed();
}

fn check_rows(window: &StringConverterWindow, state: &Value, context: &str) {
    let value = converter(&state["value"]);
    let recorded = state["rows"].as_array().unwrap();
    let rows = window.get_rows();
    assert_eq!(rows.row_count(), recorded.len(), "{context}");
    let mut selected = Vec::new();
    for (i, theirs) in recorded.iter().enumerate() {
        let row = rows.row_data(i).unwrap();
        let cell = |n: usize| row.cells.row_data(n).unwrap().to_string();
        assert_eq!(cell(0), theirs[0].as_str().unwrap(), "{context}");
        assert_eq!(cell(1), theirs[1].as_str().unwrap(), "{context}");
        let loose = value.conversions[..=i].iter().any(|v| {
            random_or_date(v)
                && (theirs[2].as_str().unwrap().starts_with("ERROR:")
                    || matches!(v, Conversion::AppendRandom { .. }))
        });
        same_result(&cell(2), theirs[2].as_str().unwrap(), loose, context);
        if row.selected {
            selected.push(i);
        }
    }
    assert_eq!(json!(selected), state["selected"], "{context}");
    assert_eq!(window.get_can_up(), state["can_up"], "{context}");
    assert_eq!(window.get_can_down(), state["can_down"], "{context}");
}

fn replay_case(fixture: &str, c: usize) {
    let recorded = hydrus_testkit::fixture_json(fixture);
    let case = &recorded["cases"][c];
    let _windows = headless::init();
    let slots = string_processor_window::Slots::default();
    let applied = Rc::new(RefCell::new(None));
    let window = string_processor_window::open_converter(
        &converter(&case["converter"]),
        case["example"].as_str().map(str::to_owned),
        &slots,
        {
            let applied = applied.clone();
            Rc::new(move |value| *applied.borrow_mut() = Some(value))
        },
    )
    .unwrap();
    let steps = case["steps"].as_array().unwrap();
    check_rows(&window, &steps[0]["state"], &format!("case {c} start"));
    for (s, step) in steps.iter().enumerate().skip(1) {
        let context = format!("case {c} step {s}: {}", step["do"]);
        let action = step["do"].as_array().unwrap();
        let said = step["said"].as_array().unwrap();
        let opened = step["opened"].as_array().unwrap();
        match action[0].as_str().unwrap() {
            "click" => window.invoke_row_clicked(
                i32::try_from(action[1].as_u64().unwrap()).unwrap(),
                action[2].as_bool().unwrap(),
                false,
            ),
            "example" => {
                window.set_example(action[1].as_str().unwrap().into());
                window.invoke_example_edited();
            }
            kind @ ("add" | "edit") => {
                assert_eq!(said[0], json!({"dialog": "edit conversion"}), "{context}");
                if kind == "add" {
                    window.invoke_add();
                } else {
                    window.invoke_edit();
                }
                let conversion = slots.conversion.borrow().as_ref().unwrap().clone_strong();
                // (what it opens with is the reference's own default, or its
                // last used conversion, or the one being edited)
                let record = &opened[0];
                // the conversion the recording had in the window at each point
                let before = converter(&record["opened"]["value"]).conversions[0].clone();
                check_conversion(&conversion, &record["opened"], &before, &context);
                let mut ok = true;
                if let Some(edits) = action[1].as_object() {
                    for (key, v) in edits {
                        edit(&conversion, key, v);
                    }
                    let after = converter(&record["edited"]["value"]).conversions[0].clone();
                    check_conversion(&conversion, &record["edited"], &after, &context);
                    let asked: Vec<&Value> = said.iter().filter_map(|s| s.get("asked")).collect();
                    if conversion.get_asking() {
                        assert_eq!(
                            asked,
                            [&json!(conversion.get_asking_message().to_string())],
                            "{context}"
                        );
                    } else {
                        conversion.invoke_apply();
                        if conversion.get_asking() {
                            assert_eq!(
                                asked,
                                [&json!(conversion.get_asking_message().to_string())],
                                "{context}"
                            );
                        } else {
                            assert!(asked.is_empty(), "{context}");
                        }
                    }
                    if conversion.get_asking() {
                        if edits.get("ok") == Some(&json!(false)) {
                            conversion.invoke_cancelled();
                            ok = false;
                        } else {
                            conversion.invoke_chosen(0);
                        }
                    }
                    assert_eq!(json!(ok), record["ok"], "{context}");
                } else {
                    ok = false;
                }
                if !ok {
                    // cancelled: the conversion window closes without giving anything
                    // (opened and cancelled without edits records no "ok" at all)
                    if action[1].is_object() {
                        assert_eq!(record["ok"], json!(false), "{context}");
                    }
                    conversion.invoke_cancel();
                }
                assert!(slots.conversion.borrow().is_none(), "{context}");
            }
            "delete" => {
                window.invoke_delete();
                let asked: Vec<Value> = if window.get_asking() {
                    vec![json!({"asked": window.get_asking_message().to_string()})]
                } else {
                    Vec::new()
                };
                assert_eq!(said, &asked, "{context}");
                if window.get_asking() {
                    window.invoke_chosen(0);
                }
            }
            "up" => {
                if window.get_can_up() {
                    window.invoke_up();
                }
            }
            "down" => {
                if window.get_can_down() {
                    window.invoke_down();
                }
            }
            other => panic!("{other}"),
        }
        check_rows(&window, &step["state"], &context);
    }
    // "apply" gives the converter the recording ended with
    window.invoke_apply();
    let last = converter(&steps[steps.len() - 1]["state"]["value"]);
    let given = applied.borrow().clone().expect("apply gives the converter");
    assert_eq!(given.conversions.len(), last.conversions.len(), "case {c}");
    for (given, last) in given.conversions.iter().zip(&last.conversions) {
        if !random_or_date(given) && !random_or_date(last) {
            assert_eq!(given, last, "case {c}");
        }
    }
}

// leaf: audit-network-converter-sequence
// leaf: audit-network-conversion-ordinary
// leaf: audit-network-conversion-encoding
#[test]
fn the_string_converter_window_works_as_the_references_does() {
    // (a thread each: the window remembers the last conversion made)
    let recorded = hydrus_testkit::fixture_json("string_converter_editor.json");
    for c in 0..recorded["cases"].as_array().unwrap().len() {
        std::thread::spawn(move || replay_case("string_converter_editor.json", c))
            .join()
            .unwrap_or_else(|e| std::panic::resume_unwind(e));
    }
}

#[test]
fn the_date_conversion_window_follows_the_recording() {
    std::thread::spawn(|| replay_case("string_date_editor.json", 0))
        .join()
        .unwrap_or_else(|e| std::panic::resume_unwind(e));
}
