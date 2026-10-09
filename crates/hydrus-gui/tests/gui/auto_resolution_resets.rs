//! The auto-resolution tab's reset buttons against the reference's, replaying
//! `oracle/fixtures/auto_resolution_resets.json`
//! (`oracle/record_auto_resolution_resets.py`: the real panel on the database
//! the reference's auto-resolution run left, buttons pressed with rules
//! selected or none, the question answered yes or no): the question asked and
//! every rule's pair counts by status after each step.

use std::collections::BTreeMap;

use serde_json::Value;
use slint::Model as _;

use crate::auto_resolution_review::opened;
use hydrus_store::Store;
use hydrus_store::duplicates::auto;

/// Each rule's pairs by status, as the recording has them.
fn counts(store: &Store) -> BTreeMap<String, BTreeMap<String, i64>> {
    let rules = store.read(auto::rules).unwrap();
    rules
        .into_iter()
        .map(|(id, rule)| {
            let by_status = store.read(move |c| auto::counts(c, id)).unwrap();
            (
                rule.name,
                by_status
                    .into_iter()
                    .filter(|(_, n)| *n > 0)
                    .map(|(s, n)| (s.code().to_string(), i64::try_from(n).unwrap()))
                    .collect(),
            )
        })
        .collect()
}

fn recorded_counts(value: &Value) -> BTreeMap<String, BTreeMap<String, i64>> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, by_status)| {
            (
                name.clone(),
                by_status
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(s, n)| (s.clone(), n.as_i64().unwrap()))
                    .collect(),
            )
        })
        .collect()
}

