//! Paged importer options met by the importers that read them.
//!
//! The three "additional fixed time to wait between gallery page fetches"
//! options: a downloader page's URL
//! queue, a watcher and a subscription each fetch gallery pages from one site
//! through the real network engine, and the server sees how far apart the
//! requests of that kind arrive.
//!
//! The reference's rule (`TryToConsumeAGalleryToken`: a kind's next page of a
//! site goes only once `now > last + that kind's delay`, whole seconds) is
//! recorded in `oracle/fixtures/gallery_wait_options.json`; which option each
//! kind reads is the recording's `options` map. Here the kind under test has
//! a short wait and the other two a wait that would outlast the test, so a
//! page that comes at all came on its own kind's wait. Only a lower bound is
//! asserted on the gap (the rule says a page goes no earlier).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use parking_lot::Mutex;

use hydrus_core::bandwidth::Rules;
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_core::url::strings::StringMatch;
use hydrus_core::url::{
    AnyGug, DomainMask, Gug, Gugs, StringProcessor, UrlClass, UrlClassSettings, UrlType,
};
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{Job, NetEngine, NetOptions};
use hydrus_parse::Downloaders;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch};
use hydrus_store::Store;
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::subscriptions as subs;

const GUG_KEY: &str = "0badc0de";
/// Long enough that a page held to it never arrives during a test.
const NEVER: i64 = 3600;

#[derive(Default)]
struct Site {
    hits: Mutex<Vec<(String, Instant)>>,
    /// Pages numbered 90 and up are held open until this is notified.
    release: tokio::sync::Notify,
}

type Hits = Arc<Site>;

/// A page of links to posts that don't exist (404): the fetches under test
/// are the pages, not what they point at. Gallery and search pages chain on
/// to a next page up to page 2.
async fn page(State(hits): State<Hits>, Path((kind, id)): Path<(String, u32)>) -> Response {
    hits.hits
        .lock()
        .push((format!("{kind}/{id}"), Instant::now()));
    if id >= 90 {
        hits.release.notified().await;
    }
    let next = if kind != "thread" && id == 1 {
        format!(r#"<a class="next" href="/{kind}/2">next</a>"#)
    } else {
        String::new()
    };
    let html = format!(
        r#"<html><head><title>{kind} {id}</title></head><body>
        <a class="thumb" href="/post/{kind}{id}a">a</a>
        <a class="thumb" href="/post/{kind}{id}b">b</a>
        {next}</body></html>"#
    );
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

fn html_formula(tag: &str, attrs: &[(&str, &str)], attr: &str) -> Formula {
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
            content: HtmlContent::Attribute(attr.into()),
        },
        processor: StringProcessor::default(),
    }
}

fn parser(url_type: i64, class: &str, attrs: &[(&str, &str)], attr: &str) -> ContentParser {
    ContentParser {
        name: class.into(),
        kind: ContentKind::Url {
            url_type,
            priority: 50,
        },
        formula: html_formula("a", attrs, attr),
    }
}

fn page_parser(key: &str) -> PageParser {
    PageParser {
        reference_auxiliary: None,
        name: format!("pages {key}"),
        key: key.into(),
        converter: hydrus_core::url::StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: vec![
            parser(7, "posts", &[("class", "thumb")], "href"),
            parser(6, "next", &[("class", "next")], "href"),
        ],
        example_urls: Vec::new(),
    }
}

fn class(host: &str, key: u8, url_type: UrlType, path: &[StringMatch]) -> UrlClass {
    UrlClass {
        name: format!("local {key}"),
        key: vec![key],
        url_type,
        preferred_scheme: "http".into(),
        domain_mask: DomainMask::new(vec![host.to_owned()], vec![], false, false),
        path_components: path.iter().map(|m| (m.clone(), None)).collect(),
        ..UrlClass::default()
    }
}

