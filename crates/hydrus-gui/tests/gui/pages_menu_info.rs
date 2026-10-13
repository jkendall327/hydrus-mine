//! The Pages menu's history submenu, session weight report and clearing of
//! multiwatcher highlights, replayed against `oracle/fixtures/pages_menu_info.json`
//! (`record_pages_menu_info.py`) through the real menu bar and pages.

use hydrus_core::pages::{DownloaderKind, DownloaderPageState, Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::sessions;

use crate::search_pages_menu::{choose, client, labels, open, store};

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("pages_menu_info.json")
}

/// The recorded history menu's entries as the window labels them
/// ("---" for a separator).
fn recorded_history(entries: &serde_json::Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| match e.as_str() {
            Some(sep) => sep.to_owned(),
            None => e["text"].as_str().unwrap().to_owned(),
        })
        .collect()
}

fn history(ui: &MainWindow) -> Vec<String> {
    open(ui, "pages", &["history"]);
    let entries = labels(ui, 1);
    ui.invoke_menu_dismissed();
    entries
}

// leaf: audit-options-menu-menu-pages-no-tab-history
#[test]
fn the_history_menu_has_no_tab_history_row_once_shown_as_in_the_reference() {
    let _windows = headless::init();
    let fixture = recorded();
    let (_dirs, _store, ui, _bound) = client(&["one", "two"]);

    // the reference's menu is built with a plain "no tab history" label, but
    // `aboutToShow` rebuilds it before anyone sees it: with an empty history
    // the user sees only "Clear History"
    open(&ui, "pages", &["history"]);
    choose(&ui, "Clear History");
    assert_eq!(
        history(&ui),
        recorded_history(&fixture["history_empty"]),
        "an empty history"
    );
    assert!(
        !history(&ui).contains(&"no tab history".to_owned()),
        "the placeholder is never shown"
    );

    // pages shown one, two, one: newest first
    for tab in [0, 1, 0] {
        ui.invoke_tab_chosen(0, tab);
    }
    assert_eq!(
        history(&ui),
        recorded_history(&fixture["history_filled"]),
        "a history"
    );

    open(&ui, "pages", &["history"]);
    choose(&ui, "Clear History");
    assert_eq!(
        history(&ui),
        recorded_history(&fixture["history_cleared"]),
        "after Clear History"
    );
}

fn seed(url: &str) -> hydrus_store::queues::NewFileSeed {
    use hydrus_store::queues::{FileSeedMeta, NewFileSeed, SeedType};
    NewFileSeed {
        seed_type: SeedType::Url,
        data: url.to_owned(),
        data_for_comparison: url.to_owned(),
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    }
}

