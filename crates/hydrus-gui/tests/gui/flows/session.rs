//! Flow (`oracle/record_flow_session.py`): pages built (a search page for
//! `safe` with two files selected; a page of pages holding a URL downloader
//! that has downloaded a post and a watcher that has checked a thread),
//! saved as the client saves them when it closes, and opened again in a new
//! client on the same store, with a new daemon. Before and after, the tree
//! of pages, each page's search or importer, what it shows and which page
//! is shown must be what the reference showed, and nothing is fetched again.

use serde_json::{Value as Json, json};
use slint::ComponentHandle as _;

use hydrus_core::pages::{DownloaderKind, Page, PageContent};
use hydrus_gui::{Bound, MainWindow, Pages, bind, headless};
use hydrus_gui_model::page_chooser::NewPage;
use hydrus_store::{Store, queues};

use super::{Daemon, Known, Normaliser, Site, State, compare, install};
use crate::common::widgets;

/// What this flow shows that the reference does not, each explained in
/// `docs/rust/DIFFERENCES.md`.
const KNOWN: &[Known] = &[
    // hydrus-rs opens on the page it showed last; the reference on the first
    Known {
        path: "(restored)/shown",
        why: "The client and the daemon: the client opens on the page it showed last",
    },
];

const THREAD: u32 = 100;

/// The post the URL page downloads (none of the thread's files).
const POST: u32 = 4;

/// A page as the recorder describes it.
fn describe(bound: &Bound, state: &State<'_>, page: &Page) -> Json {
    match &page.content {
        PageContent::Pages(children) => json!({
            "name": page.name,
            "kind": "pages",
            "pages": children.iter().map(|c| describe(bound, state, c)).collect::<Vec<_>>(),
        }),
        PageContent::Search { .. } => {
            let opened = bound.pages.borrow_mut().page(&page.key).expect("the page");
            let opened = opened.borrow();
            json!({
                "name": page.name,
                "kind": "search",
                "shown files": opened.files().len(),
                "predicates": opened.predicates(),
                "selected": opened.selected_files().len(),
            })
        }
        PageContent::Downloader { kind, queues, .. } => {
            let shown = bound
                .pages
                .borrow_mut()
                .page(&page.key)
                .expect("the page")
                .borrow()
                .files()
                .len();
            match kind {
                DownloaderKind::Urls => json!({
                    "name": page.name,
                    "kind": "urls",
                    "shown files": shown,
                    "files": state.file_log(queues[0]),
                }),
                DownloaderKind::Watchers => {
                    let watchers: Vec<Json> = queues
                        .iter()
                        .map(|&q| {
                            let row = state
                                .store
                                .read(move |c| queues::queue(c, q))
                                .unwrap()
                                .unwrap();
                            let w = hydrus_store::watchers::watcher_state(&row).unwrap();
                            json!({
                                "url": state.n.text(&w.url),
                                "subject": w.subject,
                                "files": state.file_log(q),
                            })
                        })
                        .collect();
                    json!({
                        "name": page.name,
                        "kind": "watcher",
                        "shown files": shown,
                        "watchers": watchers,
                    })
                }
                other => panic!("a {other:?} page in the flow"),
            }
        }
        other => panic!("a page the flow did not make: {other:?}"),
    }
}

/// The tree, the shown page's names from the top (as the window's tabs
/// show them), and the site's requests.
fn tree(step: &str, ui: &MainWindow, bound: &Bound, state: &State<'_>, site: &Site) -> Json {
    (bound.sync)();
    widgets::settle();
    let session = bound.pages.borrow().session().clone();
    let tabs = super::tab_strip(ui);
    // (the window shows the top notebook's tabs, and each notebook's on the
    // way to the page shown: the rest of the tree is the session's)
    let top: Vec<String> = session.pages.iter().map(|p| p.name.clone()).collect();
    assert_eq!(tabs[0].0, top, "the top notebook's tabs");
    let pages: Vec<Json> = session
        .pages
        .iter()
        .map(|p| describe(bound, state, p))
        .collect();
    let shown: Vec<String> = tabs
        .iter()
        .map(|(names, selected)| names[*selected].clone())
        .collect();
    json!({"step": step, "pages": pages, "shown": shown, "hits": site.take_hits()})
}

