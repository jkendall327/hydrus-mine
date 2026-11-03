//! URL downloader pages the Client API asks for (`/add_urls/add_url` to a
//! page that is not open, `GetOrMakeURLImportPage`): the client takes the
//! daemon's queue up as a page where a new page goes, shown only when the
//! call asked; and a page's selection is not kept when the client closes,
//! as the reference's session keeps none (`ClientGUISession.py`).

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::pages::{DownloaderKind, PageContent, PageKey};
use hydrus_gui::{Pages, page_chooser::NewPage};
use hydrus_store::{Store, queues, sessions};
use std::rc::Rc;
use std::sync::Arc;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn waiting(store: &Store, name: &str, show: bool) -> i64 {
    let name = name.to_owned();
    store
        .write(move |ctx| {
            queues::create_waiting_url_queue(
                ctx.conn(),
                &name,
                &ImportOptionsSlice::default(),
                show,
                1,
            )
        })
        .unwrap()
}

/// The names of the notebook's pages `depth` levels down the way to the
/// page shown.
fn names(pages: &Pages, depth: usize) -> Vec<String> {
    pages.tabs()[depth].names.clone()
}

/// The page holding `queue`, if one does.
fn page_of(pages: &Pages, queue: i64) -> Option<PageKey> {
    pages
        .session()
        .all_pages()
        .into_iter()
        .find_map(|page| match &page.content {
            PageContent::Downloader {
                kind: DownloaderKind::Urls,
                queues,
                ..
            } if queues.contains(&queue) => Some(page.key),
            _ => None,
        })
}

#[test]
fn a_queue_the_api_made_becomes_a_page_where_new_pages_go() {
    let (_dirs, store) = store();
    let mut pages = Pages::open(store.clone()).unwrap();
    // (a page of pages shown, on its blank search page: new pages go there)
    pages.new_page(&NewPage::Pages).unwrap();
    let shown = pages.shown().key;
    assert_eq!(pages.tabs().len(), 2);
    let before = names(&pages, 1);

    let queue = waiting(&store, "api downloads", false);
    assert!(pages.adopt_api_queues().unwrap());
    let key = page_of(&pages, queue).expect("a page for the queue");
    let mut expected = before.clone();
    expected.push("api downloads".into());
    assert_eq!(names(&pages, 1), expected, "in the notebook shown");
    assert_eq!(pages.shown().key, shown, "not shown: the call did not ask");
    let row = store.read(|c| queues::queue(c, queue)).unwrap().unwrap();
    assert_eq!(row.page_key.as_deref(), Some(&key.0[..]));
    assert!(!queues::waits_for_url_page(&row));
    // (the stored session has the page as soon as its queue has the key, in
    // the same write, before any sync: the Client API finds it there)
    let stored = store
        .read(|c| sessions::load(c, sessions::LAST_SESSION))
        .unwrap()
        .unwrap();
    assert!(stored.all_pages().iter().any(|p| p.key == key));
    // (taken up once)
    assert!(!pages.adopt_api_queues().unwrap());

    // shown, when the call asked
    let shown_queue = waiting(&store, "shown downloads", true);
    assert!(pages.adopt_api_queues().unwrap());
    let key = page_of(&pages, shown_queue).unwrap();
    assert_eq!(pages.shown().key, key);
    assert_eq!(pages.shown().name, "shown downloads");
}

#[test]
fn the_gui_sends_named_urls_to_the_renamed_page_by_its_name() {
    let (_dirs, store) = store();
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.new_page(&NewPage::Urls).unwrap();
    let renamed = pages.shown().key;
    pages.rename_shown("my downloads");
    pages.select(0, 0);
    pages
        .import_urls_to_named_page(&["https://example.com/a.png".into()], "my downloads", None)
        .unwrap();
    let urls: Vec<_> = pages
        .session()
        .all_pages()
        .into_iter()
        .filter(|p| matches!(p.content, PageContent::Downloader { .. }))
        .map(|p| p.key)
        .collect();
    assert_eq!(urls, [renamed], "the renamed page, not a new one");
}

