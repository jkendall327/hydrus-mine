//! Subscriptions, their queries and the queries' histories, and checker
//! timing, against `oracle/fixtures/subscriptions.json` (made by
//! `oracle/dump_subscriptions.py`): seeds saved in every older version read
//! as the reference reads them, and every field lands where the reference
//! puts it.

use serde_json::{Value as Json, json};

use hydrus_core::subscriptions::{CheckerOptions, SeedTime};
use hydrus_legacy::objects::import_options::tags;
use hydrus_legacy::objects::subscriptions::{
    LegacyFileSeed, LegacyGallerySeed, file_seed, gallery_seed, query_log, subscription,
};
use hydrus_legacy::serialisable::SerialisableObject;

fn object(value: &Json) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

fn sorted(items: &[String]) -> Json {
    let mut items = items.to_vec();
    items.sort();
    json!(items)
}

fn sorted_pairs(items: &[(String, String)]) -> Json {
    let mut items = items.to_vec();
    items.sort();
    json!(items)
}

fn service_tags(items: &[(String, Vec<String>)]) -> Json {
    items
        .iter()
        .map(|(key, tags)| (key.clone(), sorted(tags)))
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn file_seed_facts(f: &LegacyFileSeed) -> Json {
    json!({
        "type": f.seed_type,
        "data": f.data,
        // (the oracle has no URL classes, so it cannot normalise either)
        "comparison": f.data_for_comparison.as_ref().unwrap_or(&f.data),
        "created": f.created,
        "modified": f.modified,
        "source_time": f.source_time,
        "status": f.status,
        "note": f.note,
        "referral": f.referral_url,
        "headers": sorted_pairs(&f.request_headers),
        "filterable": sorted(&f.external_filterable_tags),
        "additional": service_tags(&f.external_additional_tags),
        "primary": sorted(&f.primary_urls),
        "source": sorted(&f.source_urls),
        "tags": sorted(&f.tags),
        "notes": sorted_pairs(&f.notes),
        "hashes": sorted_pairs(&f.hashes),
    })
}

fn gallery_seed_facts(g: &LegacyGallerySeed) -> Json {
    json!({
        "url": g.url,
        "can_generate_more_pages": g.can_generate_more_pages,
        "created": g.created,
        "modified": g.modified,
        "status": g.status,
        "note": g.note,
        "referral": g.referral_url,
        "headers": sorted_pairs(&g.request_headers),
        "filterable": sorted(&g.external_filterable_tags),
        "additional": service_tags(&g.external_additional_tags),
    })
}

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

#[test]
fn subscriptions_read_as_the_reference_reads_them() {
    let recorded = hydrus_testkit::fixture_json("subscriptions.json");
    for case in recorded["subscriptions"].as_array().unwrap() {
        let s = subscription(&object(&case["stored"])).unwrap();
        let facts = &case["facts"];
        let queries: Vec<Json> = s
            .queries
            .iter()
            .zip(facts["queries"].as_array().unwrap())
            .map(|(q, expected)| {
                assert_eq!(
                    q.tag_import_options.as_ref(),
                    Some(&tags(&object(&expected["tag_import_options"])).unwrap())
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
        assert!(s.import_options.is_some());
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