/// Every importer page done and showing what it got.
fn importers_done(bound: &Bound, store: &Store) -> bool {
    (bound.sync)();
    let session = bound.pages.borrow().session().clone();
    let mut stack: Vec<Page> = session.pages;
    while let Some(page) = stack.pop() {
        match page.content {
            PageContent::Pages(children) => stack.extend(children),
            PageContent::Downloader { queues, .. } => {
                let mut logged = 0;
                for q in &queues {
                    let files = store.read(|c| queues::file_seeds(c, *q)).unwrap();
                    if files.is_empty()
                        || files
                            .iter()
                            .any(|f| f.status == queues::SeedStatus::Unknown)
                    {
                        return false;
                    }
                    logged += files
                        .iter()
                        .filter_map(|f| f.meta.hash("sha256"))
                        .collect::<std::collections::BTreeSet<_>>()
                        .len();
                }
                let shown = bound
                    .pages
                    .borrow_mut()
                    .page(&page.key)
                    .map_or(0, |p| p.borrow().files().len());
                if shown != logged {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

#[test]
fn pages_left_open_come_back_as_the_reference_brings_them_back() {
    let recording = hydrus_testkit::fixture_json("flow_session.json");
    let (_dirs, store) = super::basic_store();
    install(&store, &recording);
    let site = Site::serve("flow_session.json", &recording);
    let n = Normaliser::new(&site);
    let _windows = headless::init();
    let mut steps = Vec::new();

    // 1. build, in the first client
    {
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let daemon = Daemon::start(&store);
        let state = State {
            store: &store,
            ui: &ui,
            site: Some(&site),
            n: &n,
        };
        let my_files = store
            .snapshot()
            .services
            .by_name("my files")
            .unwrap()
            .key
            .clone();
        // a search page for safe, its first two files selected
        (bound.open_page)(&NewPage::Search {
            domain: my_files,
            name: "files".into(),
        });
        ui.invoke_search_edited("safe".into());
        ui.invoke_search_accepted();
        widgets::settle();
        ui.invoke_select_none();
        ui.invoke_thumbnail_clicked(0, false, false);
        ui.invoke_thumbnail_clicked(1, true, false);
        widgets::settle();
        // a page of pages: a URL downloader and a watcher in it
        (bound.open_page)(&NewPage::Pages);
        (bound.open_page)(&NewPage::Urls);
        ui.invoke_url_entered(format!("{}/post/{POST}", site.base).into());
        (bound.open_page)(&NewPage::Watcher);
        ui.invoke_watcher_urls(format!("{}/thread/{THREAD}", site.base).into());
        site.wait_for_a_request(20, "the downloader pages");
        super::wait_until_idle(&daemon, || importers_done(&bound, &store));
        steps.push(tree("build", &ui, &bound, &state, &site));
        // closed: the pages saved as the client saves them on exit
        bound.pages.borrow_mut().save(0).unwrap();
        drop(daemon);
        ui.hide().unwrap();
    }

    // 2. restored, in a new client on the same store
    {
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        let mut daemon = Daemon::start(&store);
        daemon.serve_api(super::api_port("flow_session.json"));
        let state = State {
            store: &store,
            ui: &ui,
            site: Some(&site),
            n: &n,
        };
        super::wait_until_idle(&daemon, || importers_done(&bound, &store));
        steps.push(tree("restored", &ui, &bound, &state, &site));
        // the Client API lists no selection either, as the reference keeps
        // none with its session
        // (the search page for safe, second in the top notebook)
        let search = bound.pages.borrow().session().pages[1].key;
        let (status, info) = daemon.api(
            "GET",
            &format!(
                "/manage_pages/get_page_info?page_key={}&simple=false",
                search.to_hex()
            ),
            None,
        );
        assert_eq!(status, 200, "{info}");
        let media = &info["page_info"]["media"];
        assert!(media["num_files"].as_u64().unwrap() > 0, "{info}");
        assert_eq!(media["num_files_selected"], json!(0), "{info}");
        assert_eq!(media["hashes_selected"], json!([]), "{info}");
    }

    compare(&steps, &recording, KNOWN);
}
