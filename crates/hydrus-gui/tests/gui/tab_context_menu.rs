//! The tab right-click menu put through the reference's recordings
//! (`oracle/fixtures/tab_actions.json` and `tab_context.json`) as a user
//! would: the menu opened on a tab, its entries (and submenus) clicked, the
//! bulk-close question answered, and the pages read back from the notebook.

use std::sync::Arc;

use slint::Model as _;

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
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

fn search() -> PageContent {
    PageContent::Search {
        search: FileSearchContext::default(),
        synchronised: false,
        sort: None,
        lock: None,
        collect: None,
    }
}

fn page(name: &str, content: PageContent) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    }
}

fn labels(ui: &MainWindow, pane: usize) -> Vec<String> {
    let lines = ui.get_menu_panes().row_data(pane).unwrap().lines;
    (0..lines.row_count())
        .map(|i| lines.row_data(i).unwrap().label.to_string())
        .collect()
}

/// Click the line `label` of menu pane `pane` (a submenu line opens the next pane).
fn choose(ui: &MainWindow, pane: usize, label: &str) {
    let at = labels(ui, pane)
        .iter()
        .position(|l| l == label)
        .unwrap_or_else(|| panic!("{label:?} in {:?}", labels(ui, pane)));
    ui.invoke_menu_line_clicked(
        i32::try_from(pane).unwrap(),
        i32::try_from(at).unwrap(),
        200.0,
        100.0,
        10.0,
    );
}

fn seed(store: &Arc<Store>, pages: Vec<Page>) {
    let saved = Session {
        name: LAST_SESSION.into(),
        pages,
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &saved, 100))
        .unwrap();
}

fn names(pages: &Pages) -> Vec<String> {
    pages
        .session()
        .pages
        .iter()
        .map(|p| p.name.clone())
        .collect()
}

#[test]
fn close_other_left_and_right_from_the_tab_menu_ask_and_close_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let original = vec![
        page("a", search()),
        page(
            "nested",
            PageContent::Pages(vec![page("child 0", search()), page("child 1", search())]),
        ),
        page("c", search()),
        page("d", search()),
    ];
    for step in fixture["close"].as_array().unwrap() {
        seed(&store, original.clone());
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, 2);
        let index = i32::try_from(step["index"].as_u64().unwrap()).unwrap();
        ui.invoke_tab_menu_requested(0, index, 30.0, 55.0);
        let entry = match step["side"].as_str().unwrap() {
            "left" => "pages to the left",
            "right" => "pages to the right",
            _ => "other pages",
        };
        choose(&ui, 0, "close");
        choose(&ui, 1, entry);
        assert_eq!(ui.get_question(), step["asked"][0].as_str().unwrap());
        ui.invoke_answer(step["accepted"].as_bool().unwrap());
        assert_eq!(
            serde_json::json!(names(&bound.pages.borrow())),
            step["pages"],
            "{step}"
        );
        assert_eq!(
            bound.pages.borrow_mut().closed_names().len(),
            step["closed_indices"].as_array().unwrap().len(),
            "{step}"
        );
        // and undone, the pages are back where they were
        while bound.pages.borrow_mut().unclose() {}
        assert_eq!(bound.pages.borrow().session().pages, original);
    }
}

