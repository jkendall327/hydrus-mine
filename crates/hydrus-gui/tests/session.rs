//! The last session's pages over the `basic` fixture: tabs for each
//! notebook on the way to the page shown, each search page as it was left
//! (its search, sort and files, not searched again), and the pages we don't
//! open yet saying so.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::HashId;
use hydrus_core::pages::{
    DownloaderKind, Page, PageContent, PageKey, PageSort, PageSortBy, Session,
};
use hydrus_gui::{MainWindow, Pages, Tabs, bind, headless};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, SortBy, SortOrder, parse_api_search, search_files,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::sessions::{self, LAST_SESSION};

/// A store imported from the fixture (whose files it uses in place, so the
/// fixture's directory is kept too).
fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn page(name: &str, content: PageContent) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    }
}

#[test]
fn the_last_session_opens_as_it_was_left() {
    let (_dirs, store) = store();
    // a search page left showing some of what its search finds, largest first
    let search = FileSearchContext {
        predicates: parse_api_search(&serde_json::json!(["system:everything"])).unwrap(),
        ..FileSearchContext::default()
    };
    let everything: Vec<HashId> = store
        .read(|conn| {
            Ok(search_files(
                conn,
                &store.snapshot(),
                &search,
                FileSort {
                    by: SortBy::FileSize,
                    order: SortOrder::Descending,
                },
                &Clock::system(),
            )
            .unwrap())
        })
        .unwrap();
    assert!(everything.len() > 5);
    let shown: Vec<HashId> = everything[..5].to_vec();
    let search_page = page(
        "my search",
        PageContent::Search {
            search: search.clone(),
            synchronised: true,
            sort: Some(PageSort {
                by: PageSortBy::System(0),
                ascending: false,
            }),
        },
    );
    let downloader = page(
        "threads",
        PageContent::Downloader {
            kind: DownloaderKind::Watchers,
            queues: vec![1, 2],
            sort: None,
        },
    );
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            page(
                "pages",
                PageContent::Pages(vec![search_page.clone(), downloader.clone()]),
            ),
            page("downloaders", PageContent::Pages(Vec::new())),
        ],
    };
    let (search_key, downloader_key) = (search_page.key, downloader.key);
    let (session_again, shown_again) = (session.clone(), shown.clone());
    let (files, other_files) = (shown.clone(), everything[5..7].to_vec());
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            sessions::save(conn, &session, 0)?;
            sessions::set_page_files(conn, &search_key, &files)?;
            sessions::set_page_files(conn, &downloader_key, &other_files)
        })
        .unwrap();

    let mut pages = Pages::open(store.clone()).unwrap();
    // the first page of the first notebook, as the reference opens
    assert_eq!(
        pages.tabs(),
        [
            Tabs {
                names: vec!["pages".into(), "downloaders".into()],
                selected: 0
            },
            Tabs {
                names: vec!["my search".into(), "threads".into()],
                selected: 0
            },
        ]
    );
    {
        let opened = pages.current();
        let opened = opened.borrow();
        assert_eq!(opened.predicates(), ["system:everything"]);
        assert_eq!(opened.results(), shown, "as left, not searched again");
        assert_eq!(opened.sort().by, SortBy::FileSize);
        assert_eq!(opened.sort().order, SortOrder::Descending);
        assert!(opened.note().is_none());
    }
    // a new sort sorts the files shown, and doesn't search again
    pages
        .current()
        .borrow_mut()
        .set_sort_order(SortOrder::Ascending);
    let mut reversed = shown.clone();
    reversed.reverse();
    assert_eq!(pages.current().borrow().results(), reversed);

    pages.select(1, 1);
    let opened = pages.current();
    assert!(
        opened
            .borrow()
            .note()
            .unwrap()
            .contains("watcher downloader page")
    );
    assert_eq!(opened.borrow().results(), &everything[5..7]);
    assert!(!opened.borrow_mut().add_predicate("system:inbox"));

    pages.select(0, 1);
    assert_eq!(
        pages.tabs().len(),
        1,
        "an empty notebook has no tabs of its own"
    );
    assert!(pages.current().borrow().results().is_empty());

    // a page opened before is as it was left
    pages.select(0, 0);
    assert_eq!(pages.current().borrow().results(), reversed);

    // saved, and opened again: as it was left (the pages not opened too)
    pages.current().borrow_mut().add_predicate("system:inbox");
    let inbox = pages.current().borrow().results().to_vec();
    assert!(!inbox.is_empty() && inbox.len() < everything.len());
    pages.save(10).unwrap();
    let mut again = Pages::open(store.clone()).unwrap();
    assert_eq!(again.tabs(), pages.tabs());
    {
        let opened = again.current();
        let opened = opened.borrow();
        assert_eq!(opened.predicates(), ["system:everything", "system:inbox"]);
        assert_eq!(opened.sort().by, SortBy::FileSize);
        assert_eq!(opened.sort().order, SortOrder::Ascending);
        assert_eq!(opened.results(), inbox);
    }
    again.select(1, 1);
    assert_eq!(again.current().borrow().results(), &everything[5..7]);
    assert!(again.current().borrow().note().is_some());
    // put back as the window below expects it
    store
        .write(move |ctx| {
            let conn = ctx.conn();
            sessions::save(conn, &session_again, 0)?;
            sessions::set_page_files(conn, &search_key, &shown_again)
        })
        .unwrap();

    // the window: tabs for each notebook on the way to the page shown
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let rows = ui.get_tab_rows();
    assert_eq!(rows.row_count(), 2);
    assert_eq!(
        rows.row_data(1).unwrap().names.row_data(0).unwrap(),
        "my search"
    );
    assert_eq!(ui.get_status(), "5 files");
    assert_eq!(ui.get_note(), "");
    ui.invoke_tab_chosen(1, 1);
    assert_eq!(ui.get_tab_rows().row_data(1).unwrap().selected, 1);
    assert_eq!(ui.get_status(), "2 files");
    assert!(ui.get_note().contains("watcher"));
    assert_eq!(bound.current.borrow().borrow().results().len(), 2);
    ui.invoke_tab_chosen(1, 0);
    ui.show().unwrap();
    let main_window = windows.get(0).unwrap();
    let (width, height) = (1100, 700);
    headless::render(&main_window, width, height);
    let pixels = headless::render(&main_window, width, height);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("session.png"), &pixels, width, height).unwrap();
    // the page's thumbnails were drawn
    let colours: std::collections::HashSet<&[u8]> = pixels.chunks(4).collect();
    assert!(colours.len() > 1000, "{} colours", colours.len());
}
