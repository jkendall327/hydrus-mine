//! The main window's menu bar is the reference's (`oracle/record_main_menu.py`:
//! its menu bar as the running client showed it on the fixtures): the same
//! menus and entries, in the same order, enabled and ticked alike, from
//! the store migrated from each fixture. Entries hydrus-rs can't do yet
//! are there all the same (greyed out); a tick hydrus-rs doesn't keep is
//! not compared. Left out of the reference's: other help > debug tools and "about Qt",
//! historical imported session backups (not migrated to the native archive).

use std::sync::Arc;

use hydrus_core::pages::PageKey;
use hydrus_gui_model::main_menu::{Entry, Facts, menubar, shown};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::Value;

fn migrated(fixture: &str) -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture(fixture);
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    (native, store)
}

/// The facts the recordings were made with, beyond the store's: the
/// driver boots the client with network traffic paused, and the session
/// has its one page, which has been shown.
fn facts(store: &Store, recorded: &Value) -> Facts {
    let mut facts = Facts::from_store(store).unwrap();
    facts.pauses.network_traffic = recorded["facts"]["pause_all_new_network_traffic"]
        .as_bool()
        .unwrap();
    facts.page_count = 1;
    facts.history = Some(vec![(PageKey::random(), "files".to_owned())]);
    facts
}

/// A recorded entry's text as hydrus-rs has it: Qt's `&&` is an `&`.
fn unescaped(text: &str) -> String {
    text.replace("&&", "&")
}

/// The reference's menus less what hydrus-rs leaves out, separators shown
/// as Qt shows them.
fn kept(entries: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for entry in entries {
        let left_out =
            entry.get("menu").is_some_and(|m| m == "append backup") || entry == "about Qt";
        if left_out {
            continue;
        }
        if entry == "---" && out.last().is_none_or(|l| l == "---") {
            continue;
        }
        let mut entry = entry.clone();
        if entry.get("menu").is_some_and(|menu| menu == "debug") {
            let mut gui = entry["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e.get("menu").is_some_and(|name| name == "gui actions"))
                .unwrap()
                .clone();
            let long_text = gui["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| *e == "make a long text popup")
                .unwrap()
                .clone();
            let delayed = gui["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| *e == "make a popup in five seconds")
                .unwrap()
                .clone();
            let recorded = gui["entries"].as_array().unwrap();
            assert!(
                recorded
                    .iter()
                    .position(|entry| entry == &long_text)
                    .unwrap()
                    < recorded.iter().position(|entry| entry == &delayed).unwrap()
            );
            gui["entries"] = serde_json::json!([long_text, delayed]);
            let mut memory = entry["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e.get("menu").is_some_and(|name| name == "memory actions"))
                .unwrap()
                .clone();
            let clear = memory["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| *e == "clear thumbnail cache")
                .unwrap()
                .clone();
            memory["entries"] = serde_json::json!([clear]);
            entry["entries"] = serde_json::json!([gui, memory]);
        }

        if let Some(inner) = entry.get("entries").and_then(Value::as_array) {
            entry["entries"] = Value::Array(kept(inner));
        }
        out.push(entry);
    }
    while out.last().is_some_and(|l| l == "---") {
        out.pop();
    }
    out
}

/// Ours is theirs: at `path`, each entry alike.
fn compare(ours: &[Entry], theirs: &[Value], path: &str, top: bool) {
    let ours = shown(ours);
    let names =
        |ours: &[&Entry]| -> Vec<String> { ours.iter().map(|e| e.label().to_owned()).collect() };
    assert_eq!(
        ours.len(),
        theirs.len(),
        "{path}: ours {:?}, theirs {theirs:?}",
        names(&ours)
    );
    for (entry, recorded) in ours.iter().zip(theirs) {
        let label = |text: &Value| -> String {
            let text = text.as_str().unwrap();
            if top {
                text.to_owned()
            } else {
                unescaped(text)
            }
        };
        match entry {
            Entry::Separator => assert_eq!(recorded, "---", "{path}"),
            Entry::Item {
                label: ours,
                enabled,
                ..
            } => {
                if let Some(disabled) = recorded.get("disabled") {
                    assert_eq!(*ours, label(disabled), "{path}");
                    assert!(!enabled, "{path} > {ours} is disabled in the reference");
                } else {
                    assert_eq!(*ours, label(recorded), "{path}");
                    assert!(enabled, "{path} > {ours} is enabled in the reference");
                }
            }
            Entry::Check {
                label: ours,
                command,
                checked,
            } => {
                assert_eq!(*ours, label(&recorded["check"]), "{path}");
                if command.is_some() {
                    assert_eq!(
                        *checked,
                        recorded["checked"].as_bool().unwrap(),
                        "{path} > {ours}"
                    );
                }
            }
            Entry::Menu {
                label: ours,
                entries,
                enabled,
            } => {
                assert_eq!(*ours, label(&recorded["menu"]), "{path}");
                let disabled = recorded.get("disabled").is_some_and(|d| d == true);
                assert_eq!(*enabled, !disabled, "{path} > {ours}");
                compare(
                    entries,
                    recorded["entries"].as_array().unwrap(),
                    &format!("{path} > {ours}"),
                    false,
                );
            }
        }
    }
}

fn compare_bar(ours: &[Entry], theirs: &Value, what: &str) {
    compare(ours, &kept(theirs.as_array().unwrap()), what, true);
}

#[test]
fn the_menu_bar_is_the_reference_s() {
    let recorded = hydrus_testkit::fixture_json("main_menu.json");
    let (_dir, store) = migrated("basic");
    let mut facts = facts(&store, &recorded);
    compare_bar(&menubar(&facts), &recorded["default"], "default");
    // with its one page closed: the undo menu offers it back, and the
    // history has been emptied of it
    facts.page_count = 0;
    facts.closed_pages = vec![recorded["closed"]["page"].as_str().unwrap().to_owned()];
    facts.history = Some(Vec::new());
    compare_bar(&menubar(&facts), &recorded["closed"]["menus"], "closed");
    // then in advanced mode
    facts.advanced = true;
    compare_bar(&menubar(&facts), &recorded["advanced"], "advanced");
}

#[test]
fn import_and_export_folders_are_offered_to_run_now() {
    let recorded = hydrus_testkit::fixture_json("main_menu.json");
    for fixture in ["import_folder", "export_folder"] {
        let (_dir, store) = migrated(fixture);
        let facts = facts(&store, &recorded);
        let ours = menubar(&facts);
        let theirs = &recorded[format!("{fixture}_file_menu")];
        compare(
            std::slice::from_ref(&ours[0]),
            &kept(std::slice::from_ref(theirs)),
            fixture,
            true,
        );
    }
}

#[test]
fn repositories_add_their_pages_and_pending_menu() {
    let recorded = hydrus_testkit::fixture_json("main_menu.json");
    let (_dir, store) = migrated("repositories");
    let facts = facts(&store, &recorded);
    compare_bar(&menubar(&facts), &recorded["repositories"], "repositories");
}
