//! The string converter editor and its conversion editor, replayed step by
//! step against the reference's (`oracle/fixtures/string_converter_editor.json`,
//! from `oracle/record_string_converter_editor.py`): the numbered rows with
//! the example converted up to each, the selection and moves, what each
//! conversion editor is given and shows, what "ok" asks, and the values.
//! Random results are checked structurally. Deterministic date execution
//! and invalid input are covered by the separate date recorder replay.

use hydrus_core::url::strings::{Conversion, ProcessingStep, StringConverter};
use hydrus_gui_model::string_editors::{
    CONVERSION_TYPES, ConversionEditor, ConverterEditor, ENCODINGS, HASH_FUNCTIONS, TIMEZONES,
};
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};

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

/// An error's text up to its reason.
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
        // Random conversions are the only successful values with unstable text.
        assert_eq!(
            ours.chars().count(),
            theirs.chars().count(),
            "{context}: {ours} {theirs}"
        );
    }
}

fn check_conversion(editor: &ConversionEditor, state: &Value, context: &str) {
    assert_eq!(CONVERSION_TYPES[editor.kind].1, state["type"], "{context}");
    let shown = editor.shown();
    assert_eq!(
        json!({
            "text": shown.text, "number": shown.number, "encoding": shown.encoding,
            "decoding": shown.decoding, "regex": shown.regex, "date_link": shown.date_link,
            "timezone_decode": shown.timezone_decode, "timezone_offset": shown.timezone_offset,
            "timezone_encode": shown.timezone_encode, "hash": shown.hash,
            "dateparser": shown.dateparser,
        }),
        state["shown"],
        "{context}"
    );
    assert_eq!(editor.text, state["text"], "{context}");
    assert_eq!(json!(editor.number), state["number"], "{context}");
    assert_eq!(ENCODINGS[editor.encoding].1, state["encoding"], "{context}");
    assert_eq!(ENCODINGS[editor.decoding].1, state["decoding"], "{context}");
    assert_eq!(editor.pattern, state["pattern"], "{context}");
    assert_eq!(editor.replacement, state["replacement"], "{context}");
    assert_eq!(
        TIMEZONES[editor.timezone_decode], state["timezone_decode"],
        "{context}"
    );
    assert_eq!(json!(editor.timezone_offset), state["offset"], "{context}");
    assert_eq!(
        TIMEZONES[editor.timezone_encode], state["timezone_encode"],
        "{context}"
    );
    assert_eq!(HASH_FUNCTIONS[editor.hash].1, state["hash"], "{context}");
    assert_eq!(editor.example, state["example"], "{context}");
    let value = editor.value();
    assert_eq!(
        value,
        converter(&state["value"]).conversions[0],
        "{context}"
    );
    same_result(
        &editor.result(),
        state["result"].as_str().unwrap(),
        random_or_date(&value)
            && (state["result"].as_str().unwrap().starts_with("ERROR:")
                || matches!(value, Conversion::AppendRandom { .. })),
        context,
    );
}

fn edit(editor: &mut ConversionEditor, edits: &serde_json::Map<String, Value>) {
    let index = |choices: &[&str], v: &Value| choices.iter().position(|c| c == v).unwrap();
    for (key, v) in edits {
        match key.as_str() {
            "type" => editor.set_kind(index(&CONVERSION_TYPES.map(|(_, l)| l), v)),
            "text" => v.as_str().unwrap().clone_into(&mut editor.text),
            "number" => editor.set_number(v.as_i64().unwrap()),
            "encoding" => editor.encoding = index(&ENCODINGS.map(|(_, l)| l), v),
            "decoding" => editor.decoding = index(&ENCODINGS.map(|(_, l)| l), v),
            "pattern" => v.as_str().unwrap().clone_into(&mut editor.pattern),
            "replacement" => v.as_str().unwrap().clone_into(&mut editor.replacement),
            "timezone_decode" => editor.timezone_decode = index(&TIMEZONES, v),
            "offset" => editor.timezone_offset = v.as_i64().unwrap(),
            "timezone_encode" => editor.timezone_encode = index(&TIMEZONES, v),
            "hash" => editor.hash = index(&HASH_FUNCTIONS.map(|(_, l)| l), v),
            "ok" => {}
            other => panic!("{other}"),
        }
    }
}

/// A conversion editor opened, edited (or cancelled) and "ok"ed, against
/// the recording; the conversion it gives, if any.
fn edit_conversion(
    mut editor: ConversionEditor,
    edits: &Value,
    said: &[Value],
    opened: &[Value],
    context: &str,
) -> Option<Conversion> {
    let record = &opened[0];
    check_conversion(&editor, &record["opened"], context);
    let edits = edits.as_object()?;
    edit(&mut editor, edits);
    check_conversion(&editor, &record["edited"], context);
    let asked: Vec<&Value> = said.iter().filter_map(|s| s.get("asked")).collect();
    match editor.ok_question() {
        Some(question) => assert_eq!(asked, [question], "{context}"),
        None => assert!(asked.is_empty(), "{context}"),
    }
    let ok = editor.ok_question().is_none() || edits.get("ok") != Some(&json!(false));
    assert_eq!(json!(ok), record["ok"], "{context}");
    ok.then(|| editor.value())
}

