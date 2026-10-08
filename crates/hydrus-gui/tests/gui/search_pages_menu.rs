//! The Pages and Undo menus driven through the real menu bar and store: the
//! navigation history, closed pages, new pages, refresh and saved sessions,
//! as the reference's `_InitialiseMenuInfoPages` and `_InitialiseMenuInfoUndo`
//! menus do.

use std::sync::Arc;

use slint::Model as _;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::import::import_legacy;
use hydrus_store::{Store, sessions};

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

fn titles(ui: &MainWindow) -> Vec<(String, bool)> {
    let titles = ui.get_menu_titles();
    (0..titles.row_count())
        .map(|i| {
            let t = titles.row_data(i).unwrap();
            (t.label.to_string(), t.usable)
        })
        .collect()
}

/// The open menus' lines: (label, usable), separators as "---".
fn panes(ui: &MainWindow) -> Vec<Vec<(String, bool)>> {
    let panes = ui.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|i| {
                    let line = lines.row_data(i).unwrap();
                    let label = if line.kind == 2 {
                        "---".to_owned()
                    } else {
                        line.label.to_string()
                    };
                    (label, line.usable)
                })
                .collect()
        })
        .collect()
}

fn labels(ui: &MainWindow, pane: usize) -> Vec<String> {
    panes(ui)[pane].iter().map(|l| l.0.clone()).collect()
}