struct Setup {
    runner: Arc<QueueRunner>,
    downloader: Arc<Downloader>,
    store: Arc<Store>,
    hits: Hits,
    base: String,
    _dir: tempfile::TempDir,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Setup {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// A site with gallery, watched-thread and search pages, and the client's
/// saved gallery-page waits as given: (downloader pages, subscriptions,
/// watchers).
async fn setup(waits: (i64, i64, i64)) -> Setup {
    let hits = Hits::default();
    let app = Router::new()
        .route("/{kind}/{id}", get(page))
        .with_state(Arc::clone(&hits));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let any = StringMatch::any;
    let gallery = class(
        &host,
        0xce,
        UrlType::Gallery,
        &[StringMatch::fixed("gallery"), any()],
    );
    let search = class(
        &host,
        0xcf,
        UrlType::Gallery,
        &[StringMatch::fixed("search"), any()],
    );
    let thread = class(
        &host,
        0xd0,
        UrlType::Watchable,
        &[StringMatch::fixed("thread"), any()],
    );
    let post = class(
        &host,
        0xcd,
        UrlType::Post,
        &[StringMatch::fixed("post"), any()],
    );
    let settings = UrlClassSettings {
        parser_links: [&gallery, &search, &thread]
            .iter()
            .map(|c| (hex::encode(&c.key), Some("ac".to_owned())))
            .collect(),
        parser_keys: vec!["ac".into()],
        url_classes: vec![gallery, search, thread, post],
        collapse_leading_slashes: false,
    };
    let downloaders = Downloaders {
        parsers: vec![page_parser("ac")],
        gugs: Gugs {
            gugs: vec![AnyGug::Single(Gug {
                name: "local search".into(),
                key: GUG_KEY.into(),
                url_template: format!("http://{host}/search/%tags%"),
                replacement_phrase: "%tags%".into(),
                separator: "+".into(),
                initial_search_text: String::new(),
                example_search_text: "x".into(),
            })],
            keys_to_display: vec![GUG_KEY.into()],
        },
        ..Downloaders::default()
    };
    // (the reference's default per-domain limits would hold a local server
    // back for longer than the waits under test)
    let bandwidth = BandwidthSettings {
        rules: vec![(
            hydrus_store::network::NetworkContext::global(),
            Rules::new([]),
        )],
        gallery_page_wait_pages: waits.0,
        gallery_page_wait_subscriptions: waits.1,
        watcher_page_wait: waits.2,
        ..BandwidthSettings::default()
    };
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &settings)?;
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            hydrus_store::settings::set(ctx.conn(), &bandwidth)
        })
        .unwrap();
    // (bandwidth is obeyed: that is where the waits live)
    let net = Arc::new(NetEngine::new(Arc::clone(&store), NetOptions::default()).unwrap());
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    let downloader = Arc::new(Downloader::new(Arc::clone(&store), net, importer).unwrap());
    Setup {
        runner: QueueRunner::new(Arc::clone(&downloader), 60),
        downloader,
        store,
        hits,
        base: format!("http://{host}"),
        _dir: dir,
        server,
    }
}

/// When the server had `prefix`'s pages, in order, once `count` had come.
async fn arrivals(s: &Setup, prefix: &str, count: usize, within: Duration) -> Vec<Instant> {
    let start = Instant::now();
    loop {
        let times: Vec<Instant> = s
            .hits
            .hits
            .lock()
            .iter()
            .filter(|(path, _)| path.starts_with(prefix))
            .map(|(_, at)| *at)
            .collect();
        if times.len() >= count {
            return times;
        }
        assert!(
            start.elapsed() < within,
            "only {} of {count} `{prefix}` pages came: {:?}",
            times.len(),
            s.hits.hits.lock()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn assert_gap_at_least(times: &[Instant], wait: i64) {
    let gap = times[1].duration_since(times[0]);
    let wait = Duration::from_secs(wait.unsigned_abs());
    assert!(gap >= wait, "pages {gap:?} apart, under the {wait:?} wait");
}

// leaf: audit-options-downloading-gallery-downloader-additional-fixed-time-in-seconds-to-wait-between-gallery-page-fetches
#[tokio::test(flavor = "multi_thread")]
async fn a_downloader_page_waits_its_own_option_between_gallery_pages() {
    let s = setup((2, NEVER, NEVER)).await;
    s.runner.start_all().unwrap();
    let queue = s.runner.url_queue_for(None, None, None).unwrap();
    s.runner
        .pend_urls(
            queue.id,
            &[
                format!("{}/gallery/1", s.base),
                format!("{}/gallery/2", s.base),
            ],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    // two gallery pages of one site: the second comes once the 2 second wait
    // is over
    let times = arrivals(&s, "gallery/", 2, Duration::from_secs(20)).await;
    assert_gap_at_least(&times, 2);
}

// leaf: audit-options-downloading-watchers-additional-fixed-time-in-seconds-to-wait-between-watcher-checks
#[tokio::test(flavor = "multi_thread")]
async fn watchers_wait_their_own_option_between_checks_of_one_site() {
    let s = setup((NEVER, NEVER, 2)).await;
    s.runner.start_all().unwrap();
    for id in [1, 2] {
        s.runner
            .watch(
                &format!("{}/thread/{id}", s.base),
                None,
                None,
                None,
                &BTreeSet::new(),
                &[],
            )
            .unwrap();
    }
    let times = arrivals(&s, "thread/", 2, Duration::from_secs(20)).await;
    assert_gap_at_least(&times, 2);
}

// leaf: audit-options-downloading-subscriptions-additional-fixed-time-in-seconds-to-wait-between-gallery-page-fetches
#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_waits_its_own_option_between_search_pages() {
    let s = setup((NEVER, 2, NEVER)).await;
    let settings = SubscriptionSettings {
        gug_key: GUG_KEY.into(),
        gug_name: "local search".into(),
        initial_file_limit: Some(50),
        ..SubscriptionSettings::default()
    };
    let id = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "waits", &settings)?.unwrap();
            subs::add_query(ctx.conn(), id, &QueryState::new("1"), 0)?;
            Ok(id)
        })
        .unwrap();
    let downloader = Arc::clone(&s.downloader);
    let run = tokio::spawn(async move { downloader.run_subscription(id, &Job::new()).await });
    let times = arrivals(&s, "search/", 2, Duration::from_secs(20)).await;
    assert_gap_at_least(&times, 2);
    let _ = run.await;
}

