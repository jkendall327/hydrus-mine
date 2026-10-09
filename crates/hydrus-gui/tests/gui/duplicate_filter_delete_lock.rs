//! The archived-file delete lock where duplicates are resolved, against
//! the reference: with "delete lock for archived files" and a reinbox option
//! set in the real Options window and every file archived,
//!
//! - "this is better, delete the other" chosen in the real filter window and
//!   committed (`oracle/record_duplicate_filter_delete_lock.py`), and
//! - a pending auto-resolution action approved in the real review window
//!   (`oracle/record_auto_resolution_delete_lock.py`)
//!
//! leave each file in the inbox or archive, and in my files or the trash, as
//! the reference left it.

use std::collections::BTreeMap;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_core::service::builtin_keys;
use hydrus_gui::OptionsWindow;
use serde_json::Value;

use crate::auto_resolution_review::{Opened, opened};

const LOCK: &str = "Do not permit archived files to be deleted from the trash: ";
const REINBOX: &str = "After duplicate filter, ensure deletees are inboxed before delete: ";
const REINBOX_AUTO: &str =
    "In duplicates auto-resolution, ensure deletees are inboxed before delete: ";

/// file > options…, on "files and trash".
fn files_and_trash(o: &Opened) -> OptionsWindow {
    o.ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = o.ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .expect("file > options");
    o.ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let window = o.bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = window.get_pages();
    let page = (0..pages.row_count())
        .position(|i| pages.row_data(i).unwrap().text == "files and trash")
        .unwrap();
    let page = i32::try_from(page).unwrap();
    window.set_page(page);
    window.invoke_page_chosen(page);
    window
}

fn check(window: &OptionsWindow, label: &str, on: bool) {
    let rows = window.get_rows();
    let at = (0..rows.row_count())
        .position(|i| rows.row_data(i).unwrap().label == label)
        .unwrap_or_else(|| panic!("{label:?}"));
    assert_eq!(rows.row_data(at).unwrap().kind, 1, "{label:?}");
    window.invoke_check_toggled(i32::try_from(at).unwrap(), on);
}

/// Each file's (inbox, in my files, in the trash).
fn states(o: &Opened, files: &[HashId]) -> BTreeMap<String, (bool, bool, bool)> {
    let services = &o.store.snapshot().services;
    let my_files = services.builtin(builtin_keys::MY_FILES).unwrap().id;
    let trash = services.builtin(builtin_keys::TRASH).unwrap().id;
    let files = files.to_vec();
    let (in_my_files, in_trash, inbox) = o
        .store
        .read(move |c| {
            let inbox: Vec<HashId> = c
                .prepare("SELECT hash_id FROM file_inbox")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok((
                hydrus_store::media::current_in(c, my_files, &files)?,
                hydrus_store::media::current_in(c, trash, &files)?,
                inbox,
            ))
        })
        .unwrap();
    o.hex
        .iter()
        .filter(|(id, _)| in_my_files.contains(id) || in_trash.contains(id))
        .map(|(id, hex)| {
            (
                hex.clone(),
                (
                    inbox.contains(id),
                    in_my_files.contains(id),
                    in_trash.contains(id),
                ),
            )
        })
        .collect()
}

