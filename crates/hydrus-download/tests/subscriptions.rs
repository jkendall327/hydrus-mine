//! Subscriptions against a local booru with a tag search: the first sync
//! stops at the initial file limit, a later one catches up on new uploads
//! and stops once it sees what it found before, the files are downloaded
//! with the query's own tags added, and a missing downloader pauses the
//! subscription.

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
}

async fn setup() -> Setup {
    let site = Arc::new(Site::default());
    let app = Router::new()
        .route("/search/{tag}/{page}", get(search))
        .route("/post/{id}", get(post))
        .route("/files/{name}", get(file))
        .with_state(Arc::clone(&site));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
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
    }
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