fn line(ui: &MainWindow, label: &str) -> (i32, i32) {
    let panes = panes(ui);
    let p = panes.len() - 1;
    let i = panes[p]
        .iter()
        .position(|l| l.0 == label)
        .unwrap_or_else(|| panic!("{label} in {:?}", panes[p]));
    (p as i32, i as i32)
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

fn choose(ui: &MainWindow, label: &str) {
    let (p, i) = line(ui, label);
    ui.invoke_menu_line_clicked(p, i, 0.0, 0.0, 0.0);
}

/// Open top-level menu `title`, then hover down the `path` of submenus.
fn open(ui: &MainWindow, title: &str, path: &[&str]) {
    let at = titles(ui).iter().position(|(t, _)| t == title).unwrap() as i32;
    ui.invoke_menu_title_pressed(at, 80.0, 22.0);
    for step in path {
        hover(ui, step);
    }
}

fn tabs(ui: &MainWindow) -> Vec<String> {
    let row = ui.get_tab_rows().row_data(0).unwrap();
    (0..row.names.row_count())
        .map(|i| row.names.row_data(i).unwrap().to_string())
        .collect()
}

fn search(name: &str) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: false,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

/// A client whose last session is the named search pages.
fn client(
    names: &[&str],
) -> (
    [tempfile::TempDir; 2],
    Arc<Store>,
    MainWindow,
    hydrus_gui::Bound,
) {
    let (dirs, store) = store();
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: names.iter().map(|n| search(n)).collect(),
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    (dirs, store, ui, bound)
}

// leaf: audit-options-menu-menu-pages-history-page
// leaf: audit-options-menu-menu-pages-clear-history
#[test]
fn the_history_menu_shows_pages_newest_first_and_clear_history_empties_it() {
    let _windows = headless::init();
    let (_dirs, _store, ui, bound) = client(&["one", "two", "three"]);
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_tab_chosen(0, 2);
    ui.invoke_tab_chosen(0, 0);

    open(&ui, "pages", &["history"]);
    let history = labels(&ui, 1);
    assert_eq!(history[0], "1: one");
    assert_eq!(history[1], "2: three");
    assert_eq!(history[2], "3: two");
    assert_eq!(history[history.len() - 2..], ["---", "Clear History"]);
    // choosing an entry shows that page
    choose(&ui, "2: three");
    assert_eq!(bound.pages.borrow().shown().name, "three");

    // Clear History empties it: only the command is left, and the next
    // page shown is the first of a new history
    open(&ui, "pages", &["history"]);
    choose(&ui, "Clear History");
    assert!(bound.pages.borrow().history().is_empty());
    open(&ui, "pages", &["history"]);
    assert_eq!(
        labels(&ui, 1).last().map(String::as_str),
        Some("Clear History")
    );
    assert!(!labels(&ui, 1).iter().any(|l| l.contains(": ")));
    ui.invoke_menu_dismissed();
    ui.invoke_tab_chosen(0, 0);
    open(&ui, "pages", &["history"]);
    assert_eq!(labels(&ui, 1)[0], "1: one");
}

// leaf: audit-options-menu-menu-undo-closed-page
// leaf: audit-options-menu-menu-undo-clear-all
#[test]
fn closed_pages_return_where_they_were_and_clear_all_asks_first() {
    let _windows = headless::init();
    let (_dirs, _store, ui, bound) = client(&["one", "two", "three"]);

    // close the middle page: undo offers it, and it comes back in place
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_close_page();
    assert_eq!(tabs(&ui).len(), 2);
    assert_eq!(titles(&ui)[1], ("undo".to_owned(), true));
    open(&ui, "undo", &["closed pages"]);
    assert_eq!(
        labels(&ui, 1),
        ["clear all\u{2026}", "---", "two"],
        "the same names the closed pages have"
    );
    choose(&ui, "two");
    let names: Vec<String> = bound.pages.borrow().tabs()[0].names.clone();
    assert_eq!(names, ["one", "two", "three"], "back at its place");
    assert_eq!(bound.pages.borrow().shown().name, "two");

    // two closed: clear all asks, "no" keeps them, "yes" forgets them
    ui.invoke_close_page();
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_close_page();
    open(&ui, "undo", &["closed pages"]);
    assert_eq!(labels(&ui, 1)[0], "clear all\u{2026}");
    choose(&ui, "clear all\u{2026}");
    assert_eq!(ui.get_question(), "Clear the 2 closed pages?");
    ui.invoke_answer(false);
    assert_eq!(bound.pages.borrow_mut().closed_names().len(), 2);
    assert_eq!(titles(&ui)[1], ("undo".to_owned(), true));
    open(&ui, "undo", &["closed pages"]);
    choose(&ui, "clear all\u{2026}");
    ui.invoke_answer(true);
    assert!(bound.pages.borrow_mut().closed_names().is_empty());
    ui.invoke_menu_title_pressed(1, 40.0, 22.0);
    assert!(panes(&ui).is_empty(), "nothing left to undo");
}

// leaf: audit-options-menu-menu-pages-new-page
// leaf: audit-options-menu-menu-pages-refresh
#[test]
fn new_page_asks_the_chooser_and_refresh_searches_a_paused_page_again() {
    let _windows = headless::init();
    let (_dirs, _store, ui, bound) = client(&["one"]);
    assert_eq!(ui.get_chooser_labels().row_count(), 0);
    open(&ui, "pages", &[]);
    choose(&ui, "new page\u{2026}");
    assert!(
        ui.get_chooser_labels().row_count() > 0,
        "the chooser opened"
    );
    ui.invoke_chooser_cancel();
    assert_eq!(ui.get_chooser_labels().row_count(), 0);
    assert_eq!(tabs(&ui).len(), 1, "cancelled, no page made");

    // a paused page waits; refresh runs its search and resumes it
    let page = bound.current.borrow().clone();
    page.borrow_mut().set_synchronised(false);
    page.borrow_mut().add_predicate("system:inbox");
    assert!(page.borrow().results().is_empty());
    open(&ui, "pages", &[]);
    choose(&ui, "refresh");
    assert!(page.borrow().synchronised());
    assert!(!page.borrow().results().is_empty());
}

// leaf: audit-options-menu-menu-pages-new-url-download-page
// leaf: audit-options-menu-menu-pages-new-page-of-pages
// leaf: audit-options-menu-menu-pages-file-search-domain
#[test]
fn the_pages_menu_makes_download_special_and_search_pages_and_shows_them() {
    let _windows = headless::init();
    let (_dirs, store, ui, bound) = client(&["one"]);

    open(&ui, "pages", &["download"]);
    choose(&ui, "new url download page");
    assert_eq!(tabs(&ui).len(), 2);
    assert_eq!(bound.pages.borrow().shown().name, "url import");
    assert!(matches!(
        bound.pages.borrow().shown().content,
        PageContent::Downloader {
            kind: hydrus_core::pages::DownloaderKind::Urls,
            ..
        }
    ));

    open(&ui, "pages", &["special"]);
    choose(&ui, "new page of pages");
    assert_eq!(tabs(&ui).len(), 3);
    assert!(tabs(&ui)[2].starts_with("pages"), "{:?}", tabs(&ui));
    // (the reference gives it a blank page, which is shown)
    assert_eq!(bound.pages.borrow().tabs()[1].names, ["files"]);
    assert_eq!(bound.pages.borrow().shown().name, "files");

    // a file search page per file domain, searching that domain
    open(&ui, "pages", &["file search"]);
    let domains: Vec<String> = labels(&ui, 1);
    assert!(
        domains.iter().any(|l| l.starts_with("new \"my files\"")),
        "{domains:?}"
    );
    let before = bound.pages.borrow().session().all_pages().len();
    let wanted = domains
        .iter()
        .find(|l| l.starts_with("new \"my files\""))
        .unwrap()
        .clone();
    choose(&ui, &wanted);
    assert_eq!(bound.pages.borrow().session().all_pages().len(), before + 1);
    let location = bound.current.borrow().borrow().location().clone();
    assert_eq!(
        location,
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES.to_vec()
        ))
    );
    // the new pages are kept with the session
    bound.pages.borrow_mut().sync(50).unwrap();
    let reopened = Pages::open(store).unwrap();
    assert_eq!(
        reopened.session().all_pages().len(),
        bound.pages.borrow().session().all_pages().len()
    );
}

