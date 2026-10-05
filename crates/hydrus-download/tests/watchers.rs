//! Thread watchers against a local imageboard-like site: a thread's files
//! are downloaded, a later check finds only the new ones, the thread's
//! title becomes the watcher's subject, and a 404 stops the watcher.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use parking_lot::Mutex;

use hydrus_core::url::strings::StringMatch;
use hydrus_core::url::{DomainMask, StringProcessor, UrlClass, UrlClassSettings, UrlType};
use hydrus_core::watchers::{CheckerStatus, WatcherState};
use hydrus_download::queue::watcher_state;
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_parse::Downloaders;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch};
use hydrus_store::Store;
use hydrus_store::queues::{self, SeedStatus};

const IMAGES: &[&str] = &[
    "jpeg_420.jpg",
    "jpeg_422.jpg",
    "jpeg_444_q95.jpg",
    "jpeg_flat.jpg",
];

#[derive(Default)]
struct Site {
    /// File ids per thread; a missing thread is 404.
    threads: Mutex<HashMap<u32, Vec<usize>>>,
    hits: Mutex<HashMap<String, usize>>,
    release: tokio::sync::Notify,
}

async fn thread(State(site): State<Arc<Site>>, Path(id): Path<u32>) -> Response {
    *site.hits.lock().entry(format!("thread/{id}")).or_default() += 1;
    if id >= 90 {
        site.release.notified().await;
    }
    let Some(files) = site.threads.lock().get(&id).cloned() else {
        return (StatusCode::NOT_FOUND, "thread gone").into_response();
    };
    let mut html = format!("<html><head><title>thread {id}</title></head><body>");
    for f in files {
        html += &format!(r#"<a class="file" href="/files/{f}.jpg">file</a>"#);
    }
    html += "</body></html>";
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn file(Path(name): Path<String>) -> Response {
    let Some(id) = name
        .strip_suffix(".jpg")
        .and_then(|n| n.parse::<usize>().ok())
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let bytes = std::fs::read(hydrus_testkit::fixture_path(format!(
        "media/{}",
        IMAGES[id % IMAGES.len()]
    )))
    .unwrap();
    ([("content-type", "image/jpeg")], bytes).into_response()
}

fn html_formula(tag: &str, attrs: &[(&str, &str)], content: HtmlContent) -> Formula {
    Formula {
        reference_auxiliary: None,
        name: String::new(),
        kind: FormulaKind::Html {
            rules: vec![HtmlRule {
                walk: HtmlWalk::Descendants(TagSearch {
                    attrs: attrs
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                        .collect(),
                    index: None,
                }),
                tag_name: Some(tag.to_owned()),
                text_match: None,
            }],
            content,
        },
        processor: StringProcessor::default(),
    }
}

fn thread_parser() -> PageParser {
    PageParser {
        reference_auxiliary: None,
        name: "thread".into(),
        key: "ad".into(),
        converter: hydrus_core::url::StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: vec![
            ContentParser {
                name: "files".into(),
                kind: ContentKind::Url {
                    url_type: 7,
                    priority: 50,
                },
                formula: html_formula(
                    "a",
                    &[("class", "file")],
                    HtmlContent::Attribute("href".into()),
                ),
            },
            ContentParser {
                name: "title".into(),
                kind: ContentKind::Title { priority: 50 },
                formula: html_formula("title", &[], HtmlContent::Text),
            },
        ],
        example_urls: Vec::new(),
    }
}

struct Setup {
    runner: Arc<QueueRunner>,
    store: Arc<Store>,
    site: Arc<Site>,
    base: String,
    _dir: tempfile::TempDir,
}

async fn setup() -> Setup {
    let site = Arc::new(Site::default());
    let app = Router::new()
        .route("/thread/{id}", get(thread))
        .route("/files/{name}", get(file))
        .with_state(Arc::clone(&site));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let class = UrlClass {
        name: "local board thread".into(),
        key: vec![0xcf],
        url_type: UrlType::Watchable,
        preferred_scheme: "http".into(),
        domain_mask: DomainMask::new(vec![host.clone()], vec![], false, false),
        path_components: vec![
            (StringMatch::fixed("thread"), None),
            (StringMatch::any(), None),
        ],
        ..UrlClass::default()
    };
    let settings = UrlClassSettings {
        parser_links: vec![(hex::encode(&class.key), Some("ad".into()))],
        parser_keys: vec!["ad".into()],
        url_classes: vec![class],
        collapse_leading_slashes: false,
    };
    let downloaders = Downloaders {
        parsers: vec![thread_parser()],
        ..Downloaders::default()
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &settings)?;
            hydrus_store::settings::set(ctx.conn(), &downloaders)
        })
        .unwrap();
    let net = Arc::new(
        NetEngine::new(
            Arc::clone(&store),
            NetOptions {
                obey_bandwidth: false,
                ..NetOptions::default()
            },
        )
        .unwrap(),
    );
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let downloader = Arc::new(Downloader::new(Arc::clone(&store), net, importer).unwrap());
    Setup {
        runner: QueueRunner::new(downloader, 60),
        store,
        site,
        base: format!("http://{host}"),
        _dir: dir,
    }
}

fn state(store: &Store, queue: i64) -> WatcherState {
    let q = store
        .read(|conn| queues::queue(conn, queue))
        .unwrap()
        .unwrap();
    watcher_state(&q).unwrap()
}

