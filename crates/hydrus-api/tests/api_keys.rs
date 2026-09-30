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
