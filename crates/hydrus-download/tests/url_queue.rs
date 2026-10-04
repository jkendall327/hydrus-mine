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
