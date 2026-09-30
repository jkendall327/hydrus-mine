//! Searching by URL class against the reference's
//! (`oracle/fixtures/url_class_search.json`, made by
//! `oracle/record_url_class_search.py`): the `basic` fixture, given the same
//! URL classes and the same URLs, must classify each URL the same, answer
//! every search with the same files (or the same error), and test files in
//! memory the same.

use std::collections::{BTreeMap, BTreeSet};

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt as _;
use serde_json::{Value as Json, json};
use tower::ServiceExt as _;

use hydrus_core::Sha256;
use hydrus_core::url::UrlClasses;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_search::Clock;
use hydrus_search::media;

mod common;

fn access_key() -> String {
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    manifest["access_keys"]["full"].as_str().unwrap().to_owned()
}

async fn call(router: &axum::Router, request: Request<Body>) -> (u16, Json) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Json::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

#[tokio::test]
async fn url_class_searches_match_the_reference() {
    let recorded = hydrus_testkit::fixture_json("url_class_search.json");
    let fixture = common::imported_store("basic");
    let store = fixture.state.store.clone();
    let domain_manager =
        SerialisableObject::from_tuple_str(&recorded["domain_manager"].to_string()).unwrap();
    let settings = hydrus_legacy::objects::domain::url_class_settings(&domain_manager).unwrap();
    let mut report = String::new();

    // each URL's classes
    let classes = UrlClasses::new(settings.clone());
    for (url, expected) in recorded["url_matches"].as_object().unwrap() {
        let matches: Vec<&str> = settings
            .url_classes
            .iter()
            .filter(|c| c.matches(url, settings.collapse_leading_slashes))
            .map(|c| c.name.as_str())
            .collect();
        let classified = classes.class_for(url).map(|c| c.name.as_str());
        if json!(matches) != expected["matches"] || json!(classified) != expected["classified_as"] {
            report.push_str(&format!(
                "{url}: ours matches {matches:?} and is {classified:?}, theirs {expected}\n"
            ));
        }
    }

    store
        .write_and_refresh(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
        .unwrap();
    let router = hydrus_api::router(fixture.state.clone());
    for (hash, urls) in recorded["urls"].as_object().unwrap() {
        let body = json!({"hash": hash, "urls_to_add": urls, "normalise_urls": false});
        let request = Request::post("/add_urls/associate_url")
            .header("Hydrus-Client-API-Access-Key", access_key())
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let (status, response) = call(&router, request).await;
        assert_eq!(status, 200, "{response}");
    }

    for case in recorded["searches"].as_array().unwrap() {
        let tags = case["tags"].to_string();
        let query: String = form_urlencoded::Serializer::new(String::new())
            .append_pair("tags", &tags)
            .append_pair("return_hashes", "true")
            .finish();
        let request = Request::get(format!("/get_files/search_files?{query}"))
            .header("Hydrus-Client-API-Access-Key", access_key())
            .body(Body::empty())
            .unwrap();
        let (status, response) = call(&router, request).await;
        let ours = if status == 200 {
            let mut hashes: Vec<Json> = response["hashes"].as_array().unwrap().clone();
            hashes.sort_by_key(ToString::to_string);
            json!({"status": status, "hashes": hashes})
        } else {
            json!({"status": status, "exception_type": response["exception_type"]})
        };
        let mut theirs = case.clone();
        let theirs = theirs.as_object_mut().unwrap();
        theirs.remove("tags");
        theirs.remove("error");
        if ours != Json::Object(theirs.clone()) {
            report.push_str(&format!(
                "{tags}: ours {ours} {response}\n  theirs {theirs:?}\n"
            ));
        }
    }

    // in memory, over every file of the fixture
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let hashes: Vec<Sha256> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["hash"].as_str().unwrap().parse().unwrap())
        .collect();
    let snapshot = store.snapshot();
    let facts: BTreeMap<String, media::FileFacts> = store
        .read(|conn| {
            let ids = hydrus_store::master::hash_ids(conn, &hashes)?;
            let by_id: Vec<_> = hashes.iter().map(|h| ids[h]).collect();
            let facts = media::load_facts(conn, &snapshot, &by_id)?;
            Ok(hashes
                .iter()
                .filter_map(|h| Some((h.to_hex(), facts.get(&ids[h])?.clone())))
                .collect())
        })
        .unwrap();
    let clock = Clock::system();
    for case in recorded["in_memory"].as_array().unwrap() {
        let Some(expected) = case["matching"].as_array() else {
            continue;
        };
        let text = case["text"].as_str().unwrap();
        let [predicate] = &hydrus_search::parse_api_search(&json!([text])).unwrap()[..] else {
            panic!("{text} is one predicate");
        };
        let ours: BTreeSet<&str> = facts
            .iter()
            .filter(|(_, f)| media::test(predicate, f, &clock))
            .map(|(h, _)| h.as_str())
            .collect();
        let expected: BTreeSet<&str> = expected.iter().map(|h| h.as_str().unwrap()).collect();
        if ours != expected {
            report.push_str(&format!(
                "in memory, {text}: we also match {:?}, and miss {:?}\n",
                ours.difference(&expected).collect::<Vec<_>>(),
                expected.difference(&ours).collect::<Vec<_>>()
            ));
        }
    }
    assert!(report.is_empty(), "{report}");
}
