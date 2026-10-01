//! A search locked to a `system:hash` of its page's files, as the
//! reference's pages lock: "open in a new page" opens one, the lock box
//! stands in for the search, the hash lets go of files removed from the
//! page (if it follows removals), refresh leaves the page be, unlocking
//! makes the search its `system:hash`, the lock button locks a search to
//! the files in view (asking first as the reference does), and a saved
//! session keeps the lock.

use std::sync::Arc;

use slint::Model as _;

use hydrus_core::pages::{HashLock, PageContent};
use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

struct Fixture {
    _legacy: tempfile::TempDir,
    _native: tempfile::TempDir,
    store: Arc<Store>,
}

fn fixture() -> Fixture {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    Fixture {
        _legacy: legacy,
        _native: native,
        store,
    }
}

/// A window on a page searching everything.
fn window(store: &Arc<Store>) -> (MainWindow, Bound) {
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    (ui, bound)
}

/// The menu entry starting `label` among `rows`.
fn find(rows: &slint::ModelRc<hydrus_gui::MenuRow>, label: &str) -> i32 {
    (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap())
        .find(|row| row.label.starts_with(label))
        .unwrap_or_else(|| panic!("{label}"))
        .id
}

const ASK: &str = "This will lock the page, collapsing the current search to a system:hash of \
                   the current files. Is this ok?";

#[test]
fn a_page_opened_on_files_is_locked_to_them() {
    let fixture = fixture();
    let windows = headless::init();
    let (ui, bound) = window(&fixture.store);
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    assert!(files.len() >= 4, "{}", files.len());
    assert!(!ui.get_search_locked());
    assert!(ui.get_can_lock_search());

    // open → in a new page: locked to the three files
    page.borrow_mut().select_files(&files[..3]);
    ui.invoke_thumbnail_menu_requested(-1);
    ui.invoke_menu_chosen(find(&ui.get_thumbnail_menu().open_a, "in a new page"));
    let opened = bound.current.borrow().clone();
    assert_eq!(opened.borrow().results(), &files[..3]);
    assert_eq!(opened.borrow().lock(), Some(HashLock::default()));
    assert_eq!(opened.borrow().predicates(), ["system:hash is in 3 hashes"]);
    assert!(ui.get_search_locked());
    assert_eq!(ui.get_lock_label(), "Locked at 3 files.");
    assert!(ui.get_lock_syncs_new() && ui.get_lock_syncs_removes());
    let main_window = windows.get(0).unwrap();
    let pixels = headless::render(&main_window, 1100, 700);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("search_lock.png"), &pixels, 1100, 700).unwrap();

    // remove → selected: the hash lets go of it
    opened.borrow_mut().select_files(&files[..1]);
    ui.invoke_thumbnail_menu_requested(-1);
    ui.invoke_menu_chosen(find(&ui.get_thumbnail_menu().remove.g1, "selected (1)"));
    assert_eq!(opened.borrow().results(), &files[1..3]);
    assert_eq!(ui.get_lock_label(), "Locked at 2 files.");

    opened.borrow_mut().remove_files(&files[1..2]);
    assert_eq!(opened.borrow().results(), &files[2..3]);
    assert_eq!(opened.borrow().locked_count(), 1);

    // the cog: no longer following removals, the hash keeps what leaves
    ui.invoke_lock_syncs_changed(true, false);
    assert_eq!(
        opened.borrow().lock(),
        Some(HashLock {
            syncs_new: true,
            syncs_removes: false,
        })
    );
    opened.borrow_mut().remove_files(&files[2..3]);
    assert!(opened.borrow().results().is_empty());
    assert_eq!(opened.borrow().locked_count(), 1);
    // and refresh doesn't search a locked page, so it stays empty
    ui.invoke_thumbnail_menu_requested(-1);
    ui.invoke_menu_chosen(find(&ui.get_thumbnail_menu().head, "refresh"));
    assert!(opened.borrow().results().is_empty());

    // unlocking: the search is its system:hash again, and searches
    ui.invoke_unlock_search();
    assert!(!ui.get_search_locked());
    assert_eq!(opened.borrow().lock(), None);
    assert_eq!(ui.get_predicates().row_count(), 1);
    opened.borrow_mut().refresh();
    assert_eq!(opened.borrow().results(), &files[2..3]);

    // and the lock button locks it again, asking nothing, as the hash is
    // of just the files in view (keeping what the cog said)
    ui.invoke_lock_search();
    assert_eq!(ui.get_question(), "");
    assert!(ui.get_search_locked());
    assert_eq!(ui.get_lock_label(), "Locked at 1 files.");
    assert_eq!(
        opened.borrow().lock(),
        Some(HashLock {
            syncs_new: true,
            syncs_removes: false,
        })
    );
    // a locked search can't be changed
    let before = opened.borrow().predicates();
    assert!(before[0].starts_with("system:hash is "), "{before:?}");
    assert!(!opened.borrow_mut().add_predicate("system:inbox"));
    opened.borrow_mut().remove_predicate(0);
    assert_eq!(opened.borrow().predicates(), before);
}