// leaf: audit-media-rule-sidebar-resets
#[test]
fn the_reset_buttons_ask_and_move_pairs_as_the_reference_does() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("auto_resolution_resets.json");
    let o = opened();
    let (ui, store) = (&o.ui, &o.store);
    // the same database to begin with
    assert_eq!(
        counts(store),
        recorded_counts(&recorded["start"]),
        "the starting pair counts"
    );

    let names = |ui: &hydrus_gui::MainWindow| -> Vec<String> {
        let rows = ui.get_duplicates_rules();
        (0..rows.row_count())
            .map(|r| {
                rows.row_data(r)
                    .unwrap()
                    .cells
                    .row_data(0)
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    for step in recorded["steps"].as_array().unwrap() {
        let button = step["button"].as_str().unwrap();
        let action = match button {
            "reset_search" => "reset search",
            "reset_test" => "reset test",
            "reset_denied" => "reset denied",
            // the reference's regen numbers and resync buttons for the rules are
            // not ported (docs/rust/DIFFERENCES.md); the counts stay as they were
            _ => continue,
        };
        // the selection: the named rules, or none
        let listed = names(ui);
        if let Some(selected) = step["selected"].as_array() {
            // (clear what is selected, then select each named rule)
            for (i, _) in listed.iter().enumerate() {
                let rows = ui.get_duplicates_rules();
                if rows.row_data(i).unwrap().selected {
                    ui.invoke_duplicates_action("rule".into(), i as i32, true, false);
                }
            }
            for name in selected {
                let at = listed
                    .iter()
                    .position(|n| n == name.as_str().unwrap())
                    .unwrap();
                ui.invoke_duplicates_action("rule".into(), at as i32, true, false);
            }
        } else {
            for (i, _) in listed.iter().enumerate() {
                let rows = ui.get_duplicates_rules();
                if rows.row_data(i).unwrap().selected {
                    ui.invoke_duplicates_action("rule".into(), i as i32, true, false);
                }
            }
        }
        ui.invoke_duplicates_action(action.into(), 0, false, false);
        let asked = step["asked"].as_array().unwrap();
        assert_eq!(asked.len(), 1, "{step}");
        let question = ui.get_duplicates().asking_message.to_string();
        assert_eq!(
            question,
            asked[0]["message"].as_str().unwrap(),
            "{button} {}",
            step["selected"]
        );
        if step["answer"] == "yes" {
            ui.invoke_duplicates_action("chosen".into(), 0, false, false);
        } else {
            ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
        }
        assert_eq!(
            counts(store),
            recorded_counts(&step["counts"]),
            "after {button} on {} answered {}",
            step["selected"],
            step["answer"]
        );
    }
}

// leaf: audit-media-preparation-storage-resync
#[test]
fn resyncing_potential_pairs_to_local_storage_clears_the_pairs_the_reference_does() {
    let _windows = hydrus_gui::headless::init();
    let recorded = hydrus_testkit::fixture_json("potential_pairs_resync.json");
    let o = opened();
    let (ui, store) = (&o.ui, &o.store);
    // the orphans: the recorded files taken out of the local file tables
    // before the reference started (and the pairs it recorded)
    let orphans: Vec<_> = recorded["orphans"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| o.ids[&h.as_str().unwrap().to_lowercase()])
        .collect();
    let snapshot = store.snapshot();
    let domains: Vec<_> = {
        use hydrus_core::service::builtin_keys as keys;
        [
            keys::HYDRUS_LOCAL_FILE_STORAGE,
            keys::COMBINED_LOCAL_FILE_DOMAINS,
            keys::MY_FILES,
        ]
        .into_iter()
        .map(|k| snapshot.services.builtin(k).unwrap().id)
        .collect()
    };
    let removed = orphans.clone();
    store
        .write(move |ctx| {
            for id in &removed {
                for domain in &domains {
                    ctx.conn().execute(
                        "DELETE FROM file_domain_current WHERE hash_id = ?1 AND service_id = ?2",
                        rusqlite::params![id, domain],
                    )?;
                }
            }
            Ok(())
        })
        .unwrap();
    let pairs_now = || -> Vec<Vec<String>> {
        let rows: Vec<(hydrus_core::HashId, hydrus_core::HashId)> = store
            .read(|c| {
                let mut q = c.prepare(
                    "SELECT k1.king_hash_id, k2.king_hash_id FROM potential_pairs p
                     JOIN dup_groups k1 ON k1.group_id = p.smaller_group_id
                     JOIN dup_groups k2 ON k2.group_id = p.larger_group_id",
                )?;
                Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<Result<_, _>>()?)
            })
            .unwrap();
        let mut out: Vec<Vec<String>> = rows
            .into_iter()
            .map(|(a, b)| vec![o.hex[&a].clone(), o.hex[&b].clone()])
            .collect();
        out.sort();
        out
    };
    let theirs = |pairs: &Value| -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = pairs
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                vec![
                    p[0].as_str().unwrap().to_lowercase(),
                    p[1].as_str().unwrap().to_lowercase(),
                ]
            })
            .collect();
        out.sort();
        out
    };
    // (the pairs as the reference had them, each pair's files by king)
    assert_eq!(pairs_now(), theirs(&recorded["before"]));
    for press in recorded["presses"].as_array().unwrap() {
        ui.invoke_duplicates_action("resync pairs".into(), 0, false, false);
        let asked = press["asked"].as_array().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(
            ui.get_duplicates().asking_message,
            asked[0]["message"].as_str().unwrap()
        );
        if press["answer"] == "yes" {
            ui.invoke_duplicates_action("chosen".into(), 0, false, false);
        } else {
            ui.invoke_duplicates_action("cancelled".into(), 0, false, false);
        }
        assert_eq!(
            pairs_now(),
            theirs(&press["pairs"]),
            "answered {}",
            press["answer"]
        );
        let job = press["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["title"] == "resyncing potential pairs to hydrus local file storage");
        if press["answer"] == "yes" {
            let job = job.expect("the recorded resync job");
            let popups = store
                .read(|c| hydrus_store::popups::all(c, i64::MAX / 4))
                .unwrap();
            let popup = popups
                .iter()
                .find(|p| p.status_title.as_deref() == Some(job["title"].as_str().unwrap()))
                .expect("the resync's popup");
            assert_eq!(popup.status_text_1.as_deref(), job["text"].as_str());
        } else {
            assert!(job.is_none(), "no resync when answered no");
        }
    }
}
