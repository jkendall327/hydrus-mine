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
                widgets::click_nth(win, action[1].as_str().unwrap(), 0);
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

/// Closing an importer page of each kind, asked as the reference asks, with
/// the option on and off, replayed from `oracle/record_importer_page_close.py`
/// (`AskIfAbleToClose` over a urls, simple downloader, gallery, watcher and
/// local import page holding three import objects each).
// leaf: audit-options-gui-pages-opening-and-closing-confirm-when-closing-a-non-empty-importer-page
#[test]
fn closing_each_kind_of_importer_page_asks_as_recorded_with_the_option_on_and_not_off() {
    let recorded = hydrus_testkit::fixture_json("importer_page_close.json");
    let held = recorded["items"].as_u64().unwrap();
    assert_eq!(held, 3);
    let urls = json!([
        "https://example.com/0.jpg",
        "https://example.com/1.jpg",
        "https://example.com/2.jpg"
    ]);
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    // (page name, kind, queue kind) as the reference names them
    let kinds = [
        ("url import", DownloaderKind::Urls, QueueKind::Urls),
        (
            "simple downloader",
            DownloaderKind::Simple,
            QueueKind::SimpleDownloader,
        ),
        ("gallery", DownloaderKind::Gallery, QueueKind::Gallery),
        ("watcher", DownloaderKind::Watchers, QueueKind::Watcher),
        ("import", DownloaderKind::Local, QueueKind::LocalImport),
    ];
    let pages_urls = urls.clone();
    let queues_made: Vec<i64> = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let options = ImportOptionsSlice::default();
            kinds
                .iter()
                .map(|(name, _, queue_kind)| {
                    let queue = queues::create_queue(conn, *queue_kind, name, None, &options, 0)?;
                    if *queue_kind == QueueKind::Watcher {
                        queues::set_queue_extra(
                            conn,
                            queue,
                            &serde_json::to_value(hydrus_core::watchers::WatcherState::new(
                                "https://boards.example.com/a/thread/1",
                                hydrus_core::subscriptions::CheckerOptions::default(),
                                0,
                            ))?,
                        )?;
                    }
                    hold(conn, queue, &pages_urls)?;
                    Ok(queue)
                })
                .collect()
        })
        .unwrap();
    let mut pages: Vec<Page> = vec![search("a")];
    for ((name, kind, _), queue) in kinds.iter().zip(&queues_made) {
        pages.push(page(
            name,
            PageContent::Downloader {
                kind: *kind,
                queues: vec![*queue],
                sort: None,
                page: None,
            },
        ));
    }
    pages.push(search("z"));
    let session = Session {
        name: LAST_SESSION.into(),
        pages,
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let win = ui.window();
    widgets::lay_out(win, 1100.0, 700.0);
    let tab_names = || -> Vec<String> {
        bound
            .pages
            .borrow()
            .tabs()
            .into_iter()
            .next()
            .map(|t| t.names)
            .unwrap_or_default()
    };
    let asked = |name: &str, option: bool| -> Vec<Value> {
        let kind = match name {
            "url import" => "urls",
            "simple downloader" => "simple",
            "import" => "local",
            other => other,
        };
        recorded["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["kind"] == kind
                    && r["option"] == option
                    && r["for_session"] == false
                    && r["answer_yes"] == true
            })
            .cloned()
            .collect()
    };
    let close_current = |expect: Option<&str>, answer: bool, context: &str| {
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        widgets::ctrl_key(win, "w");
        let question = ui.get_question();
        match expect {
            Some(text) => {
                assert_eq!(question.as_str(), text, "{context}");
                widgets::click(win, if answer { "yes" } else { "no" });
                assert_eq!(ui.get_question(), "", "{context}");
            }
            None => assert_eq!(question, "", "{context}: closed at once"),
        }
    };

    // the option on (as recorded): each kind asks, "no" keeps it
    for (name, _, _) in &kinds {
        let record = asked(name, true);
        assert_eq!(record.len(), 1, "{name}");
        let question = record[0]["questions"][0].as_str().unwrap();
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        widgets::click(win, name);
        let before = tab_names();
        close_current(Some(question), false, name);
        assert_eq!(tab_names(), before, "{name}: declining keeps the page");
    }
    // a session loaded over them is a session close: the reference asks
    // nothing of a non-empty importer page then (`for_session_close`), and
    // the pages are replaced without a second question
    for (name, _, _) in &kinds {
        let kind = match *name {
            "url import" => "urls",
            "simple downloader" => "simple",
            "import" => "local",
            other => other,
        };
        for record in recorded["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == kind && r["option"] == true && r["for_session"] == true)
        {
            assert_eq!(record["questions"], json!([]), "{name}");
            assert!(record["vetoed"].is_null(), "{name}");
        }
    }
    bound.pages.borrow_mut().save_session("restore", 0).unwrap();
    let title = crate::menu_bar::titles(&ui)
        .iter()
        .position(|(t, _)| t == "pages")
        .unwrap() as i32;
    ui.invoke_menu_title_pressed(title, 80.0, 22.0);
    crate::menu_bar::hover(&ui, "sessions");
    crate::menu_bar::hover(&ui, "clear and load");
    crate::menu_bar::choose(&ui, "restore");
    assert_eq!(
        ui.get_question(),
        hydrus_gui::session_saving::clear_and_load_question("restore")
    );
    widgets::click(win, "yes");
    assert_eq!(
        ui.get_question(),
        "",
        "no question about the importer pages"
    );
    for (name, _, _) in &kinds {
        assert!(tab_names().contains(&(*name).to_owned()), "{name} is back");
    }

    // ("yes" closes one of them)
    let record = asked("watcher", true);
    headless::render(&windows.get(0).unwrap(), 1100, 700);
    widgets::click(win, "watcher");
    close_current(
        Some(record[0]["questions"][0].as_str().unwrap()),
        true,
        "watcher",
    );
    assert!(!tab_names().contains(&"watcher".to_owned()));

    // a page still importing asks whatever the option says: its files
    // resumed (as its sidebar's button does), closing it asks, and "no" keeps it
    let importing = |option: bool| -> String {
        recorded["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| {
                r["kind"] == "urls still importing"
                    && r["option"] == option
                    && r["answer_yes"] == true
            })
            .unwrap()["questions"][0]
            .as_str()
            .unwrap()
            .to_owned()
    };
    {
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
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        widgets::click(win, "url import");
        let before = tab_names();
        close_current(Some(&importing(true)), false, "importing, option on");
        assert_eq!(tab_names(), before, "declining keeps the importing page");
    }

    // the option off, through Options: the same pages close at once
    crate::options_window::open(&ui);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    crate::options_window::show_page(&options, "gui pages");
    let label = "Confirm when closing a non-empty importer page: ";
    let (at, row) = crate::options_window::row(&options, label);
    assert!(row.checked, "on by default, as recorded");
    options.invoke_check_toggled(at, false);
    options.invoke_apply();
    // (the importing page still asks, as recorded)
    headless::render(&windows.get(0).unwrap(), 1100, 700);
    widgets::click(win, "url import");
    close_current(Some(&importing(false)), true, "importing, option off");
    assert!(
        !tab_names().contains(&"url import".to_owned()),
        "closed after yes"
    );
    for (name, _, _) in kinds
        .iter()
        .filter(|(name, _, _)| *name != "watcher" && *name != "url import")
    {
        assert!(
            asked(name, false)
                .iter()
                .all(|r| r["questions"].as_array().unwrap().is_empty()),
            "{name}: the reference asks nothing with the option off"
        );
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        widgets::click_nth(win, name, 0);
        close_current(None, true, name);
        assert!(!tab_names().contains(&(*name).to_owned()), "{name}: closed");
    }
}