#[test]
fn a_pages_selection_is_not_kept_when_the_client_opens_again() {
    let (_dirs, store) = store();
    let pages = Pages::open(store.clone()).unwrap();
    let key = pages.shown().key;
    store
        .write(move |ctx| {
            sessions::set_page_files(ctx.conn(), &key, &[hydrus_core::HashId(1)])?;
            sessions::set_page_selected(ctx.conn(), &key, &[hydrus_core::HashId(1)])
        })
        .unwrap();
    drop(pages);
    let mut pages = Pages::open(store.clone()).unwrap();
    assert_eq!(pages.shown().key, key);
    assert!(
        store
            .read(|c| sessions::page_selected(c, &key))
            .unwrap()
            .is_empty()
    );
    assert!(pages.current().borrow().selected_files().is_empty());
}

/// The open window takes the queue up on its next pass, and shows the page
/// when the call asked for it: its tab selected, the page's view shown.
#[test]
fn the_open_window_shows_an_api_page_it_was_asked_to_show() {
    use slint::{ComponentHandle as _, Model as _};
    let (_dirs, store) = store();
    let _windows = hydrus_gui::headless::init();
    let ui = hydrus_gui::MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = hydrus_gui::bind(&ui, Pages::open(store.clone()).unwrap());
    waiting(&store, "api downloads", false);
    (bound.sync)();
    crate::common::widgets::settle();
    let row = ui.get_tab_rows().row_data(0).unwrap();
    let names: Vec<String> = row.full_names.iter().map(|n| n.to_string()).collect();
    assert_eq!(names, ["files", "api downloads"]);
    assert_eq!(row.selected, 0, "not shown");
    waiting(&store, "shown downloads", true);
    (bound.sync)();
    crate::common::widgets::settle();
    let row = ui.get_tab_rows().row_data(0).unwrap();
    assert_eq!(row.full_names.row_count(), 3);
    assert_eq!(
        row.full_names
            .row_data(usize::try_from(row.selected).unwrap())
            .unwrap(),
        "shown downloads"
    );
    assert!(Rc::ptr_eq(
        &bound.current.borrow(),
        &bound.pages.borrow_mut().current()
    ));
}

/// The GUI's named URL import skips a page of the name whose import
/// options differ from those asked for, as the reference's forced URL
/// downloader does, and makes one with them.
#[test]
fn a_named_page_with_other_import_options_is_not_used() {
    let (_dirs, store) = store();
    let mut pages = Pages::open(store.clone()).unwrap();
    pages.new_page(&NewPage::Urls).unwrap();
    let plain = pages.shown().key;
    pages.rename_shown("my downloads");
    let options = ImportOptionsSlice {
        locations: Some(hydrus_core::import_options::LocationOptions::default()),
        ..ImportOptionsSlice::default()
    };
    pages
        .import_urls_to_named_page(
            &["https://example.com/a.png".into()],
            "my downloads",
            Some(options.clone()),
        )
        .unwrap();
    let named: Vec<PageKey> = pages
        .session()
        .all_pages()
        .into_iter()
        .filter(|p| p.name == "my downloads")
        .map(|p| p.key)
        .collect();
    assert_eq!(named.len(), 2, "a new page for the other options");
    let made = *named.iter().find(|k| **k != plain).unwrap();
    // (a second import with the same options goes to that one)
    pages
        .import_urls_to_named_page(
            &["https://example.com/b.png".into()],
            "my downloads",
            Some(options),
        )
        .unwrap();
    let count = pages
        .session()
        .all_pages()
        .into_iter()
        .filter(|p| p.name == "my downloads")
        .count();
    assert_eq!(count, 2);
    let queue = page_queue(&pages, made);
    let made = store
        .read(move |c| queues::queue(c, queue))
        .unwrap()
        .unwrap();
    assert!(
        made.options.locations.is_some(),
        "made with the options asked for"
    );
}

fn page_queue(pages: &Pages, key: PageKey) -> i64 {
    pages
        .session()
        .all_pages()
        .into_iter()
        .find_map(|p| match &p.content {
            PageContent::Downloader { queues, .. } if p.key == key => Some(queues[0]),
            _ => None,
        })
        .unwrap()
}
