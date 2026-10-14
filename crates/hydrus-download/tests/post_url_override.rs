//! "Force file downloads to occur quickly after Post URL fetches", met by the
//! importer that reads it: a URL queue fetches a post page, whose one file
//! then runs into a bandwidth rule that is used up. With the option on the
//! file's wait is overridden a few seconds after the post (the reference's
//! `network_job.OverrideBandwidth( 3 )` in
//! `ClientImportFileSeeds.WorkOnURL`); with it off the file keeps waiting for
//! the rule. The reference's own client did the same with a real network job
//! (`oracle/record_post_url_override.py` -> `fixtures/post_url_override.json`):
//! with the option on the file was asked for 3.43 seconds after the post page;
//! with it off, not within 15 seconds. The arrival here is bounded from below
//! by the recorded whole seconds and from above by the recorded watch.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use parking_lot::Mutex;

use hydrus_core::bandwidth::{BandwidthType, Rule, Rules};
use hydrus_core::url::strings::StringMatch;
use hydrus_core::url::{DomainMask, StringProcessor, UrlClass, UrlClassSettings, UrlType};
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_parse::Downloaders;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch};
use hydrus_store::Store;
use hydrus_store::bandwidth::BandwidthSettings;

#[derive(Default)]
struct Site {
    post: Mutex<Option<Instant>>,
    file: Mutex<Option<Instant>>,
}

async fn post(State(site): State<Arc<Site>>) -> Response {
    *site.post.lock() = Some(Instant::now());
    let html = r#"<html><body><img id="image" src="/files/1.jpg"></body></html>"#;
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn file(State(site): State<Arc<Site>>) -> Response {
    *site.file.lock() = Some(Instant::now());
    let bytes = std::fs::read(hydrus_testkit::fixture_path("media/jpeg_420.jpg")).unwrap();
    ([("content-type", "image/jpeg")], bytes).into_response()
}

async fn missing() -> StatusCode {
    StatusCode::NOT_FOUND
}

fn post_parser() -> PageParser {
    PageParser {
        reference_auxiliary: None,
        name: "post".into(),
        key: "ab".into(),
        converter: hydrus_core::url::StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: vec![ContentParser {
            name: "file".into(),
            kind: ContentKind::Url {
                url_type: 7,
                priority: 50,
            },
            formula: Formula {
                reference_auxiliary: None,
                name: String::new(),
                kind: FormulaKind::Html {
                    rules: vec![HtmlRule {
                        walk: HtmlWalk::Descendants(TagSearch {
                            attrs: [("id".to_owned(), "image".to_owned())].into(),
                            index: None,
                        }),
                        tag_name: Some("img".to_owned()),
                        text_match: None,
                    }],
                    content: HtmlContent::Attribute("src".into()),
                },
                processor: StringProcessor::default(),
            },
        }],
        example_urls: Vec::new(),
    }
}

struct Run {
    site: Arc<Site>,
    _dir: tempfile::TempDir,
    started: Instant,
}

/// Queue one post under a bandwidth rule allowing one request a day (the
/// post page uses it), with the option as given.
async fn run(override_on_file_urls_from_posts: bool) -> Run {
    let site = Arc::new(Site::default());
    let app = Router::new()
        .route("/post/{id}", get(post))
        .route("/files/{name}", get(file))
        .fallback(missing)
        .with_state(Arc::clone(&site));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let class = UrlClass {
        name: "local post".into(),
        key: vec![0xcd],
        url_type: UrlType::Post,
        preferred_scheme: "http".into(),
        domain_mask: DomainMask::new(vec![host.clone()], vec![], false, false),
        path_components: vec![
            (StringMatch::fixed("post"), None),
            (StringMatch::any(), None),
        ],
        ..UrlClass::default()
    };
    let settings = UrlClassSettings {
        parser_links: vec![(hex::encode(&class.key), Some("ab".into()))],
        parser_keys: vec!["ab".into()],
        url_classes: vec![class],
        collapse_leading_slashes: false,
    };
    let downloaders = Downloaders {
        parsers: vec![post_parser()],
        ..Downloaders::default()
    };
    let bandwidth = BandwidthSettings {
        rules: vec![(
            hydrus_store::network::NetworkContext::global(),
            Rules::new([Rule::new(BandwidthType::Requests, Some(86_400), 1)]),
        )],
        override_on_file_urls_from_posts,
        ..BandwidthSettings::default()
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &settings)?;
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &bandwidth)
        })
        .unwrap();
    let net = Arc::new(NetEngine::new(Arc::clone(&store), NetOptions::default()).unwrap());
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let downloader = Arc::new(Downloader::new(Arc::clone(&store), net, importer).unwrap());
    let runner = QueueRunner::new(downloader, 60);
    runner.start_all().unwrap();
    let queue = runner.url_queue_for(None, None, None).unwrap();
    let started = Instant::now();
    runner
        .pend_urls(
            queue.id,
            &[format!("http://{host}/post/1")],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    // (the runner lives as long as the test's store directory)
    std::mem::forget(runner);
    Run {
        site,
        _dir: dir,
        started,
    }
}

// leaf: audit-options-downloading-gallery-downloader-force-file-downloads-to-occur-quickly-after-post-url-fetches
#[tokio::test(flavor = "multi_thread")]
async fn with_the_option_on_a_posts_file_goes_a_few_seconds_after_the_post() {
    let r = run(true).await;
    let file_at = loop {
        if let Some(at) = *r.site.file.lock() {
            break at;
        }
        assert!(
            r.started.elapsed() < Duration::from_secs(30),
            "the file was never fetched"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let post_at = r.site.post.lock().expect("the post came first");
    let recorded = hydrus_testkit::fixture_json("post_url_override.json");
    let on = &recorded["on"];
    assert!(on["file_seen"].as_bool().unwrap());
    let gap = file_at.duration_since(post_at);
    let at_least = on["file_after_post_seconds"].as_f64().unwrap().floor();
    let within = on["watched_seconds"].as_f64().unwrap();
    assert!(gap.as_secs_f64() >= at_least, "{gap:?}");
    assert!(gap.as_secs_f64() < within, "{gap:?}");
}

// leaf: audit-options-downloading-gallery-downloader-force-file-downloads-to-occur-quickly-after-post-url-fetches
#[tokio::test(flavor = "multi_thread")]
async fn with_the_option_off_a_posts_file_keeps_waiting_for_bandwidth() {
    let r = run(false).await;
    // the post came; the file is still held after the time the recorded
    // reference watched with the option off
    let recorded = hydrus_testkit::fixture_json("post_url_override.json");
    let off = &recorded["off"];
    assert!(!off["file_seen"].as_bool().unwrap());
    tokio::time::sleep(Duration::from_secs_f64(
        off["watched_seconds"].as_f64().unwrap(),
    ))
    .await;
    assert!(r.site.post.lock().is_some(), "the post page was fetched");
    assert!(
        r.site.file.lock().is_none(),
        "the file went despite the rule"
    );
}