fn check(editor: &ConverterEditor, state: &Value, context: &str) {
    let value = editor.value();
    assert_eq!(value, converter(&state["value"]), "{context}");
    let rows = editor.rows();
    let recorded = state["rows"].as_array().unwrap();
    assert_eq!(rows.len(), recorded.len(), "{context}");
    for (i, (row, theirs)) in rows.iter().zip(recorded).enumerate() {
        assert_eq!(row[0], theirs[0], "{context}");
        assert_eq!(row[1], theirs[1], "{context}");
        let loose = value.conversions[..=i].iter().any(|v| {
            random_or_date(v)
                && (theirs[2].as_str().unwrap().starts_with("ERROR:")
                    || matches!(v, Conversion::AppendRandom { .. }))
        });
        same_result(&row[2], theirs[2].as_str().unwrap(), loose, context);
    }
    assert_eq!(json!(editor.selected()), state["selected"], "{context}");
    assert_eq!(editor.can_move_up(), state["can_up"], "{context}");
    assert_eq!(editor.can_move_down(), state["can_down"], "{context}");
}

// leaf: audit-network-converter-sequence
// leaf: audit-network-conversion-ordinary
// leaf: audit-network-conversion-encoding
#[test]
fn the_string_converter_editor_works_as_the_references_does() {
    replay("string_converter_editor.json");
}

#[test]
fn date_controls_and_live_preview_follow_real_qt_accept_cancel_and_reorder() {
    replay("string_date_editor.json");
}

fn replay(fixture: &str) {
    let recorded = hydrus_testkit::fixture_json(fixture);
    for (c, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let mut editor = ConverterEditor::new(
            &converter(&case["converter"]),
            case["example"].as_str().map(str::to_owned),
        );
        let mut last_used: Option<Conversion> = None;
        let steps = case["steps"].as_array().unwrap();
        check(&editor, &steps[0]["state"], &format!("case {c} start"));
        for (s, step) in steps.iter().enumerate().skip(1) {
            let context = format!("case {c} step {s}: {}", step["do"]);
            let action = step["do"].as_array().unwrap();
            let said = step["said"].as_array().unwrap();
            let opened = step["opened"].as_array().unwrap();
            match action[0].as_str().unwrap() {
                "click" => editor.click(
                    usize::try_from(action[1].as_u64().unwrap()).unwrap(),
                    action[2].as_bool().unwrap(),
                ),
                "example" => action[1].as_str().unwrap().clone_into(&mut editor.example),
                "add" => {
                    assert_eq!(said[0], json!({"dialog": "edit conversion"}), "{context}");
                    let adding = editor.adding(last_used.as_ref());
                    if let Some(conversion) =
                        edit_conversion(adding, &action[1], said, opened, &context)
                    {
                        last_used = Some(conversion.clone());
                        editor.add(conversion);
                    }
                }
                "edit" => {
                    assert_eq!(said[0], json!({"dialog": "edit conversion"}), "{context}");
                    let (index, editing) = editor.editing().unwrap();
                    if let Some(conversion) =
                        edit_conversion(editing, &action[1], said, opened, &context)
                    {
                        last_used = Some(conversion.clone());
                        editor.replace(index, conversion);
                    }
                }
                "delete" => {
                    let question = editor.delete_question();
                    assert_eq!(
                        said,
                        &question
                            .map(|q| vec![json!({"asked": q})])
                            .unwrap_or_default(),
                        "{context}"
                    );
                    editor.delete();
                }
                "up" => {
                    if editor.can_move_up() {
                        editor.move_selected(-1);
                    }
                }
                "down" => {
                    if editor.can_move_down() {
                        editor.move_selected(1);
                    }
                }
                other => panic!("{other}"),
            }
            check(&editor, &step["state"], &context);
        }
    }
}

#[test]
fn last_conversion_loads_reference_options_then_native_override() {
    use hydrus_store::string_conversion::{LastStringConversion, load};
    let fixture = hydrus_testkit::fixture_json("string_conversion_preference.json");
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT); CREATE TABLE legacy_objects(source TEXT, type_id INTEGER, version INTEGER, dump TEXT);").unwrap();
    assert_eq!(load(&conn).unwrap(), LastStringConversion::default());
    for step in fixture["steps"].as_array().unwrap() {
        conn.execute("DELETE FROM legacy_objects", []).unwrap();
        conn.execute(
            "INSERT INTO legacy_objects VALUES ('json_dumps',22,8,?)",
            [step["options"][2].to_string()],
        )
        .unwrap();
        let expected =
            hydrus_downloader_exchange::processing::decode_text(&step["saved"].to_string())
                .unwrap();
        let ProcessingStep::Convert(expected) = &expected[0] else {
            panic!("reference preference is a converter")
        };
        assert_eq!(
            load(&conn).unwrap().0.as_ref(),
            expected.conversions.first()
        );
    }
    let saved = LastStringConversion(Some(Conversion::Append("native".into())));
    hydrus_store::settings::set(&conn, &saved).unwrap();
    assert_eq!(load(&conn).unwrap(), saved);
    hydrus_store::settings::set(&conn, &LastStringConversion(None)).unwrap();
    assert_eq!(load(&conn).unwrap(), LastStringConversion(None));
}
