//! Subscriptions against a local booru with a tag search: the first sync
//! stops at the initial file limit, a later one catches up on new uploads
//! and stops once it sees what it found before, the files are downloaded
//! with the query's own tags added, a missing downloader pauses the
//! subscription, and a subscription shows what it does in a popup, which
//! can cancel it.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use parking_lot::Mutex;

use hydrus_core::import_options::{ServiceTagImportOptions, TagImportOptions};
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_core::url::strings::StringMatch;
use hydrus_core::url::{
    AnyGug, DomainMask, Gug, Gugs, StringProcessor, UrlClass, UrlClassSettings, UrlType,
};
use hydrus_download::Downloader;
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{Job, NetEngine, NetOptions};
use hydrus_parse::Downloaders;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch};
use hydrus_store::Store;
use hydrus_store::queues::{self, SeedStatus};
use hydrus_store::subscriptions as subs;

const PER_PAGE: usize = 3;
const IMAGES: &[&str] = &[
    "jpeg_420.jpg",
    "jpeg_422.jpg",
    "jpeg_444_q95.jpg",
    "jpeg_flat.jpg",
    "jpeg_gray.jpg",
    "jpeg_orient2.jpg",
    "jpeg_orient3.jpg",
    "jpeg_orient4.jpg",
    "jpeg_orient5.jpg",
    "jpeg_orient6.jpg",
];

#[derive(Default)]
struct Site {
    /// Post ids per tag, oldest first.
    tags: Mutex<HashMap<String, Vec<usize>>>,
    hits: Mutex<HashMap<String, usize>>,
    /// Searches wait for this, if set.
    hold: Mutex<Option<Arc<tokio::sync::Notify>>>,
}

impl Site {
    fn upload(&self, tag: &str, ids: impl IntoIterator<Item = usize>) {
        self.tags
            .lock()
            .entry(tag.to_owned())
            .or_default()
            .extend(ids);
    }
}