// leaf: audit-options-weight-detail
#[test]
fn the_weight_report_matches_the_references_for_open_and_closed_pages() {
    use hydrus_core::import_options::ImportOptionsSlice;
    use hydrus_store::queues::{self, QueueKind};

    let _windows = headless::init();
    let fixture = recorded();
    let weight = &fixture["weight"];
    let (_dirs, store) = store();

    // a search page of five files, a URLs page of three URLs and a watcher
    // page whose watcher has four file seeds, as the recording made them
    let files = weight["files"].as_u64().unwrap() as usize;
    let urls: Vec<String> = weight["urls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().to_owned())
        .collect();
    let watcher_seeds = weight["watcher_seeds"].as_u64().unwrap() as usize;
    let (urls_key, watchers_key) = (PageKey::random(), PageKey::random());
    let urls_queue = store
        .write({
            let urls = urls.clone();
            move |ctx| {
                let conn = ctx.conn();
                let options = ImportOptionsSlice::default();
                let urls_queue = queues::create_queue(
                    conn,
                    QueueKind::Urls,
                    "urls",
                    Some(&urls_key.0),
                    &options,
                    1,
                )?;
                let seeds: Vec<_> = urls.iter().map(|u| seed(u)).collect();
                queues::add_file_seeds(conn, urls_queue, &seeds, false, 1)?;
                Ok(urls_queue)
            }
        })
        .unwrap();
    let watcher = match hydrus_store::watchers::add_watcher(
        &store,
        &hydrus_store::watchers::WatcherPage {
            name: "watchers",
            key: Some(&watchers_key.0),
            queues: &[],
            checker: &hydrus_core::subscriptions::CheckerOptions::default(),
            options: &ImportOptionsSlice::default(),
        },
        "https://boards.example/a/thread/1",
        1,
    )
    .unwrap()
    {
        hydrus_store::watchers::Watched::New(queue) => queue.id,
        _ => panic!("a new watcher"),
    };
    store
        .write(move |ctx| {
            let seeds: Vec<_> = (0..watcher_seeds)
                .map(|i| seed(&format!("https://boards.example/a/file/{i}.jpg")))
                .collect();
            queues::add_file_seeds(ctx.conn(), watcher, &seeds, false, 1)?;
            Ok(())
        })
        .unwrap();
    let mut predicates = hydrus_search::api::parse_api_search(&serde_json::json!([
        "system:everything",
        format!("system:limit = {files}")
    ]))
    .unwrap();
    predicates.shrink_to_fit();
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![
            Page {
                key: PageKey::random(),
                name: "files".into(),
                content: PageContent::Search {
                    search: hydrus_search::FileSearchContext {
                        location: hydrus_search::LocationContext::single(
                            hydrus_core::ServiceKey::new(
                                hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
                            ),
                        ),
                        predicates,
                        ..hydrus_search::FileSearchContext::default()
                    },
                    synchronised: true,
                    sort: None,
                    lock: None,
                    collect: None,
                },
            },
            Page {
                key: urls_key,
                name: "urls".into(),
                content: PageContent::Downloader {
                    kind: DownloaderKind::Urls,
                    queues: vec![urls_queue],
                    sort: None,
                    page: Some(Box::new(DownloaderPageState::default())),
                },
            },
            Page {
                key: watchers_key,
                name: "watchers".into(),
                content: PageContent::Downloader {
                    kind: DownloaderKind::Watchers,
                    queues: vec![watcher],
                    sort: None,
                    page: Some(Box::new(DownloaderPageState::default())),
                },
            },
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_tab_chosen(0, 0);
    open(&ui, "pages", &[]);
    choose(&ui, "refresh");

    // (the menu's two labels and the report its weight item opens)
    let steps = weight["steps"].as_array().unwrap();
    let ask = |ui: &MainWindow, step: &serde_json::Value, blank_pages: u64| {
        let report = &step["report"];
        // (closing the last page leaves hydrus-rs a new blank one, which the
        // reference's empty notebook doesn't have: DIFFERENCES.md, #183)
        let pages_open = |text: &str, from: &str, to: &str| text.replacen(from, to, 1);
        let (count_label, message) = if blank_pages == 0 {
            (
                report["count_label"].as_str().unwrap().to_owned(),
                report["message"].as_str().unwrap().to_owned(),
            )
        } else {
            (
                pages_open(report["count_label"].as_str().unwrap(), "0 pages open", "1 pages open"),
                pages_open(report["message"].as_str().unwrap(), "Your 0 open pages' total", "Your 1 open pages' total"),
            )
        };
        open(ui, "pages", &["weight"]);
        let pane = labels(ui, 1);
        assert_eq!(
            pane,
            [count_label, report["label"].as_str().unwrap().to_owned()],
            "{}: the weight submenu",
            step["when"]
        );
        choose(ui, report["label"].as_str().unwrap());
        let window = hydrus_gui::message_window().expect("the report is shown");
        let text = window.get_message().to_string();
        window.invoke_cancelled();
        assert_eq!(text, message, "{}: the report", step["when"]);
    };

    ask(&ui, &steps[0], 0);

    // the search page is closed: its files are the undo list's
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_close_page();
    ask(&ui, &steps[1], 0);

    // the two downloader pages are still importing, and ask
    let questions: Vec<String> = fixture["close_questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| q.as_str().unwrap().to_owned())
        .collect();
    for question in &questions {
        // (as time passes the page reads its importers' state)
        let next = bound.current.borrow().borrow().next_import_status_time();
        bound
            .downloader_updates
            .set_clock(std::rc::Rc::new(move || next + 0.25));
        bound.downloader_updates.refresh();
        ui.invoke_close_page();
        assert_eq!(&ui.get_question().to_string(), question);
        ui.invoke_answer(true);
    }
    assert_eq!(bound.pages.borrow().session().pages.len(), 1);
    ask(&ui, &steps[2], 1);
}

// leaf: audit-options-menu-menu-pages-all-multiwatcher-highlights
#[test]
fn clearing_multiwatcher_highlights_clears_each_watcher_page_as_the_reference_does() {
    let _windows = headless::init();
    let fixture = recorded();
    let multi = &fixture["multiwatcher"];
    let (_dirs, _store, ui, bound) = client(&["one"]);
    let urls: Vec<String> = multi["urls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().to_owned())
        .collect();
    let highlighted_urls = |bound: &hydrus_gui::Bound| -> Option<String> {
        let page = bound.current.borrow();
        let page = page.borrow();
        let view = page.watchers().unwrap();
        let queue = view.state.highlighted?;
        view.watchers
            .iter()
            .find(|w| w.queue == queue)
            .map(|w| w.state.url.clone())
    };
    let expect = |value: &serde_json::Value| value["highlighted"].as_str().map(str::to_owned);

    // two watcher pages with the same two threads added
    for _ in 0..2 {
        open(&ui, "pages", &["download"]);
        choose(&ui, "new watcher page");
        ui.invoke_watcher_urls(urls.join("\n").into());
    }
    // (the page just made is shown; the first is the tab before it)
    assert_eq!(highlighted_urls(&bound), expect(&multi["before"][1]));
    ui.invoke_tab_chosen(0, 1);
    assert_eq!(highlighted_urls(&bound), expect(&multi["before"][0]));

    open(&ui, "pages", &["clear"]);
    choose(&ui, "all multiwatcher highlights");

    assert_eq!(highlighted_urls(&bound), expect(&multi["after"][0]));
    ui.invoke_tab_chosen(0, 2);
    assert_eq!(highlighted_urls(&bound), expect(&multi["after"][1]));
    // and the session keeps them so
    let session = bound.pages.borrow().session().clone();
    for page in &session.pages {
        if let PageContent::Downloader { page: state, .. } = &page.content {
            assert_eq!(state.as_ref().and_then(|s| s.highlighted), None);
        }
    }
}
