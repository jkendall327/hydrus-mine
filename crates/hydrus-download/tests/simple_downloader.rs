//! A simple downloader against a local site: each page waiting is fetched
//! and parsed by its formula (the reference's default "all files linked by
//! images in page"), the files found queued with the page as their
//! referrer and imported, and the pages logged as the reference logs them
//! ('page checked OK with formula "..." - 2 new urls', "... (2 already in
//! queue)", "page 404").

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_download::{Downloader, QueueRunner};
use hydrus_import::FileImporter;
use hydrus_media::MediaTools;
use hydrus_net::{NetEngine, NetOptions};
use hydrus_store::Store;
use hydrus_store::queues::{self, SeedStatus, SimpleDownloader, SimpleJob};
use hydrus_store::settings::SimpleDownloaderFormulae;

fn media(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../oracle/fixtures/media")
            .join(name),
    )
    .unwrap()
}

async fn page(Path(name): Path<String>) -> Response {
    if name == "missing" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let html = "<html><body><a href=\"/files/1.jpg\"><img src=\"/thumbs/1.jpg\"></a>\
                <a href=\"../files/2.jpg\"><img src=\"t2.jpg\"></a>\
                <img src=\"/thumbs/unlinked.jpg\"></body></html>";
    ([("content-type", "text/html; charset=utf-8")], html).into_response()
}

async fn file(Path(name): Path<String>) -> Response {
    let bytes = match name.as_str() {
        "1.jpg" => media("jpeg_420.jpg"),
        "2.jpg" => media("jpeg_444_q95.jpg"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([("content-type", "image/jpeg")], bytes).into_response()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_simple_downloader_parses_its_pages_for_files() {
    let app = Router::new()
        .route("/pages/{name}", get(page))
        .route("/files/{name}", get(file));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();

    // a new store's formulae are the reference's defaults
    let formulae: SimpleDownloaderFormulae = store.read(hydrus_store::settings::get).unwrap();
    assert_eq!(formulae.favourite, "all files linked by images in page");
    let formula = formulae
        .formulae
        .iter()
        .find(|f| f.name == formulae.favourite)
        .unwrap()
        .clone();
    let job = |name: &str| SimpleJob {
        url: format!("{base}/pages/{name}"),
        formula: formula.clone(),
    };
    let state = SimpleDownloader {
        formula_name: formula.name.clone(),
        pending: vec![job("one"), job("missing"), job("two")],
    };
    let id = store
        .write(move |ctx| {
            queues::create_simple_downloader(
                ctx.conn(),
                None,
                &ImportOptionsSlice::default(),
                &state,
                0,
            )
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
    let runner = QueueRunner::new(downloader, 60);
    runner.start_all().unwrap();

    // every page parsed, every file imported
    for _ in 0..600 {
        let queue = store.read(|c| queues::queue(c, id)).unwrap().unwrap();
        let pending = SimpleDownloader::of(&queue).unwrap().pending.len();
        let files = store.read(|c| queues::next_file_seed(c, id)).unwrap();
        let pages = store.read(|c| queues::gallery_seeds(c, id)).unwrap().len();
        if pending == 0 && pages == 3 && files.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let pages: Vec<(String, SeedStatus, String)> = store
        .read(|c| queues::gallery_seeds(c, id))
        .unwrap()
        .into_iter()
        .map(|s| (s.url, s.status, s.note))
        .collect();
    let name = "all files linked by images in page";
    assert_eq!(
        pages,
        [
            (
                format!("{base}/pages/one"),
                SeedStatus::SuccessfulAndNew,
                format!("page checked OK with formula \"{name}\" - 2 new urls"),
            ),
            (
                format!("{base}/pages/missing"),
                SeedStatus::Vetoed,
                "page 404".to_owned(),
            ),
            (
                format!("{base}/pages/two"),
                SeedStatus::SuccessfulAndNew,
                format!(
                    "page checked OK with formula \"{name}\" - 0 new urls (2 already in queue)"
                ),
            ),
        ]
    );
    let files: Vec<(String, SeedStatus, Option<String>)> = store
        .read(|c| queues::file_seeds(c, id))
        .unwrap()
        .into_iter()
        .map(|s| (s.data, s.status, s.referral_url))
        .collect();
    assert_eq!(
        files,
        [
            (
                format!("{base}/files/1.jpg"),
                SeedStatus::SuccessfulAndNew,
                Some(format!("{base}/pages/one")),
            ),
            (
                format!("{base}/files/2.jpg"),
                SeedStatus::SuccessfulAndNew,
                Some(format!("{base}/pages/one")),
            ),
        ]
    );
}
