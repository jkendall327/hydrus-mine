//! "Confirm sending files to trash" through the main window and the viewer:
//! deleting asks or not as `oracle/fixtures/files_trash.json` recorded, and
//! undeleting asks "Undelete this file back to ...?" or "Undelete for?" as
//! `oracle/fixtures/undelete_question.json` recorded.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use serde_json::Value;
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use hydrus_store::import::import_legacy;
use hydrus_store::settings::DeletionPreferences;

/// The basic fixture, on a page searching all local files (the trash too).
fn opened() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let all_local = hydrus_core::ServiceKey::new(
        hydrus_core::service::builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
    );
    store
        .write_and_refresh(move |ctx| {
            let mut defaults: hydrus_store::settings::SearchDefaults =
                hydrus_store::settings::get(ctx.conn())?;
            defaults.local_location = hydrus_search::LocationContext::single(all_local);
            hydrus_store::settings::set(ctx.conn(), &defaults)
        })
        .unwrap();
    (native, store)
}

fn confirm_trash(store: &Store, confirm: bool) {
    let mut preferences = store
        .read(hydrus_store::settings::get::<DeletionPreferences>)
        .unwrap();
    preferences.confirm_trash = confirm;
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &preferences))
        .unwrap();
}

/// The local file domains (by name) `file` is in and was deleted from.
fn local(store: &Store, file: HashId) -> (BTreeSet<String>, BTreeSet<String>) {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).unwrap();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap();
    let name = |id| snapshot.services.get(id).unwrap().name.clone();
    let m = &batch.results[0];
    (
        m.current
            .iter()
            .filter(|c| roles.local.contains(&c.service))
            .map(|c| name(c.service))
            .collect(),
        m.deleted
            .iter()
            .filter(|d| roles.local.contains(&d.service))
            .map(|d| name(d.service))
            .collect(),
    )
}

