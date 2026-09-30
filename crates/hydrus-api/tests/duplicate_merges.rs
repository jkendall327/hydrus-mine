//! Duplicate metadata merges against the reference's database.
//!
//! `oracle/dump_duplicate_merges.py` made random duplicate decisions on the
//! reference client and recorded, after each, what its database held for the
//! files involved: tags, ratings, notes, URLs, inbox, modified times and
//! local file domains. Here the same requests go through our Client API and
//! our store must hold the same after every decision. The reference's own
//! API answers aren't used: they come from an in-memory cache that drifts
//! from its database after merges.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::{Map, Value as Json, json};
use tower::ServiceExt as _;

use hydrus_core::{ContentStatus, Sha256};
use hydrus_store::duplicates::DuplicateMergeSettings;
use hydrus_store::media::Rating;
use hydrus_store::{master, media, settings};

mod common;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

async fn post(router: &axum::Router, key: &str, path: &str, body: &Json) {
    let request = Request::post(path)
        .header("Hydrus-Client-API-Access-Key", key)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert!(
        status.is_success(),
        "{path} {body}: {status} {}",
        String::from_utf8_lossy(&bytes)
    );
}

/// What the oracle records for a file, read from our store.
fn state_of(fixture: &common::Fixture, hash: &Sha256) -> Json {
    let store = &fixture.state.store;
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let m = store
        .read(|conn| {
            let id = master::hash_ids(conn, &[*hash])?[hash];
            let mut batch = media::load(conn, services, None, &[id])?;
            let tags = batch.tags;
            Ok((batch.results.remove(0), tags))
        })
        .unwrap();
    let (m, names) = m;
    let mut tags = Map::new();
    for service in ["my tags", "downloader tags", "second tags"] {
        let id = services.by_name(service).unwrap().id;
        let Some(by_status) = m.tags.get(&id) else {
            continue;
        };
        for (status, label) in [
            (ContentStatus::Current, "current"),
            (ContentStatus::Deleted, "deleted"),
            (ContentStatus::Pending, "pending"),
        ] {
            let mut list: Vec<String> = by_status
                .by_status
                .get(&status)
                .into_iter()
                .flatten()
                .map(|t| names[t].to_string())
                .collect();
            list.sort();
            if !list.is_empty() {
                tags.insert(format!("{service} {label}"), json!(list));
            }
        }
    }
    let mut ratings = Map::new();
    for service in ["favourites", "stars", "counter"] {
        let id = services.by_name(service).unwrap().id;
        match m.ratings.get(&id) {
            Some(Rating::Fraction(f)) => {
                ratings.insert(service.into(), json!(f));
            }
            Some(Rating::IncDec(n)) if *n != 0 => {
                ratings.insert(service.into(), json!(n));
            }
            _ => {}
        }
    }
    let notes: BTreeMap<&str, &str> = m
        .notes
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let mut urls = m.urls.clone();
    urls.sort();
    let domain_times: BTreeMap<&str, i64> = m
        .domain_modified
        .iter()
        .map(|(d, t)| (d.as_str(), t.0))
        .collect();
    let domains: Vec<&str> = ["art", "my files", "trash"]
        .into_iter()
        .filter(|name| m.is_current_in(services.by_name(name).unwrap().id))
        .collect();
    json!({
        "tags": tags,
        "ratings": ratings,
        "notes": notes,
        "urls": urls,
        "inbox": m.inbox,
        "file_modified_ms": m.info.as_ref().and_then(|i| i.file_modified).map(|t| t.0),
        "domain_modified_ms": domain_times,
        "domains": domains,
    })
}

/// Ratings are fractions; compare those with a tolerance.
fn same(expected: &Json, actual: &Json) -> bool {
    match (expected, actual) {
        (Json::Number(a), Json::Number(b)) if a.is_f64() || b.is_f64() => {
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-9
        }
        (Json::Object(a), Json::Object(b)) => {
            a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| same(v, w)))
        }
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(v, w)| same(v, w))
        }
        _ => expected == actual,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn merges_match_the_reference_database() {
    let root = repo_root();
    let recorded: Json = serde_json::from_str(
        &std::fs::read_to_string(root.join("oracle/fixtures/duplicate_merges.json")).unwrap(),
    )
    .unwrap();
    let manifest: Json = serde_json::from_str(
        &std::fs::read_to_string(root.join("oracle/fixtures/legacy_db/basic.manifest.json"))
            .unwrap(),
    )
    .unwrap();
    let key = manifest["access_keys"]["full"].as_str().unwrap();
    let hash_of = |name: &str| -> Sha256 {
        let hex = manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap()["hash"]
            .as_str()
            .unwrap();
        Sha256::from_slice(&hex::decode(hex).unwrap()).unwrap()
    };

    for phase in recorded["phases"].as_array().unwrap() {
        let name = phase["name"].as_str().unwrap();
        let fixture = common::imported_store("basic");
        if !phase["settings"].is_null() {
            let merge: DuplicateMergeSettings =
                serde_json::from_value(phase["settings"].clone()).unwrap();
            fixture
                .state
                .store
                .write_content(move |w| settings::set(w.conn(), &merge))
                .unwrap();
        }
        let router = hydrus_api::router(Arc::clone(&fixture.state));
        for step in phase["seed_steps"].as_array().unwrap() {
            post(&router, key, step["path"].as_str().unwrap(), &step["json"]).await;
        }
        let pool: Vec<&str> = phase["pool"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let mut expected: BTreeMap<&str, Json> = pool
            .iter()
            .map(|n| (*n, phase["initial"][*n].clone()))
            .collect();
        let check = |when: &str, expected: &BTreeMap<&str, Json>| {
            let mut problems = Vec::new();
            for (file, want) in expected {
                let got = state_of(&fixture, &hash_of(file));
                if !same(want, &got) {
                    problems.push(format!("{file}:\n  expected {want}\n  got      {got}"));
                }
            }
            assert!(
                problems.is_empty(),
                "{name} phase, {when}:\n{}",
                problems.join("\n")
            );
        };
        check("after seeding", &expected);
        for (index, step) in phase["steps"].as_array().unwrap().iter().enumerate() {
            post(
                &router,
                key,
                "/manage_file_relationships/set_file_relationships",
                &json!({ "relationships": [step["relationship"]] }),
            )
            .await;
            for (file, state) in step["changed"].as_object().unwrap() {
                *expected.get_mut(file.as_str()).unwrap() = state.clone();
            }
            check(
                &format!("after decision {index} {}", step["relationship"]),
                &expected,
            );
        }
    }
}
