//! Duplicates auto-resolution against the reference's
//! (`oracle/fixtures/auto_resolution_run.json`, made by
//! `oracle/record_auto_resolution.py`): the same files imported in the same
//! order, searched for similar files at distance 4, with the same rules,
//! worked until no rule has work, must leave every pair with the same status
//! for every rule, the same actioned logs, the same duplicate groups and
//! kings, the same potential pairs and the same files deleted.
//!
//! Rules with comparators that read file content (visual duplicates, jpeg
//! quality) are not tested yet: for those only the search is compared, and
//! the pairs the reference tested wait untested here.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use hydrus_core::service::builtin_keys;
use hydrus_duplicates::engine::{NoShuffle, work_rules};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_search::Clock;
use hydrus_store::Store;
use hydrus_store::duplicates::auto::{self, PairStatus};
use hydrus_store::similar::{self, SimilarFilesSettings};
use serde_json::{Value, json};

fn label(status: PairStatus) -> &'static str {
    match status {
        PairStatus::DoesNotMatchSearch => "does_not_match",
        PairStatus::MatchesSearchNotTested => "not_tested",
        PairStatus::FailedTest => "failed",
        PairStatus::NotSearched => "not_searched",
        PairStatus::Denied => "denied",
        PairStatus::ReadyToAction => "pending",
        PairStatus::Actioned => "actioned",
    }
}

/// status, smaller king, larger king, A, B
type QueueRow = (i64, i64, i64, Option<i64>, Option<i64>);

fn hex(conn: &rusqlite::Connection, hash_id: i64) -> String {
    conn.query_row(
        "SELECT lower(hex(sha256)) FROM hashes WHERE hash_id = ?",
        [hash_id],
        |r| r.get(0),
    )
    .unwrap()
}

