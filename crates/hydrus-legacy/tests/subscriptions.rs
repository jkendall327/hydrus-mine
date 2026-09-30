//! Subscriptions, their queries and the queries' histories, and checker
//! timing, against `oracle/fixtures/subscriptions.json` (made by
//! `oracle/dump_subscriptions.py`): seeds saved in every older version read
//! as the reference reads them, and every field lands where the reference
//! puts it.

use serde_json::{Value as Json, json};

use hydrus_core::subscriptions::{CheckerOptions, SeedTime};
use hydrus_legacy::objects::import_options::{slice, tags};
use hydrus_legacy::objects::subscriptions::{file_seed, gallery_seed, query_log, subscription};

mod common;
use common::seeds::{file_seed_facts, gallery_seed_facts, object};

#[test]
fn seeds_of_every_version_read_as_the_reference_reads_them() {
    let recorded = hydrus_testkit::fixture_json("subscriptions.json");
    for case in recorded["file_seeds"].as_array().unwrap() {
        let stored = file_seed(&object(&case["stored"])).unwrap();
        assert_eq!(
            file_seed_facts(&stored),
            case["facts"],
            "{}",
            case["stored"]
        );
        let expected = file_seed(&object(&case["expected"])).unwrap();
        assert_eq!(file_seed_facts(&expected), case["facts"]);
    }
    for case in recorded["gallery_seeds"].as_array().unwrap() {
        let stored = gallery_seed(&object(&case["stored"])).unwrap();
        assert_eq!(
            gallery_seed_facts(&stored),
            case["facts"],
            "{}",
            case["stored"]
        );
        let expected = gallery_seed(&object(&case["expected"])).unwrap();
        assert_eq!(gallery_seed_facts(&expected), case["facts"]);
    }
}

#[test]
fn query_logs_hold_their_seeds_in_order() {
    let recorded = hydrus_testkit::fixture_json("subscriptions.json");
    for case in recorded["query_logs"].as_array().unwrap() {
        let log = query_log(&object(&case["stored"])).unwrap();
        let facts = &case["facts"];
        assert_eq!(log.name, facts["name"].as_str().unwrap());
        let gallery: Vec<Json> = log.gallery_seeds.iter().map(gallery_seed_facts).collect();
        assert_eq!(json!(gallery), facts["gallery_seeds"]);
        let files: Vec<Json> = log.file_seeds.iter().map(file_seed_facts).collect();
        assert_eq!(json!(files), facts["file_seeds"]);
    }
}

/// Subscriptions of today's version, and of versions 1-3 with old-style
/// import options (of every version the reference upgrades from), which
/// must convert as the reference converts them.
#[test]
fn subscriptions_read_as_the_reference_reads_them() {
    let recorded = hydrus_testkit::fixture_json("subscriptions.json");
    let current = recorded["subscriptions"].as_array().unwrap();
    let old = recorded["old_subscriptions"].as_array().unwrap();
    for case in current.iter().chain(old) {
        let s = subscription(&object(&case["stored"])).unwrap();
        assert!(s.unconverted.is_empty(), "{:?}", s.unconverted);
        if let Some(expected) = case.get("import_options") {
            assert_eq!(
                s.import_options,
                slice(&object(expected)).unwrap(),
                "{}",
                case["stored"]
            );
        }
        let facts = &case["facts"];
        let queries: Vec<Json> = s
            .queries
            .iter()
            .zip(facts["queries"].as_array().unwrap())
            .map(|(q, expected)| {
                assert_eq!(
                    q.tag_import_options,
                    tags(&object(&expected["tag_import_options"])).unwrap()
                );
                json!({
                    "log_name": q.log_name,
                    "query_text": q.query_text,
                    "display_name": q.display_name,
                    "check_now": q.check_now,
                    "last_check_time": q.last_check_time,
                    "next_check_time": q.next_check_time,
                    "paused": q.paused,
                    "checker_status": q.checker_status,
                    "file_seed_compaction_number": q.file_seed_compaction_number,
                    "gallery_seed_compaction_number": q.gallery_seed_compaction_number,
                    "tag_import_options": expected["tag_import_options"],
                })
            })
            .collect();
        let c = &s.checker;
        assert_eq!(
            Some(c.intended_files_per_check),
            facts["checker"][0].as_f64()
        );
        assert_eq!(
            json!({
                "name": s.name,
                "gug_key": s.gug_key,
                "gug_name": s.gug_name,
                "queries": queries,
                "checker": [
                    facts["checker"][0],
                    c.never_faster_than,
                    c.never_slower_than,
                    [c.death_file_velocity.0, c.death_file_velocity.1],
                ],
                "initial_file_limit": s.initial_file_limit,
                "periodic_file_limit": s.periodic_file_limit,
                "this_is_a_random_sample": s.this_is_a_random_sample,
                "paused": s.paused,
                "no_work_until": s.no_work_until,
                "no_work_until_reason": s.no_work_until_reason,
                "presentation": [
                    s.show_a_popup_while_working,
                    s.publish_files_to_popup_button,
                    s.publish_files_to_page,
                    s.publish_label_override,
                    s.merge_query_publish_events,
                ],
            }),
            *facts
        );
    }
}