async fn search(
    State(site): State<Arc<Site>>,
    Path((tag, page)): Path<(String, usize)>,
) -> Response {
    *site
        .hits
        .lock()
        .entry(format!("search/{tag}/{page}"))
        .or_default() += 1;
    let hold = site.hold.lock().clone();
    if let Some(hold) = hold {
        hold.notified().await;
    }
    let mut posts = site.tags.lock().get(&tag).cloned().unwrap_or_default();
    posts.reverse();
    let start = (page - 1) * PER_PAGE;
    let on_page: Vec<usize> = posts.iter().skip(start).take(PER_PAGE).copied().collect();
    let mut html = String::from("<html><body>");
    for id in &on_page {
        html += &format!(r#"<a class="thumb" href="/post/{id}">post {id}</a>"#);
    }
    if posts.len() > start + PER_PAGE {
        html += &format!(
            r#"<a class="next" href="/search/{tag}/{}">next</a>"#,
            page + 1
        );
    }
    html += "</body></html>";
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn post(State(site): State<Arc<Site>>, Path(id): Path<usize>) -> Response {
    *site.hits.lock().entry(format!("post/{id}")).or_default() += 1;
    let html = format!(
        r#"<html><body><ul><li class="tag">post {id}</li></ul><img id="image" src="/files/{id}.jpg"></body></html>"#
    );
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn raw_failure(Path(kind): Path<String>) -> Response {
    match kind.as_str() {
        "500" => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "synthetic server failure",
        )
            .into_response(),
        "404" => StatusCode::NOT_FOUND.into_response(),
        _ => ([("content-type", "text/html")], "<html>missing file</html>").into_response(),
    }
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

fn url_parser(
    name: &str,
    url_type: i64,
    tag: &str,
    attrs: &[(&str, &str)],
    attr: &str,
) -> ContentParser {
    ContentParser {
        name: name.into(),
        kind: ContentKind::Url {
            url_type,
            priority: 50,
        },
        formula: html_formula(tag, attrs, HtmlContent::Attribute(attr.into())),
    }
}

fn parsers() -> Vec<PageParser> {
    vec![
        PageParser {
            reference_auxiliary: None,
            name: "post".into(),
            key: "ab".into(),
            converter: hydrus_core::url::StringConverter::default(),
            subsidiary: Vec::new(),
            content_parsers: vec![
                url_parser("file", 7, "img", &[("id", "image")], "src"),
                ContentParser {
                    name: "tags".into(),
                    kind: ContentKind::Tag { namespace: None },
                    formula: html_formula("li", &[("class", "tag")], HtmlContent::Text),
                },
            ],
            example_urls: Vec::new(),
        },
        PageParser {
            reference_auxiliary: None,
            name: "search".into(),
            key: "ac".into(),
            converter: hydrus_core::url::StringConverter::default(),
            subsidiary: Vec::new(),
            content_parsers: vec![
                url_parser("posts", 7, "a", &[("class", "thumb")], "href"),
                url_parser("next", 6, "a", &[("class", "next")], "href"),
            ],
            example_urls: Vec::new(),
        },
    ]
}

fn class(host: &str, name: &str, key: u8, url_type: UrlType, path: &[StringMatch]) -> UrlClass {
    UrlClass {
        name: name.into(),
        key: vec![key],
        url_type,
        preferred_scheme: "http".into(),
        domain_mask: DomainMask::new(vec![host.to_owned()], vec![], false, false),
        path_components: path.iter().map(|m| (m.clone(), None)).collect(),
        ..UrlClass::default()
    }
}

const GUG_KEY: &str = "0badc0de";
const GUG_NAME: &str = "local booru tag search";

struct Setup {
    downloader: Arc<Downloader>,
    store: Arc<Store>,
    site: Arc<Site>,
    _dir: tempfile::TempDir,
    base: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Setup {
    fn drop(&mut self) {
        self.server.abort();
    }
}

async fn setup() -> Setup {
    let site = Arc::new(Site::default());
    let app = Router::new()
        .route("/search/{tag}/{page}", get(search))
        .route("/post/{id}", get(post))
        .route("/files/{name}", get(file))
        .route("/raw/{kind}", get(raw_failure))
        .with_state(Arc::clone(&site));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let post = class(
        &host,
        "post",
        0xcd,
        UrlType::Post,
        &[StringMatch::fixed("post"), StringMatch::any()],
    );
    let search = class(
        &host,
        "search",
        0xce,
        UrlType::Gallery,
        &[
            StringMatch::fixed("search"),
            StringMatch::any(),
            StringMatch::any(),
        ],
    );
    let settings = UrlClassSettings {
        parser_links: vec![
            (hex::encode(&post.key), Some("ab".into())),
            (hex::encode(&search.key), Some("ac".into())),
        ],
        parser_keys: vec!["ab".into(), "ac".into()],
        url_classes: vec![post, search],
        collapse_leading_slashes: false,
    };
    let downloaders = Downloaders {
        parsers: parsers(),
        gugs: Gugs {
            gugs: vec![AnyGug::Single(Gug {
                name: GUG_NAME.into(),
                key: GUG_KEY.into(),
                url_template: format!("http://{host}/search/%tags%/1"),
                replacement_phrase: "%tags%".into(),
                separator: "+".into(),
                initial_search_text: String::new(),
                example_search_text: "blue_eyes".into(),
            })],
            keys_to_display: vec![GUG_KEY.into()],
        },
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
        downloader,
        store,
        site,
        _dir: dir,
        base: format!("http://{host}"),
        server,
    }
}

/// A popup showing: its text, and its files' label and number.
type Shown = (Option<String>, Option<(String, usize)>);

fn popups_shown(store: &Store) -> Vec<Shown> {
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    store
        .read(|conn| hydrus_store::popups::all(conn, now))
        .unwrap()
        .into_iter()
        .map(|job| {
            let files = job
                .files
                .map(|(hashes, label)| (label.unwrap_or_default(), hashes.len()));
            (job.status_text_1, files)
        })
        .collect()
}

fn post_ids(store: &Store, queue: i64) -> Vec<(usize, SeedStatus)> {
    store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap()
        .iter()
        .map(|s| {
            let id = s.data.rsplit('/').next().unwrap().parse().unwrap();
            (id, s.status)
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_syncs_catches_up_and_downloads() {
    let s = setup().await;
    s.site.upload("blue_eyes", 1..=7);
    let my_tags = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let settings = SubscriptionSettings {
        gug_key: GUG_KEY.into(),
        gug_name: GUG_NAME.into(),
        initial_file_limit: Some(4),
        ..SubscriptionSettings::default()
    };
    let mut state = QueryState::new("blue_eyes");
    state.tag_import_options = TagImportOptions {
        services: vec![(
            my_tags.clone(),
            ServiceTagImportOptions {
                additional_tags: vec!["from my sub".into()],
                ..ServiceTagImportOptions::default()
            },
        )],
    };
    let (id, queue) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "blue eyes", &settings)?.unwrap();
            let queue = subs::add_query(ctx.conn(), id, &state, 0)?;
            Ok((id, queue))
        })
        .unwrap();

    // first sync: the newest four, oldest first, then downloaded
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!(report.new_urls, 4, "{report:?}");
    assert!(report.notices.is_empty(), "{:?}", report.notices);
    let ok = SeedStatus::SuccessfulAndNew;
    assert_eq!(
        post_ids(&s.store, queue),
        [(4, ok), (5, ok), (6, ok), (7, ok)]
    );
    let pages = |store: &Store| -> Vec<(String, SeedStatus, String)> {
        store
            .read(|conn| queues::gallery_seeds(conn, queue))
            .unwrap()
            .into_iter()
            .map(|g| {
                let path = g.url.splitn(4, '/').nth(3).unwrap().to_owned();
                (path, g.status, g.note)
            })
            .collect()
    };
    let first = pages(&s.store);
    assert_eq!(first.len(), 2, "{first:?}");
    assert_eq!(first[0].0, "search/blue_eyes/1");
    assert_eq!(first[1].0, "search/blue_eyes/2");
    assert!(first[1].2.ends_with("hit initial file limit"), "{first:?}");
    let query = s
        .store
        .read(|conn| subs::query(conn, queue))
        .unwrap()
        .unwrap();
    assert!(query.state.last_check_time > 0);
    assert!(!query.state.dead && !query.state.check_now);
    assert!(query.state.next_check_time > query.state.last_check_time);

    // the query's own tag went on every file
    let hashes: Vec<String> = s
        .store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap()
        .iter()
        .map(|seed| seed.meta.hash("sha256").unwrap().to_owned())
        .collect();
    for hash in &hashes {
        assert_eq!(
            my_tags_of(&s.store, hash),
            ["from my sub".to_owned()].into(),
            "{hash}"
        );
    }
    // and its new files are offered in a popup, labelled with the
    // subscription's name (its queries publish together by default)
    assert_eq!(
        popups_shown(&s.store),
        [(None, Some(("blue eyes".to_owned(), 4)))]
    );

    // new uploads: the next check gets just those, and stops once it has
    // seen most of what it found before
    s.site.upload("blue_eyes", 8..=9);
    s.store
        .write(move |ctx| {
            let mut query = subs::query(ctx.conn(), queue)?.unwrap();
            query.state.check_now();
            subs::set_query_state(ctx.conn(), queue, &query.state)
        })
        .unwrap();
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!(report.new_urls, 2, "{report:?}");
    assert_eq!(
        post_ids(&s.store, queue),
        [(4, ok), (5, ok), (6, ok), (7, ok), (8, ok), (9, ok)]
    );
    // (the next files join the popup with the same label)
    assert_eq!(
        popups_shown(&s.store),
        [(None, Some(("blue eyes".to_owned(), 6)))]
    );
    let second = pages(&s.store);
    assert_eq!(second.len(), 4, "{second:?}");
    assert!(
        second[3]
            .2
            .contains("so much of what I already knew about that I am assuming I caught up"),
        "{second:?}"
    );
    let hits = s.site.hits.lock().clone();
    for old in 1..=3 {
        assert!(!hits.contains_key(&format!("post/{old}")), "{hits:?}");
    }
    assert_eq!(hits["post/7"], 1);

    // nothing to do until the next check
    let sub = s
        .store
        .read(|conn| subs::subscription(conn, id))
        .unwrap()
        .unwrap();
    let query = s
        .store
        .read(|conn| subs::query(conn, queue))
        .unwrap()
        .unwrap();
    assert_eq!(
        s.downloader.next_work_time(&sub).unwrap(),
        Some(query.state.next_check_time)
    );
}

fn my_tags_of(store: &Store, hash: &str) -> BTreeSet<String> {
    let snapshot = store.snapshot();
    store
        .read(|conn| {
            let hash = hydrus_core::Sha256::from_slice(&hex::decode(hash).unwrap()).unwrap();
            let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
            let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
            let my_tags = snapshot
                .services
                .builtin(hydrus_core::service::builtin_keys::MY_TAGS)?
                .id;
            Ok(batch.results[0]
                .tags
                .get(&my_tags)
                .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                .map(|tags| {
                    tags.iter()
                        .map(|t| batch.tags[t].as_str().to_owned())
                        .collect()
                })
                .unwrap_or_default())
        })
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_without_its_downloader_pauses() {
    let s = setup().await;
    let settings = SubscriptionSettings {
        gug_key: "ffff".into(),
        gug_name: "gone".into(),
        ..SubscriptionSettings::default()
    };
    let id = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "orphan", &settings)?.unwrap();
            subs::add_query(ctx.conn(), id, &QueryState::new("anything"), 0)?;
            Ok(id)
        })
        .unwrap();
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!(
        report.notices,
        [
            "The subscription \"orphan\" could not find a Gallery URL Generator for \"gone\"! The sub has paused!"
        ]
    );
    // which the user is shown
    assert_eq!(
        popups_shown(&s.store),
        [(Some(report.notices[0].clone()), None)]
    );
    let sub = s
        .store
        .read(|conn| subs::subscription(conn, id))
        .unwrap()
        .unwrap();
    assert!(sub.settings.paused);
    assert_eq!(s.downloader.next_work_time(&sub).unwrap(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_stops_when_its_bandwidth_runs_out() {
    use hydrus_core::bandwidth::{BandwidthType, Rule, Rules};
    use hydrus_core::network::{CONTEXT_SUBSCRIPTION, NetworkContext};
    use hydrus_store::bandwidth::BandwidthSettings;

    let s = setup().await;
    s.site.upload("blue_eyes", 1..=7);
    // seven requests a day per subscription query: two gallery pages of
    // three, then a post and its file per download (no waits between
    // gallery pages)
    let mut bandwidth = BandwidthSettings {
        gallery_page_wait_subscriptions: 0,
        ..BandwidthSettings::default()
    };
    for (context, rules) in &mut bandwidth.rules {
        if *context == NetworkContext::default_of_kind(CONTEXT_SUBSCRIPTION) {
            *rules = Rules::new([Rule::new(BandwidthType::Requests, Some(86_400), 7)]);
        }
    }
    s.store
        .write_and_refresh(move |ctx| hydrus_store::settings::set(ctx.conn(), &bandwidth))
        .unwrap();
    let net = Arc::new(NetEngine::new(Arc::clone(&s.store), NetOptions::default()).unwrap());
    let importer = FileImporter::new(Arc::clone(&s.store), MediaTools::new());
    let downloader = Downloader::new(Arc::clone(&s.store), net, importer).unwrap();

    let settings = SubscriptionSettings {
        gug_key: GUG_KEY.into(),
        gug_name: GUG_NAME.into(),
        initial_file_limit: Some(4),
        ..SubscriptionSettings::default()
    };
    let state = QueryState::new("blue_eyes");
    let (id, queue) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "blue eyes", &settings)?.unwrap();
            let queue = subs::add_query(ctx.conn(), id, &state, 0)?;
            Ok((id, queue))
        })
        .unwrap();
    let report = downloader.run_subscription(id, &Job::new()).await.unwrap();
    assert_eq!(report.new_urls, 4, "{report:?}");
    // two pages and two downloads use six: there isn't room for another
    // request and a megabyte, as the subscription asks before each file
    let ok = SeedStatus::SuccessfulAndNew;
    let unknown = SeedStatus::Unknown;
    assert_eq!(
        post_ids(&s.store, queue),
        [(4, ok), (5, ok), (6, unknown), (7, unknown)]
    );
    // and the subscription comes back when there is bandwidth, not at once
    let sub = s
        .store
        .read(|conn| subs::subscription(conn, id))
        .unwrap()
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let next = downloader.next_work_time(&sub).unwrap().unwrap();
    assert!(next > now + 30, "{next} vs {now}");
}

#[tokio::test(flavor = "multi_thread")]
async fn subscriptions_wait_while_paused_globally() {
    use hydrus_store::settings::Pauses;
    let s = setup().await;
    s.site.upload("blue_eyes", 1..=3);
    let settings = SubscriptionSettings {
        gug_key: GUG_KEY.into(),
        gug_name: GUG_NAME.into(),
        ..SubscriptionSettings::default()
    };
    let state = QueryState::new("blue_eyes");
    let (id, queue) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "blue eyes", &settings)?.unwrap();
            let queue = subs::add_query(ctx.conn(), id, &state, 0)?;
            Ok((id, queue))
        })
        .unwrap();
    // hydrus's "pause subscriptions", and its "pause all new network
    // traffic", each stop them
    for pauses in [
        Pauses {
            subscriptions: true,
            ..Pauses::default()
        },
        Pauses {
            network_traffic: true,
            ..Pauses::default()
        },
    ] {
        s.store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &pauses))
            .unwrap();
        let report = s
            .downloader
            .run_subscription(id, &Job::new())
            .await
            .unwrap();
        assert_eq!(report.new_urls, 0);
        assert!(post_ids(&s.store, queue).is_empty());
    }
    s.store
        .write(|ctx| hydrus_store::settings::set(ctx.conn(), &Pauses::default()))
        .unwrap();
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!(report.new_urls, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_with_no_import_destination_waits_out_its_error_delay() {
    use hydrus_core::import_options::{ImportOptionsSlice, LocationOptions, NO_IMPORT_DESTINATION};
    let s = setup().await;
    s.site.upload("blue_eyes", 1..=2);
    let settings = SubscriptionSettings {
        gug_key: GUG_KEY.into(),
        gug_name: GUG_NAME.into(),
        import_options: ImportOptionsSlice {
            locations: Some(LocationOptions {
                destinations: Vec::new(),
                ..LocationOptions::default()
            }),
            ..ImportOptionsSlice::default()
        },
        ..SubscriptionSettings::default()
    };
    let state = QueryState::new("blue_eyes");
    let (id, queue) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "nowhere", &settings)?.unwrap();
            let queue = subs::add_query(ctx.conn(), id, &state, 0)?;
            Ok((id, queue))
        })
        .unwrap();
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    // the search still runs; the files wait
    assert_eq!(report.new_urls, 2);
    let unknown = SeedStatus::Unknown;
    assert_eq!(post_ids(&s.store, queue), [(1, unknown), (2, unknown)]);
    assert_eq!(
        report.notices,
        [format!(
            "The subscription \"nowhere\" encountered an error when trying to sync: {NO_IMPORT_DESTINATION}"
        )]
    );
    let sub = s
        .store
        .read(|conn| subs::subscription(conn, id))
        .unwrap()
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // (subscription_other_error_delay: 36 hours by default)
    let wait = sub.settings.no_work_until - now;
    assert!((129_500..=129_600).contains(&wait), "{wait}");
    assert_eq!(
        sub.settings.no_work_until_reason,
        format!("error: {NO_IMPORT_DESTINATION}")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subscription_shows_what_it_does_in_a_popup_which_can_cancel_it() {
    use hydrus_store::popups;
    let s = setup().await;
    s.site.upload("blue_eyes", 1..=2);
    let hold = Arc::new(tokio::sync::Notify::new());
    *s.site.hold.lock() = Some(Arc::clone(&hold));
    let settings = SubscriptionSettings {
        gug_key: GUG_KEY.into(),
        gug_name: GUG_NAME.into(),
        ..SubscriptionSettings::default()
    };
    assert!(settings.show_a_popup_while_working, "(by default)");
    let id = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(ctx.conn(), "blue eyes", &settings)?.unwrap();
            subs::add_query(ctx.conn(), id, &QueryState::new("blue_eyes"), 0)?;
            Ok(id)
        })
        .unwrap();
    let downloader = Arc::clone(&s.downloader);
    let run = tokio::spawn(async move { downloader.run_subscription(id, &Job::new()).await });

    // while it waits for its first gallery page: what it is doing, and the
    // page's download (written four times a second)
    for _ in 0..500 {
        if s.site.hits.lock().contains_key("search/blue_eyes/1") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    let now = || hydrus_core::time::TimestampMs::now().millis() / 1000;
    let shown = s.store.read(|conn| popups::all(conn, now())).unwrap();
    assert_eq!(shown.len(), 1, "{shown:?}");
    let popup = &shown[0];
    assert_eq!(
        popup.status_title.as_deref(),
        Some("subscriptions - blue eyes")
    );
    assert_eq!(
        popup.status_text_1.as_deref(),
        Some("synchronising (0/1) \"blue_eyes\": downloading gallery page")
    );
    assert_eq!(popup.popup_gauge_1, Some((0, 1)));
    assert!(popup.cancellable && !popup.done);
    let download = popup.network_job.as_ref().expect("the page's download");
    assert!(
        download.url.ends_with("/search/blue_eyes/1"),
        "{download:?}"
    );

    // the client cancels it: it stops once the page is read, for a while
    let key = popup.key;
    s.store
        .write(move |ctx| popups::update(ctx.conn(), &key, now(), popups::Job::cancel))
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    hold.notify_one();
    let report = run.await.unwrap().unwrap();
    assert_eq!((report.new_urls, report.files_worked), (0, 0), "{report:?}");
    let sub = s
        .store
        .read(|conn| subs::subscription(conn, id))
        .unwrap()
        .unwrap();
    assert_eq!(
        sub.settings.no_work_until_reason,
        "gallery parsing cancelled, likely by user"
    );
    assert!(sub.settings.no_work_until > now());
    // and its popup goes
    assert!(popups_shown(&s.store).is_empty());
}

