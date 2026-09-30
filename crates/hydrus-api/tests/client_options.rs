//! `/manage_database/get_client_options` on a store that wasn't migrated
//! reports a new reference client's options. (Migrated options are checked
//! against the reference by the `client_options` conformance scenario.)

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::Value as Json;
use tower::ServiceExt as _;

use hydrus_api::auth::{AccessPermissions, Permission};

#[tokio::test]
async fn a_new_store_reports_a_new_clients_options() {
    let dir = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(dir.path()).unwrap();
    let state = hydrus_api::AppState::new(store).unwrap();
    let key = vec![7u8; 32];
    state.access.insert(AccessPermissions {
        access_key: key.clone(),
        name: "test".into(),
        permits_everything: false,
        basic: [Permission::ManageDatabase].into_iter().collect(),
        search_filter: hydrus_core::TagFilter::default(),
    });
    let router = hydrus_api::router(state);
    let request = Request::get("/manage_database/get_client_options")
        .header("Hydrus-Client-API-Access-Key", hex::encode(&key))
        .body(Body::empty())
        .unwrap();
    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let body: Json =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(body["options"]["booleans"].as_object().unwrap().len() > 200);
    assert_eq!(body["options"]["default_sort"]["sort_metatype"], "system");
    assert_eq!(
        body["old_options"]["thumbnail_dimensions"],
        serde_json::json!([150, 125])
    );
    assert_eq!(
        body["options"]["default_local_location_context"]["current_service_keys"],
        serde_json::json!([hex::encode("local files")])
    );
}