// leaf: audit-options-tabs-context-action-2141-first-page
// leaf: audit-options-tabs-context-action-2146-page-to-the-left
// leaf: audit-options-tabs-context-action-2151-page-to-the-right
// leaf: audit-options-tabs-context-action-2156-last-page
#[test]
fn select_from_the_tab_menu_goes_where_the_reference_went() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_actions.json");
    let original: Vec<_> = (0..4)
        .map(|i| page(&format!("page {i}"), search()))
        .collect();
    // the menu the reference offered with the second-from-right tab selected
    // and clicked: which of the four movements it had
    let menu = fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["selected"] == 2 && m["clicked"] == 2)
        .unwrap();
    let offered: Vec<String> = menu["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["menu"] == "select")
        .unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .collect();
    let mut driven = 0;
    for step in fixture["navigation"].as_array().unwrap() {
        seed(&store, original.clone());
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, 2);
        ui.invoke_tab_menu_requested(0, 2, 30.0, 55.0);
        choose(&ui, 0, "select");
        let ours = labels(&ui, 1);
        assert_eq!(ours, offered, "the select submenu");
        let entry = match step["movement"].as_str().unwrap() {
            "first" => "first page",
            "left" => "page to the left",
            "right" => "page to the right",
            _ => "last page",
        };
        if offered.iter().any(|o| o == entry) {
            choose(&ui, 1, entry);
            assert_eq!(
                bound.pages.borrow().tabs()[0].selected,
                usize::try_from(step["selected"].as_u64().unwrap()).unwrap(),
                "{step}"
            );
            driven += 1;
        } else {
            // (not offered with this tab selected: the reference's keys did it)
            assert!(!ours.iter().any(|o| o == entry), "{step}");
        }
    }
    assert_eq!(driven, 3, "first, left and right are offered here");
    // "last page" is offered with the first tab selected (the recorded menu
    // for that), and goes to the last one, as the recorded `last` did
    let first_menu = fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["selected"] == 0 && m["clicked"] == 0)
        .unwrap();
    let offered: Vec<String> = first_menu["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["menu"] == "select")
        .unwrap()["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_owned())
        .collect();
    seed(&store, original.clone());
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.pages.borrow_mut().select(0, 0);
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(&ui, 0, "select");
    assert_eq!(labels(&ui, 1), offered, "the select submenu");
    choose(&ui, 1, "last page");
    let last = fixture["navigation"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["movement"] == "last")
        .unwrap();
    assert_eq!(
        bound.pages.borrow().tabs()[0].selected,
        usize::try_from(last["selected"].as_u64().unwrap()).unwrap()
    );
}

// leaf: audit-options-tabs-context-action-2178-to-left-end
// leaf: audit-options-tabs-context-action-2183-left
// leaf: audit-options-tabs-context-action-2188-right
// leaf: audit-options-tabs-context-action-2193-to-right-end
#[test]
fn move_page_from_the_tab_menu_reorders_as_the_reference_did() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_context.json");
    let count = fixture["pages"].as_array().unwrap().len();
    let shown = usize::try_from(fixture["selected"].as_u64().unwrap()).unwrap();
    let original: Vec<_> = (0..count)
        .map(|i| page(&format!("p{i}"), search()))
        .collect();
    let mut moved = 0;
    for step in fixture["moves"].as_array().unwrap() {
        seed(&store, original.clone());
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        bound.pages.borrow_mut().select(0, shown);
        let index = usize::try_from(step["index"].as_u64().unwrap()).unwrap();
        ui.invoke_tab_menu_requested(0, i32::try_from(index).unwrap(), 30.0, 55.0);
        let entry = match step["movement"].as_str().unwrap() {
            "first" => "to left end",
            "left" => "left",
            "right" => "right",
            _ => "to right end",
        };
        choose(&ui, 0, "move page");
        let offered = labels(&ui, 1);
        let expected: Vec<String> = step["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| format!("p{}", i.as_u64().unwrap()))
            .collect();
        if offered.iter().any(|o| o == entry) {
            choose(&ui, 1, entry);
            moved += 1;
        } else {
            // (a move that goes nowhere is not offered, and changes nothing)
            assert_eq!(
                expected,
                (0..count).map(|i| format!("p{i}")).collect::<Vec<_>>(),
                "{step}"
            );
        }
        assert_eq!(names(&bound.pages.borrow()), expected, "{step}");
        // the page that was shown still is (the recording's `selected` is the
        // original index of the current page)
        assert_eq!(
            bound.pages.borrow().shown().name,
            format!("p{shown}"),
            "{step}"
        );
    }
    assert!(moved > 8, "{moved}");
}
