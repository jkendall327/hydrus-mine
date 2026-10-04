//! Asking for an access key (`/request_new_permissions`): refused unless
//! `hydrus api-keys listen` has opened registration; the request waits for
//! the user, and a key accepted by another process works at once.

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::Value as Json;
use tower::ServiceExt as _;

use hydrus_api::auth::{self, AccessPermissions, Permission, Registration};

async fn get(router: &axum::Router, uri: &str, key: Option<&str>) -> (u16, Json) {
    let mut request = Request::get(uri);
    if let Some(key) = key {
        request = request.header("Hydrus-Client-API-Access-Key", key);
    }
    let response = router
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Json::Null))
}

#[tokio::test]
async fn a_requested_key_works_once_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let state = hydrus_api::AppState::new(store.clone()).unwrap();
    let router = hydrus_api::router(state);
    let ask = "/request_new_permissions?name=a%20tool&basic_permissions=%5B0%2C3%5D";

    let (status, _) = get(&router, ask, None).await;
    assert_eq!(status, 409);

    // `hydrus api-keys listen`
    let until = hydrus_core::time::TimestampMs::now().millis() + 60_000;
    store
        .write(move |ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &Registration {
                    open_until_ms: Some(until),
                    requests: Vec::new(),
                },
            )
        })
        .unwrap();
    let (status, body) = get(&router, ask, None).await;
    assert_eq!(status, 200, "{body}");
    let key = body["access_key"].as_str().unwrap().to_owned();
    let (status, _) = get(&router, "/verify_access_key", Some(&key)).await;
    assert_eq!(status, 403, "not accepted yet");

    let registration: Registration = store.read(hydrus_store::settings::get).unwrap();
    let [(requested, name, everything, basic)] = &registration.requests[..] else {
        panic!("{registration:?}");
    };
    assert_eq!(
        (requested, name.as_str(), *everything),
        (&key, "a tool", false)
    );
    assert_eq!(basic, &[Permission::AddUrls, Permission::SearchFiles]);

    // accepted by the other process
    let accepted = AccessPermissions {
        access_key: hex::decode(&key).unwrap(),
        name: name.clone(),
        permits_everything: false,
        basic: basic.iter().copied().collect(),
        search_filter: hydrus_core::TagFilter::default(),
    };
    store
        .write(move |ctx| auth::save_key(ctx.conn(), &accepted))
        .unwrap();
    let (status, body) = get(&router, "/verify_access_key", Some(&key)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["name"], "a tool");
    assert_eq!(body["basic_permissions"], serde_json::json!([0, 3]));
}

#[tokio::test]
async fn edits_and_revocation_apply_to_existing_keys_and_sessions_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let mut permissions = AccessPermissions {
        access_key: vec![42; 32],
        name: "tool".into(),
        permits_everything: true,
        basic: std::collections::BTreeSet::default(),
        search_filter: hydrus_core::TagFilter::default(),
    };
    let saved = permissions.clone();
    store
        .write(move |ctx| auth::save_key(ctx.conn(), &saved))
        .unwrap();
    let state = hydrus_api::AppState::new(store.clone()).unwrap();
    let router = hydrus_api::router(state.clone());
    let key = hex::encode(&permissions.access_key);
    assert_eq!(
        get(&router, "/manage_pages/get_pages", Some(&key)).await.0,
        200
    );
    let session = state.access.new_session(&permissions.access_key);
    permissions.permits_everything = false;
    permissions.basic.insert(Permission::SearchFiles);
    permissions.search_filter = restricted_filter("safe");
    let old = permissions.clone();
    let saved = old.clone();
    store
        .write(move |ctx| auth::save_key(ctx.conn(), &saved))
        .unwrap();
    assert_eq!(get(&router, "/verify_access_key", Some(&key)).await.0, 200);
    state.access.record_search(&old, &[hydrus_core::HashId(1)]);
    assert!(
        state
            .access
            .check_can_see(&old, &[hydrus_core::HashId(1)])
            .is_ok()
    );
    permissions.search_filter = restricted_filter("other");
    let stale = AccessPermissions {
        search_filter: restricted_filter("safe"),
        ..permissions.clone()
    };
    let saved = permissions.clone();
    store
        .write(move |ctx| auth::save_key(ctx.conn(), &saved))
        .unwrap();
    assert_eq!(
        get(&router, "/manage_pages/get_pages", Some(&key)).await.0,
        403
    );
    state
        .access
        .record_search(&stale, &[hydrus_core::HashId(1)]);
    assert!(
        state
            .access
            .check_can_see(&permissions, &[hydrus_core::HashId(1)])
            .is_err(),
        "old search results invalidated"
    );
    let response = router
        .clone()
        .oneshot(
            Request::get("/manage_pages/get_pages")
                .header(auth::SESSION_KEY_HEADER, hex::encode(&session))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 403, "session obeys narrowed permissions");
    let deleted = permissions.access_key.clone();
    store
        .write(move |ctx| auth::delete_key(ctx.conn(), &deleted).map(|_| ()))
        .unwrap();
    assert_eq!(get(&router, "/verify_access_key", Some(&key)).await.0, 403);
    let response = router
        .oneshot(
            Request::get("/verify_access_key")
                .header(auth::SESSION_KEY_HEADER, hex::encode(&session))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 419, "revoked session removed");
}
fn restricted_filter(tag: &str) -> hydrus_core::TagFilter {
    use hydrus_core::tag_filter::FilterRule::{Blacklist, Whitelist};
    hydrus_core::TagFilter::new()
        .with_rule("", Blacklist)
        .with_rule(":", Blacklist)
        .with_rule(tag, Whitelist)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn permission_revision_check_does_not_block_database_unlock() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let permissions = AccessPermissions {
        access_key: vec![77; 32],
        name: "lock admin".into(),
        permits_everything: true,
        basic: std::collections::BTreeSet::default(),
        search_filter: hydrus_core::TagFilter::default(),
    };
    store
        .write(move |ctx| auth::save_key(ctx.conn(), &permissions))
        .unwrap();
    let state = hydrus_api::AppState::new(store).unwrap();
    let router = hydrus_api::router(state);
    let key = hex::encode([77; 32]);
    for endpoint in ["/manage_database/lock_on", "/manage_database/lock_off"] {
        let request = Request::post(endpoint)
            .header(auth::ACCESS_KEY_HEADER, &key)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            router.clone().oneshot(request),
        )
        .await
        .expect("database unlock must not wait on the paused read pool")
        .unwrap();
        assert_eq!(response.status(), 200);
    }
    assert_eq!(get(&router, "/verify_access_key", Some(&key)).await.0, 200);
}