fn replay(phase: &Value) {
    let o = opened();
    let lock = phase["delete_lock"].as_bool().unwrap();
    let reinbox = phase["reinbox_after_duplicate_filter"].as_bool().unwrap();
    let what = format!("lock {lock}, reinbox {reinbox}");
    // the options, set in the real window
    let window = files_and_trash(&o);
    check(&window, LOCK, lock);
    check(&window, REINBOX, reinbox);
    window.invoke_apply();
    let saved: hydrus_store::delete_lock::DeleteLock =
        o.store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(
        (saved.archived, saved.reinbox_after_duplicate_filter),
        (lock, reinbox)
    );
    // every file archived
    let everything: Vec<HashId> = o.hex.keys().copied().collect();
    o.store
        .write_content(move |w| w.archive(&everything))
        .unwrap();

    let batch: Vec<HashId> = phase["batch"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|pair| pair.as_array().unwrap())
        .map(|h| o.ids[h.as_str().unwrap()])
        .collect();

    // "this is better, delete the other" on the first pair, then close and
    // commit, as the reference's harness pressed
    o.ui.invoke_launch_filter();
    let filter = o
        .bound
        .filter
        .borrow()
        .as_ref()
        .expect("the filter opens")
        .clone_strong();
    filter.invoke_decide("better-delete".into());
    filter.invoke_close_requested();
    let buttons: Vec<String> = filter.get_answers().iter().map(|a| a.to_string()).collect();
    let commit = buttons
        .iter()
        .position(|b| b == "commit")
        .unwrap_or_else(|| panic!("{what}: {buttons:?}"));
    filter.invoke_answer(i32::try_from(commit).unwrap());
    assert!(!filter.window().is_visible(), "{what}: closed");

    // each file as the reference left it
    let expected: BTreeMap<String, (bool, bool, bool)> = phase["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(hex, state)| {
            let domains: Vec<&str> = state["domains"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d.as_str().unwrap())
                .collect();
            (
                hex.clone(),
                (
                    state["inbox"].as_bool().unwrap(),
                    domains.contains(&"my files"),
                    domains.contains(&"trash"),
                ),
            )
        })
        .collect();
    let native = states(&o, &batch);
    let native: BTreeMap<String, (bool, bool, bool)> = native
        .into_iter()
        .filter(|(hex, _)| expected.contains_key(hex))
        .collect();
    assert_eq!(native, expected, "{what}");
}

fn replay_approval(phase: &Value) {
    use crate::auto_resolution_review::{listed, review, settle};

    let o = opened();
    let lock = phase["delete_lock"].as_bool().unwrap();
    let reinbox = phase["reinbox_in_auto_resolution"].as_bool().unwrap();
    let what = format!("lock {lock}, reinbox {reinbox}");
    let window = files_and_trash(&o);
    check(&window, LOCK, lock);
    check(&window, REINBOX_AUTO, reinbox);
    window.invoke_apply();
    let saved: hydrus_store::delete_lock::DeleteLock =
        o.store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(
        (saved.archived, saved.reinbox_in_auto_resolution),
        (lock, reinbox)
    );
    let everything: Vec<HashId> = o.hex.keys().copied().collect();
    o.store
        .write_content(move |w| w.archive(&everything))
        .unwrap();

    // the recorded pair approved in the rule's review window
    let name = phase["rule"].as_str().unwrap();
    let pair = (
        phase["pair"][0].as_str().unwrap().to_owned(),
        phase["pair"][1].as_str().unwrap().to_owned(),
    );
    let rule = o
        .store
        .read(hydrus_store::duplicates::auto::rules)
        .unwrap()
        .into_iter()
        .find(|(_, r)| r.name == name)
        .unwrap()
        .0;
    let window = review(&o.ui, &o.bound, name);
    let pending = listed(&o.store, rule, "pending", &o.hex);
    let row = pending.iter().position(|p| *p == pair).expect("pending");
    window.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    window.invoke_approve();
    settle(&window);

    let expected: BTreeMap<String, (bool, bool, bool)> = phase["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(hex, state)| {
            (
                hex.clone(),
                (
                    state["inbox"].as_bool().unwrap(),
                    state["my files"].as_bool().unwrap(),
                    state["trash"].as_bool().unwrap(),
                ),
            )
        })
        .collect();
    let files: Vec<HashId> = expected.keys().map(|h| o.ids[h]).collect();
    let native: BTreeMap<String, (bool, bool, bool)> = states(&o, &files)
        .into_iter()
        .filter(|(hex, _)| expected.contains_key(hex))
        .collect();
    assert_eq!(native, expected, "{what}");
}

// leaf: audit-options-files-and-trash-delete-lock-in-duplicates-auto-resolution-ensure-deletees-are-inboxed-before-delete
#[test]
fn approved_auto_resolution_actions_inbox_locked_deletees_as_the_reference_does() {
    let _windows = hydrus_gui::headless::init();
    let recording = hydrus_testkit::fixture_json("auto_resolution_delete_lock.json");
    for phase in recording["phases"].as_array().unwrap() {
        replay_approval(phase);
    }
}

// leaf: audit-options-files-and-trash-delete-lock-after-duplicate-filter-ensure-deletees-are-inboxed-before-delete
#[test]
fn the_duplicate_filter_inboxes_locked_deletees_as_the_reference_does() {
    let _windows = hydrus_gui::headless::init();
    let recording = hydrus_testkit::fixture_json("duplicate_filter_delete_lock.json");
    for phase in recording["phases"].as_array().unwrap() {
        replay(phase);
    }
}
