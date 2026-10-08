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
            // the debug entries hydrus-rs has, in the recorded order
            let label = |e: &Value| -> String {
                e.as_str()
                    .or_else(|| {
                        e.get("check")
                            .or_else(|| e.get("menu"))
                            .and_then(Value::as_str)
                    })
                    .unwrap_or_default()
                    .to_owned()
            };
            let keep: [(&str, &[&str]); 7] = [
                (
                    "debug modes",
                    &["force idle mode", "---", "use faulthandler to log crashes"],
                ),
                ("profiling", &["what is this?"]),
                (
                    "report modes",
                    &[
                        "blurhash mode",
                        "cache report mode",
                        "daemon report mode",
                        "file report mode",
                        "file import report mode",
                        "gui report mode",
                        "idle report mode",
                        "network report mode",
                        "network report mode (silent)",
                        "similar files metadata generation report mode",
                        "shortcut report mode",
                        "subprocess report mode",
                        "subscription report mode",
                    ],
                ),
                (
                    "gui actions",
                    &[
                        "autocomplete delay mode",
                        "close and reload current gui session",
                        "make a long text popup",
                        "make a modal popup in five seconds",
                        "make a new page in five seconds",
                        "make a non-cancellable modal popup in five seconds",
                        "make a popup in five seconds",
                        "make a QMessageBox",
                        "make some popups",
                        "reset multi-column list settings to default",
                        "save 'last session' gui session",
                    ],
                ),
                (
                    "data actions",
                    &[
                        "flush log",
                        "force database commit",
                        "scan file storage folders",
                        "show env",
                        "---",
                        "simulate program exit signal",
                    ],
                ),
                (
                    "memory actions",
                    &["clear all rendering caches", "clear thumbnail cache"],
                ),
                (
                    "network actions",
                    &["review current network jobs", "fetch a url"],
                ),
            ];
            let submenus: Vec<Value> = keep
                .iter()
                .map(|(name, labels)| {
                    let mut submenu = entry["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|e| e.get("menu").is_some_and(|m| m == name))
                        .unwrap_or_else(|| panic!("{name} recorded"))
                        .clone();
                    let kept: Vec<Value> = submenu["entries"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| labels.contains(&label(e).as_str()))
                        .cloned()
                        .collect();
                    assert_eq!(
                        kept.iter().filter(|e| *e != "---").count(),
                        labels.iter().filter(|l| **l != "---").count(),
                        "{name}"
                    );
                    submenu["entries"] = Value::Array(kept);
                    submenu
                })
                .collect();
            entry["entries"] = Value::Array(submenus);
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

/// Before any page has been shown the history menu is the reference's single
/// plain label (`AppendMenuLabel( ..., 'no tab history', ..., no_copy = True )`);
/// once there is a history it is replaced by the entries and "Clear History".
// leaf: audit-options-menu-menu-pages-no-tab-history
#[test]
fn the_history_menu_is_a_plain_label_until_there_is_a_history() {
    let recorded = hydrus_testkit::fixture_json("main_menu.json");
    let (_dir, store) = migrated("basic");
    let mut facts = facts(&store, &recorded);
    let history = |facts: &Facts| -> Vec<Entry> {
        let bar = menubar(facts);
        let Entry::Menu { entries, .. } = &bar[2] else {
            panic!("the pages menu");
        };
        entries
            .iter()
            .find_map(|e| match e {
                Entry::Menu {
                    label, entries: h, ..
                } if label == "history" => Some(h.clone()),
                _ => None,
            })
            .expect("a history menu")
    };
    facts.history = None;
    let none = history(&facts);
    assert_eq!(none.len(), 1);
    assert!(matches!(
        &none[0],
        Entry::Item { label, command: None, .. } if label == "no tab history"
    ));
    facts.history = Some(Vec::new());
    let entries = history(&facts);
    let labels: Vec<&str> = entries.iter().map(Entry::label).collect();
    assert_eq!(labels, ["", "Clear History"]);
}
