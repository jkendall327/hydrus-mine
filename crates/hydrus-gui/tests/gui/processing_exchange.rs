//! The string processor editor's export of selected steps and its paste, replayed against the
//! reference's own queue (`oracle/fixtures/processing_exchange_export.json`, from
//! `oracle/record_processing_exchange_export.py`): the clipboard JSON for no step, one, and two
//! selected out of order (always in queue order), the PNG of the selection, and what each paste
//! does to the queue and the example output.
use std::{cell::RefCell, rc::Rc};

use hydrus_core::url::strings::StringProcessor;
use hydrus_gui::string_processor_window::{Slots, open};
use hydrus_legacy::objects::domain::string_processor;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

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

#[test]
fn selected_steps_export_and_paste_as_the_reference_queue_does() {
    let rec = recorded();
    let _windows = headless::init();

    // export: the selection, in queue order, as the clipboard JSON
    for export in rec["exports"].as_array().unwrap() {
        let e = editing(&rec["start"]);
        let rows: Vec<usize> = serde_json::from_value(export["selected"].clone()).unwrap();
        select(&e, &rows);
        assert_eq!(selected(&e), {
            let mut sorted = rows.clone();
            sorted.sort_unstable();
            sorted
        });
        e.window.invoke_exchange("export".into());
        if rows.is_empty() {
            // nothing selected: the reference puts nothing on the clipboard
            assert!(
                e.slots.exchange.0.borrow().is_none(),
                "an export window opened for no selection"
            );
            continue;
        }
        let window = e.slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
        assert_eq!(
            serde_json::from_str::<Value>(window.get_text().as_str()).unwrap(),
            export["clipboard"],
            "{rows:?}"
        );
        window.invoke_action("cancel".into());
    }

    // export to png: the selection's PNG carries the same objects as the reference's
    let png = &rec["png"];
    let e = editing(&rec["start"]);
    select(&e, &[0, 2]);
    e.window.invoke_exchange("export".into());
    let window = e.slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(png["png_file"].as_str().unwrap());
    window.set_path(path.to_str().unwrap().into());
    window.invoke_action("save".into());
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
    window.invoke_action("cancel".into());

    // paste: what each clipboard does to the queue and its example output
    let e = editing(&rec["start"]);
    for paste in rec["pastes"].as_array().unwrap() {
        let name = paste["name"].as_str().unwrap();
        let text = paste["text"].as_str().map(str::to_owned);
        hydrus_gui::set_clipboard_reader(move || Ok(text.clone()));
        let before = e.window.get_steps().row_count();
        e.window.invoke_exchange("paste".into());
        let child = e.slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
        child.invoke_action("review".into());
        let recorded_steps = paste["state"]["processor"][2][2].as_array().unwrap().len();
        if child.get_ready() {
            let added = recorded_steps - before;
            assert!(
                child
                    .get_review()
                    .contains(&format!("Append {added} processing steps")),
                "{name}: {}",
                child.get_review()
            );
            child.invoke_action("accept".into());
        } else {
            assert_eq!(recorded_steps, before, "{name}: nothing was added");
            child.invoke_action("cancel".into());
        }
        assert_eq!(e.window.get_steps().row_count(), recorded_steps, "{name}");
        assert_eq!(json!(processed(&e)), paste["state"]["processed"], "{name}");
        assert_eq!(json!(selected(&e)), paste["state"]["selected"], "{name}");
    }
    e.window.invoke_apply();
    let value = e.accepted.borrow().clone().unwrap();
    assert_eq!(
        value.steps.len(),
        rec["pastes"].as_array().unwrap().last().unwrap()["state"]["processor"][2][2]
            .as_array()
            .unwrap()
            .len()
    );
    hydrus_gui::clear_clipboard_reader();
}
