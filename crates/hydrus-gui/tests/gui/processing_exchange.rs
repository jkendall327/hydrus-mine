//! The string processor editor's step queue's "export", "import" and "duplicate", replayed
//! against the reference's own queue (`oracle/fixtures/processing_exchange_export.json`, from
//! `oracle/record_processing_exchange_export.py`): the clipboard JSON for no step, one, and two
//! selected out of order (always in queue order), the png panel and the png of the selection,
//! and what each paste, the duplicate and each png import say and do to the queue and the
//! example output.
use std::{cell::RefCell, rc::Rc};

use hydrus_core::url::strings::StringProcessor;
use hydrus_gui::string_processor_window::{Slots, open};
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};
use slint::Model as _;

use crate::subscriptions::store;
use hydrus_gui::headless;

fn processor(value: &Value) -> StringProcessor {
    string_processor(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap()).unwrap()
}

fn recorded() -> Value {
    hydrus_testkit::fixture_json("processing_exchange_export.json")
}

struct Editing {
    _dirs: [tempfile::TempDir; 2],
    window: hydrus_gui::StringProcessorWindow,
    slots: Slots,
    accepted: Rc<RefCell<Option<StringProcessor>>>,
}

fn editing(start: &Value) -> Editing {
    let (dirs, store) = store();
    let slots = Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let window = open(
        &store,
        &processor(&start["processor"]),
        vec!["a,b".into()],
        &slots,
        Rc::new({
            let accepted = accepted.clone();
            move |value| *accepted.borrow_mut() = Some(value)
        }),
    )
    .unwrap();
    Editing {
        _dirs: dirs,
        window,
        slots,
        accepted,
    }
}

fn select(e: &Editing, rows: &[usize]) {
    // (a control-click on each selected row clears the selection first)
    for row in selected(e) {
        e.window
            .invoke_row_clicked(i32::try_from(row).unwrap(), true, false);
    }
    for (n, row) in rows.iter().enumerate() {
        // (a click, then control-clicks for the others)
        e.window
            .invoke_row_clicked(i32::try_from(*row).unwrap(), n > 0, false);
    }
}

fn processed(e: &Editing) -> Vec<String> {
    e.window
        .get_processed()
        .iter()
        .map(|r| r.cells.row_data(0).unwrap().to_string())
        .collect()
}

fn selected(e: &Editing) -> Vec<usize> {
    e.window
        .get_steps()
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.selected.then_some(i))
        .collect()
}

fn copier() -> Rc<RefCell<Vec<String>>> {
    let copied: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    copied
}

/// The queue as the recording states it: its number of steps, the
/// example's output and the selected rows.
fn assert_state(e: &Editing, state: &Value, what: &str) {
    assert_eq!(
        e.window.get_steps().row_count(),
        state["processor"][2][2].as_array().unwrap().len(),
        "{what}: the steps"
    );
    assert_eq!(
        json!(processed(e)),
        state["processed"],
        "{what}: the example's output"
    );
    assert_eq!(
        json!(selected(e)),
        state["selected"],
        "{what}: the selection"
    );
}

