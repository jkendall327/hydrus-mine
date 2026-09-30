//! The downloader end to end, against what the reference did on the same
//! site (`oracle/fixtures/downloads.json`, made by
//! `oracle/record_downloads.py`).
//!
//! The recorded site is served on the port its URL classes name, the
//! reference's domain manager (its URL classes, parsers and GUG) is
//! installed as a migration would, and the reference's steps are replayed
//! through our Client API and downloader: URLs sent to a URL page, a thread
//! watched until it 404s, a subscription synced twice. Every importer's
//! file and gallery log, and what every file got, must match.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use parking_lot::Mutex;
use serde_json::{Value as Json, json};
use tower::ServiceExt as _;

use hydrus_core::subscriptions::{CheckerDefaults, QueryState, SubscriptionSettings};
use hydrus_core::watchers::CheckerStatus;
use hydrus_download::queue::watcher_state;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_net::Job;
use hydrus_store::Store;
use hydrus_store::queues::{self, FileSeed, GallerySeed, Queue, QueueKind, SeedStatus};
use hydrus_store::subscriptions as subs;

mod common;

struct Site {
    phases: Vec<(HashMap<String, String>, HashMap<String, String>)>,
    phase: AtomicUsize,
    hits: Mutex<Vec<(usize, String)>>,
}

async fn serve(State(site): State<Arc<Site>>, uri: Uri) -> Response {
    let phase = site.phase.load(Ordering::SeqCst);
    let path = uri.path().to_owned();
    site.hits.lock().push((phase, path.clone()));
    let (pages, files) = &site.phases[phase];
    if let Some(html) = pages.get(&path) {
        return ([("content-type", "text/html; charset=utf-8")], html.clone()).into_response();
    }
    if let Some(name) = files.get(&path) {
        let bytes = std::fs::read(hydrus_testkit::fixture_path(format!("media/{name}"))).unwrap();
        return ([("content-type", "image/jpeg")], bytes).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        [("content-type", "text/html; charset=utf-8")],
        "not found",
    )
        .into_response()
}

fn string_map(value: &Json) -> HashMap<String, String> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect()
}

fn access_key() -> String {
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    manifest["access_keys"]["full"].as_str().unwrap().to_owned()
}

async fn add_url(router: &Router, body: &Json) {
    let request = Request::post("/add_urls/add_url")
        .header("Hydrus-Client-API-Access-Key", access_key())
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), 200, "{body}");
}

// ---------------------------------------------------------------- records

struct Normaliser {
    base: String,
    time: regex::Regex,
    ago: regex::Regex,
}

impl Normaliser {
    fn new(base: &str) -> Self {
        Self {
            base: base.to_owned(),
            time: regex::Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap(),
            ago: regex::Regex::new(r"which was .*? ago").unwrap(),
        }
    }

    fn url(&self, url: &str) -> String {
        url.replace(&self.base, "BASE")
    }

    fn note(&self, note: &str) -> String {
        let note = self.url(note);
        let note = self.time.replace_all(&note, "TIME");
        self.ago.replace_all(&note, "which was X ago").into_owned()
    }

    fn urls<'a>(&self, urls: impl IntoIterator<Item = &'a String>) -> Vec<String> {
        let mut out: Vec<String> = urls.into_iter().map(|u| self.url(u)).collect();
        out.sort();
        out
    }

    fn file_seed(&self, f: &FileSeed) -> Json {
        json!({
            "data": self.url(&f.data),
            "status": f.status.code(),
            "note": self.note(&f.note),
            "referral": f.referral_url.as_deref().map(|u| self.url(u)),
            "primary_urls": self.urls(&f.meta.primary_urls),
            "source_urls": self.urls(&f.meta.source_urls),
            "tags": f.meta.tags,
            "external_filterable_tags": f.meta.external_filterable_tags,
            "sha256": f.meta.hash("sha256"),
        })
    }

    fn gallery_seed(&self, g: &GallerySeed) -> Json {
        json!({
            "url": self.url(&g.url),
            "status": g.status.code(),
            "note": self.note(&g.note),
        })
    }

    fn log(&self, store: &Store, queue: i64) -> Json {
        let files = store.read(|conn| queues::file_seeds(conn, queue)).unwrap();
        let galleries = store
            .read(|conn| queues::gallery_seeds(conn, queue))
            .unwrap();
        json!({
            "files": files.iter().map(|f| self.file_seed(f)).collect::<Vec<_>>(),
            "galleries": galleries.iter().map(|g| self.gallery_seed(g)).collect::<Vec<_>>(),
        })
    }
}