/// Our statuses for a rule, as the reference's are recorded.
fn statuses(store: &Store, rule_id: i64) -> BTreeMap<String, Vec<Vec<String>>> {
    store
        .read(|conn| {
            let mut out: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
            for status in PairStatus::ALL {
                if status != PairStatus::Actioned {
                    out.insert(label(status).into(), Vec::new());
                }
            }
            let mut stmt = conn.prepare(
                "SELECT q.status, gs.king_hash_id, gl.king_hash_id, q.hash_id_a, q.hash_id_b FROM dup_auto_pairs AS q
                 JOIN dup_groups AS gs ON gs.group_id = q.smaller_group_id
                 JOIN dup_groups AS gl ON gl.group_id = q.larger_group_id
                 WHERE q.rule_id = ?",
            )?;
            let rows: Vec<QueueRow> = stmt
                .query_map([rule_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            for (status, ks, kl, a, b) in rows {
                let status = PairStatus::from_code(status).unwrap();
                let pair = if status == PairStatus::ReadyToAction {
                    vec![hex(conn, a.unwrap()), hex(conn, b.unwrap())]
                } else {
                    let mut p = vec![hex(conn, ks), hex(conn, kl)];
                    p.sort();
                    p
                };
                out.get_mut(label(status)).unwrap().push(pair);
            }
            for list in out.values_mut() {
                list.sort();
            }
            Ok(out)
        })
        .unwrap()
}

fn run_until_done(store: &Store, clock: &Clock) {
    let mut passes = 0;
    loop {
        passes += 1;
        let done = work_rules(store, Duration::from_secs(600), &mut NoShuffle, clock).unwrap();
        if done.searched + done.tested == 0 && !done.more_to_do {
            break;
        }
        assert!(passes < 50, "the rules never finish");
    }
}

#[test]
fn rules_do_what_the_reference_did() {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_run.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    for file in recorded["files"].as_array().unwrap() {
        let name = file[0].as_str().unwrap();
        let result = importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("auto_resolution/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap();
        assert_eq!(
            result.hash.unwrap().to_hex(),
            file[1].as_str().unwrap(),
            "{name}"
        );
    }
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &SimilarFilesSettings {
                    search_distance: 4,
                    ..SimilarFilesSettings::default()
                },
            )
        })
        .unwrap();
    while similar::run_search(&store, 100).unwrap() > 0 {}

    // the reference's rules, with its ids
    let mut rule_ids = Vec::new();
    for case in recorded["rules"].as_array().unwrap() {
        let stored = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
            &case["stored"].to_string(),
        )
        .unwrap();
        let legacy =
            hydrus_legacy::objects::auto_resolution::AutoResolutionRule::from_object(&stored)
                .unwrap();
        let rule = hydrus_store::import::auto_resolution_rule(&legacy, &mut Vec::new()).unwrap();
        let id = legacy.id;
        store
            .write(move |ctx| auto::add_rule(ctx.conn(), &rule, Some(id)).map(|_| ()))
            .unwrap();
        rule_ids.push(id);
    }

    let clock = Clock::system();
    run_until_done(&store, &clock);

    // the human's decisions on the capped rule's waiting pairs
    let capped = store
        .read(auto::rules)
        .unwrap()
        .into_iter()
        .find(|(_, r)| r.name == "pixel-perfect gifs vs pngs")
        .unwrap()
        .0;
    for decision in recorded["decisions"].as_array().unwrap() {
        let id = |i: usize| {
            let hash: hydrus_core::Sha256 = decision[i].as_str().unwrap().parse().unwrap();
            store
                .read(|conn| hydrus_store::master::hash_id(conn, &hash))
                .unwrap()
                .unwrap()
        };
        let pair = [(id(0), id(1))];
        match decision[2].as_str().unwrap() {
            "approve" => assert_eq!(
                hydrus_duplicates::approve(&store, capped, &pair).unwrap(),
                1
            ),
            _ => assert_eq!(hydrus_duplicates::deny(&store, capped, &pair).unwrap(), 1),
        }
    }
    run_until_done(&store, &clock);

    let mut report = String::new();
    let rules = store.read(auto::rules).unwrap();
    for (case, rule_id) in recorded["rules"].as_array().unwrap().iter().zip(&rule_ids) {
        let rule = &rules.iter().find(|(id, _)| id == rule_id).unwrap().1;
        let name = &rule.name;
        let ours = statuses(&store, *rule_id);
        let theirs: BTreeMap<String, Vec<Vec<String>>> =
            serde_json::from_value(case["statuses"].clone()).unwrap();
        let untestable = rule
            .comparators
            .iter()
            .any(hydrus_duplicates::selector::needs_file_content);
        if untestable {
            // only the search: what the reference tested waits here
            let mut matched: Vec<Vec<String>> = ["not_tested", "failed", "pending"]
                .iter()
                .flat_map(|k| theirs[*k].iter().cloned())
                .collect();
            matched.sort();
            if ours["not_tested"] != matched || ours["does_not_match"] != theirs["does_not_match"] {
                report.push_str(&format!(
                    "{name}: search differs\n  ours {ours:?}\n  theirs {theirs:?}\n"
                ));
            }
            continue;
        }
        if ours != theirs {
            report.push_str(&format!(
                "{name}: statuses differ\n  ours   {ours:?}\n  theirs {theirs:?}\n"
            ));
        }
        let actioned: Vec<Value> = store
            .read(|conn| {
                let mut rows: Vec<Value> = auto::actioned(conn, *rule_id, None)?
                    .into_iter()
                    .map(|(a, b, t, _)| {
                        json!([
                            hex(conn, a.get().into()),
                            hex(conn, b.get().into()),
                            t.code()
                        ])
                    })
                    .collect();
                rows.sort_by_key(ToString::to_string);
                Ok(rows)
            })
            .unwrap();
        let mut expected: Vec<Value> = case["actioned"].as_array().unwrap().clone();
        expected.sort_by_key(ToString::to_string);
        if actioned != expected {
            report.push_str(&format!(
                "{name}: actioned differ\n  ours   {actioned:?}\n  theirs {expected:?}\n"
            ));
        }
    }

    // the files' groups
    let kings: BTreeMap<String, String> = store
        .read(|conn| {
            let mut out = BTreeMap::new();
            let mut stmt = conn.prepare(
                "SELECT m.hash_id, g.king_hash_id FROM dup_group_members AS m JOIN dup_groups AS g USING (group_id)",
            )?;
            let rows: Vec<(i64, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (file, king) in rows {
                out.insert(hex(conn, file), hex(conn, king));
            }
            Ok(out)
        })
        .unwrap();
    for (file, king) in recorded["kings"].as_object().unwrap() {
        let ours = kings.get(file).unwrap_or(file);
        if ours != king.as_str().unwrap() {
            report.push_str(&format!(
                "{file}: our king is {ours}, the reference's {king}\n"
            ));
        }
    }

    let potentials: BTreeSet<String> = store
        .read(|conn| {
            let mut stmt = conn.prepare(
                "SELECT gs.king_hash_id, gl.king_hash_id, p.distance FROM potential_pairs AS p
                 JOIN dup_groups AS gs ON gs.group_id = p.smaller_group_id
                 JOIN dup_groups AS gl ON gl.group_id = p.larger_group_id",
            )?;
            let rows: Vec<(i64, i64, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows
                .into_iter()
                .map(|(a, b, d)| {
                    let mut p = [hex(conn, a), hex(conn, b)];
                    p.sort();
                    json!([p[0], p[1], d]).to_string()
                })
                .collect())
        })
        .unwrap();
    let expected: BTreeSet<String> = recorded["potentials"]
        .as_array()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    if potentials != expected {
        report.push_str(&format!(
            "potential pairs differ: ours only {:?}, theirs only {:?}\n",
            potentials.difference(&expected).collect::<Vec<_>>(),
            expected.difference(&potentials).collect::<Vec<_>>()
        ));
    }

    let current: BTreeSet<String> = store
        .read(|conn| {
            let snapshot = store.snapshot();
            let my_files = snapshot.services.builtin(builtin_keys::MY_FILES)?.id;
            let mut stmt =
                conn.prepare("SELECT hash_id FROM file_domain_current WHERE service_id = ?")?;
            let ids: Vec<i64> = stmt
                .query_map([my_files], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(ids.into_iter().map(|id| hex(conn, id)).collect())
        })
        .unwrap();
    let expected: BTreeSet<String> = recorded["current_in_my_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    if current != expected {
        report.push_str(&format!(
            "files in my files differ: ours only {:?}, theirs only {:?}\n",
            current.difference(&expected).collect::<Vec<_>>(),
            expected.difference(&current).collect::<Vec<_>>()
        ));
    }
    assert!(report.is_empty(), "{report}");
}

/// The reference's database after the same run, imported: every rule comes
/// across with every pair's status and its actioned log, and running the
/// rules again finds nothing to do.
#[test]
fn a_migrated_database_keeps_the_rules_progress() {
    let recorded = hydrus_testkit::fixture_json("auto_resolution_run.json");
    let legacy = hydrus_testkit::legacy_fixture("auto_resolution");
    let dir = tempfile::tempdir().unwrap();
    let report = hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    assert!(
        !report
            .warnings
            .iter()
            .any(|w| w.contains("auto-resolution")),
        "{:?}",
        report.warnings
    );
    let store = Store::open(dir.path()).unwrap();
    let rules = store.read(auto::rules).unwrap();
    let cases = recorded["rules"].as_array().unwrap();
    assert_eq!(rules.len(), cases.len());
    let mut problems = String::new();
    for case in cases {
        let id = case["stored"][3][0].as_i64().unwrap();
        let name = case["stored"][1].as_str().unwrap();
        let (_, rule) = rules.iter().find(|(i, _)| *i == id).unwrap();
        assert_eq!(rule.name, name);
        let theirs: BTreeMap<String, Vec<Vec<String>>> =
            serde_json::from_value(case["statuses"].clone()).unwrap();
        let ours = statuses(&store, id);
        if ours != theirs {
            problems.push_str(&format!("{name}: ours {ours:?}\n  theirs {theirs:?}\n"));
        }
        let n = store
            .read(|conn| auto::actioned(conn, id, None))
            .unwrap()
            .len();
        assert_eq!(n, case["actioned"].as_array().unwrap().len(), "{name}");
    }
    assert!(problems.is_empty(), "{problems}");
    let done = work_rules(
        &store,
        Duration::from_secs(600),
        &mut NoShuffle,
        &Clock::system(),
    )
    .unwrap();
    assert_eq!(done.searched + done.actioned, 0, "{done:?}");
}