// leaf: audit-network-processor-exchange
#[test]
fn selected_steps_export_import_and_duplicate_as_the_reference_queue_does() {
    let rec = recorded();
    let _windows = headless::init();
    let copied = copier();

    // to clipboard: the selection, in queue order, as the reference's text
    for export in rec["exports"].as_array().unwrap() {
        let e = editing(&rec["start"]);
        let rows: Vec<usize> = serde_json::from_value(export["selected"].clone()).unwrap();
        select(&e, &rows);
        let before = copied.borrow().len();
        e.window.invoke_exchange("to clipboard".into());
        let text = copied.borrow().get(before).cloned();
        if rows.is_empty() {
            // nothing selected: the reference puts nothing on the clipboard
            assert_eq!(text, None);
            continue;
        }
        assert_eq!(
            serde_json::from_str::<Value>(&text.expect("copied")).unwrap(),
            export["clipboard"],
            "{rows:?}"
        );
    }

    // to png: the reference's panel, and its png's objects
    let png = &rec["png"];
    let e = editing(&rec["start"]);
    select(&e, &[0, 2]);
    e.window.invoke_exchange("to png".into());
    let panel = e.slots.png.window().expect("the png export opens");
    assert_eq!(panel.get_png_title(), png["title"].as_str().unwrap());
    assert_eq!(
        panel.get_payload_description(),
        png["payload_description"].as_str().unwrap()
    );
    assert_eq!(
        panel.get_description(),
        png["description"].as_str().unwrap()
    );
    assert_eq!(
        i64::from(panel.get_png_width()),
        png["width"].as_i64().unwrap()
    );
    assert_eq!(panel.get_path(), png["path"].as_str().unwrap());
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(png["png_file"].as_str().unwrap());
    panel.set_path(path.to_str().unwrap().into());
    panel.invoke_action("update".into());
    panel.invoke_action("export".into());
    assert!(panel.get_done(), "{}", panel.get_error());
    panel.invoke_action("close".into());
    let ours =
        hydrus_downloader_exchange::processing::decode_png(&std::fs::read(&path).unwrap()).unwrap();
    let theirs = hydrus_downloader_exchange::processing::decode_png(
        &std::fs::read(hydrus_testkit::fixture_path(
            "processing_exchange_export.png",
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(ours, theirs);
    assert_eq!(
        ours,
        hydrus_downloader_exchange::processing::decode_text(&png["png_payload"].to_string())
            .unwrap()
    );

    // from clipboard, on one queue as the reference's: appended at once,
    // saying what the reference says
    let e = editing(&rec["start"]);
    for paste in rec["pastes"].as_array().unwrap() {
        let name = paste["name"].as_str().unwrap();
        let text = paste["text"].as_str().map(str::to_owned);
        // (no text at all is the reference's `DataMissing`, a clipboard that can't be read)
        hydrus_gui::set_clipboard_reader(move || {
            text.clone()
                .map(Some)
                .ok_or_else(|| "There was no text on the clipboard!".to_owned())
        });
        select(&e, &[]);
        e.window.invoke_exchange("from clipboard".into());
        assert_eq!(
            Value::from(crate::rules_exchange::said()),
            paste["messages"],
            "{name}"
        );
        assert_state(&e, &paste["state"], name);
    }
    hydrus_gui::clear_clipboard_reader();

    // duplicate: the selected again, at the end; the selection stays
    let duplicate = &rec["duplicate"];
    let rows: Vec<usize> = serde_json::from_value(duplicate["selected"].clone()).unwrap();
    select(&e, &rows);
    e.window.invoke_exchange("duplicate".into());
    assert_eq!(
        Value::from(crate::rules_exchange::said()),
        duplicate["messages"],
        "duplicate"
    );
    assert_state(&e, &duplicate["state"], "duplicate");

    // from pngs: the reference's png, and a file that is no png
    let files: Rc<RefCell<Vec<std::path::PathBuf>>> = Rc::default();
    let titles: Rc<RefCell<Vec<String>>> = Rc::default();
    hydrus_gui::set_picker({
        let files = files.clone();
        let titles = titles.clone();
        move |_, title| {
            titles.borrow_mut().push(title.to_owned());
            files.borrow().clone()
        }
    });
    let bad = folder.path().join("bad.png");
    std::fs::write(&bad, b"not a png").unwrap();
    let mut last = &duplicate["state"];
    for import in rec["png_imports"].as_array().unwrap() {
        let name = import["name"].as_str().unwrap();
        let path = if name.contains("no png") {
            bad.clone()
        } else {
            hydrus_testkit::fixture_path("processing_exchange_export.png")
        };
        *files.borrow_mut() = vec![path.clone()];
        select(&e, &[]);
        e.window.invoke_exchange("from pngs".into());
        // (the reference's message names its own temporary file)
        let mut expected = import["messages"].clone();
        if let Some(text) = expected[0]["text"].as_str()
            && text.ends_with("did not appear to be a valid image!")
        {
            expected[0]["text"] =
                format!("\"{}\" did not appear to be a valid image!", path.display()).into();
        }
        assert_eq!(
            Value::from(crate::rules_exchange::said()),
            expected,
            "{name}"
        );
        assert_state(&e, &import["state"], name);
        last = &import["state"];
    }
    // what the queue holds after all of it is the reference's
    e.window.invoke_apply();
    assert_eq!(
        e.accepted.borrow().as_ref().unwrap(),
        &processor(&last["processor"])
    );
    assert_eq!(
        *titles.borrow(),
        vec![rec["png_picker_title"].as_str().unwrap(); 2]
    );
}
