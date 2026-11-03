//! The parser test panels' results wording against the reference's, for the
//! cases `oracle/record_parser_preview_format.py` records
//! (`oracle/fixtures/parser_preview_format.json`): local times at the recorded
//! zone's offset, Python's integer and hash decoding and their errors, tags
//! that clean to nothing, nested and merged posts, and parse errors. The model
//! is given the recorded offset; the GUI test replays the same recording
//! through the windows in that zone.
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_gui_model::parser_editors::ContentEditor;
use hydrus_gui_model::parser_preview::{content_preview, page_preview};
use hydrus_legacy::{objects::parsers, serialisable::SerialisableObject};
use hydrus_parse::formula::ParsingContext;

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("parser_preview_format.json")
}

fn offset(recorded: &serde_json::Value) -> jiff::tz::Offset {
    let seconds = recorded["offset_seconds"].as_i64().unwrap();
    jiff::tz::Offset::from_seconds(i32::try_from(seconds).unwrap()).unwrap()
}

/// The reference's text with its Python traceback line taken off (hydrus-rs
/// has no traceback to show).
fn without_trace(text: &str) -> &str {
    text.strip_suffix("\nTraceback (most recent call last):")
        .unwrap_or(text)
}

#[test]
fn content_results_are_worded_as_the_reference_words_them() {
    let recorded = recorded();
    let offset = offset(&recorded);
    let mut compared = 0;
    for case in recorded["content"].as_array().unwrap() {
        let document = case["document"].as_str().unwrap();
        if document.is_empty() {
            continue; // the window's own check (the GUI test)
        }
        let name = case["name"].as_str().unwrap();
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let parser = parsers::content_parser(&object).unwrap();
        let editor = ContentEditor::new(
            &parser,
            FormulaTestData {
                text: document.into(),
                ..FormulaTestData::default()
            },
        );
        let shown = content_preview(&parser.name, editor.preview(), offset);
        assert_eq!(
            shown,
            without_trace(case["results"].as_str().unwrap()),
            "{name}"
        );
        compared += 1;
    }
    assert!(compared >= 30, "{compared}");
}

#[test]
fn page_results_are_laid_out_as_the_reference_lays_them_out() {
    let recorded = recorded();
    let offset = offset(&recorded);
    for case in recorded["page"].as_array().unwrap() {
        let document = case["document"].as_str().unwrap();
        if document.is_empty() {
            continue;
        }
        let name = case["name"].as_str().unwrap();
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let parser = parsers::page_parser(&object).unwrap();
        let shown = page_preview(parser.parse(&mut ParsingContext::new(), document), offset);
        assert_eq!(
            shown,
            without_trace(case["results"].as_str().unwrap()),
            "{name}"
        );
    }
}
