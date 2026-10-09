//! pages > refresh, chosen from the real menu bar on the page shown, against
//! what the reference's `_RefreshCurrentPage` did on the same kinds of page
//! (`oracle/record_page_refresh.py`): a paused search that has had a predicate
//! typed since (refresh searches it, and un-pauses it), a search that was run
//! and then paused, a page opened on given files, and a URL downloader page.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::import::import_legacy;
use hydrus_store::{Store, sessions};

fn everything(name: &str) -> Page {
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
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    }
}

fn refresh_from_the_menu(ui: &MainWindow) {
    let titles = ui.get_menu_titles();
    let at = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "pages")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
    let panes = ui.get_menu_panes();
    let lines = panes.row_data(0).unwrap().lines;
    let line = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "refresh")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(line).unwrap(), 0.0, 0.0, 0.0);
}

// leaf: audit-options-menu-menu-pages-refresh
#[test]
fn refresh_from_the_pages_menu_searches_again_as_the_reference_did() {
    let _windows = headless::init();
    let recorded = hydrus_testkit::fixture_json("page_refresh.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![everything("typed"), everything("paused")],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 1))
        .unwrap();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let state = |bound: &hydrus_gui::Bound| -> (bool, usize) {
        let page = bound.pages.borrow_mut().current();
        let page = page.borrow();
        (page.synchronised(), page.results().len())
    };
    let theirs = |name: &str| -> serde_json::Value {
        recorded["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["page"] == name)
            .unwrap()
            .clone()
    };
    let expect = |case: &serde_json::Value, key: &str| -> (bool, usize) {
        (
            case[key]["synchronised"].as_bool().unwrap_or(false),
            usize::try_from(case[key]["files"].as_u64().unwrap()).unwrap(),
        )
    };

    // paused, with a predicate typed since the search ran
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    {
        let page = bound.pages.borrow_mut().current();
        page.borrow_mut().set_synchronised(false);
        page.borrow_mut().add_predicate("system:inbox");
    }
    let case = theirs("paused with a predicate typed since");
    assert_eq!(state(&bound), expect(&case, "before"));
    refresh_from_the_menu(&ui);
    assert_eq!(state(&bound), expect(&case, "after"));

    // searched, then paused
    ui.invoke_tab_chosen(0, 1);
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    {
        let page = bound.pages.borrow_mut().current();
        page.borrow_mut().set_synchronised(false);
    }
    let case = theirs("searched, then paused");
    assert_eq!(state(&bound), expect(&case, "before"));
    refresh_from_the_menu(&ui);
    assert_eq!(state(&bound), expect(&case, "after"));

    // a page opened on given files: its search is locked to them
    let hashes: Vec<hydrus_core::Sha256> = {
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(hydrus_testkit::fixture_path(
                "legacy_db/basic.manifest.json",
            ))
            .unwrap(),
        )
        .unwrap();
        manifest["files"]
            .as_array()
            .unwrap()
            .iter()
            .take(usize::try_from(recorded["given"].as_u64().unwrap()).unwrap())
            .map(|f| f["hash"].as_str().unwrap().parse().unwrap())
            .collect()
    };
    let files: Vec<hydrus_core::HashId> = store
        .read(|c| {
            hashes
                .iter()
                .map(|h| Ok(hydrus_store::master::hash_id(c, h)?.unwrap()))
                .collect::<hydrus_store::Result<_>>()
        })
        .unwrap();
    bound.pages.borrow_mut().open_files(
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
        )),
        files,
        None,
        None,
    );
    let case = theirs("opened on given files");
    let (_, files_before) = state(&bound);
    assert_eq!(files_before, expect(&case, "before").1);
    refresh_from_the_menu(&ui);
    assert_eq!(state(&bound).1, expect(&case, "after").1);

    // a URL downloader page has no search to run
    let titles = ui.get_menu_titles();
    let at = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "pages")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 80.0, 22.0);
    ui.invoke_menu_dismissed();
    bound.pages.borrow_mut().new_page_in(None);
    let case = theirs("url importer");
    let names_before = bound.pages.borrow().session().all_pages().len();
    refresh_from_the_menu(&ui);
    assert_eq!(
        bound.pages.borrow().session().all_pages().len(),
        names_before
    );
    assert_eq!(expect(&case, "after").1, 0);
}
