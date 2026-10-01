//! The archived-file delete lock against the reference's database.
//!
//! `oracle/dump_delete_lock.py` turned the lock on and made Client API
//! deletes from each file domain, duplicate decisions that delete a file
//! (with and without the default merge, and with and without re-inboxing),
//! and emptied the trash; after each step it recorded the answer and each
//! file's inbox state and local domains, from its database. Here the same
//! steps must give the same answers and leave our store the same.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::{Value as Json, json};
use tower::ServiceExt as _;

use hydrus_core::Sha256;
use hydrus_store::delete_lock::DeleteLock;
use hydrus_store::trash::{self, TrashSettings};
use hydrus_store::{master, media, settings};

mod common;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_owned()
}

async fn post(router: &axum::Router, key: &str, path: &str, body: &Json) -> (u16, Json) {
    let request = Request::post(path)
        .header("Hydrus-Client-API-Access-Key", key)
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Json::Null))
}

/// What the oracle records for a file, read from our store.
fn state_of(fixture: &common::Fixture, hash: &Sha256) -> Json {
    let store = &fixture.state.store;
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let m = store
        .read(|conn| {
            let id = master::hash_ids(conn, &[*hash])?[hash];
            Ok(media::load(conn, services, None, &[id])?.results.remove(0))
        })
        .unwrap();
    let domains: Vec<&str> = ["art", "hydrus local file storage", "my files", "trash"]
        .into_iter()
        .filter(|name| m.is_current_in(services.by_name(name).unwrap().id))
        .collect();
    json!({ "inbox": m.inbox, "domains": domains })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_delete_lock_matches_the_reference() {
    let root = repo_root();
    let recorded: Json = serde_json::from_str(
        &std::fs::read_to_string(root.join("oracle/fixtures/delete_lock.json")).unwrap(),
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
        let lock: DeleteLock = serde_json::from_value(phase["settings"].clone()).unwrap();
        fixture
            .state
            .store
            .write(move |ctx| settings::set(ctx.conn(), &lock))
            .unwrap();
        let router = hydrus_api::router(Arc::clone(&fixture.state));
        for step in phase["seed_steps"].as_array().unwrap() {
            let (status, body) =
                post(&router, key, step["path"].as_str().unwrap(), &step["json"]).await;
            assert_eq!(status, 200, "{name}: {step} {body}");
        }
        let pool: Vec<&str> = phase["pool"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_str().unwrap())
            .collect();
        let check = |when: &str, expected: &Json| {
            let problems: Vec<String> = pool
                .iter()
                .filter_map(|file| {
                    let want = &expected[*file];
                    let got = state_of(&fixture, &hash_of(file));
                    (*want != got).then(|| format!("{file}: expected {want}, got {got}"))
                })
                .collect();
            assert!(
                problems.is_empty(),
                "{name} phase, {when}:\n{}",
                problems.join("\n")
            );
        };
        check("after seeding", &phase["initial"]);

        for (index, step) in phase["steps"].as_array().unwrap().iter().enumerate() {
            if step["empty_trash"] == true {
                // (the age limit counts whole seconds)
                std::thread::sleep(std::time::Duration::from_millis(1100));
                let store = &fixture.state.store;
                let limits = TrashSettings {
                    max_age_hours: Some(0),
                    max_size_mb: None,
                };
                store
                    .write(move |ctx| settings::set(ctx.conn(), &limits))
                    .unwrap();
                trash::maintain_trash(store, 256).unwrap();
                check(&format!("step {index}, emptying the trash"), &step["state"]);
                continue;
            }
            let path = step["path"].as_str().unwrap();
            let (status, body) = post(&router, key, path, &step["json"]).await;
            let when = format!("step {index}, {path} {}", step["json"]);
            assert_eq!(
                u64::from(status),
                step["status"].as_u64().unwrap(),
                "{name} phase, {when}: {body}"
            );
            if status != 200 {
                assert_eq!(body["exception_type"], step["exception_type"], "{when}");
                assert_eq!(body["error"], step["error"], "{when}");
            }
            check(&when, &step["state"]);
        }
    }
}
