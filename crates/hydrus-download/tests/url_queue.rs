//! A URL queue against a local booru-like site: post pages parsed for
//! their file and tags, files downloaded and imported, what was learned
//! written to the files, known URLs not fetched again.

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
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_parse::Downloaders;
use hydrus_parse::content::{ContentKind, ContentParser, PageParser};
use hydrus_parse::formula::{Formula, FormulaKind, HtmlContent, HtmlRule, HtmlWalk, TagSearch};
use hydrus_store::Store;
use hydrus_store::queues::{self, SeedStatus};

#[derive(Default)]
struct Site {
    hits: Mutex<HashMap<String, usize>>,
    /// Holds gallery page 9 open until notified.
    release: tokio::sync::Notify,
}

fn media(name: &str) -> Vec<u8> {
    std::fs::read(hydrus_testkit::fixture_path(format!("media/{name}"))).unwrap()
}

async fn post(State(site): State<Arc<Site>>, Path(id): Path<String>) -> Response {
    *site.hits.lock().entry(format!("post/{id}")).or_default() += 1;
    if id.starts_with("slow-") {
        site.release.notified().await;
    }
    if id == "404" {
        return (StatusCode::NOT_FOUND, "no such post").into_response();
    }
    if id == "403" {
        return (StatusCode::FORBIDDEN, "log in first").into_response();
    }
    if id == "500" {
        return (StatusCode::INTERNAL_SERVER_ERROR, "the site broke").into_response();
    }
    let html = format!(
        r#"<html><head><title>post {id}</title></head><body>
        <ul id="tags"><li class="tag">blue eyes</li><li class="tag">creator:someone</li><li class="tag">post {id}</li></ul>
        <img id="image" src="/files/{id}.jpg">
        <a class="source" href="https://elsewhere.example/art/{id}">source</a>
        </body></html>"#
    );
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn gallery(State(site): State<Arc<Site>>, Path(page): Path<u32>) -> Response {
    *site
        .hits
        .lock()
        .entry(format!("gallery/{page}"))
        .or_default() += 1;
    // (a slow page)
    if page == 9 {
        site.release.notified().await;
    }
    if page == 404 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let posts: &[u32] = match page {
        1 => &[1, 2],
        2 => &[3],
        _ => &[],
    };
    let links = posts.iter().fold(String::new(), |mut html, p| {
        use std::fmt::Write as _;
        let _ = write!(html, r#"<a class="thumb" href="/post/{p}">post {p}</a>"#);
        html
    });
    let next = if page == 1 {
        r#"<a class="next" href="/gallery/2">next</a>"#
    } else {
        ""
    };
    let html = format!("<html><body>{links}{next}</body></html>");
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn file(State(site): State<Arc<Site>>, Path(name): Path<String>) -> Response {
    *site.hits.lock().entry(format!("files/{name}")).or_default() += 1;
    let bytes = match name.as_str() {
        "1.jpg" => media("jpeg_420.jpg"),
        "2.jpg" => media("jpeg_444_q95.jpg"),
        "3.jpg" => media("jpeg_flat.jpg"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
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

fn booru_parser() -> PageParser {
    PageParser {
        reference_auxiliary: None,
        name: "local booru post".into(),
        key: "ab".into(),
        converter: hydrus_core::url::StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: vec![
            ContentParser {
                name: "file".into(),
                kind: ContentKind::Url {
                    url_type: 7,
                    priority: 50,
                },
                formula: html_formula(
                    "img",
                    &[("id", "image")],
                    HtmlContent::Attribute("src".into()),
                ),
            },
            ContentParser {
                name: "source".into(),
                kind: ContentKind::Url {
                    url_type: 8,
                    priority: 50,
                },
                formula: html_formula(
                    "a",
                    &[("class", "source")],
                    HtmlContent::Attribute("href".into()),
                ),
            },
            ContentParser {
                name: "tags".into(),
                kind: ContentKind::Tag { namespace: None },
                formula: html_formula("li", &[("class", "tag")], HtmlContent::Text),
            },
        ],
        example_urls: Vec::new(),
    }
}

fn gallery_parser() -> PageParser {
    PageParser {
        reference_auxiliary: None,
        name: "local booru gallery".into(),
        key: "ac".into(),
        converter: hydrus_core::url::StringConverter::default(),
        subsidiary: Vec::new(),
        content_parsers: vec![
            ContentParser {
                name: "posts".into(),
                kind: ContentKind::Url {
                    url_type: 7,
                    priority: 50,
                },
                formula: html_formula(
                    "a",
                    &[("class", "thumb")],
                    HtmlContent::Attribute("href".into()),
                ),
            },
            ContentParser {
                name: "next".into(),
                kind: ContentKind::Url {
                    url_type: 6,
                    priority: 50,
                },
                formula: html_formula(
                    "a",
                    &[("class", "next")],
                    HtmlContent::Attribute("href".into()),
                ),
            },
        ],
        example_urls: Vec::new(),
    }
}

fn gallery_class(host: &str) -> UrlClass {
    UrlClass {
        name: "local booru gallery".into(),
        key: vec![0xce],
        url_type: UrlType::Gallery,
        preferred_scheme: "http".into(),
        domain_mask: DomainMask::new(vec![host.to_owned()], vec![], false, false),
        path_components: vec![
            (StringMatch::fixed("gallery"), None),
            (StringMatch::any(), None),
        ],
        example_url: format!("http://{host}/gallery/1"),
        ..UrlClass::default()
    }
}

fn post_class(host: &str) -> UrlClass {
    UrlClass {
        name: "local booru post".into(),
        key: vec![0xcd],
        url_type: UrlType::Post,
        preferred_scheme: "http".into(),
        domain_mask: DomainMask::new(vec![host.to_owned()], vec![], false, false),
        path_components: vec![
            (StringMatch::fixed("post"), None),
            (StringMatch::any(), None),
        ],
        example_url: format!("http://{host}/post/1"),
        ..UrlClass::default()
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
        .route("/post/{id}", get(post))
        .route("/files/{name}", get(file))
        .route("/gallery/{page}", get(gallery))
        .with_state(Arc::clone(&site));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let host = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let class = post_class(&host);
    let gallery = gallery_class(&host);
    let settings = UrlClassSettings {
        parser_links: vec![
            (hex::encode(&class.key), Some("ab".into())),
            (hex::encode(&gallery.key), Some("ac".into())),
        ],
        parser_keys: vec!["ab".into(), "ac".into()],
        url_classes: vec![class, gallery],
        collapse_leading_slashes: false,
    };
    let downloaders = Downloaders {
        parsers: vec![booru_parser(), gallery_parser()],
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

async fn wait_until_done(store: &Store, queue: i64) {
    for _ in 0..400 {
        let pending = store
            .read(|conn| queues::next_file_seed(conn, queue))
            .unwrap();
        if pending.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the queue did not finish");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_url_queue_downloads_posts_and_files() {
    let s = setup().await;
    s.runner.start_all().unwrap();
    let queue = s
        .runner
        .url_queue_for(Some("my downloads"), None, None)
        .unwrap();
    let urls = vec![
        format!("{}/post/1", s.base),
        format!("{}/post/1", s.base),
        format!("{}/post/404", s.base),
        format!("{}/files/2.jpg", s.base),
    ];
    let added = s
        .runner
        .pend_urls(queue.id, &urls, &BTreeSet::new(), &[])
        .unwrap();
    assert_eq!(added, 3);
    wait_until_done(&s.store, queue.id).await;

    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue.id))
        .unwrap();
    let outcome: Vec<(String, SeedStatus, String)> = seeds
        .iter()
        .map(|seed| {
            (
                seed.data.replace(&s.base, ""),
                seed.status,
                seed.note.clone(),
            )
        })
        .collect();
    assert_eq!(
        outcome,
        vec![
            (
                "/post/1".to_owned(),
                SeedStatus::SuccessfulAndNew,
                String::new()
            ),
            ("/post/404".to_owned(), SeedStatus::Vetoed, "404".to_owned()),
            (
                "/files/2.jpg".to_owned(),
                SeedStatus::SuccessfulAndNew,
                String::new()
            ),
        ]
    );
    let post_seed = &seeds[0];
    assert_eq!(
        post_seed.meta.tags,
        ["blue eyes", "creator:someone", "post 1"]
            .map(str::to_owned)
            .into()
    );
    assert!(
        post_seed
            .meta
            .source_urls
            .contains("https://elsewhere.example/art/1")
    );

    // what the file got: downloader tags, its URLs, the site's time
    let hash = post_seed.meta.hash("sha256").unwrap().to_owned();
    let snapshot = s.store.snapshot();
    let (urls, tags) = s
        .store
        .read(|conn| {
            let hash = hydrus_core::Sha256::from_slice(&hex::decode(&hash).unwrap()).unwrap();
            let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
            let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
            let result = &batch.results[0];
            let downloader_tags = snapshot
                .services
                .builtin(hydrus_core::service::builtin_keys::DOWNLOADER_TAGS)?
                .id;
            let tags: BTreeSet<String> = result.tags[&downloader_tags].by_status
                [&hydrus_core::ContentStatus::Current]
                .iter()
                .map(|t| batch.tags[t].as_str().to_owned())
                .collect();
            Ok((result.urls.clone(), tags))
        })
        .unwrap();
    assert_eq!(
        tags,
        ["blue eyes", "creator:someone", "post 1"]
            .map(str::to_owned)
            .into()
    );
    let urls: BTreeSet<String> = urls.into_iter().map(|u| u.replace(&s.base, "")).collect();
    assert_eq!(
        urls,
        ["/post/1", "/files/1.jpg", "https://elsewhere.example/art/1"]
            .map(str::to_owned)
            .into()
    );

    // a second queue with the same post knows it by URL: no fetch
    let other = s.runner.url_queue_for(Some("again"), None, None).unwrap();
    s.runner
        .pend_urls(
            other.id,
            &[format!("{}/post/1", s.base)],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    wait_until_done(&s.store, other.id).await;
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, other.id))
        .unwrap();
    assert_eq!(seeds[0].status, SeedStatus::SuccessfulButRedundant);
    assert!(
        seeds[0].note.starts_with("url recognised: Imported at"),
        "{}",
        seeds[0].note
    );
    assert_eq!(s.site.hits.lock()["post/1"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_gallery_url_in_a_url_queue_queues_its_posts() {
    let s = setup().await;
    s.runner.start_all().unwrap();
    let queue = s.runner.url_queue_for(None, None, None).unwrap();
    s.runner
        .pend_urls(
            queue.id,
            &[format!("{}/gallery/1", s.base)],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    // the gallery page makes post seeds, which then download
    for _ in 0..400 {
        let seeds = s
            .store
            .read(|conn| queues::file_seeds(conn, queue.id))
            .unwrap();
        if seeds.len() == 2 && seeds.iter().all(|seed| seed.status != SeedStatus::Unknown) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let pages = s
        .store
        .read(|conn| queues::gallery_seeds(conn, queue.id))
        .unwrap();
    assert_eq!(pages.len(), 1, "URL queues don't follow next pages");
    assert_eq!(pages[0].status, SeedStatus::SuccessfulAndNew);
    assert_eq!(pages[0].note, "2 new urls found");
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue.id))
        .unwrap();
    let outcome: Vec<(String, SeedStatus, Option<String>)> = seeds
        .iter()
        .map(|seed| {
            (
                seed.data.replace(&s.base, ""),
                seed.status,
                seed.referral_url.as_ref().map(|r| r.replace(&s.base, "")),
            )
        })
        .collect();
    assert_eq!(
        outcome,
        vec![
            (
                "/post/1".to_owned(),
                SeedStatus::SuccessfulAndNew,
                Some("/gallery/1".to_owned())
            ),
            (
                "/post/2".to_owned(),
                SeedStatus::SuccessfulAndNew,
                Some("/gallery/1".to_owned())
            ),
        ]
    );
    // (the gallery page is the seeds' referral URL, so, as in the
    // reference, not also one of their primary URLs)
    assert!(
        !seeds[0]
            .meta
            .primary_urls
            .contains(&format!("{}/gallery/1", s.base))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn files_import_while_later_gallery_pages_are_read() {
    let s = setup().await;
    s.runner.start_all().unwrap();
    let queue = s.runner.url_queue_for(None, None, None).unwrap();
    // a gallery page, and a second that the site holds open
    s.runner
        .pend_urls(
            queue.id,
            &[
                format!("{}/gallery/1", s.base),
                format!("{}/gallery/9", s.base),
            ],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    // the second page is being read (and held)...
    for _ in 0..400 {
        if s.site.hits.lock().contains_key("gallery/9") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(s.site.hits.lock().get("gallery/9"), Some(&1));
    // ...and the first page's posts import meanwhile, as the reference's
    // file and gallery work run side by side
    let mut imported = false;
    for _ in 0..400 {
        let seeds = s
            .store
            .read(|conn| queues::file_seeds(conn, queue.id))
            .unwrap();
        if seeds.len() == 2
            && seeds
                .iter()
                .all(|seed| seed.status == SeedStatus::SuccessfulAndNew)
        {
            imported = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(imported, "the first page's files imported");
    let pages = s
        .store
        .read(|conn| queues::gallery_seeds(conn, queue.id))
        .unwrap();
    assert_eq!(pages[1].status, SeedStatus::Unknown, "still being read");
    s.site.release.notify_one();
}

#[tokio::test(flavor = "multi_thread")]
async fn queues_wait_while_their_downloads_are_paused_globally() {
    use hydrus_store::settings::Pauses;
    let s = setup().await;
    // hydrus's "pause all file import queues"
    let paused = Pauses {
        file_queues: true,
        ..Pauses::default()
    };
    s.store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &paused))
        .unwrap();
    s.runner.start_all().unwrap();
    let queue = s
        .runner
        .url_queue_for(Some("my downloads"), None, None)
        .unwrap();
    let urls = vec![format!("{}/post/1", s.base)];
    s.runner
        .pend_urls(queue.id, &urls, &BTreeSet::new(), &[])
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let waiting = s
        .store
        .read(|conn| queues::next_file_seed(conn, queue.id))
        .unwrap();
    assert!(waiting.is_some(), "nothing was downloaded");
    assert!(s.site.hits.lock().is_empty());
    // resumed: the queue gets on with it
    s.store
        .write(|ctx| hydrus_store::settings::set(ctx.conn(), &Pauses::default()))
        .unwrap();
    s.runner.wake(queue.id);
    wait_until_done(&s.store, queue.id).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_that_fails_does_not_hold_up_the_rest_of_its_queue() {
    let s = setup().await;
    s.runner.start_all().unwrap();
    let queue = s
        .runner
        .url_queue_for(Some("my downloads"), None, None)
        .unwrap();
    let urls = vec![format!("{}/post/500", s.base), format!("{}/post/1", s.base)];
    s.runner
        .pend_urls(queue.id, &urls, &BTreeSet::new(), &[])
        .unwrap();
    wait_until_done(&s.store, queue.id).await;
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue.id))
        .unwrap();
    let statuses: Vec<SeedStatus> = seeds.iter().map(|seed| seed.status).collect();
    assert_eq!(
        statuses,
        [SeedStatus::Error, SeedStatus::SuccessfulAndNew],
        "{seeds:?}"
    );
    assert!(seeds[0].note.contains("500"), "{}", seeds[0].note);
    assert_eq!(s.runner.status(queue.id).delayed_until, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_from_a_site_hydrus_logged_in_to_says_why() {
    let s = setup().await;
    let host = s.base.trim_start_matches("http://").to_owned();
    let logins = hydrus_store::network::LoginDomains(vec![host]);
    s.store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &logins))
        .unwrap();
    s.runner.start_all().unwrap();
    let queue = s
        .runner
        .url_queue_for(Some("my downloads"), None, None)
        .unwrap();
    let urls = vec![format!("{}/post/403", s.base)];
    s.runner
        .pend_urls(queue.id, &urls, &BTreeSet::new(), &[])
        .unwrap();
    wait_until_done(&s.store, queue.id).await;
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue.id))
        .unwrap();
    assert_eq!(seeds[0].status, SeedStatus::Vetoed);
    assert!(seeds[0].note.contains("login script"), "{}", seeds[0].note);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queue_with_no_import_destination_pauses_its_files() {
    use hydrus_core::import_options::{ImportOptionsSlice, LocationOptions};
    let s = setup().await;
    s.runner.start_all().unwrap();
    let options = ImportOptionsSlice {
        locations: Some(LocationOptions {
            destinations: Vec::new(),
            ..LocationOptions::default()
        }),
        ..ImportOptionsSlice::default()
    };
    let queue = s
        .runner
        .url_queue_for(Some("nowhere"), None, Some(&options))
        .unwrap();
    let urls = vec![format!("{}/post/1", s.base)];
    s.runner
        .pend_urls(queue.id, &urls, &BTreeSet::new(), &[])
        .unwrap();
    let mut paused = false;
    for _ in 0..100 {
        let q = s.store.read(|conn| queues::queue(conn, queue.id)).unwrap();
        if q.is_some_and(|q| q.files_paused) {
            paused = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(paused, "the queue's files paused");
    // the URL waits, untouched
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue.id))
        .unwrap();
    assert_eq!(seeds[0].status, SeedStatus::Unknown);
    assert!(s.site.hits.lock().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_files_download_again_in_their_own_queue() {
    let s = setup().await;
    s.runner.start_all().unwrap();
    // (as the reference's integrity checks send a bad file's URLs)
    let added = s
        .runner
        .redownload(&[format!("{}/post/1", s.base), "not a url".to_owned()])
        .unwrap();
    assert_eq!(added, 1);
    let all = s
        .store
        .read(|conn| queues::queues(conn, Some(queues::QueueKind::Urls)))
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].name,
        hydrus_import::maintenance::REDOWNLOAD_PAGE_NAME
    );
    wait_until_done(&s.store, all[0].id).await;
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, all[0].id))
        .unwrap();
    assert_eq!(seeds.len(), 1);
    assert_eq!(seeds[0].status, SeedStatus::SuccessfulAndNew);
    // later ones join it
    s.runner
        .redownload(&[format!("{}/post/2", s.base)])
        .unwrap();
    let all = s
        .store
        .read(|conn| queues::queues(conn, Some(queues::QueueKind::Urls)))
        .unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_downloader_uses_edited_parser_and_new_link_after_reload() {
    let s = setup().await;
    let existing = s.runner.downloader().clone();
    assert!(existing.definitions().parser("edited").is_none());
    let mut edited = booru_parser();
    edited.key = "ee".repeat(32);
    edited.content_parsers[2].kind = ContentKind::Tag {
        namespace: Some("edited".into()),
    };
    let text =
        hydrus_downloader_exchange::encode_text(&[hydrus_downloader_exchange::Definition::new(
            hydrus_downloader_exchange::Native::Page(edited),
        )])
        .unwrap();
    let definitions = hydrus_downloader_exchange::decode_text(&text).unwrap();
    let hydrus_downloader_exchange::Native::Page(edited) = definitions[0].native.clone() else {
        unreachable!()
    };
    let imported_key = edited.key.clone();
    s.store
        .write_and_refresh(move |ctx| {
            let conn = ctx.conn();
            let mut definitions: Downloaders = hydrus_store::settings::get(conn)?;
            let edited_key = edited.key.clone();
            definitions.parsers.push(edited);
            let mut classes: UrlClassSettings = hydrus_store::settings::get(conn)?;
            classes.parser_keys.push(edited_key.clone());
            classes.parser_links[0].1 = Some(edited_key);
            hydrus_store::settings::set(conn, &definitions)?;
            hydrus_store::settings::set(conn, &classes)
        })
        .unwrap();
    assert!(s.runner.reload_settings().unwrap());
    assert!(existing.definitions().parser(&imported_key).is_some());
    s.runner.start_all().unwrap();
    let queue = s
        .runner
        .url_queue_for(Some("edited parser"), None, None)
        .unwrap();
    s.runner
        .pend_urls(
            queue.id,
            &[format!("{}/post/1", s.base)],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    wait_until_done(&s.store, queue.id).await;
    let seeds = s
        .store
        .read(|conn| queues::file_seeds(conn, queue.id))
        .unwrap();
    assert_eq!(seeds[0].status, SeedStatus::SuccessfulAndNew);
    assert_eq!(
        seeds[0].meta.tags,
        [
            "edited:blue eyes",
            "edited:creator:someone",
            "edited:post 1"
        ]
        .map(str::to_owned)
        .into()
    );
}

async fn held_posts_started(site: &Site, expected: usize) {
    for _ in 0..200 {
        let count: usize = site
            .hits
            .lock()
            .iter()
            .filter(|(url, _)| url.starts_with("post/slow-"))
            .map(|(_, count)| count)
            .sum();
        if count == expected {
            return;
        }
        assert!(count < expected, "a slot limit admitted too much work");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("held post work did not start");
}
async fn pending_files(runner: &QueueRunner, queue: i64) {
    for _ in 0..200 {
        if runner.status(queue).files_status == "pending" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("queue did not wait for an importer permit");
}
fn file_work_queue(s: &Setup, kind: queues::QueueKind, name: &str) -> i64 {
    let name = name.to_owned();
    s.store
        .write(move |ctx| {
            let id = queues::create_queue(
                ctx.conn(),
                kind,
                &name,
                None,
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                0,
            )?;
            if kind == queues::QueueKind::Watcher {
                let mut state = hydrus_core::watchers::WatcherState::new(
                    "https://watcher.example/thread/1",
                    hydrus_core::subscriptions::CheckerOptions::default(),
                    0,
                );
                state.checking_paused = true;
                queues::set_queue_extra(ctx.conn(), id, &serde_json::to_value(state).unwrap())?;
            }
            Ok(id)
        })
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn running_file_queues_keep_limits_when_lowered_and_release_on_cancel_or_owner_close() {
    use hydrus_store::settings::{self, ImportWorkSlots};
    for kind in [
        queues::QueueKind::Gallery,
        queues::QueueKind::Watcher,
        queues::QueueKind::Urls,
    ] {
        let s = setup().await;
        let set_limit = |limit| {
            s.store
                .write(move |ctx| {
                    settings::set(
                        ctx.conn(),
                        &ImportWorkSlots {
                            gallery_files: limit,
                            gallery_search: 15,
                            watcher_files: limit,
                            watcher_check: 15,
                            misc: limit,
                        },
                    )
                })
                .unwrap();
        };
        set_limit(2);
        let queues: Vec<_> = (0..4)
            .map(|index| file_work_queue(&s, kind, &format!("slot queue {index}")))
            .collect();
        for (index, &queue) in queues.iter().enumerate() {
            s.runner
                .pend_urls(
                    queue,
                    &[format!("{}/post/slow-{index}", s.base)],
                    &BTreeSet::new(),
                    &[],
                )
                .unwrap();
            if index == 2 {
                break;
            }
        }
        s.runner.start_all().unwrap();
        held_posts_started(&s.site, 2).await;
        let pending = queues
            .iter()
            .copied()
            .take(3)
            .find(|&queue| {
                s.runner
                    .live()
                    .iter()
                    .find(|(id, _)| *id == queue)
                    .is_some_and(|(_, live)| live.file_job.is_none())
            })
            .unwrap();
        pending_files(&s.runner, pending).await;
        set_limit(1);
        s.runner.reload_settings().unwrap();
        let running: Vec<_> = queues
            .iter()
            .copied()
            .take(3)
            .filter(|queue| *queue != pending)
            .collect();
        s.runner
            .cancel(running[0], hydrus_store::live::JobKind::File);
        for _ in 0..200 {
            if s.runner
                .live()
                .iter()
                .find(|(id, _)| *id == running[0])
                .is_some_and(|(_, live)| live.file_job.is_none())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        pending_files(&s.runner, pending).await;
        held_posts_started(&s.site, 2).await;
        s.store
            .write(move |ctx| queues::set_paused(ctx.conn(), pending, Some(true), None))
            .unwrap();
        s.runner
            .cancel(running[1], hydrus_store::live::JobKind::File);
        for _ in 0..200 {
            if s.runner
                .live()
                .iter()
                .find(|(id, _)| *id == pending)
                .is_some_and(|(_, live)| live.files_status.is_empty())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            s.runner
                .live()
                .iter()
                .find(|(id, _)| *id == pending)
                .is_some_and(|(_, live)| live.file_job.is_none() && live.files_status.is_empty())
        );
        held_posts_started(&s.site, 2).await;
        s.store
            .write(move |ctx| queues::set_paused(ctx.conn(), pending, Some(false), None))
            .unwrap();
        s.runner.wake(pending);
        held_posts_started(&s.site, 3).await;
        s.runner
            .pend_urls(
                queues[3],
                &[format!("{}/post/slow-3", s.base)],
                &BTreeSet::new(),
                &[],
            )
            .unwrap();
        pending_files(&s.runner, queues[3]).await;
        s.store
            .write(move |ctx| queues::set_page_closed(ctx.conn(), pending, true))
            .unwrap();
        held_posts_started(&s.site, 4).await;
        let retired = queues[3];
        s.store
            .write(move |ctx| queues::delete_queue(ctx.conn(), retired))
            .unwrap();
        let successor = file_work_queue(&s, kind, "slot queue 3");
        assert!(successor > retired);
        let url = format!("{}/post/slow-successor", s.base);
        s.runner
            .pend_urls(successor, std::slice::from_ref(&url), &BTreeSet::new(), &[])
            .unwrap();
        held_posts_started(&s.site, 5).await;
        let successor_seeds = s
            .store
            .read(|conn| queues::file_seeds(conn, successor))
            .unwrap();
        assert_eq!(successor_seeds.len(), 1);
        assert_eq!(successor_seeds[0].data, url);
        assert_eq!(successor_seeds[0].status, SeedStatus::Unknown);
        s.runner
            .cancel(successor, hydrus_store::live::JobKind::File);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn running_gallery_searches_pick_up_capacity_growth_and_release_on_error() {
    use hydrus_store::settings::{self, ImportWorkSlots};
    let s = setup().await;
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &ImportWorkSlots {
                    gallery_search: 1,
                    ..ImportWorkSlots::default()
                },
            )
        })
        .unwrap();
    let first = file_work_queue(&s, queues::QueueKind::Gallery, "first search");
    let second = file_work_queue(&s, queues::QueueKind::Gallery, "second search");
    for queue in [first, second] {
        s.runner
            .pend_urls(
                queue,
                &[format!("{}/gallery/9", s.base)],
                &BTreeSet::new(),
                &[],
            )
            .unwrap();
    }
    s.runner.start_all().unwrap();
    for _ in 0..200 {
        if s.site.hits.lock().get("gallery/9") == Some(&1) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(s.site.hits.lock().get("gallery/9"), Some(&1));
    for _ in 0..200 {
        if [first, second]
            .iter()
            .any(|&id| s.runner.status(id).gallery_status == "pending")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        [first, second]
            .iter()
            .any(|&id| s.runner.status(id).gallery_status == "pending")
    );
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &ImportWorkSlots {
                    gallery_search: 2,
                    ..ImportWorkSlots::default()
                },
            )
        })
        .unwrap();
    s.runner.reload_settings().unwrap();
    for _ in 0..200 {
        if s.site.hits.lock().get("gallery/9") == Some(&2) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(s.site.hits.lock().get("gallery/9"), Some(&2));
    s.runner.cancel(first, hydrus_store::live::JobKind::Gallery);
    s.runner
        .cancel(second, hydrus_store::live::JobKind::Gallery);
    let failed = file_work_queue(&s, queues::QueueKind::Gallery, "failed search");
    let successor = file_work_queue(&s, queues::QueueKind::Gallery, "successor search");
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &ImportWorkSlots {
                    gallery_search: 1,
                    ..ImportWorkSlots::default()
                },
            )
        })
        .unwrap();
    s.runner
        .pend_urls(
            failed,
            &[format!("{}/gallery/404", s.base)],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    for _ in 0..200 {
        if s.store
            .read(|conn| queues::gallery_seeds(conn, failed))
            .unwrap()
            .iter()
            .any(|seed| seed.status != SeedStatus::Unknown)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        s.store
            .read(|conn| queues::gallery_seeds(conn, failed))
            .unwrap()
            .iter()
            .any(|seed| seed.status != SeedStatus::Unknown)
    );
    s.runner
        .pend_urls(
            successor,
            &[format!("{}/gallery/9", s.base)],
            &BTreeSet::new(),
            &[],
        )
        .unwrap();
    for _ in 0..200 {
        let failed_done = s
            .store
            .read(|conn| queues::gallery_seeds(conn, failed))
            .unwrap()
            .iter()
            .any(|seed| seed.status != SeedStatus::Unknown);
        if failed_done && s.site.hits.lock().get("gallery/9") == Some(&3) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        s.store
            .read(|conn| queues::gallery_seeds(conn, failed))
            .unwrap()
            .iter()
            .any(|seed| seed.status != SeedStatus::Unknown)
    );
    assert_eq!(s.site.hits.lock().get("gallery/9"), Some(&3));
    s.runner
        .cancel(successor, hydrus_store::live::JobKind::Gallery);
}