#[test]
fn locking_a_search_asks_first() {
    let fixture = fixture();
    let _windows = headless::init();
    let (ui, bound) = window(&fixture.store);
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();

    // a search other than a system:hash asks
    ui.invoke_lock_search();
    assert_eq!(ui.get_question(), ASK);
    ui.invoke_answer(false);
    assert!(!ui.get_search_locked());
    assert_eq!(page.borrow().predicates(), ["system:everything"]);
    ui.invoke_lock_search();
    ui.invoke_answer(true);
    assert!(ui.get_search_locked());
    assert_eq!(
        ui.get_lock_label(),
        format!("Locked at {} files.", files.len())
    );
    assert_eq!(page.borrow().locked_count(), files.len());

    // a system:hash of other files than those in view asks otherwise
    ui.invoke_unlock_search();
    page.borrow_mut().remove_files(&files[..1]);
    assert_eq!(page.borrow().locked_count(), files.len());
    ui.invoke_lock_search();
    assert!(
        ui.get_question().starts_with(
            "This will lock the page, collapsing the current search to a system:hash of the \
             current files.\n\nYour search already has a system:hash, but its files are \
             different than what is currently in view."
        ),
        "{}",
        ui.get_question()
    );
    ui.invoke_answer(true);
    assert_eq!(page.borrow().locked_count(), files.len() - 1);

    // an empty search locks without asking
    let empty_ui = MainWindow::new().unwrap();
    let empty = bind(
        &empty_ui,
        Pages::single(SearchPage::new(fixture.store.clone())),
    );
    empty_ui.invoke_lock_search();
    assert_eq!(empty_ui.get_question(), "");
    assert!(empty_ui.get_search_locked());
    assert_eq!(empty_ui.get_lock_label(), "Locked at 0 files.");
    assert_eq!(
        empty.current.borrow().borrow().lock(),
        Some(HashLock::default())
    );
}

#[test]
fn a_saved_session_keeps_the_lock() {
    let fixture = fixture();
    let _windows = headless::init();
    let (ui, bound) = window(&fixture.store);
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    page.borrow_mut().select_files(&files[..2]);
    ui.invoke_thumbnail_menu_requested(-1);
    ui.invoke_menu_chosen(find(&ui.get_thumbnail_menu().open_a, "in a new page"));
    ui.invoke_lock_syncs_changed(false, true);
    bound.pages.borrow_mut().save(1).unwrap();

    let mut pages = Pages::open(fixture.store.clone()).unwrap();
    let locked: Vec<Option<HashLock>> = pages
        .session()
        .all_pages()
        .iter()
        .filter_map(|p| match &p.content {
            PageContent::Search { lock, .. } => Some(*lock),
            _ => None,
        })
        .collect();
    let expected = HashLock {
        syncs_new: false,
        syncs_removes: true,
    };
    assert_eq!(locked, [None, Some(expected)]);
    // and the page opens locked, showing its files
    pages.select(0, 1);
    let reopened = pages.current();
    assert_eq!(reopened.borrow().lock(), Some(expected));
    assert_eq!(reopened.borrow().results(), &files[..2]);
    assert_eq!(reopened.borrow().locked_count(), 2);
}