fn pending_seed(url: String) -> queues::NewFileSeed {
    queues::NewFileSeed {
        seed_type: queues::SeedType::Url,
        data_for_comparison: url.clone(),
        data: url,
        source_time: None,
        referral_url: None,
        meta: queues::FileSeedMeta::default(),
    }
}

fn future_query(name: &str) -> QueryState {
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    QueryState {
        last_check_time: now,
        next_check_time: now + 86_400,
        ..QueryState::new(name)
    }
}

fn configure_error_limit(s: &Setup, threshold: Option<u64>) {
    s.store
        .write(move |ctx| {
            let mut settings: hydrus_store::network::NetworkSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.subscription_file_error_cancel_threshold = threshold;
            settings.subscription_other_error_delay = 37;
            settings.process_subs_in_random_order = false;
            settings.domain_error_number = 100;
            settings.max_get_attempts = 1;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
    s.downloader.reload_settings().unwrap();
}

// leaf: audit-options-downloading-subscriptions-if-a-subscription-has-this-many-failed-file-imports-stop-and-continue-later
#[tokio::test(flavor = "multi_thread")]
async fn handled_http_and_missing_file_failures_do_not_spend_the_outer_error_budget() {
    let s = setup().await;
    configure_error_limit(&s, Some(1));
    let urls: Vec<_> = ["500", "404", "missing"]
        .iter()
        .map(|kind| pending_seed(format!("{}/raw/{kind}", s.base)))
        .collect();
    let (id, queue) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(
                ctx.conn(),
                "handled failures",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            let queue = subs::add_query(ctx.conn(), id, &future_query("0"), 0)?;
            queues::add_file_seeds(ctx.conn(), queue, &urls, false, 0)?;
            Ok((id, queue))
        })
        .unwrap();
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!(report.files_worked, 3);
    assert_eq!(
        report.file_errors, 0,
        "WorkOnURL swallows these, like the real reference"
    );
    assert!(report.notices.is_empty());
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue))
        .unwrap();
    assert_eq!(
        seeds.iter().map(|seed| seed.status).collect::<Vec<_>>(),
        [SeedStatus::Error, SeedStatus::Vetoed, SeedStatus::Error]
    );
    assert_eq!(seeds[1].note, "404");
    let recorded = hydrus_testkit::fixture_json("subscription_failure_limit.json");
    for (seed, case) in seeds
        .iter()
        .zip(recorded["handled_work_on_url"].as_array().unwrap())
    {
        assert_eq!(serde_json::json!(seed.status.code()), case["status"]);
        assert_eq!(serde_json::json!(report.file_errors), case["file_errors"]);
    }

    assert_eq!(
        s.store
            .read(|conn| subs::subscription(conn, id))
            .unwrap()
            .unwrap()
            .settings
            .no_work_until,
        0
    );
}