fn file_records(store: &Store, normaliser: &Normaliser, hashes: &BTreeSet<String>) -> Json {
    use hydrus_core::service::builtin_keys::{DOWNLOADER_TAGS, MY_TAGS};

    let snapshot = store.snapshot();
    let mut out = serde_json::Map::new();
    for hash in hashes {
        let record = store
            .read(|conn| {
                let sha = hydrus_core::Sha256::from_slice(&hex::decode(hash).unwrap()).unwrap();
                let id = hydrus_store::master::hash_id(conn, &sha)?.unwrap();
                let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
                let result = &batch.results[0];
                let tags_of = |key: &[u8]| -> Vec<String> {
                    let service = snapshot.services.builtin(key).unwrap().id;
                    let mut tags: Vec<String> = result
                        .tags
                        .get(&service)
                        .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                        .map(|tags| {
                            tags.iter()
                                .map(|t| batch.tags[t].as_str().to_owned())
                                .collect()
                        })
                        .unwrap_or_default();
                    tags.sort();
                    tags
                };
                Ok(json!({
                    "downloader_tags": tags_of(DOWNLOADER_TAGS),
                    "my_tags": tags_of(MY_TAGS),
                    "urls": normaliser.urls(&result.urls),
                }))
            })
            .unwrap();
        out.insert(hash.clone(), record);
    }
    Json::Object(out)
}

// ---------------------------------------------------------------- waiting

