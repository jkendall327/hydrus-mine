//! The parser test panels' results text against the reference's, for the cases
//! `oracle/record_parser_preview_format.py` records
//! (`oracle/fixtures/parser_preview_format.json`): through the content, page
//! and subsidiary page parser editor windows, 'test parse' pressed on each
//! recorded parser and document.
//!
//! The reference shows times in the machine's local time, and the recording
//! was made in Asia/Kolkata (no summer time, so its offset holds whenever this
//! runs). `TZ` is read when a process starts, so the windows are driven in a
//! child run of this test binary with `TZ` set to that zone in its POSIX form
//! (`IST-5:30`, which needs no time zone database).
use hydrus_gui::{
    ParserEditWindow, headless,
    parser_editors_window::{self as windows, Slots},
};
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_legacy::{objects::parsers, serialisable::SerialisableObject};
use hydrus_parse::downloaders::Downloaders;
use hydrus_store::{Store, settings};
use slint::ComponentHandle as _;
use std::process::Command;
use std::rc::Rc;

const CHILD_VAR: &str = "HYDRUS_PARSER_PREVIEW_FORMAT_CHILD";
/// Asia/Kolkata, the recorded zone, as a POSIX `TZ`.
const POSIX_ZONE: &str = "IST-5:30";

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("parser_preview_format.json")
}

/// The reference's text with its Python traceback line taken off (hydrus-rs
/// has no traceback to show); every other line is compared.
fn without_trace(text: &str) -> &str {
    text.strip_suffix("\nTraceback (most recent call last):")
        .unwrap_or(text)
}

/// A store whose only page parser is the recorded one, and that parser's
/// editor opened from the parser list.
fn page_editor(tuple: &serde_json::Value) -> (tempfile::TempDir, Slots, ParserEditWindow) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
    let definitions = Downloaders {
        parsers: vec![parsers::page_parser(&object).unwrap()],
        ..Downloaders::default()
    };
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &definitions))
        .unwrap();
    let slots = Slots::default();
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = slots.page.borrow().as_ref().unwrap().clone_strong();
    (dir, slots, page)
}

fn test_parse(window: &ParserEditWindow, document: &str) -> String {
    window.set_document(document.into());
    window.invoke_action("test".into());
    window.get_preview().to_string()
}

/// Run in a child with `TZ` set: every recorded case through its window.
#[test]
fn child_replays_the_recorded_test_panels_in_the_recorded_zone() {
    let Ok(zone) = std::env::var(CHILD_VAR) else {
        return; // an ordinary run; the parent test drives this one
    };
    let recorded = recorded();
    assert_eq!(zone, POSIX_ZONE);
    assert_eq!(recorded["zone"], "Asia/Kolkata");
    assert_eq!(
        i64::from(hydrus_gui_model::parser_preview::local_offset().seconds()),
        recorded["offset_seconds"].as_i64().unwrap(),
        "the child runs at the recorded offset"
    );
    let _windows = headless::init();
    let mut compared = 0;
    for case in recorded["content"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let parser = parsers::content_parser(&object).unwrap();
        let slots = Slots::default();
        let window: ParserEditWindow = windows::open_content(
            &store,
            &parser,
            FormulaTestData::default(),
            &slots,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8],
            Rc::new(|_| Ok(())),
        )
        .unwrap();
        let shown = test_parse(&window, case["document"].as_str().unwrap());
        assert_eq!(
            shown,
            without_trace(case["results"].as_str().unwrap()),
            "content: {name}"
        );
        window.invoke_action("cancel".into());
        compared += 1;
    }
    for case in recorded["page"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let (_dir, _slots, page) = page_editor(&case["tuple"]);
        let shown = test_parse(&page, case["document"].as_str().unwrap());
        assert_eq!(
            shown,
            without_trace(case["results"].as_str().unwrap()),
            "page: {name}"
        );
        page.invoke_action("cancel".into());
        compared += 1;
    }
    for case in recorded["subsidiary"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let (_dir, slots, page) = page_editor(&case["parent_tuple"]);
        page.invoke_subsidiary_clicked(0, false, false);
        page.invoke_action("edit-subsidiary".into());
        let owned = (**slots.child.borrow().as_ref().unwrap()).clone();
        let subsidiary = owned.page.borrow().as_ref().unwrap().clone_strong();
        let shown = test_parse(&subsidiary, case["document"].as_str().unwrap());
        assert_eq!(
            shown,
            without_trace(case["results"].as_str().unwrap()),
            "subsidiary: {name}"
        );
        subsidiary.invoke_action("cancel".into());
        page.invoke_action("cancel".into());
        compared += 1;
    }
    println!("compared {compared} recorded test parses in {zone}");
}

// leaf: parser-context
#[test]
fn the_test_panels_word_and_lay_out_results_as_the_reference_does_in_local_time() {
    if std::env::var(CHILD_VAR).is_ok() {
        return;
    }
    let recorded = recorded();
    let zone = POSIX_ZONE;
    let out = Command::new(std::env::current_exe().unwrap())
        .env("TZ", zone)
        .env(CHILD_VAR, zone)
        .args([
            "--exact",
            "parser_preview_format::child_replays_the_recorded_test_panels_in_the_recorded_zone",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{zone}: {stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases = ["content", "page", "subsidiary"]
        .iter()
        .map(|kind| recorded[kind].as_array().unwrap().len())
        .sum::<usize>();
    assert!(
        stdout.contains(&format!("compared {cases} recorded test parses in {zone}")),
        "{zone}: the child did not replay every case: {stdout}"
    );
}
