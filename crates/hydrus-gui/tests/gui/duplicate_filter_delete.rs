//! Deleting from the duplicate filter, as the reference's
//! `CanvasFilterDuplicates._Delete` does (`oracle/record_duplicate_filter_delete.py`,
//! on the auto-resolution fixture's pairs in batches of six): the top
//! hover's trash button and the delete key ask "Delete just this file, or
//! both?", then the delete files dialog, and the deletion is the pair's
//! decision; going back undoes it; the batch's end asks to commit its
//! deletes, which then delete the files with the recorded reasons. Each
//! recorded step is replayed with real clicks and keys, and the question,
//! its answers, the delete dialog's files, choices and reason, the index
//! string and the file shown are compared after it.

use slint::ComponentHandle as _;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{LogicalPosition, Model as _};

use hydrus_core::HashId;
use serde_json::Value;

use super::common::widgets;
use crate::auto_resolution_review::{Opened, opened};

fn shown(o: &Opened) -> Option<String> {
    hydrus_gui::shown_pair().map(|(a, _)| o.hex[&a].clone())
}

fn assert_now(o: &Opened, window: &hydrus_gui::DuplicateFilterWindow, now: &Value, what: &str) {
    assert_eq!(window.get_index_text(), now["index"].as_str().unwrap(), "{what}: the index");
    assert_eq!(shown(o).as_deref(), now["shown"].as_str(), "{what}: the file shown");
}

/// The trash button in the top hover's row, clicked.
fn click_trash(window: &hydrus_gui::DuplicateFilterWindow) {
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(500.0, 4.0),
    });
    widgets::settle();
    assert!(window.get_info_showing(), "the hover is raised");
    let at = (0..widgets::count(window.window(), "send to trash"))
        .map(|n| widgets::position(window.window(), "send to trash", n))
        .find(|at| at.y <= 40.0)
        .expect("the trash button");
    widgets::click_at(window.window(), at, PointerEventButton::Left);
    widgets::settle();
}

fn answers(window: &hydrus_gui::DuplicateFilterWindow) -> Vec<String> {
    window.get_answers().iter().map(|a| a.to_string()).collect()
}

// leaf: audit-media-duplicate-filter-top-hover
#[test]
fn the_duplicate_filter_deletes_this_file_or_both_as_the_reference_s_does() {
    let recorded = hydrus_testkit::fixture_json("duplicate_filter_delete.json");
    let _windows = hydrus_gui::headless::init();
    let o = opened();
    o.store
        .write(|ctx| {
            let mut settings: hydrus_store::duplicates::DuplicateFilterSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.max_batch_size = 6;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    o.ui.invoke_launch_filter();
    let window = o
        .bound
        .filter
        .borrow()
        .as_ref()
        .expect("the filter opens")
        .clone_strong();
    window
        .window()
        .set_size(slint::LogicalSize::new(1000.0, 750.0));
    widgets::settle();
    let steps = recorded["steps"].as_array().unwrap();
    assert_now(&o, &window, &steps[0]["start"], "as it opens");
    for (n, step) in steps.iter().enumerate().skip(1) {
        let did = step["did"].as_str().unwrap();
        let what = format!("step {n}, {did}");
        assert_now(&o, &window, &step["before"], &format!("{what}: before"));
        let events = step["events"].as_array().unwrap();
        match did {
            "trash button" => click_trash(&window),
            "delete key" => {
                widgets::key(window.window(), Key::Delete);
                widgets::settle();
            }
            "switch" => {
                widgets::key(window.window(), Key::RightArrow);
                widgets::settle();
            }
            "back" => {
                // (the middle click, the reference's default)
                window.window().dispatch_event(WindowEvent::PointerMoved {
                    position: LogicalPosition::new(300.0, 400.0),
                });
                widgets::click_at(
                    window.window(),
                    LogicalPosition::new(300.0, 400.0),
                    PointerEventButton::Middle,
                );
                widgets::settle();
            }
            "skip" => {
                widgets::key(window.window(), Key::UpArrow);
                widgets::settle();
            }
            other => panic!("{other}"),
        }
        for event in events {
            if let Some(asked) = event["asked"].as_str() {
                assert_eq!(window.get_question(), asked, "{what}: asked");
                let mut expected: Vec<String> = event["yes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().to_owned())
                    .collect();
                expected.push(event["no"].as_str().unwrap().to_owned());
                assert_eq!(answers(&window), expected, "{what}: its answers");
                let answer = match event["answer"].as_str().unwrap() {
                    "no" => event["no"].as_str().unwrap(),
                    other => other,
                };
                widgets::click(window.window(), answer);
                widgets::settle();
                assert!(window.get_question().is_empty(), "{what}: answered");
            }
            if !event["delete dialog"].is_null() {
                let dialog = hydrus_gui::delete_files_window::last_opened()
                    .expect("the delete files dialog opens");
                widgets::lay_out(dialog.window(), 500.0, 400.0);
                widgets::settle();
                let actions: Vec<String> =
                    dialog.get_actions().iter().map(|a| a.to_string()).collect();
                let recorded_actions: Vec<&str> = event["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect();
                assert_eq!(actions, recorded_actions, "{what}: the dialog's choices");
                let reasons: Vec<String> =
                    dialog.get_reasons().iter().map(|a| a.to_string()).collect();
                let selected = usize::try_from(dialog.get_selected_reason()).unwrap();
                assert_eq!(
                    reasons.get(selected).map(String::as_str),
                    event["reason"].as_str(),
                    "{what}: the default reason"
                );
                widgets::click(dialog.window(), "delete");
                widgets::settle();
                assert!(
                    hydrus_gui::delete_files_window::last_opened().is_none(),
                    "{what}: the dialog closed"
                );
            }
            if let Some(label) = event["interstitial"].as_str() {
                assert_eq!(window.get_question(), label, "{what}: the commit question");
                widgets::click(window.window(), "commit and continue");
                widgets::settle();
            }
        }
        // (nothing asked that the reference did not ask)
        if !events.iter().any(|e| !e["interstitial"].is_null()) {
            assert!(window.get_question().is_empty(), "{what}: no question left");
        }
        assert_now(&o, &window, &step["after"], &format!("{what}: after"));
    }
    // the files, as the commit left them
    let trash = o.store.snapshot().services.by_name("trash").unwrap().id;
    let my_files = o
        .store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
        .unwrap()
        .id;
    for (hex, after) in recorded["files_after"].as_object().unwrap() {
        let id: HashId = o.ids[hex];
        let snapshot = o.store.snapshot();
        let batch = o
            .store
            .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
            .unwrap();
        let m = &batch.results[0];
        let what = format!("{hex} after the commit");
        assert_eq!(
            m.current.iter().any(|c| c.service == my_files),
            after["in_my_files"].as_bool().unwrap(),
            "{what}: in my files"
        );
        assert_eq!(
            m.current.iter().any(|c| c.service == trash),
            after["trashed"].as_bool().unwrap(),
            "{what}: in the trash"
        );
        assert_eq!(
            m.deletion_reason.as_deref(),
            after["deletion_reason"].as_str(),
            "{what}: its reason"
        );
        assert_eq!(m.inbox, after["inbox"].as_bool().unwrap(), "{what}: inbox");
    }
}
