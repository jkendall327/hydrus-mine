//! The string conversion editor's "datestring to timestamp (easy)" step: its
//! preview of the example, driven through the processor, converter and
//! conversion windows as a user opens them, against the reference's real
//! easy-conversion outputs (a dateparser-less install, plus English relative dates) (`oracle/record_dateparser_corpus.py`). The preview
//! is of forms that name their own time zone, so that it doesn't depend on
//! the machine's clock or zone; the rest of the corpus is replayed against
//! the parser itself in hydrus-core (`string_dates.rs`).

use std::rc::Rc;

use hydrus_core::url::strings::{Conversion, ProcessingStep, StringConverter, StringProcessor};
use hydrus_gui::{headless, string_processor_window};
use slint::ComponentHandle as _;

/// The preview the conversion window shows for `example`.
fn preview(store: &std::sync::Arc<hydrus_store::Store>, example: &str) -> (String, String) {
    let slots = string_processor_window::Slots::default();
    let processor = StringProcessor {
        steps: vec![ProcessingStep::Convert(StringConverter {
            conversions: vec![Conversion::DateParse],
            example: example.into(),
        })],
    };
    let w = string_processor_window::open(
        store,
        &processor,
        vec![example.into()],
        &slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *slots.processor.borrow_mut() = Some(w.clone_strong());
    w.invoke_row_clicked(0, false, false);
    w.invoke_edit();
    let converter = slots.converter.borrow().as_ref().unwrap().clone_strong();
    converter.invoke_row_clicked(0, false, false);
    converter.invoke_edit();
    let conversion = slots.conversion.borrow().as_ref().unwrap().clone_strong();
    let shown = (
        conversion.get_example().to_string(),
        conversion.get_result().to_string(),
    );
    assert!(
        !conversion.get_dateparser_note().is_empty(),
        "the easy parser says it is dateparser"
    );
    w.invoke_cancel();
    shown
}

/// An error's text up to its reason (hydrus-rs words the reasons of some
/// date errors its own way; this one it words the same).
fn before_reason(text: &str) -> &str {
    text.split("\": ").next().unwrap_or(text)
}

// leaf: audit-network-conversion-dateparser
#[test]
fn the_easy_date_conversions_preview_is_dateparsers_for_the_recorded_forms() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("dateparser_corpus.json");
    // forms that carry their zone or offset (so they are the same anywhere),
    // Unix timestamps, and text that isn't a date
    let independent = |case: &serde_json::Value| {
        let text = case["text"].as_str().unwrap();
        let zone = [
            "Z",
            "+00:00",
            "-05:00",
            "+02:00",
            "+0200",
            "-0500",
            " UTC",
            " GMT",
            "+09:00",
            "UTC+9",
            "GMT+2",
            "Europe/Paris",
        ]
        .iter()
        .any(|z| text.ends_with(z));
        zone || case["group"] == "junk"
            || case["group"] == "other_languages"
            || text == "02/30/2024"
    };
    let mut checked = 0;
    for case in recorded["cases"].as_array().unwrap() {
        if !independent(case) {
            continue;
        }
        let text = case["text"].as_str().unwrap();
        // (dateutil's odd readings of non-dates are listed in the parser's own
        // test)
        if matches!(text, "-1" | "1 2 3" | "120" | "010203") {
            continue;
        }
        let (example, result) = preview(&store, text);
        assert_eq!(example, text);
        let theirs = case["result"].as_str().unwrap();
        if case["error"] == true {
            assert_eq!(before_reason(&result), before_reason(theirs), "{text:?}");
            assert!(result.starts_with("ERROR:"), "{text:?}: {result}");
        } else {
            assert_eq!(result, theirs, "{text:?}");
        }
        checked += 1;
    }
    assert!(checked >= 30, "{checked} forms previewed");
}
