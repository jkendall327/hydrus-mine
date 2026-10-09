//! Pages closed by a middle click on their tab and by Ctrl+W, the
//! importer pages asking first, replayed against the reference's
//! (`oracle/fixtures/page_close.json`, from `oracle/record_page_close.py`):
//! a search page, a url import page holding four URLs, a simple
//! downloader page holding two and another search page; tabs clicked,
//! middle-clicked and Ctrl+W pressed, the questions answered with their
//! buttons, and after each step the questions asked, the tabs and the page
//! shown compared.

use std::sync::Arc;

use slint::ComponentHandle as _;
use slint::platform::PointerEventButton;

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedType};
use hydrus_store::sessions::{self, LAST_SESSION};
use serde_json::{Value, json};

use crate::common::widgets;

fn page(name: &str, content: PageContent) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    }
}

fn search(name: &str) -> Page {
    page(
        name,
        PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    )
}

/// URLs waiting in a queue, its files paused.
fn hold(conn: &rusqlite::Connection, queue: i64, urls: &Value) -> hydrus_store::Result<()> {
    let seeds: Vec<NewFileSeed> = urls
        .as_array()
        .unwrap()
        .iter()
        .map(|u| NewFileSeed {
            seed_type: SeedType::Url,
            data: u.as_str().unwrap().into(),
            data_for_comparison: u.as_str().unwrap().into(),
            source_time: None,
            referral_url: None,
            meta: FileSeedMeta::default(),
        })
        .collect();
    queues::add_file_seeds(conn, queue, &seeds, false, 0)?;
    queues::set_paused(conn, queue, Some(true), Some(true))
}

// leaf: audit-options-tabs-close
#[test]
fn pages_close_by_middle_click_and_ctrl_w_asking_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("page_close.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let (urls, simple) = (recorded["urls"].clone(), recorded["simple_urls"].clone());
    let (url_queue, simple_queue) = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let options = ImportOptionsSlice::default();
            let url_queue =
                queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
            hold(conn, url_queue, &urls)?;
            let simple_queue = queues::create_simple_downloader(
                conn,
                None,
                &options,
                &queues::SimpleDownloader::default(),
                0,
            )?;
            hold(conn, simple_queue, &simple)?;
            Ok((url_queue, simple_queue))
        })
        .unwrap();
    assert_eq!(
        store
            .read(|c| queues::queue(c, simple_queue))
            .unwrap()
            .unwrap()
            .kind,
        QueueKind::SimpleDownloader
    );
    let downloader = |name: &str, kind: DownloaderKind, queue: i64| {
        page(
            name,
            PageContent::Downloader {
                kind,
                queues: vec![queue],
                sort: None,
                page: None,
            },
        )
    };
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("a"),
            downloader("url import", DownloaderKind::Urls, url_queue),
            downloader("simple downloader", DownloaderKind::Simple, simple_queue),
            search("c"),
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    // (the reference's options as recorded are the defaults)
    assert_eq!(
        recorded["options"]["confirm_non_empty_downloader_page_close"],
        true
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let win = ui.window();
    widgets::lay_out(win, 1100.0, 700.0);
    let state = || {
        let pages = bound.pages.borrow();
        let tabs = pages
            .tabs()
            .into_iter()
            .next()
            .map(|t| t.names)
            .unwrap_or_default();
        json!({ "tabs": tabs, "current": pages.shown().name })
    };
    let steps = recorded["steps"].as_array().unwrap();
    assert_eq!(state(), steps[0]["state"], "start");
    for step in &steps[1..] {
        let action = step["do"].as_array().unwrap();
        let context = step["do"].to_string();
        // (drawn, so the tabs report where they are for the pointer)
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        let answer = match action[0].as_str().unwrap() {
            "select" => {
                widgets::click(win, action[1].as_str().unwrap());
                None
            }
            "play" => {
                // (the url import page's files resumed, as its sidebar's
                // button does, the page not shown)
                let key = bound
                    .pages
                    .borrow()
                    .session()
                    .pages
                    .iter()
                    .find(|p| p.name == "url import")
                    .unwrap()
                    .key;
                let page = bound.pages.borrow_mut().page(&key).unwrap();
                page.borrow_mut().pause_play_files();
                assert!(!page.borrow().importer().unwrap().paused);
                None
            }
            "middle" => {
                let at = widgets::position(win, action[1].as_str().unwrap(), 0);
                widgets::click_at(win, at, PointerEventButton::Middle);
                action[2].as_bool()
            }
            "ctrl_w" => {
                widgets::ctrl_key(win, "w");
                action[1].as_bool()
            }
            other => panic!("{other}"),
        };
        let mut asked = Vec::new();
        let question = ui.get_question();
        if !question.is_empty() {
            asked.push(question.to_string());
            widgets::click(win, if answer.unwrap() { "yes" } else { "no" });
            assert_eq!(ui.get_question(), "", "{context}");
        }
        assert_eq!(json!(asked), step["asked"], "{context}");
        if step["state"]["tabs"] == json!([]) {
            // the last page closed: the reference's notebook is left
            // empty; hydrus-rs's top notebook always has a page, and a new
            // search page takes its place (DIFFERENCES.md)
            assert_eq!(state(), json!({ "tabs": ["files"], "current": "files" }));
            continue;
        }
        assert_eq!(state(), step["state"], "{context}");
    }
}
