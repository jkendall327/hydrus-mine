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
    assert_eq!(
        window.get_index_text(),
        now["index"].as_str().unwrap(),
        "{what}: the index"
    );
    assert_eq!(
        shown(o).as_deref(),
        now["shown"].as_str(),
        "{what}: the file shown"
    );
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

/// Each recorded file's state: in my files, in the trash, its deletion
/// reason (the reference reads "Unknown deletion reason." for a file never
/// deleted, where hydrus-rs keeps none), in the inbox.
fn assert_files(o: &Opened, files: &Value, what: &str) {
    let trash = o.store.snapshot().services.by_name("trash").unwrap().id;
    let my_files = o
        .store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
        .unwrap()
        .id;
    for (hex, after) in files.as_object().unwrap() {
        let id: HashId = o.ids[hex];
        let snapshot = o.store.snapshot();
        let batch = o
            .store
            .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
            .unwrap();
        let m = &batch.results[0];
        let what = format!("{what}: {hex}");
        let trashed = after["trashed"].as_bool().unwrap();
        assert_eq!(
            m.current.iter().any(|c| c.service == my_files),
            after["in_my_files"].as_bool().unwrap(),
            "{what}: in my files"
        );
        assert_eq!(
            m.current.iter().any(|c| c.service == trash),
            trashed,
            "{what}: in the trash"
        );
        if trashed {
            assert_eq!(
                m.deletion_reason.as_deref(),
                after["deletion_reason"].as_str(),
                "{what}: its reason"
            );
        } else {
            assert_eq!(m.deletion_reason, None, "{what}: no reason");
        }
        assert_eq!(m.inbox, after["inbox"].as_bool().unwrap(), "{what}: inbox");
    }
}

/// The filter, open on the recorded batch (six pairs), with the recording's
/// files archived and the delete lock's "inbox the duplicate filter's
/// deletees" option on.
fn open(recorded: &Value) -> (Opened, hydrus_gui::DuplicateFilterWindow) {
    let o = opened();
    o.store
        .write(|ctx| {
            let mut settings: hydrus_store::duplicates::DuplicateFilterSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.max_batch_size = 6;
            hydrus_store::settings::set(ctx.conn(), &settings)?;
            let mut lock: hydrus_store::delete_lock::DeleteLock =
                hydrus_store::settings::get(ctx.conn())?;
            lock.reinbox_after_duplicate_filter = true;
            hydrus_store::settings::set(ctx.conn(), &lock)
        })
        .unwrap();
    let archived: Vec<HashId> = recorded["archived"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| o.ids[h.as_str().unwrap()])
        .collect();
    o.store
        .write_content(move |w| w.archive(&archived))
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
    (o, window)
}

/// While the delete files dialog is open, the filter takes no input, as
/// the reference's modal dialog has it: a decision click, the skip key, a
/// middle click (back) and the trash button do nothing.
fn assert_modal(window: &hydrus_gui::DuplicateFilterWindow, what: &str) {
    let index = window.get_index_text();
    let canvas = LogicalPosition::new(300.0, 400.0);
    widgets::click_at(window.window(), canvas, PointerEventButton::Left);
    widgets::key(window.window(), Key::UpArrow);
    widgets::click_at(window.window(), canvas, PointerEventButton::Middle);
    widgets::key(window.window(), Key::Delete);
    widgets::settle();
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(500.0, 4.0),
    });
    widgets::settle();
    assert!(
        !window.get_info_showing(),
        "{what}: no hover over the dialog"
    );
    assert_eq!(window.get_index_text(), index, "{what}: nothing decided");
    assert!(window.get_question().is_empty(), "{what}: nothing asked");
}