async fn wait_until(what: &str, mut test: impl FnMut() -> bool) {
    for _ in 0..1200 {
        if test() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

fn queue_named(store: &Store, kind: QueueKind, name: Option<&str>) -> Option<Queue> {
    store
        .read(|conn| queues::queues(conn, Some(kind)))
        .unwrap()
        .into_iter()
        .find(|q| name.is_none_or(|n| q.name == n))
}

fn idle(store: &Store, queue: i64) -> bool {
    let files = store.read(|conn| queues::file_seeds(conn, queue)).unwrap();
    let galleries = store
        .read(|conn| queues::gallery_seeds(conn, queue))
        .unwrap();
    !files.is_empty()
        && files.iter().all(|f| f.status != SeedStatus::Unknown)
        && galleries.iter().all(|g| g.status != SeedStatus::Unknown)
}

fn checks(store: &Store, queue: i64) -> usize {
    store
        .read(|conn| queues::gallery_seeds(conn, queue))
        .unwrap()
        .iter()
        .filter(|g| g.status != SeedStatus::Unknown)
        .count()
}

/// `CheckNow`, without waiting out its half-minute: the check is due at once.
fn check_watcher_now(store: &Store, runner: &Arc<hydrus_download::QueueRunner>, queue: i64) {
    runner.check_watcher_now(queue).unwrap();
    let q = store
        .read(|conn| queues::queue(conn, queue))
        .unwrap()
        .unwrap();
    let mut state = watcher_state(&q).unwrap();
    state.next_check_time = 0;
    let extra = serde_json::to_value(&state).unwrap();
    store
        .write(move |ctx| queues::set_queue_extra(ctx.conn(), queue, &extra))
        .unwrap();
    runner.wake(queue);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_downloader_does_what_the_reference_did() {
    let recorded = hydrus_testkit::fixture_json("downloads.json");
    let port = recorded["site_port"].as_u64().unwrap();
    let base = format!("http://127.0.0.1:{port}");
    let site = Arc::new(Site {
        phases: recorded["phases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (string_map(&p["pages"]), string_map(&p["files"])))
            .collect(),
        phase: AtomicUsize::new(0),
        hits: Mutex::default(),
    });
    let app = Router::new().fallback(serve).with_state(Arc::clone(&site));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port as u16))
        .await
        .unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    // a migrated client, given the reference's downloader definitions
    let fixture = common::imported_store("basic");
    let store = fixture.state.store.clone();
    let domain_manager =
        SerialisableObject::from_tuple_str(&recorded["domain_manager"].to_string()).unwrap();
    let url_classes = hydrus_legacy::objects::domain::url_class_settings(&domain_manager).unwrap();
    let downloaders = hydrus_legacy::objects::parsers::downloaders(&domain_manager).unwrap();
    assert!(
        downloaders.unconverted.is_empty(),
        "{:?}",
        downloaders.unconverted
    );
    let gug = downloaders.gugs.gugs[0].clone();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &url_classes)?;
            hydrus_store::settings::set(ctx.conn(), &downloaders)
        })
        .unwrap();
    let state = hydrus_api::AppState::new(store.clone()).unwrap();
    let router = hydrus_api::router(state.clone());
    let runner = state.downloads.clone().unwrap();
    runner.start_all().unwrap();
    let downloader = Arc::clone(runner.downloader());

    // phase 0: a URL page, a watcher, a new subscription
    for (url, extra) in [
        ("/post/1", json!({})),
        ("/post/6", json!({"filterable_tags": ["From Companion"]})),
        ("/post/404", json!({})),
        ("/files/direct.jpg", json!({})),
        ("/search/red_hair/1", json!({})),
    ] {
        let mut body = json!({"url": format!("{base}{url}"), "destination_page_name": "urls"});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        add_url(&router, &body).await;
    }
    let url_page = queue_named(&store, QueueKind::Urls, Some("urls")).unwrap();
    wait_until("the url page", || idle(&store, url_page.id)).await;

    add_url(&router, &json!({"url": format!("{base}/thread/5")})).await;
    let watcher = queue_named(&store, QueueKind::Watcher, None).unwrap();
    wait_until("the watcher", || {
        checks(&store, watcher.id) >= 1 && idle(&store, watcher.id)
    })
    .await;

    let checkers: CheckerDefaults = store.read(hydrus_store::settings::get).unwrap();
    let settings = SubscriptionSettings {
        gug_key: gug.key().to_owned(),
        gug_name: gug.name().to_owned(),
        checker: checkers.subscriptions,
        initial_file_limit: Some(4),
        periodic_file_limit: Some(100),
        ..SubscriptionSettings::default()
    };
    let (sub_id, query_queue) = store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "blue eyes sub", &settings)?.unwrap();
            let queue = subs::add_query(ctx.conn(), id, &QueryState::new("blue_eyes"), 0)?;
            Ok((id, queue))
        })
        .unwrap();
    let report = downloader
        .run_subscription(sub_id, &Job::new())
        .await
        .unwrap();
    assert!(report.notices.is_empty(), "{:?}", report.notices);

    // phase 1: new uploads, new thread posts
    site.phase.store(1, Ordering::SeqCst);
    check_watcher_now(&store, &runner, watcher.id);
    wait_until("the watcher again", || {
        checks(&store, watcher.id) >= 2 && idle(&store, watcher.id)
    })
    .await;
    store
        .write(move |ctx| {
            let mut query = subs::query(ctx.conn(), query_queue)?.unwrap();
            query.state.check_now();
            subs::set_query_state(ctx.conn(), query_queue, &query.state)
        })
        .unwrap();
    let report = downloader
        .run_subscription(sub_id, &Job::new())
        .await
        .unwrap();
    assert!(report.notices.is_empty(), "{:?}", report.notices);

    // phase 2: the thread is gone
    site.phase.store(2, Ordering::SeqCst);
    check_watcher_now(&store, &runner, watcher.id);
    wait_until("the watcher to 404", || {
        let q = store
            .read(|conn| queues::queue(conn, watcher.id))
            .unwrap()
            .unwrap();
        checks(&store, watcher.id) >= 3 && watcher_state(&q).unwrap().status != CheckerStatus::Ok
    })
    .await;

    // compare
    let n = Normaliser::new(&base);
    let expected = &recorded["recorded"];
    let url_log = n.log(&store, url_page.id);
    assert_eq!(url_log, expected["url_page"], "the url page");

    let w = watcher_state(
        &store
            .read(|conn| queues::queue(conn, watcher.id))
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let mut watcher_log = n.log(&store, watcher.id);
    watcher_log["subject"] = json!(w.subject);
    watcher_log["checking_status"] = json!(match w.status {
        CheckerStatus::Ok => 0,
        CheckerStatus::Dead => 1,
        CheckerStatus::NotFound => 2,
    });
    assert_eq!(watcher_log, expected["watcher"], "the watcher");

    let query = store
        .read(|conn| subs::query(conn, query_queue))
        .unwrap()
        .unwrap();
    let mut sub_log = n.log(&store, query_queue);
    sub_log["dead"] = json!(query.state.dead);
    sub_log["paused"] = json!(query.state.paused);
    assert_eq!(sub_log, expected["subscription"], "the subscription");

    let mut hashes = BTreeSet::new();
    for log in [&url_log, &watcher_log, &sub_log] {
        for f in log["files"].as_array().unwrap() {
            if let Some(h) = f["sha256"].as_str() {
                hashes.insert(h.to_owned());
            }
        }
    }
    assert_eq!(
        file_records(&store, &n, &hashes),
        expected["files"],
        "the files"
    );

    // the same requests in each phase (the order within one can differ,
    // as the reference works its importers' pages and files side by side)
    let per_phase = |hits: Vec<(usize, String)>| -> BTreeMap<usize, Vec<String>> {
        let mut out: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (phase, path) in hits {
            out.entry(phase).or_default().push(path);
        }
        for paths in out.values_mut() {
            paths.sort();
        }
        out
    };
    let theirs = per_phase(
        expected["hits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                (
                    h[0].as_u64().unwrap() as usize,
                    h[1].as_str().unwrap().to_owned(),
                )
            })
            .collect(),
    );
    assert_eq!(per_phase(site.hits.lock().clone()), theirs, "the requests");
}
