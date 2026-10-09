//! The pages menu's entries that open a page (new page of pages, the
//! download and special entries, the file search domains), chosen from the
//! real menu bar and compared with what the reference's own menu did
//! (`oracle/record_new_page_menu.py`): the page it opened, named and typed
//! as it names and types it, where it went (into the deepest page of
//! pages, else beside the current page), and which tabs are current.

use slint::Model as _;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::import::import_legacy;
use hydrus_store::{Store, sessions};

/// A search page of "my files", as the client begins with.
fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext {
                location: hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                    hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
                )),
                ..FileSearchContext::default()
            },
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

fn panes(ui: &MainWindow) -> Vec<Vec<String>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|i| lines.row_data(i).unwrap().label.to_string())
                .collect()
        })
        .collect()
}

fn line(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = panes(ui);
    let p = panes.len() - 1;
    let i = panes[p]
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", panes[p]));
    (i32::try_from(p).unwrap(), i32::try_from(i).unwrap())
}

fn hover(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_hovered(
        p,
        i,
        300.0 + 150.0 * p as f32,
        40.0 + 22.0 * i as f32,
        150.0 * p as f32,
    );
}

/// The tree of pages as the recording describes it: name, type, pages (or,
/// of a search page, the file domains it searches).
fn tree(store: &Store, pages: &[Page]) -> serde_json::Value {
    let names = |keys: &std::collections::BTreeSet<hydrus_core::ServiceKey>| -> Vec<String> {
        let snapshot = store.snapshot();
        let mut names: Vec<String> = keys
            .iter()
            .map(|key| snapshot.services.by_key(key).unwrap().name.clone())
            .collect();
        names.sort();
        names
    };
    pages
        .iter()
        .map(|page| match &page.content {
            PageContent::Pages(children) => serde_json::json!({
                "name": page.name, "type": "pages", "pages": tree(store, children),
            }),
            PageContent::Search { search, .. } => serde_json::json!({
                "name": page.name,
                "type": 6,
                "location": {
                    "current": names(search.location.current()),
                    "deleted": names(search.location.deleted()),
                },
            }),
            content => serde_json::json!({ "name": page.name, "type": content.page_type() }),
        })
        .collect()
}

// leaf: audit-options-menu-menu-pages-new-page-of-pages
// leaf: audit-options-menu-menu-pages-new-simple-downloader-page
// leaf: audit-options-menu-menu-pages-new-url-download-page
// leaf: audit-options-menu-menu-pages-new-watcher-page
// leaf: audit-options-menu-menu-pages-new-gallery-page
// leaf: audit-options-menu-menu-pages-new-duplicates-processing-page
// leaf: audit-options-menu-menu-pages-file-search-domain
#[test]
fn pages_menu_entries_open_the_pages_the_reference_opened_where_it_opened_them() {
    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("new_page_menu.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let mut made = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let inside = case["inside"].as_bool().unwrap();
        let session = Session {
            name: sessions::LAST_SESSION.into(),
            pages: if inside {
                vec![Page {
                    key: PageKey::random(),
                    name: "outer".into(),
                    content: PageContent::Pages(vec![search("inner")]),
                }]
            } else {
                vec![search("start")]
            },
        };
        store
            .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        assert_eq!(
            tree(&store, &bound.pages.borrow().session().pages),
            case["before"],
            "{case}"
        );

        let entry: Vec<&str> = case["entry"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
            .collect();
        let titles = ui.get_menu_titles();
        let at = (0..titles.row_count())
            .position(|i| titles.row_data(i).unwrap().label == "pages")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
        hover(&ui, entry[0]);
        // (the file search entries are the reference's, by domain)
        let recorded_searches: Vec<&str> = recorded["search_entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_str().unwrap())
            .collect();
        if entry[0] == "file search" {
            let ours = panes(&ui).last().unwrap().clone();
            assert_eq!(ours, recorded_searches, "the file search entries");
        }
        let (p, i) = line(&ui, entry[1]);
        ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);

        let pages = bound.pages.borrow();
        assert_eq!(
            tree(&store, &pages.session().pages),
            case["after"],
            "after {case}"
        );
        let current: Vec<u64> = pages.tabs().iter().map(|t| t.selected as u64).collect();
        let theirs: Vec<u64> = case["current"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_u64().unwrap())
            .collect();
        assert_eq!(current, theirs, "the tabs that are current after {case}");
        made += 1;
    }
    assert_eq!(made, 18);
}
