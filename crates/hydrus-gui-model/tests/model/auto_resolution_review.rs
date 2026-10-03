//! The "review actions" window's tabs, as the reference lists them for
//! each rule of the database `oracle/record_auto_resolution.py` left
//! (`oracle/record_auto_resolution_review.py`): the tab it starts on, each
//! tab's label, and each pair with its third column.

use std::collections::HashMap;

use hydrus_core::HashId;
use hydrus_gui_model::auto_resolution_review::{
    FULLY_AUTOMATIC, TABS, actioned_cell, denied_cell, found, pending_cell, start_tab,
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
            for (_, _, text) in &theirs {
                assert_eq!(text.lines().next().unwrap(), pending_cell(rule), "{name}");
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