// leaf: audit-media-duplicate-filter-top-hover
#[test]
fn the_duplicate_filter_deletes_this_file_or_both_as_the_reference_s_does() {
    let recorded = hydrus_testkit::fixture_json("duplicate_filter_delete.json");
    let _windows = hydrus_gui::headless::init();
    let (o, window) = open(&recorded);
    let steps = recorded["steps"].as_array().unwrap();
    assert_now(&o, &window, &steps[0]["start"], "as it opens");
    let mut checked_modal = false;
    // the files of the deletion going back undid
    let mut restored: Vec<String> = Vec::new();
    let mut last_deleted: Vec<String> = Vec::new();
    for (n, step) in steps.iter().enumerate().skip(1) {
        if let Some(files) = step.get("files") {
            assert_files(&o, files, &format!("step {n}"));
            continue;
        }
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
                restored = last_deleted.clone();
                // (the middle click, the reference's default)
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
            "close" => {
                window.window().dispatch_event(WindowEvent::PointerMoved {
                    position: LogicalPosition::new(500.0, 4.0),
                });
                widgets::settle();
                let at = (0..widgets::count(window.window(), "close"))
                    .map(|n| widgets::position(window.window(), "close", n))
                    .find(|at| at.y <= 40.0)
                    .expect("the close button");
                widgets::click_at(window.window(), at, PointerEventButton::Left);
                widgets::settle();
            }
            other => panic!("{other}"),
        }
        let mut dialog = None;
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
            if let Some(files) = event["delete dialog"].as_array() {
                last_deleted = files
                    .iter()
                    .map(|f| f.as_str().unwrap().to_owned())
                    .collect();
            }
            if !event["delete dialog"].is_null() {
                let opened = hydrus_gui::delete_files_window::last_opened()
                    .expect("the delete files dialog opens");
                widgets::lay_out(opened.window(), 500.0, 400.0);
                widgets::settle();
                let actions: Vec<String> =
                    opened.get_actions().iter().map(|a| a.to_string()).collect();
                let recorded_actions: Vec<&str> = event["actions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect();
                assert_eq!(actions, recorded_actions, "{what}: the dialog's choices");
                let reasons: Vec<String> =
                    opened.get_reasons().iter().map(|a| a.to_string()).collect();
                let selected = usize::try_from(opened.get_selected_reason()).unwrap();
                assert_eq!(
                    reasons.get(selected).map(String::as_str),
                    event["reason"].as_str(),
                    "{what}: the default reason"
                );
                if !checked_modal {
                    assert_modal(&window, &what);
                    checked_modal = true;
                }
                dialog = Some(opened);
            }
            if let Some(answer) = event["delete dialog answer"].as_str() {
                let opened = dialog.take().expect("the dialog answered");
                widgets::click(opened.window(), answer);
                widgets::settle();
                assert!(!opened.window().is_visible(), "{what}: the dialog closed");
            }
            if let Some(label) = event["interstitial"].as_str() {
                // (the reference's `_RewindProcessing` releases only one file
                // of a "delete both" gone back over, so it skips the pairs of
                // the other for the rest of the batch; hydrus-rs releases
                // both (DIFFERENCES.md), and shows such a pair: skipped here)
                if window.get_question().is_empty() {
                    let (a, b) = hydrus_gui::shown_pair().expect("a pair shown");
                    assert!(
                        restored.contains(&o.hex[&a]) || restored.contains(&o.hex[&b]),
                        "{what}: a pair the reference would show, at {}",
                        window.get_index_text()
                    );
                    widgets::key(window.window(), Key::UpArrow);
                    widgets::settle();
                }
                assert_eq!(window.get_question(), label, "{what}: the commit question");
                widgets::click(window.window(), "commit and continue");
                widgets::settle();
            }
            if let Some(label) = event["finish"].as_str() {
                assert_eq!(window.get_question(), label, "{what}: the closing question");
                widgets::click(window.window(), "forget");
                widgets::settle();
                // (and the forgetting confirmed, as the reference's
                // `DoForget` asks)
                widgets::click(window.window(), "yes");
                widgets::settle();
                assert!(o.bound.filter.borrow().is_none(), "{what}: closed");
            }
        }
        if did == "close" {
            continue;
        }
        // (nothing asked that the reference did not ask)
        if !events.iter().any(|e| !e["interstitial"].is_null()) {
            assert!(window.get_question().is_empty(), "{what}: no question left");
        }
        assert_now(&o, &window, &step["after"], &format!("{what}: after"));
    }
    assert!(checked_modal, "the dialog was open once");
}

// (undelete, and shift+delete, from the duplicate filter: asked as the media
// viewer asks, `_Undelete`)
#[test]
fn the_duplicate_filter_s_undelete_button_and_shift_delete_undelete() {
    let recorded = hydrus_testkit::fixture_json("duplicate_filter_delete.json");
    let _windows = hydrus_gui::headless::init();
    let o = opened();
    let first = recorded["steps"][0]["start"]["shown"].as_str().unwrap();
    let id = o.ids[first];
    let my_files = o
        .store
        .snapshot()
        .services
        .builtin(hydrus_core::service::builtin_keys::MY_FILES)
        .unwrap()
        .id;
    let in_my_files = |o: &Opened| {
        o.store
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT COUNT(*) FROM file_domain_current WHERE service_id = ?1 AND hash_id = ?2",
                    (my_files, id),
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
            == 1
    };
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
    assert_eq!(shown(&o).as_deref(), Some(first), "the first file shown");
    let answer_yes = || {
        if let Some(chooser) = hydrus_gui::undelete::last_chooser() {
            widgets::lay_out(chooser.window(), 400.0, 200.0);
            widgets::settle();
            widgets::click(chooser.window(), "yes");
            widgets::settle();
        }
    };
    for how in ["the undelete button", "shift+delete"] {
        o.store
            .write_content(move |w| w.delete_files(my_files, &[id], None))
            .unwrap();
        // (the hover shows the file as it is now)
        window.invoke_switch_media();
        window.invoke_switch_media();
        widgets::settle();
        assert!(!in_my_files(&o), "{how}: trashed");
        assert!(window.get_file_undeletable(), "{how}: undeletable");
        match how {
            "the undelete button" => {
                window.window().dispatch_event(WindowEvent::PointerMoved {
                    position: LogicalPosition::new(500.0, 4.0),
                });
                widgets::settle();
                let at = (0..widgets::count(window.window(), "undelete"))
                    .map(|n| widgets::position(window.window(), "undelete", n))
                    .find(|at| at.y <= 40.0)
                    .expect("the undelete button");
                widgets::click_at(window.window(), at, PointerEventButton::Left);
            }
            _ => widgets::key_with(
                window.window(),
                Key::Delete,
                widgets::Mods {
                    ctrl: false,
                    shift: true,
                    alt: false,
                },
            ),
        }
        widgets::settle();
        answer_yes();
        assert!(in_my_files(&o), "{how}: undeleted");
        assert!(
            window.get_question().is_empty(),
            "{how}: nothing else asked"
        );
    }
}
