//! Real authenticated routes replay Qt messages/no-ops and durable finished jobs.
use axum::{body::Body, http::Request};
use http_body_util::BodyExt as _;
use hydrus_api::{
    AppState,
    auth::{self, AccessPermissions, Permission},
};
use hydrus_store::{
    api_update_toasts::{self, Preferences},
    popups, settings,
};
use tower::ServiceExt as _;

#[tokio::test]
async fn real_cookie_header_requests_replay_qt_jobs_noops_errors_and_backend_values() {
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    for (key, allowed) in [(71, true), (72, false)] {
        let permissions = AccessPermissions {
            access_key: vec![key; 32],
            name: "toast replay".into(),
            permits_everything: false,
            basic: if allowed {
                [Permission::ManageHeaders].into()
            } else {
                Default::default()
            },
            search_filter: Default::default(),
        };
        store
            .write(move |ctx| auth::save_key(ctx.conn(), &permissions))
            .unwrap();
    }
    let router = hydrus_api::router(AppState::new(store.clone()).unwrap());
    let fixture = hydrus_testkit::fixture_json("api_update_toasts.json");
    assert_eq!(fixture["requests"].as_array().unwrap().len(), 20);
    for case in fixture["requests"].as_array().unwrap() {
        let enabled = case["enabled"].as_bool().unwrap();
        store
            .write(move |ctx| {
                ctx.conn().execute("DELETE FROM popups", [])?;
                settings::set(ctx.conn(), &Preferences { enabled })
            })
            .unwrap();
        let path = if case["worker"] == "cookies" {
            "/manage_cookies/set_cookies"
        } else {
            "/manage_headers/set_headers"
        };
        let key = u8::try_from(case["key"].as_u64().unwrap()).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri(path)
            .header("Hydrus-Client-API-Access-Key", hex::encode([key; 32]))
            .header("Content-Type", "application/json")
            .body(Body::from(case["body"].to_string()))
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(
            u64::from(response.status().as_u16()),
            case["status"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        if case["status"] != 200 {
            assert!(!body.is_empty());
        }
        let now = hydrus_core::TimestampMs::now().0 / 1000;
        let jobs = store.read(move |conn| popups::all(conn, now)).unwrap();
        let expected = case["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), expected.len(), "{}", case["name"]);
        for (job, recorded) in jobs.iter().zip(expected) {
            assert_eq!(job.status_text_1.as_deref(), recorded["text"].as_str());
            assert!(job.done && !job.pausable && !job.cancellable);
            let start = job.creation_time as i64;
            assert_eq!(job.dismiss_at, Some(start + 5));
            for boundary in fixture["dismissal"].as_array().unwrap() {
                assert_eq!(
                    job.is_dismissed(start + boundary["seconds"].as_i64().unwrap()),
                    boundary["dismissed"].as_bool().unwrap()
                );
            }
        }
        // The same-value route must preserve actual approval/reason as well
        // as suppressing its toast; observing only the message is insufficient.
        if case["name"] == "same_header_noop" {
            let headers = store
                .read(|conn| {
                    hydrus_store::network::headers(
                        conn,
                        &hydrus_store::network::NetworkContext::domain("a.toast.example.invalid"),
                    )
                })
                .unwrap();
            let header = headers.iter().find(|h| h.name == "A-New").unwrap();
            assert_eq!(header.value, "a");
            assert_ne!(header.approval, hydrus_store::network::Approval::Denied);
            assert_ne!(header.reason, "same value");
        }
    }
    assert!(store.read(api_update_toasts::load).unwrap().enabled);
    // A request rejected for permission never creates either a header or toast.
    let headers = store
        .read(|conn| {
            hydrus_store::network::headers(
                conn,
                &hydrus_store::network::NetworkContext::domain("a.toast.example.invalid"),
            )
        })
        .unwrap();
    assert!(!headers.iter().any(|h| h.name == "blocked"));
    assert!(
        headers
            .iter()
            .any(|h| h.name == "A-Before-Error" && h.value == "persisted"),
        "late error preserves accepted preceding header but publishes no success toast"
    );
    assert!(!headers.iter().any(|h| h.name == "Z-Missing-Entry"));
}
