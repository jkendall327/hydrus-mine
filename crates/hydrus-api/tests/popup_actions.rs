//! Callable API auth, current labels and owner-qualified live producer dispatch.
use axum::{body::Body, http::Request};
use http_body_util::BodyExt as _;
use hydrus_api::{
    AppState,
    auth::{self, AccessPermissions, Permission},
};
use hydrus_store::{popup_actions, popups};
use serde_json::{Value, json};
use tower::ServiceExt as _;

async fn call(
    router: &axum::Router,
    key: u8,
    method: &str,
    path: &str,
    body: Value,
) -> (u16, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("Hydrus-Client-API-Access-Key", hex::encode([key; 32]))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn callable_dispatch_requires_permission_and_a_current_producer_and_reports_its_label() {
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    for (key, permitted) in [(71, true), (72, false)] {
        let permissions = AccessPermissions {
            access_key: vec![key; 32],
            name: "popup test".into(),
            permits_everything: false,
            basic: if permitted {
                [Permission::ManagePopups].into()
            } else {
                Default::default()
            },
            search_filter: hydrus_core::TagFilter::default(),
        };
        store
            .write(move |ctx| auth::save_key(ctx.conn(), &permissions))
            .unwrap();
    }
    let router = hydrus_api::router(AppState::new(store.clone()).unwrap());
    let mut job = popups::Job::text("finished callable", 0.0);
    let key = job.key;
    let owner = [4; 32];
    job.action_owner = Some(owner);
    job.user_callable_label = Some("actual label".into());
    store
        .write(move |ctx| {
            popup_actions::begin(ctx.conn(), &key, &owner)?;
            popups::add(ctx.conn(), &job, 0)
        })
        .unwrap();
    let body = json!({"job_status_key": hex::encode(key)});
    assert_eq!(
        call(
            &router,
            72,
            "POST",
            "/manage_popups/call_user_callable",
            body.clone()
        )
        .await
        .0,
        403
    );
    assert!(
        store
            .write(move |ctx| popup_actions::take_owned(ctx.conn(), &key, &owner))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        call(&router, 71, "GET", "/manage_popups/get_popups", Value::Null)
            .await
            .1["job_statuses"][0]["user_callable_label"],
        "actual label"
    );
    assert_eq!(
        call(
            &router,
            71,
            "POST",
            "/manage_popups/call_user_callable",
            body.clone()
        )
        .await
        .0,
        200
    );
    let pending = store
        .write(move |ctx| popup_actions::take_owned(ctx.conn(), &key, &owner))
        .unwrap();
    assert_eq!(
        pending,
        [popup_actions::Pending {
            request: popup_actions::Request::Call,
            gui_owner: None
        }]
    );
    store
        .write(move |ctx| popup_actions::retire(ctx.conn(), &key, &owner))
        .unwrap();
    assert_eq!(
        call(
            &router,
            71,
            "POST",
            "/manage_popups/call_user_callable",
            body
        )
        .await
        .0,
        400
    );
    assert!(
        call(&router, 71, "GET", "/manage_popups/get_popups", Value::Null)
            .await
            .1["job_statuses"][0]
            .get("user_callable_label")
            .is_none()
    );
}