// "Number of other paged importer jobs that can run at the same time": the
// reference gives this slot to the URL import page's gallery jobs
// (`ClientImportSimpleURLs`) and simple-downloader jobs, and to the local
// import page's files (`ClientImportLocal`). Each is run here with the
// option at 1: a second queue's job is left pending behind the first, which
// the site holds open, until the saved option is raised.

fn set_misc_slots(s: &Setup, misc: i64) {
    use hydrus_store::settings::{self, ImportWorkSlots};
    s.store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &ImportWorkSlots {
                    misc,
                    ..ImportWorkSlots::default()
                },
            )
        })
        .unwrap();
}

fn plain_queue(s: &Setup, kind: hydrus_store::queues::QueueKind, name: &str) -> i64 {
    use hydrus_store::queues;
    let name = name.to_owned();
    s.store
        .write(move |ctx| {
            queues::create_queue(
                ctx.conn(),
                kind,
                &name,
                None,
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                0,
            )
        })
        .unwrap()
}

fn held_pages(s: &Setup) -> usize {
    s.hits
        .hits
        .lock()
        .iter()
        .filter(|(path, _)| {
            path.rsplit('/')
                .next()
                .and_then(|id| id.parse::<u32>().ok())
                .is_some_and(|id| id >= 90)
        })
        .count()
}