fn fail_query_tag_writes(s: &Setup) {
    s.store.write(|ctx| {
        ctx.conn().execute_batch("CREATE TRIGGER fail_outer_tag BEFORE INSERT ON subtags WHEN NEW.subtag = 'outer failure' BEGIN SELECT RAISE(ABORT, 'synthetic outer tag failure'); END;")?;
        Ok(())
    }).unwrap();
}

// leaf: audit-options-downloading-subscriptions-if-a-subscription-has-this-many-failed-file-imports-stop-and-continue-later
#[tokio::test(flavor = "multi_thread")]
async fn escaped_query_tag_errors_count_across_queries_delay_persist_and_reset_next_run() {
    let s = setup().await;
    configure_error_limit(&s, Some(2));
    fail_query_tag_writes(&s);
    let base = s.base.clone();
    let (id, first, second) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(
                ctx.conn(),
                "synthetic failures",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            let mut query = future_query("0");
            query.tag_import_options = TagImportOptions {
                services: vec![(
                    hex::encode(hydrus_core::service::builtin_keys::MY_TAGS),
                    ServiceTagImportOptions {
                        additional_tags: vec!["outer failure".into()],
                        ..Default::default()
                    },
                )],
            };
            let first = subs::add_query(ctx.conn(), id, &query, 0)?;
            query.query_text = "1".into();
            let second = subs::add_query(ctx.conn(), id, &query, 0)?;
            queues::add_file_seeds(
                ctx.conn(),
                first,
                &[pending_seed(format!("{base}/post/12"))],
                false,
                0,
            )?;
            queues::add_file_seeds(
                ctx.conn(),
                second,
                &[
                    pending_seed(format!("{base}/post/13")),
                    pending_seed(format!("{base}/post/14")),
                ],
                false,
                0,
            )?;
            Ok((id, first, second))
        })
        .unwrap();
    let before = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    let after = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let recorded = hydrus_testkit::fixture_json("subscription_failure_limit.json");
    assert_eq!(
        report.file_errors,
        recorded["cases"][0]["file_errors"].as_u64().unwrap()
    );
    assert_eq!(report.files_worked, 2);
    assert!(
        after - before >= 10,
        "two escaped failures have a five-second throttle"
    );
    assert_eq!(post_ids(&s.store, first), [(12, SeedStatus::Error)]);
    assert_eq!(
        post_ids(&s.store, second),
        [(13, SeedStatus::Error), (14, SeedStatus::Unknown)]
    );
    let persisted = s
        .store
        .read(|conn| subs::subscription(conn, id))
        .unwrap()
        .unwrap();
    assert_eq!(
        persisted.settings.no_work_until_reason,
        recorded["cases"][0]["reason"]
    );
    assert!((after + 36..=after + 38).contains(&persisted.settings.no_work_until));
    assert!(report.notices[0].contains(recorded["cases"][0]["messages"][1].as_str().unwrap()));
    s.store
        .write(move |ctx| {
            ctx.conn().execute_batch("DROP TRIGGER fail_outer_tag;")?;
            let mut sub = subs::subscription(ctx.conn(), id)?.unwrap();
            sub.settings.no_work_until = 0;
            sub.settings.no_work_until_reason.clear();
            subs::set_subscription_settings(ctx.conn(), id, &sub.settings)
        })
        .unwrap();
    let next = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!(next.file_errors, 0);
    assert_eq!(next.files_worked, 1);
    assert_eq!(
        post_ids(&s.store, second)[1],
        (14, SeedStatus::SuccessfulAndNew)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn none_disables_abandonment_for_real_escaped_store_failures() {
    let s = setup().await;
    configure_error_limit(&s, None);
    fail_query_tag_writes(&s);
    let base = s.base.clone();
    let (id, queue) = s
        .store
        .write(move |ctx| {
            let id = subs::create_subscription(
                ctx.conn(),
                "unlimited failures",
                &SubscriptionSettings::default(),
            )?
            .unwrap();
            let mut query = future_query("0");
            query.tag_import_options = TagImportOptions {
                services: vec![(
                    hex::encode(hydrus_core::service::builtin_keys::MY_TAGS),
                    ServiceTagImportOptions {
                        additional_tags: vec!["outer failure".into()],
                        ..Default::default()
                    },
                )],
            };
            let queue = subs::add_query(ctx.conn(), id, &query, 0)?;
            queues::add_file_seeds(
                ctx.conn(),
                queue,
                &[
                    pending_seed(format!("{base}/post/15")),
                    pending_seed(format!("{base}/post/16")),
                    pending_seed(format!("{base}/post/17")),
                ],
                false,
                0,
            )?;
            Ok((id, queue))
        })
        .unwrap();
    let report = s
        .downloader
        .run_subscription(id, &Job::new())
        .await
        .unwrap();
    assert_eq!((report.files_worked, report.file_errors), (3, 3));
    assert!(report.notices.is_empty());
    assert!(
        post_ids(&s.store, queue)
            .iter()
            .all(|(_, status)| *status == SeedStatus::Error)
    );
    assert_eq!(
        s.store
            .read(|conn| subs::subscription(conn, id))
            .unwrap()
            .unwrap()
            .settings
            .no_work_until,
        0
    );
}

fn runner_subscriptions(s: &Setup) -> Vec<i64> {
    s.store
        .write(|ctx| {
            ["sub 1", "sub 2", "sub 10"]
                .into_iter()
                .map(|name| {
                    let settings = SubscriptionSettings {
                        gug_key: GUG_KEY.into(),
                        gug_name: GUG_NAME.into(),
                        ..SubscriptionSettings::default()
                    };
                    let id = subs::create_subscription(ctx.conn(), name, &settings)?.unwrap();
                    let query = QueryState::new(name.replace(' ', "_"));
                    subs::add_query(ctx.conn(), id, &query, 0)?;
                    Ok(id)
                })
                .collect()
        })
        .unwrap()
}

fn runner_limit(s: &Setup, limit: u32) {
    s.store
        .write(move |ctx| {
            let mut settings: hydrus_store::network::NetworkSettings =
                hydrus_store::settings::get(ctx.conn())?;
            settings.max_simultaneous_subscriptions = limit;
            settings.process_subs_in_random_order = false;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

async fn wait_runner(mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while !ready() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("subscription runner did not reach the expected state");
}

// leaf: audit-options-downloading-subscriptions-maximum-number-of-subscriptions-that-can-sync-simultaneously
#[tokio::test(flavor = "multi_thread")]
async fn subscription_runner_bounds_real_http_overlap_and_reloads_live_limit() {
    use hydrus_download::subscriptions::SubscriptionRunner;
    let s = setup().await;
    let recording = hydrus_testkit::fixture_json("subscription_concurrency.json");
    let ids = runner_subscriptions(&s);
    let held = Arc::new(tokio::sync::Notify::new());
    *s.site.hold.lock() = Some(Arc::clone(&held));
    runner_limit(&s, 1);
    let runner = SubscriptionRunner::new(Arc::clone(&s.downloader));
    runner.start();
    runner.start(); // Starting twice must not create a second scheduler.
    wait_runner(|| s.site.hits.lock().contains_key("search/sub_1/1")).await;
    assert_eq!(runner.status().active.len(), 1);
    assert_eq!(runner.status().active[0].0, ids[0]);
    assert_eq!(recording["cases"][0]["chosen"], "sub 1");
    runner_limit(&s, 2);
    runner.wake();
    wait_runner(|| s.site.hits.lock().contains_key("search/sub_2/1")).await;
    assert_eq!(runner.status().active.len(), 2, "real requests overlap");
    assert_eq!(recording["cases"][2]["chosen"], "sub 2");
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert_eq!(
        s.site.hits.lock().len(),
        2,
        "limit prevents a third request"
    );
    runner_limit(&s, 1);
    runner.wake();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert_eq!(
        runner.status().active.len(),
        2,
        "lowering leaves jobs running"
    );
    assert_eq!(s.site.hits.lock().len(), 2);
    assert!(recording["cases"][5]["chosen"].is_null());
    s.store
        .write(|ctx| {
            let mut pauses: hydrus_store::settings::Pauses =
                hydrus_store::settings::get(ctx.conn())?;
            pauses.subscriptions = true;
            hydrus_store::settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
    runner_limit(&s, 3);
    runner.wake();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert_eq!(
        s.site.hits.lock().len(),
        2,
        "global pause prevents admission"
    );
    assert!(recording["cases"][6]["chosen"].is_null());
    s.store
        .write(|ctx| {
            let mut pauses: hydrus_store::settings::Pauses =
                hydrus_store::settings::get(ctx.conn())?;
            pauses.subscriptions = false;
            hydrus_store::settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
    runner.wake();
    wait_runner(|| s.site.hits.lock().contains_key("search/sub_10/1")).await;
    assert_eq!(runner.status().active.len(), 3);
    assert_eq!(recording["cases"][4]["chosen"], "sub 10");
    assert!(
        s.site.hits.lock().values().all(|count| *count == 1),
        "each id is single-flight"
    );
    *s.site.hold.lock() = None;
    held.notify_waiters();
    wait_runner(|| runner.status().active.is_empty()).await;
    assert!(runner.status().running.is_none());
    assert_eq!(s.site.hits.lock().len(), 3);
    runner.shutdown();
    tokio::time::timeout(std::time::Duration::from_secs(15), runner.wait_stopped())
        .await
        .unwrap();
    runner.wake();
    runner.start();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(runner.status().active.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn subscription_runner_cancels_selected_peer_and_joins_all_on_shutdown() {
    use hydrus_download::subscriptions::SubscriptionRunner;
    let s = setup().await;
    let ids = runner_subscriptions(&s);
    let held = Arc::new(tokio::sync::Notify::new());
    *s.site.hold.lock() = Some(Arc::clone(&held));
    runner_limit(&s, 3);
    let runner = SubscriptionRunner::new(Arc::clone(&s.downloader));
    runner.start();
    wait_runner(|| s.site.hits.lock().len() == 3).await;
    assert_eq!(runner.status().active.len(), 3);
    assert!(!runner.cancel(-1));
    assert!(runner.cancel(ids[1]));
    wait_runner(|| runner.status().active.len() == 2).await;
    let active: Vec<_> = runner
        .status()
        .active
        .into_iter()
        .map(|row| row.0)
        .collect();
    assert_eq!(active, [ids[0], ids[2]], "cancellation is id-scoped");
    let cancelled = s
        .store
        .read(|conn| subs::subscription(conn, ids[1]))
        .unwrap()
        .unwrap();
    assert!(
        cancelled.settings.no_work_until > hydrus_core::time::TimestampMs::now().millis() / 1000
    );
    runner.shutdown();
    tokio::time::timeout(std::time::Duration::from_secs(15), runner.wait_stopped())
        .await
        .unwrap();
    assert!(runner.status().active.is_empty());
    assert!(s.site.hits.lock().values().all(|count| *count == 1));
    *s.site.hold.lock() = None;
    held.notify_waiters();
}

// leaf: audit-network-pause-nudge
#[tokio::test(flavor = "multi_thread")]
async fn the_clients_nudge_subscriptions_awake_is_consumed_and_wakes_the_runner() {
    use hydrus_download::QueueRunner;
    use hydrus_download::subscriptions::SubscriptionRunner;
    use hydrus_store::queues;
    let s = setup().await;
    let ids = runner_subscriptions(&s);
    let held = Arc::new(tokio::sync::Notify::new());
    *s.site.hold.lock() = Some(Arc::clone(&held));
    runner_limit(&s, 1);
    let runner = SubscriptionRunner::new(Arc::clone(&s.downloader));
    runner.start();
    wait_runner(|| s.site.hits.lock().contains_key("search/sub_1/1")).await;
    assert_eq!(runner.status().active.len(), 1);
    // the daemon's queue runner, with nothing waiting for it
    let queue_runner = QueueRunner::new(Arc::clone(&s.downloader), 60);
    assert!(!queue_runner.take_nudges(Some(&runner)).unwrap());
    // the client's menu entry writes the nudge; room for another sub is made
    runner_limit(&s, 2);
    s.store
        .write(|ctx| queues::nudge(ctx.conn(), queues::SUBSCRIPTIONS_NUDGE))
        .unwrap();
    assert!(s.store.read(queues::any_nudged).unwrap());
    assert!(queue_runner.take_nudges(Some(&runner)).unwrap());
    // taken, and the runner looked again at once
    assert!(!s.store.read(queues::any_nudged).unwrap());
    wait_runner(|| s.site.hits.lock().contains_key("search/sub_2/1")).await;
    assert_eq!(runner.status().active.len(), 2);
    assert_eq!(runner.status().active[0].0, ids[0]);
    *s.site.hold.lock() = None;
    held.notify_waiters();
    wait_runner(|| runner.status().active.is_empty()).await;
    runner.shutdown();
    tokio::time::timeout(std::time::Duration::from_secs(15), runner.wait_stopped())
        .await
        .unwrap();
}