// leaf: audit-options-menu-menu-pages-sessions-append-saved-session
// leaf: audit-options-menu-menu-pages-sessions-delete-saved-session
#[test]
fn saved_sessions_append_as_a_page_of_pages_and_delete_after_asking() {
    let _windows = headless::init();
    let (_dirs, store, ui, bound) = client(&["one"]);
    let saved = Session {
        name: "work".into(),
        pages: vec![search("inside work")],
    };
    let seed = saved.clone();
    store
        .write(move |ctx| sessions::save(ctx.conn(), &seed, 5))
        .unwrap();

    // reserved sessions are not offered to delete; the new one is
    open(&ui, "pages", &["sessions"]);
    hover(&ui, "delete");
    assert_eq!(labels(&ui, 2), ["work"]);
    ui.invoke_menu_dismissed();

    // append: a page of pages named after the session, with a copy of its pages
    open(&ui, "pages", &["sessions", "append"]);
    choose(&ui, "work");
    assert_eq!(tabs(&ui).len(), 2);
    assert!(tabs(&ui)[1].starts_with("work"), "{:?}", tabs(&ui));
    let tabs_now = bound.pages.borrow().tabs();
    assert_eq!(tabs_now[1].names, ["inside work"]);
    let copy = bound.pages.borrow().shown().key;
    assert_ne!(copy, saved.pages[0].key, "an independent copy");
    assert_eq!(
        store.read(|c| sessions::load(c, "work")).unwrap().unwrap(),
        saved,
        "the saved session is as it was"
    );

    // delete asks; no keeps it, yes removes it from the store and the menu
    open(&ui, "pages", &["sessions", "delete"]);
    choose(&ui, "work");
    assert_eq!(ui.get_question(), "Delete session \"work\"?");
    ui.invoke_answer(false);
    assert!(store.read(|c| sessions::load(c, "work")).unwrap().is_some());
    open(&ui, "pages", &["sessions", "delete"]);
    choose(&ui, "work");
    ui.invoke_answer(true);
    assert!(store.read(|c| sessions::load(c, "work")).unwrap().is_none());
    open(&ui, "pages", &["sessions"]);
    assert!(!labels(&ui, 1).contains(&"delete".to_owned()));
    assert!(
        !labels(&ui, 1).contains(&"append".to_owned()) || {
            hover(&ui, "append");
            !labels(&ui, 2).contains(&"work".to_owned())
        }
    );
}

// leaf: audit-options-undo-manager-undo-last-content-operation
// leaf: audit-options-undo-manager-redo-last-content-operation
#[test]
fn the_undo_menu_undoes_and_redoes_the_last_content_change() {
    let _windows = headless::init();
    let (_dirs, store, ui, _bound) = client(&["one"]);
    let inbox = || -> Vec<u32> {
        store
            .read(|conn| {
                Ok(conn
                    .prepare("SELECT hash_id FROM file_inbox ORDER BY hash_id LIMIT 2")?
                    .query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<Vec<u32>>>()?)
            })
            .unwrap()
    };
    let files: Vec<hydrus_core::HashId> = inbox().into_iter().map(hydrus_core::HashId).collect();
    assert_eq!(files.len(), 2);

    // nothing to undo yet
    ui.invoke_menu_title_pressed(1, 40.0, 22.0);
    assert!(panes(&ui).is_empty());

    hydrus_gui_model::media_actions::archive(&store, &files).unwrap();
    assert!(inbox().is_empty() || !inbox().contains(&files[0].0));
    open(&ui, "undo", &[]);
    assert_eq!(labels(&ui, 0)[0], "undo archive 2 files");
    choose(&ui, "undo archive 2 files");
    assert!(inbox().contains(&files[0].0) && inbox().contains(&files[1].0));

    // now redo is offered, and archives them again
    open(&ui, "undo", &[]);
    assert_eq!(labels(&ui, 0)[0], "redo archive 2 files");
    assert!(!labels(&ui, 0).iter().any(|l| l.starts_with("undo ")));
    choose(&ui, "redo archive 2 files");
    assert!(!inbox().contains(&files[0].0) && !inbox().contains(&files[1].0));
    open(&ui, "undo", &[]);
    assert_eq!(labels(&ui, 0)[0], "undo archive 2 files");
}
