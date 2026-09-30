//! In-memory predicate tests against the reference's
//! (`oracle/fixtures/media_tests.json`, made by
//! `oracle/record_media_tests.py`): the `basic` fixture is imported, each
//! file's facts are loaded from the store, and every recorded predicate
//! (decoded from its stored form) must match the same files, and every
//! comparable property must have the same value, as in the reference.

use std::collections::{BTreeSet, HashMap};

use hydrus_core::search::comparable::Comparable;
use hydrus_core::{HashId, Sha256};
use hydrus_search::Clock;
use hydrus_search::media::{self, FileFacts};
use hydrus_store::Store;

fn load() -> (serde_json::Value, HashMap<String, FileFacts>) {
    let recorded = hydrus_testkit::fixture_json("media_tests.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let work = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &work.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(work.path()).unwrap();
    let snapshot = store.snapshot();
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hashes: Vec<Sha256> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["hash"].as_str().unwrap().parse().unwrap())
        .collect();
    let facts = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            let by_id: Vec<HashId> = hashes.iter().map(|h| ids[h]).collect();
            let facts = media::load_facts(conn, &snapshot, &by_id)?;
            Ok(hashes
                .iter()
                .filter_map(|h| Some((h.to_hex(), facts.get(&ids[h])?.clone())))
                .collect())
        })
        .unwrap();
    (recorded, facts)
}

#[test]
fn predicates_match_the_same_files_as_in_the_reference() {
    let (recorded, facts) = load();
    let offset = recorded["utc_offset_seconds"].as_i64().unwrap();
    let clock = Clock::fixed(
        recorded["now_ms"].as_i64().unwrap(),
        jiff::tz::TimeZone::fixed(
            jiff::tz::Offset::from_seconds(i32::try_from(offset).unwrap()).unwrap(),
        ),
    );
    let mut report = String::new();
    let mut checked = 0;
    for case in recorded["tests"].as_array().unwrap() {
        let stored = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
            &case["serialised"].to_string(),
        )
        .unwrap();
        let text = case["text"].as_str().unwrap();
        let predicate = match hydrus_legacy::objects::predicates::predicate(&stored) {
            Ok(p) => p,
            Err(e) => {
                report.push_str(&format!("{text}: could not decode: {e}\n"));
                continue;
            }
        };
        assert!(media::can_test(&predicate), "{text}");
        let ours: BTreeSet<&str> = facts
            .iter()
            .filter(|(_, f)| media::test(&predicate, f, &clock))
            .map(|(h, _)| h.as_str())
            .collect();
        let expected: BTreeSet<&str> = case["matching"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap())
            .collect();
        checked += 1;
        if ours != expected {
            let extra: Vec<_> = ours.difference(&expected).collect();
            let missing: Vec<_> = expected.difference(&ours).collect();
            report.push_str(&format!(
                "{text}: we also match {extra:?}, and miss {missing:?}\n"
            ));
        }
    }
    println!("{checked} predicates checked");
    assert!(report.is_empty(), "in-memory tests differ:\n{report}");
}

#[test]
fn comparable_values_match_the_reference() {
    let (recorded, facts) = load();
    let mut report = String::new();
    for (code, values) in recorded["values"].as_object().unwrap() {
        let what = Comparable::from_predicate_type(code.parse().unwrap())
            .unwrap_or_else(|| panic!("predicate type {code} is not comparable here"));
        for (hash, expected) in values.as_object().unwrap() {
            let ours = media::extract(what, &facts[hash]);
            let same = match (ours, expected.as_f64()) {
                (None, None) => expected.is_null(),
                (Some(a), Some(b)) => (a - b).abs() <= 1e-9 * b.abs().max(1.0),
                _ => false,
            };
            if !same {
                report.push_str(&format!(
                    "{what:?} of {hash}: ours {ours:?}, reference {expected}\n"
                ));
            }
        }
    }
    assert!(report.is_empty(), "values differ:\n{report}");
}
