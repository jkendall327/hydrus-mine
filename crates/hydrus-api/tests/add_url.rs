//! `/add_urls/add_url`: URLs land in the right URL queue with the tags they
//! were given; bad URLs are refused as the reference refuses them.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::{Value as Json, json};
use tower::ServiceExt as _;

use hydrus_store::queues::{self, QueueKind};

mod common;

fn access_key() -> String {
    let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Json = serde_json::from_str(
        &std::fs::read_to_string(root.join("oracle/fixtures/legacy_db/basic.manifest.json"))
            .unwrap(),
    )
    .unwrap();
    manifest["access_keys"]["full"].as_str().unwrap().to_owned()
}

async fn add_url(router: &axum::Router, body: &Json) -> (u16, Json) {
    let request = Request::post("/add_urls/add_url")
        .header("Hydrus-Client-API-Access-Key", access_key())
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json =
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes)));
    (status, json)
}

/// (The downloader is not started here, so nothing is fetched.)
#[tokio::test]
async fn urls_go_to_named_queues_with_their_tags() {
    let fixture = common::imported_store("basic");
    let router = hydrus_api::router(fixture.state.clone());
    let downloader_tags = hex::encode(hydrus_core::service::builtin_keys::DOWNLOADER_TAGS);

    let (status, body) = add_url(
        &router,
        &json!({"url": "https://example.com/some file.jpg"}),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({
            "human_result_text": "\"unknown url\" URL added successfully.",
            "normalised_url": "https://example.com/some%20file.jpg",
            "version": hydrus_core::CLIENT_API_VERSION,
            "hydrus_version": hydrus_core::REFERENCE_VERSION,
        })
    );
    let (status, _) = add_url(
        &router,
        &json!({
            "url": "https://example.com/other.png",
            "destination_page_name": "companion",
            "filterable_tags": ["Blue Eyes", " solo "],
            "service_keys_to_additional_tags": {downloader_tags.clone(): ["from companion"]},
        }),
    )
    .await;
    assert_eq!(status, 200);

    let store = &fixture.state.store;
    let all = store
        .read(|conn| queues::queues(conn, Some(QueueKind::Urls)))
        .unwrap();
    let names: Vec<&str> = all.iter().map(|q| q.name.as_str()).collect();
    assert_eq!(names, ["url import", "companion"]);
    let seeds = store
        .read(|conn| queues::file_seeds(conn, all[1].id))
        .unwrap();
    assert_eq!(seeds.len(), 1);
    let meta = &seeds[0].meta;
    assert_eq!(
        meta.external_filterable_tags,
        ["blue eyes", "solo"].map(str::to_owned).into()
    );
    assert_eq!(
        meta.external_additional_tags,
        vec![(downloader_tags, ["from companion".to_owned()].into())]
    );

    // the same page again: the named queue is reused
    let (status, _) = add_url(
        &router,
        &json!({"url": "https://example.com/third.png", "destination_page_name": "companion"}),
    )
    .await;
    assert_eq!(status, 200);
    let seeds = store
        .read(|conn| queues::file_seeds(conn, all[1].id))
        .unwrap();
    assert_eq!(seeds.len(), 2);

    for (bad, why) in [
        (json!({"url": ""}), "Given URL was empty!"),
        (
            json!({"url": "not a url"}),
            "Could not parse \"not a url\" at all!",
        ),
    ] {
        let (status, body) = add_url(&router, &bad).await;
        assert_eq!(status, 400, "{bad}");
        assert_eq!(body["error"], why, "{body}");
    }
}
