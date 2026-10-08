//! The "review actions" window's tabs, as the reference lists them for
//! each rule of the database `oracle/record_auto_resolution.py` left
//! (`oracle/record_auto_resolution_review.py`): the tab it starts on, each
//! tab's label, and each pair with its third column.

use std::collections::HashMap;

use hydrus_core::HashId;
use hydrus_gui_model::auto_resolution_review::{
    FULLY_AUTOMATIC, TABS, actioned_cell, denied_cell, found, pending_cell, pending_summary,
    start_tab,
};
use hydrus_store::Store;
use hydrus_store::duplicates::auto::{self, OperationMode};
use serde_json::Value;

/// The migrated database, and the hex of each file's hash.
fn store() -> (
    tempfile::TempDir,
    std::sync::Arc<Store>,
    HashMap<HashId, String>,
) {
    let legacy = hydrus_testkit::legacy_fixture("auto_resolution");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let hexes = store
        .read(|c| {
            let ids: Vec<HashId> = c
                .prepare("SELECT hash_id FROM hashes")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            hydrus_store::master::hashes(c, &ids)
        })
        .unwrap()
        .into_iter()
        .map(|(id, h)| (id, h.to_hex()))
        .collect();
    (dir, store, hexes)
}

fn rows(value: &Value) -> Vec<(String, String, String)> {
    value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r[0].as_str().unwrap().to_owned(),
                r[1].as_str().unwrap().to_owned(),
                r[2].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn each_rules_tabs_list_its_pairs_as_the_reference() {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let now = recorded["now"].as_i64().unwrap();
    let (_dir, store, hex) = store();
    let rules = store.read(auto::rules).unwrap();
    for case in recorded["rules"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let (id, rule) = rules.iter().find(|(_, r)| r.name == name).unwrap();
        let tabs: Vec<&str> = case["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        assert_eq!(tabs, TABS);
        assert_eq!(
            start_tab(rule),
            case["tab"].as_u64().unwrap() as usize,
            "{name}"
        );

        // pending: the pairs, and the action (the reference's first line;
        // it adds the merge's summary)
        let theirs = rows(&case["pending"]);
        if rule.mode == OperationMode::FullyAutomatic {
            assert_eq!(case["pending"]["label"], FULLY_AUTOMATIC);
        } else {
            let pending = store
                .read(|c| auto::pending_pairs(c, *id, Some(250)))
                .unwrap();
            assert_eq!(
                case["pending"]["label"],
                found(pending.len()).as_str(),
                "{name}"
            );
            let mut ours: Vec<(String, String)> = pending
                .iter()
                .map(|(_, a, b)| (hex[a].clone(), hex[b].clone()))
                .collect();
            let mut their_pairs: Vec<(String, String)> = theirs
                .iter()
                .map(|(a, b, _)| (a.clone(), b.clone()))
                .collect();
            // (the reference lists them in its table's order)
            ours.sort();
            their_pairs.sort();
            assert_eq!(ours, their_pairs, "{name}");
            for (a, b, text) in &theirs {
                let id = |h: &String| *hex.iter().find(|(_, x)| *x == h).unwrap().0;
                let merge = pending_summary(&store, rule, id(a), id(b)).ok();
                assert_eq!(*text, pending_cell(rule, merge.as_deref()), "{name}");
            }
        }

        // actions taken, newest first
        let actioned = store.read(|c| auto::actioned(c, *id, Some(250))).unwrap();
        assert_eq!(
            case["actioned"]["label"],
            found(actioned.len()).as_str(),
            "{name}"
        );
        let ours: Vec<(String, String, String)> = actioned
            .iter()
            .map(|(a, b, t, when)| {
                (
                    hex[a].clone(),
                    hex[b].clone(),
                    actioned_cell(*t, *when, now),
                )
            })
            .collect();
        assert_eq!(ours, rows(&case["actioned"]), "{name}");

        // denied, by their groups' kings
        let denied = store
            .read(|c| auto::denied_pairs(c, *id, Some(250)))
            .unwrap();
        assert_eq!(
            case["denied"]["label"],
            found(denied.len()).as_str(),
            "{name}"
        );
        let ours: Vec<(String, String, String)> = denied
            .iter()
            .map(|d| {
                (
                    hex[&d.kings.0].clone(),
                    hex[&d.kings.1].clone(),
                    denied_cell(d.timestamp_ms, now),
                )
            })
            .collect();
        assert_eq!(ours, rows(&case["denied"]), "{name}");
    }
}

#[test]
fn approving_and_denying_report_progress_as_the_reference_s() {
    use hydrus_gui_model::auto_resolution_review::{action_progress, action_title};
    assert_eq!(action_progress(true, 4, 12), "approving: 4/12");
    assert_eq!(action_progress(false, 0, 1_500), "denying: 0/1,500");
    assert_eq!(action_title(true), "approving auto-resolution decisions");
}

// (not tagged audit-media-review-progress: this drives the model function, not
// the sidebar's approve/deny buttons; see docs/rust/notes/impl-small-areas.md)
#[test]
fn approving_and_denying_run_through_a_popup_job_that_shows_progress_and_goes_when_done() {
    use hydrus_gui_model::auto_resolution_review::{POPUP_AFTER, action_pairs_after};
    use std::cell::RefCell;
    use std::sync::Mutex;
    use std::time::Duration;

    // the reference waits four seconds before its popup shows
    assert_eq!(POPUP_AFTER, Duration::from_secs(4));
    let recorded = hydrus_testkit::fixture_json("auto_resolution_review.json");
    let reviewed = recorded["reviewed"].as_str().unwrap();
    for approve in [true, false] {
        let (_dir, store, _hex) = store();
        let rules = store.read(auto::rules).unwrap();
        let id = rules.iter().find(|(_, r)| r.name == reviewed).unwrap().0;
        let pairs: Vec<_> = store
            .read(|c| auto::pending_pairs(c, id, None))
            .unwrap()
            .into_iter()
            .map(|(_, a, b)| (a, b))
            .collect();
        assert_eq!(pairs.len(), 2);
        let status = Mutex::new(String::new());
        let seen: RefCell<Vec<(String, Vec<Option<String>>)>> = RefCell::default();
        let now = hydrus_core::TimestampMs::now().millis() / 1000;
        // (with the wait made nothing: before the work starts, the popup is
        // there with the reference's title; at the end, it is gone)
        action_pairs_after(
            &store,
            id,
            &pairs,
            approve,
            &status,
            Duration::ZERO,
            &|text| {
                let jobs = store.read(|c| hydrus_store::popups::all(c, now)).unwrap();
                seen.borrow_mut().push((
                    text.to_owned(),
                    jobs.iter().map(|j| j.status_text_1.clone()).collect(),
                ));
            },
        )
        .unwrap();
        let title = if approve {
            "approving auto-resolution decisions"
        } else {
            "denying auto-resolution decisions"
        };
        let progress = if approve {
            "approving: 0/2"
        } else {
            "denying: 0/2"
        };
        assert_eq!(
            *seen.borrow(),
            [
                (progress.to_owned(), vec![Some(title.to_owned())]),
                (String::new(), vec![]),
            ]
        );
        assert_eq!(*status.lock().unwrap(), progress);
        assert!(
            store
                .read(|c| hydrus_store::popups::all(c, now))
                .unwrap()
                .is_empty(),
            "finished and dismissed"
        );
        // and the pairs were decided
        assert!(
            store
                .read(|c| auto::pending_pairs(c, id, None))
                .unwrap()
                .is_empty()
        );
    }
    // no popup before the wait is up: a quick job leaves none behind
    let (_dir, store, _hex) = store();
    let rules = store.read(auto::rules).unwrap();
    let id = rules.iter().find(|(_, r)| r.name == reviewed).unwrap().0;
    let pairs: Vec<_> = store
        .read(|c| auto::pending_pairs(c, id, None))
        .unwrap()
        .into_iter()
        .map(|(_, a, b)| (a, b))
        .collect();
    let touched = RefCell::new(0);
    action_pairs_after(
        &store,
        id,
        &pairs,
        true,
        &Mutex::new(String::new()),
        Duration::from_secs(3600),
        &|_| *touched.borrow_mut() += 1,
    )
    .unwrap();
    assert_eq!(*touched.borrow(), 0);
}
