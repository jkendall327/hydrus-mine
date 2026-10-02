//! The main window's default shortcuts, as the reference's: ctrl+page up
//! and down move along the pages (`MoveSelection`: the deepest notebook
//! first, unless one above moved in the last three seconds), notebooks
//! remember the page they showed, f5 searches the page again (resuming a
//! paused search), and ctrl+i pauses and resumes searching.

use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::sessions::{self, LAST_SESSION};

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

#[test]
fn ctrl_page_up_and_down_move_along_the_pages() {
    let (_dirs, store) = store();
    // a, then a notebook of b, c and d, then e
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("a"),
            page(
                "n",
                PageContent::Pages(vec![search("b"), search("c"), search("d")]),
            ),
            search("e"),
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let mut pages = Pages::open(store.clone()).unwrap();
    let shown = |pages: &Pages| pages.shown().name.clone();
    let t0 = Instant::now();
    let at = |s: f64| t0 + Duration::from_secs_f64(s);
    assert_eq!(shown(&pages), "a");
    // into the notebook, on its first page
    assert!(pages.move_selection(1, at(0.0)));
    assert_eq!(shown(&pages), "b");
    // moved at the top a moment ago: the top moves again
    assert!(pages.move_selection(1, at(1.0)));
    assert_eq!(shown(&pages), "e");
    // never round the end
    assert!(!pages.move_selection(1, at(1.2)));
    assert_eq!(shown(&pages), "e");
    assert!(pages.move_selection(-1, at(1.5)));
    assert_eq!(shown(&pages), "b");
    // once the top has rested three seconds, the notebook's pages move
    assert!(pages.move_selection(1, at(5.0)));
    assert_eq!(shown(&pages), "c");
    assert!(pages.move_selection(1, at(5.5)));
    assert_eq!(shown(&pages), "d");
    // at its end, the top moves
    assert!(pages.move_selection(1, at(6.0)));
    assert_eq!(shown(&pages), "e");
    // and the notebook shows the page it showed last
    assert!(pages.move_selection(-1, at(20.0)));
    assert_eq!(shown(&pages), "d");
    pages.select(0, 0);
    pages.select(0, 1);
    assert_eq!(shown(&pages), "d");
}

#[test]
fn f5_and_ctrl_i() {
    let (_dirs, store) = store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let page = bound.current.borrow().clone();
    assert!(ui.get_synchronised());
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let everything = page.borrow().results().to_vec();
    assert!(!everything.is_empty());

    // ctrl+i: waiting, a new predicate doesn't search
    ui.invoke_flip_synchronised();
    assert!(!ui.get_synchronised());
    assert!(!page.borrow().synchronised());
    ui.invoke_search_edited("system:archive".into());
    ui.invoke_search_accepted();
    // (in place of system:everything, as the reference's list takes it)
    assert_eq!(page.borrow().predicates(), ["system:archive"]);
    assert_eq!(page.borrow().results(), everything);
    // f5 resumes searching, and searches
    ui.invoke_refresh_page();
    assert!(ui.get_synchronised());
    let archived = page.borrow().results().to_vec();
    assert!(archived.len() < everything.len());

    // ctrl+i back on searches at once
    ui.invoke_flip_synchronised();
    page.borrow_mut().remove_predicate(0);
    page.borrow_mut().add_predicate("system:everything");
    assert_eq!(page.borrow().results(), archived);
    ui.invoke_flip_synchronised();
    assert_eq!(page.borrow().results(), everything);

    // f5 on a locked search leaves it as it is
    page.borrow_mut().lock_search();
    let locked = page.borrow().results().to_vec();
    ui.invoke_refresh_page();
    assert_eq!(page.borrow().results(), locked);
}