/// Delete `file` from each of these local domains, by name.
fn delete_from(store: &Store, file: HashId, names: &BTreeSet<String>) {
    for name in names {
        let domain = store.snapshot().services.by_name(name).unwrap().id;
        hydrus_gui::media_actions::delete(
            store,
            &[file],
            &hydrus_gui::media_actions::Deletion::FromDomain {
                domain,
                name: name.clone(),
            },
        )
        .unwrap();
    }
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn chooser_open() -> bool {
    hydrus_gui::undelete::last_chooser().is_some_and(|c| c.window().is_visible())
}

/// What the run's recorded writes restored, per file: the domain names.
fn restored(log: &[Value], deleted: &HashMap<String, BTreeSet<String>>) -> HashMap<String, BTreeSet<String>> {
    let mut out: HashMap<String, BTreeSet<String>> = HashMap::new();
    for entry in log {
        let Some(to) = entry["undelete"].as_str() else {
            continue;
        };
        for file in strings(&entry["files"]) {
            let domains = if to == "all my files" {
                deleted[&file].clone()
            } else {
                BTreeSet::from([to.to_owned()])
            };
            out.entry(file).or_default().extend(domains);
        }
    }
    out
}

// leaf: audit-options-files-and-trash-confirm-sending-files-to-trash
#[test]
fn deleting_and_undeleting_ask_as_recorded() {
    let (_dir, store) = opened();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(hydrus_gui::SearchPage::new(store.clone())));
    ui.show().unwrap();
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    let snapshot = store.snapshot();
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &files))
        .unwrap();
    let by_hash: HashMap<String, HashId> = batch
        .results
        .iter()
        .map(|m| (m.hash.to_hex(), m.hash_id))
        .collect();
    let select = |chosen: &[HashId]| {
        ui.invoke_select_none();
        for (i, file) in chosen.iter().enumerate() {
            let index = page.borrow().results().iter().position(|f| f == file).unwrap();
            ui.invoke_thumbnail_clicked(i32::try_from(index).unwrap(), i > 0, false);
        }
        assert_eq!(page.borrow().selected_files().len(), chosen.len());
    };

    let recording = hydrus_testkit::fixture_json("undelete_question.json");
    let cases = recording.as_array().unwrap();
    let recorded: BTreeSet<HashId> = cases
        .iter()
        .flat_map(|case| strings(&case["files"]))
        .map(|h| by_hash[&h])
        .collect();

    // deleting from the window: with the option off, a file in one local
    // domain goes without a question; in two, it still asks
    let deletions = hydrus_testkit::fixture_json("files_trash.json");
    for case in deletions["deletion"].as_array().unwrap() {
        let count = usize::try_from(case["domains"].as_u64().unwrap()).unwrap();
        let file = *files
            .iter()
            .find(|&&f| !recorded.contains(&f) && local(&store, f).0.len() == count)
            .unwrap();
        confirm_trash(&store, case["confirm"].as_bool().unwrap());
        let before = local(&store, file).0;
        select(&[file]);
        ui.invoke_delete_selected();
        let asked = !ui.get_question().is_empty();
        assert_eq!(asked, !case["resolved"].as_bool().unwrap(), "{case}");
        if asked {
            ui.invoke_answer(false);
            assert_eq!(local(&store, file).0, before, "no leaves it be");
        } else {
            assert!(local(&store, file).0.is_empty(), "deleted at once");
            // (put it back for the next case)
            hydrus_gui::media_actions::undelete(&store, &[file]).unwrap();
            assert_eq!(local(&store, file).0, before);
        }
    }

    // the recorder's deletions: each one-file case's file leaves the local
    // domains it is no longer in
    for case in cases {
        let hashes = strings(&case["files"]);
        if hashes.len() == 1 {
            let file = by_hash[hashes.iter().next().unwrap()];
            let gone: BTreeSet<String> = local(&store, file)
                .0
                .difference(&strings(&case["current_in"]))
                .cloned()
                .collect();
            delete_from(&store, file, &gone);
        }
    }

    for case in cases {
        let hashes: Vec<String> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap().to_owned())
            .collect();
        let chosen: Vec<HashId> = hashes.iter().map(|h| by_hash[h]).collect();
        let deleted: HashMap<String, BTreeSet<String>> = hashes
            .iter()
            .map(|h| (h.clone(), local(&store, by_hash[h]).1))
            .collect();
        let union: BTreeSet<String> = deleted.values().flatten().cloned().collect();
        assert_eq!(union, strings(&case["deleted_from"]), "{}", case["case"]);
        for run in case["runs"].as_array().unwrap() {
            let what = format!("{} {run}", case["case"]);
            confirm_trash(&store, run["confirm"].as_bool().unwrap());
            let before: Vec<BTreeSet<String>> = chosen.iter().map(|&f| local(&store, f).0).collect();
            select(&chosen);
            ui.invoke_undelete_selected();
            let log = run["log"].as_array().unwrap();
            if let Some(asked) = log.iter().find(|e| e.get("chooser").is_some()) {
                let chooser = hydrus_gui::undelete::last_chooser().unwrap();
                assert!(chooser.window().is_visible(), "{what}");
                assert_eq!(chooser.get_window_title(), asked["chooser"].as_str().unwrap());
                assert_eq!(chooser.get_message(), asked["message"].as_str().unwrap());
                assert_eq!(chooser.get_no_label(), "");
                let choices: Vec<String> =
                    chooser.get_choices().iter().map(|c| c.to_string()).collect();
                let recorded: Vec<String> = asked["choices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c[0].as_str().unwrap().to_owned())
                    .collect();
                assert_eq!(choices, recorded, "{what}");
                match run["choice"].as_u64() {
                    Some(i) => chooser.invoke_chosen(i32::try_from(i).unwrap()),
                    None => chooser.invoke_cancelled(),
                }
                assert!(!chooser_open());
            } else {
                assert!(!chooser_open(), "{what}");
            }
            match log.iter().find_map(|e| e["question"].as_str()) {
                Some(question) => {
                    assert_eq!(ui.get_question(), question, "{what}");
                    ui.invoke_answer(run["yes"].as_bool().unwrap());
                }
                None => assert_eq!(ui.get_question(), "", "{what}"),
            }
            let expected = restored(log, &deleted);
            for (i, (&file, hash)) in chosen.iter().zip(&hashes).enumerate() {
                let now = local(&store, file).0;
                let added: BTreeSet<String> = now.difference(&before[i]).cloned().collect();
                assert_eq!(
                    added,
                    expected.get(hash).cloned().unwrap_or_default(),
                    "{what}"
                );
                // (deleted again for the next run)
                delete_from(&store, file, &added);
            }
        }
    }

    // the viewer asks the same: one domain, the question; several, the chooser
    let partial = &cases[2];
    assert_eq!(partial["case"], "deleted from one of its two domains");
    let file = by_hash[strings(&partial["files"]).iter().next().unwrap()];
    confirm_trash(&store, true);
    let index = page.borrow().results().iter().position(|&f| f == file).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_undelete();
    assert_eq!(viewer.get_question(), "Undelete this file back to my files?");
    viewer.invoke_answer(false);
    assert_eq!(local(&store, file).0, BTreeSet::from(["art".to_owned()]));
    viewer.invoke_undelete();
    viewer.invoke_answer(true);
    assert_eq!(
        local(&store, file).0,
        BTreeSet::from(["art".to_owned(), "my files".to_owned()])
    );
    viewer.invoke_close_requested();

    let both = &cases[1];
    let file = by_hash[strings(&both["files"]).iter().next().unwrap()];
    let index = page.borrow().results().iter().position(|&f| f == file).unwrap();
    ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    viewer.invoke_undelete();
    assert_eq!(viewer.get_question(), "");
    let chooser = hydrus_gui::undelete::last_chooser().unwrap();
    assert!(chooser.window().is_visible());
    assert_eq!(chooser.get_window_title(), "Undelete for?");
    chooser.invoke_chosen(2);
    assert_eq!(
        local(&store, file).0,
        BTreeSet::from(["art".to_owned(), "my files".to_owned()])
    );
}