/// Wait until the watcher has checked `checks` times and has no files left
/// to get.
async fn wait_for(store: &Store, queue: i64, checks: usize) {
    for _ in 0..400 {
        let galleries = store
            .read(|conn| queues::gallery_seeds(conn, queue))
            .unwrap();
        let checked = galleries
            .iter()
            .filter(|g| g.status != SeedStatus::Unknown)
            .count();
        let pending = store
            .read(|conn| queues::next_file_seed(conn, queue))
            .unwrap();
        if checked >= checks && pending.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the watcher did not finish");
}

/// Make the next check due now (rather than waiting minutes).
fn check_soon(s: &Setup, queue: i64) {
    let mut st = state(&s.store, queue);
    st.next_check_time = 0;
    let extra = serde_json::to_value(&st).unwrap();
    s.store
        .write(move |ctx| queues::set_queue_extra(ctx.conn(), queue, &extra))
        .unwrap();
    s.runner.wake(queue);
}

fn files(store: &Store, queue: i64) -> Vec<(String, SeedStatus)> {
    store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap()
        .into_iter()
        .map(|s| (s.data.rsplit('/').next().unwrap().to_owned(), s.status))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_watcher_follows_a_thread_until_it_404s() {
    let s = setup().await;
    s.site.threads.lock().insert(5, vec![1, 2]);
    s.runner.start_all().unwrap();
    let url = format!("{}/thread/5", s.base);
    let (queue, new) = s
        .runner
        .watch(&url, None, None, None, &BTreeSet::new(), &[])
        .unwrap();
    assert!(new);
    assert_eq!(queue.name, "watcher");
    // the same thread again: already watched
    let (again, new) = s
        .runner
        .watch(&url, None, None, None, &BTreeSet::new(), &[])
        .unwrap();
    assert!(!new);
    assert_eq!(again.id, queue.id);

    wait_for(&s.store, queue.id, 1).await;
    let ok = SeedStatus::SuccessfulAndNew;
    assert_eq!(
        files(&s.store, queue.id),
        [("1.jpg".to_owned(), ok), ("2.jpg".to_owned(), ok)]
    );
    let st = state(&s.store, queue.id);
    assert_eq!(st.subject, "thread 5");
    assert_eq!(st.status, CheckerStatus::Ok);
    assert!(st.last_check_time > 0 && st.next_check_time > st.last_check_time);

    // a new post: the next check gets just that
    s.site.threads.lock().get_mut(&5).unwrap().push(3);
    check_soon(&s, queue.id);
    wait_for(&s.store, queue.id, 2).await;
    assert_eq!(
        files(&s.store, queue.id),
        [
            ("1.jpg".to_owned(), ok),
            ("2.jpg".to_owned(), ok),
            ("3.jpg".to_owned(), ok)
        ]
    );

    // the thread is deleted: the watcher stops
    s.site.threads.lock().remove(&5);
    check_soon(&s, queue.id);
    wait_for(&s.store, queue.id, 3).await;
    let st = state(&s.store, queue.id);
    assert_eq!(st.status, CheckerStatus::NotFound);
    assert!(st.checking_paused);
    let hits = s.site.hits.lock()["thread/5"];
    assert_eq!(hits, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn watcher_checks_share_their_own_live_capacity_and_owner_close_releases_it() {
    use hydrus_store::settings::{self, ImportWorkSlots};
    let s = setup().await;
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &ImportWorkSlots {
                    watcher_check: 1,
                    ..ImportWorkSlots::default()
                },
            )
        })
        .unwrap();
    let mut queues = Vec::new();
    for thread in [90, 91, 92] {
        s.site.threads.lock().insert(thread, vec![]);
        let (queue, _) = s
            .runner
            .watch(
                &format!("{}/thread/{thread}", s.base),
                None,
                None,
                None,
                &BTreeSet::new(),
                &[],
            )
            .unwrap();
        queues.push(queue.id);
    }
    s.runner.start_all().unwrap();
    let count = || {
        s.site
            .hits
            .lock()
            .iter()
            .filter(|(url, _)| url.starts_with("thread/9"))
            .map(|(_, count)| count)
            .sum::<usize>()
    };
    for _ in 0..400 {
        if count() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(count(), 1);
    for _ in 0..200 {
        if queues
            .iter()
            .filter(|&&id| s.runner.status(id).gallery_status == "pending")
            .count()
            == 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        queues
            .iter()
            .filter(|&&id| s.runner.status(id).gallery_status == "pending")
            .count(),
        2
    );
    let blocked = queues
        .iter()
        .copied()
        .find(|&id| s.runner.status(id).gallery_status == "pending")
        .unwrap();
    s.runner
        .pend_urls(
            blocked,
            &[format!("{}/files/1.jpg", s.base)],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    for _ in 0..200 {
        if s.store
            .read(|conn| queues::next_file_seed(conn, blocked))
            .unwrap()
            .is_none()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let imported = s
        .store
        .read(|conn| queues::file_seeds(conn, blocked))
        .unwrap();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].status, SeedStatus::SuccessfulAndNew);
    assert_eq!(count(), 1, "file work does not acquire a checker permit");
    let running = s
        .runner
        .live()
        .into_iter()
        .find(|(_, live)| live.gallery_job.is_some())
        .unwrap()
        .0;
    s.store
        .write(move |ctx| queues::set_page_closed(ctx.conn(), running, true))
        .unwrap();
    for _ in 0..200 {
        if count() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(count(), 2);
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &ImportWorkSlots {
                    watcher_check: 2,
                    ..ImportWorkSlots::default()
                },
            )
        })
        .unwrap();
    s.runner.reload_settings().unwrap();
    for _ in 0..200 {
        if count() == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(count(), 3);
    for queue in queues {
        s.runner.cancel(queue, hydrus_store::live::JobKind::Gallery);
    }
}
