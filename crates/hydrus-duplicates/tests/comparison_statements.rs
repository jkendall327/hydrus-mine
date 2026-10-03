//! The duplicate filter's comparison statements against the reference's
//! (`oracle/fixtures/comparison_statements.json`, made by
//! `oracle/dump_comparison_statements.py`): for every recorded pair, the
//! same fast statements in the same order, the same total score and pixel
//! duplicate verdict, and the same slow statements (jpeg quality, visual
//! duplicates). The `basic` fixture database is migrated; the families of
//! `oracle/fixtures/auto_resolution/` are imported into a new store, with
//! the import times the reference was given.

use std::collections::HashMap;
use std::sync::Arc;

use hydrus_core::{HashId, Sha256};
use hydrus_duplicates::content::StoreContent;
use hydrus_duplicates::statements::{self, Statement};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_search::media::{self, FileFacts};
use hydrus_store::Store;
use hydrus_store::duplicates::ComparisonScores;
use serde_json::Value;

fn facts(store: &Store, hashes: &[Sha256]) -> HashMap<String, (HashId, FileFacts)> {
    let snapshot = store.snapshot();
    store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, hashes)?;
            let by_id: Vec<HashId> = hashes.iter().map(|h| ids[h]).collect();
            let facts = media::load_facts(conn, &snapshot, &by_id)?;
            Ok(hashes
                .iter()
                .map(|h| (h.to_hex(), (ids[h], facts[&ids[h]].clone())))
                .collect())
        })
        .unwrap()
}

fn recorded_statements(list: &Value) -> Vec<(String, String, i64)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s[0].as_str().unwrap().to_owned(),
                s[1].as_str().unwrap().to_owned(),
                s[2].as_i64().unwrap(),
            )
        })
        .collect()
}

fn ours(list: &[Statement]) -> Vec<(String, String, i64)> {
    list.iter()
        .map(|s| (s.key.to_owned(), s.text.clone(), i64::from(s.score)))
        .collect()
}

/// Every recorded pair of `set` compared; the differences, one per line.
fn compare(
    store: &Store,
    set: &Value,
    files: &HashMap<String, (HashId, FileFacts)>,
    scores: &ComparisonScores,
) -> String {
    let now = set["now"].as_i64().unwrap();
    let mut content = StoreContent::new(store);
    let mut report = String::new();
    let pairs = set["pairs"].as_array().unwrap();
    for pair in pairs {
        let (shown, other) = (
            &files[pair["shown"].as_str().unwrap()],
            &files[pair["other"].as_str().unwrap()],
        );
        let label = format!(
            "{} / {}",
            &pair["shown"].as_str().unwrap()[..8],
            &pair["other"].as_str().unwrap()[..8]
        );
        let fast = statements::fast(&shown.1, &other.1, scores, now);
        let expected = recorded_statements(&pair["fast"]);
        if ours(&fast.statements) != expected {
            report.push_str(&format!(
                "{label}: fast\n  ours {:?}\n  theirs {expected:?}\n",
                ours(&fast.statements)
            ));
        }
        if i64::from(fast.score()) != pair["score"].as_i64().unwrap() {
            report.push_str(&format!(
                "{label}: score {} vs {}\n",
                fast.score(),
                pair["score"]
            ));
        }
        if fast.pixel_duplicates != pair["pixel_duplicates"].as_bool().unwrap() {
            report.push_str(&format!("{label}: pixel duplicates differ\n"));
        }
        let slow = statements::slow(
            (shown.0, &shown.1),
            (other.0, &other.1),
            fast.pixel_duplicates,
            scores,
            &mut content,
        );
        let expected = recorded_statements(&pair["slow"]);
        if ours(&slow) != expected {
            report.push_str(&format!(
                "{label}: slow\n  ours {:?}\n  theirs {expected:?}\n",
                ours(&slow)
            ));
        }
    }
    println!("{} pairs compared", pairs.len());
    report
}

fn scores(recorded: &Value) -> ComparisonScores {
    let mut scores = ComparisonScores::default();
    for (name, score) in scores.by_option_name() {
        *score = i32::try_from(recorded["scores"][name].as_i64().unwrap()).unwrap();
    }
    assert_eq!(
        scores,
        ComparisonScores::default(),
        "the reference's defaults"
    );
    scores
}

#[test]
fn statements_about_the_basic_files_match() {
    let recorded = hydrus_testkit::fixture_json("comparison_statements.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let work = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &work.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(work.path()).unwrap();
    let set = &recorded["basic"];
    let hashes: Vec<Sha256> = set["imported_ms"]
        .as_object()
        .unwrap()
        .keys()
        .map(|h| h.parse().unwrap())
        .collect();
    let files = facts(&store, &hashes);
    for (hash, imported) in set["imported_ms"].as_object().unwrap() {
        assert_eq!(files[hash].1.imported_ms, imported.as_i64(), "{hash}");
    }
    let report = compare(&store, set, &files, &scores(&recorded));
    assert!(report.is_empty(), "{report}");

    // pairs are shown in the order the scores give
    let id = |hash: &Value| files[hash.as_str().unwrap()].0;
    let (mut pairs, mut expected) = (Vec::new(), Vec::new());
    for pair in set["pairs"].as_array().unwrap() {
        let (shown, other) = (id(&pair["shown"]), id(&pair["other"]));
        pairs.push((shown, other));
        expected.push(if pair["score"].as_i64().unwrap() > 0 {
            (shown, other)
        } else {
            (other, shown)
        });
    }
    let snapshot = store.snapshot();
    let ordered = store
        .read(|conn| statements::ab_order(conn, &snapshot, pairs, &scores(&recorded)))
        .unwrap();
    assert_eq!(ordered, expected);
}

#[test]
fn statements_about_the_duplicate_families_match() {
    let recorded = hydrus_testkit::fixture_json("comparison_statements.json");
    let set = &recorded["families"];
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let mut hashes = Vec::new();
    for file in set["files"].as_array().unwrap() {
        let name = file[0].as_str().unwrap();
        let result = importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("auto_resolution/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap();
        let hash = result.hash.unwrap();
        assert_eq!(hash.to_hex(), file[1].as_str().unwrap(), "{name}");
        hashes.push(hash);
    }
    let mut files = facts(&store, &hashes);
    // as the reference was given them
    for (hash, imported) in set["imported_ms"].as_object().unwrap() {
        files.get_mut(hash).unwrap().1.imported_ms = imported.as_i64();
    }
    let report = compare(&store, set, &files, &scores(&recorded));
    assert!(report.is_empty(), "{report}");
}