async fn until(what: &str, mut ready: impl FnMut() -> bool) {
    for _ in 0..400 {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{what}");
}

/// Two queues each start a job on a held page. With one misc slot only one
/// does; the other shows `pending` (read by `status`) until the slot count
/// is raised and the runner reloads it.
async fn second_job_waits_for_the_misc_slot(
    s: &Setup,
    first: i64,
    second: i64,
    status: impl Fn(i64) -> String,
) {
    s.runner.start_all().unwrap();
    until("a held page was fetched", || held_pages(s) >= 1).await;
    until("the other job is left pending", || {
        status(first) == "pending" || status(second) == "pending"
    })
    .await;
    // (still one, a moment later)
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(held_pages(s), 1);
    set_misc_slots(s, 2);
    s.runner.reload_settings().unwrap();
    until("raising the option let the other job start", || {
        held_pages(s) == 2
    })
    .await;
    s.hits.release.notify_waiters();
}

// leaf: audit-options-importing-work-slots-number-of-other-paged-importer-jobs-that-can-run-at-the-same-time
#[tokio::test(flavor = "multi_thread")]
async fn url_queue_gallery_jobs_use_the_misc_slot() {
    use hydrus_store::queues::QueueKind;
    let s = setup((1, 1, 1)).await;
    set_misc_slots(&s, 1);
    let a = plain_queue(&s, QueueKind::Urls, "urls a");
    let b = plain_queue(&s, QueueKind::Urls, "urls b");
    for (queue, page) in [(a, 90), (b, 91)] {
        s.runner
            .pend_urls(
                queue,
                &[format!("{}/gallery/{page}", s.base)],
                &BTreeSet::new(),
                &[],
            )
            .unwrap();
    }
    second_job_waits_for_the_misc_slot(&s, a, b, |q| s.runner.status(q).gallery_status).await;
}

// leaf: audit-options-importing-work-slots-number-of-other-paged-importer-jobs-that-can-run-at-the-same-time
#[tokio::test(flavor = "multi_thread")]
async fn simple_downloader_jobs_use_the_misc_slot() {
    use hydrus_store::queues::{self, SimpleDownloader, SimpleJob};
    use hydrus_store::settings::SimpleDownloaderFormulae;
    let s = setup((1, 1, 1)).await;
    set_misc_slots(&s, 1);
    let formulae: SimpleDownloaderFormulae = s.store.read(hydrus_store::settings::get).unwrap();
    let formula = formulae
        .formulae
        .iter()
        .find(|f| f.name == formulae.favourite)
        .unwrap()
        .clone();
    let ids: Vec<i64> = [90, 91]
        .into_iter()
        .map(|page| {
            let state = SimpleDownloader {
                formula_name: formula.name.clone(),
                pending: vec![SimpleJob {
                    url: format!("{}/pages/{page}", s.base),
                    formula: formula.clone(),
                }],
            };
            s.store
                .write(move |ctx| {
                    queues::create_simple_downloader(
                        ctx.conn(),
                        None,
                        &hydrus_core::import_options::ImportOptionsSlice::default(),
                        &state,
                        0,
                    )
                })
                .unwrap()
        })
        .collect();
    second_job_waits_for_the_misc_slot(&s, ids[0], ids[1], |q| s.runner.status(q).gallery_status)
        .await;
}

/// Holds the store's writer busy until dropped: whatever a job writes waits.
struct HoldWriter(std::sync::mpsc::Sender<()>);

impl HoldWriter {
    fn new(store: &Arc<Store>) -> Self {
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let (held, is_held) = std::sync::mpsc::channel::<()>();
        let store = Arc::clone(store);
        std::thread::spawn(move || {
            store
                .write(move |_| {
                    held.send(()).ok();
                    wait.recv().ok();
                    Ok(())
                })
                .ok();
        });
        is_held.recv().unwrap();
        Self(release)
    }
}

impl Drop for HoldWriter {
    fn drop(&mut self) {
        self.0.send(()).ok();
    }
}

// leaf: audit-options-importing-work-slots-number-of-other-paged-importer-jobs-that-can-run-at-the-same-time
#[tokio::test(flavor = "multi_thread")]
async fn local_import_files_use_the_misc_slot() {
    use hydrus_store::queues::{self, LocalImport};
    // (two local imports whose jobs are held up at their first write to the
    // store, with the option at 1 and then 2: the number let in follows it)
    for misc in [1_usize, 2] {
        let s = setup((1, 1, 1)).await;
        set_misc_slots(&s, i64::try_from(misc).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let ids: Vec<i64> = ["a.jpg", "b.jpg"]
            .iter()
            .map(|name| {
                let path = dir.path().join(name);
                std::fs::copy(hydrus_testkit::fixture_path("media/jpeg_420.jpg"), &path).unwrap();
                let path = path.to_string_lossy().into_owned();
                s.store
                    .write(move |ctx| {
                        queues::create_local_import(
                            ctx.conn(),
                            None,
                            &hydrus_core::import_options::ImportOptionsSlice::default(),
                            &[(path, None)],
                            &queues::PathTags::new(),
                            LocalImport::default(),
                            0,
                        )
                    })
                    .unwrap()
            })
            .collect();
        let hold = HoldWriter::new(&s.store);
        let runner = Arc::clone(&s.runner);
        tokio::task::spawn_blocking(move || runner.start_all().unwrap());
        let importing = |s: &Setup| {
            ids.iter()
                .filter(|&&id| s.runner.status(id).files_status == "importing")
                .count()
        };
        until("the imports have started", || importing(&s) >= 1).await;
        // (long enough for a second to start, if the option lets it)
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(importing(&s), misc, "{misc} local imports let in");
        let pending = ids
            .iter()
            .filter(|&&id| s.runner.status(id).files_status == "pending")
            .count();
        assert_eq!(pending, 2 - misc, "the rest wait pending");
        drop(hold);
        until("both finish", || {
            ids.iter().all(|&id| {
                s.store
                    .read(|conn| queues::next_file_seed(conn, id))
                    .unwrap()
                    .is_none()
            })
        })
        .await;
    }
}
