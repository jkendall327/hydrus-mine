//! Rating services' SVG icons against the reference's
//! (`oracle/fixtures/rating_svg.json`, made by `oracle/record_rating_svg.py`):
//! a bundled SVG, the user's own SVG from `static/star_shapes` in the
//! database directory, and an SVG that doesn't exist.

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::Value as Json;
use sha2::Digest as _;
use tower::ServiceExt as _;

use hydrus_core::ServiceKey;
use hydrus_store::services::{self, ServiceKind, StarAppearance};

mod common;

#[tokio::test]
async fn rating_svgs_are_served_as_the_reference_serves_them() {
    let recorded = hydrus_testkit::fixture_json("rating_svg.json");
    let fixture = common::imported_store("basic");
    let store = fixture.state.store.clone();
    let custom = store.dir().join("static").join("star_shapes");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(
        custom.join("mine.svg"),
        recorded["custom_svg"].as_str().unwrap(),
    )
    .unwrap();

    // the recorded services, pointed at their SVGs
    let svgs: Vec<(String, String, ServiceKey)> = recorded["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["path"] == "/get_service_rating_svg")
        .map(|r| {
            let name = r["name"].as_str().unwrap().to_owned();
            let svg = recorded["svgs"][&name].as_str().unwrap().to_owned();
            (
                name,
                svg,
                ServiceKey::new(hex::decode(r["service_key"].as_str().unwrap()).unwrap()),
            )
        })
        .collect();
    store
        .write_and_refresh(move |ctx| {
            let registry = services::ServiceRegistry::load(ctx.conn())?;
            for (name, svg, key) in svgs {
                let svg = StarAppearance::Svg(svg);
                if let Some(service) = registry.by_name(&name) {
                    let mut kind = service.kind.clone();
                    match &mut kind {
                        ServiceKind::RatingLike(c) => c.appearance = svg,
                        ServiceKind::RatingNumerical(c) => c.appearance = svg,
                        other => panic!("{name} is {other:?}"),
                    }
                    services::update_config(ctx.conn(), service.id, &kind)?;
                } else {
                    let Some(ServiceKind::RatingNumerical(mut config)) =
                        registry.by_name("stars").map(|s| s.kind.clone())
                    else {
                        panic!("no numerical rating service to copy");
                    };
                    config.appearance = svg;
                    services::insert(
                        ctx.conn(),
                        &key,
                        &name,
                        &ServiceKind::RatingNumerical(config),
                    )?;
                }
            }
            Ok(())
        })
        .unwrap();

    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let access_key = manifest["access_keys"]["full"].as_str().unwrap().to_owned();
    let router = hydrus_api::router(fixture.state.clone());
    let mut report = String::new();
    for case in recorded["requests"].as_array().unwrap() {
        let uri = format!(
            "{}?service_key={}",
            case["path"].as_str().unwrap(),
            case["service_key"].as_str().unwrap()
        );
        let request = Request::get(&uri)
            .header("Hydrus-Client-API-Access-Key", &access_key)
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let content_type = response.headers()["content-type"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let mut ours = serde_json::json!({"status": status, "content_type": content_type});
        let mut theirs =
            serde_json::json!({"status": case["status"], "content_type": case["content_type"]});
        if let Some(expected) = case.get("json") {
            let mut actual: Json = serde_json::from_slice(&bytes).unwrap();
            let mut expected = expected.clone();
            // errors compare by kind, as in the conformance runner
            for v in [&mut actual, &mut expected] {
                v.as_object_mut().unwrap().remove("error");
            }
            ours["json"] = actual;
            theirs["json"] = expected;
        } else {
            ours["sha256"] = hex::encode(sha2::Sha256::digest(&bytes)).into();
            theirs["sha256"] = case["sha256"].clone();
        }
        if ours != theirs {
            report.push_str(&format!("{uri}:\n  ours   {ours}\n  theirs {theirs}\n"));
        }
    }
    assert!(report.is_empty(), "{report}");
}