#[test]
fn checks_are_timed_as_the_reference_times_them() {
    let recorded = hydrus_testkit::fixture_json("subscriptions.json");
    for case in recorded["checkers"].as_array().unwrap() {
        let o = case["options"].as_array().unwrap();
        let checker = CheckerOptions {
            intended_files_per_check: o[0].as_f64().unwrap(),
            never_faster_than: o[1].as_i64().unwrap(),
            never_slower_than: o[2].as_i64().unwrap(),
            death_file_velocity: (o[3][0].as_i64().unwrap(), o[3][1].as_i64().unwrap()),
        };
        let seeds: Vec<SeedTime> = case["seeds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| SeedTime {
                source_time: s[0].as_i64(),
                created: s[1].as_i64().unwrap(),
            })
            .collect();
        let last = case["last_check_time"].as_i64().unwrap();
        let now = case["now"].as_i64().unwrap();
        assert_eq!(
            json!({
                "next_check_time": checker.next_check_time(&seeds, last, now),
                "is_dead": checker.is_dead(&seeds, last),
                "death_file_velocity_period": checker.death_file_velocity_period(),
            }),
            json!({
                "next_check_time": case["next_check_time"],
                "is_dead": case["is_dead"],
                "death_file_velocity_period": case["death_file_velocity_period"],
            }),
            "{case}"
        );
    }
}

#[test]
fn histories_compact_as_the_reference_compacts_them() {
    use hydrus_core::subscriptions::{
        FileLogEntry, compact_file_log, compact_gallery_log, num_master_file_seeds,
    };

    let recorded = hydrus_testkit::fixture_json("subscriptions.json");
    for case in recorded["compactions"].as_array().unwrap() {
        let files = case["files"].as_array().unwrap();
        let entries: Vec<FileLogEntry<'_>> = files
            .iter()
            .map(|f| FileLogEntry {
                unknown: f[0] == 0,
                child_files_note: (f[0] == 9).then(|| f[1].as_str().unwrap()),
                time: SeedTime {
                    source_time: f[2].as_i64(),
                    created: f[3].as_i64().unwrap(),
                },
            })
            .collect();
        let keep = case["keep"].as_u64().unwrap() as usize;
        let before = case["before"].as_i64().unwrap();
        assert_eq!(
            json!(num_master_file_seeds(&entries)),
            case["master"],
            "{case}"
        );
        assert_eq!(
            json!(compact_file_log(&entries, keep, before)),
            case["removed"],
            "{case}"
        );
        let galleries: Vec<(bool, i64)> = case["galleries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| (g[0] == 0, g[1].as_i64().unwrap()))
            .collect();
        let gallery_keep = case["gallery_keep"].as_u64().unwrap() as usize;
        assert_eq!(
            json!(compact_gallery_log(&galleries, gallery_keep, before)),
            case["gallery_removed"],
            "{case}"
        );
    }
}
