//! The thumbnail menu's select and remove rows chosen in the main window
//! against `oracle/fixtures/thumbnail_select_remove.json` (recorded by
//! `oracle/record_thumbnail_select_remove.py`: the reference's grid menu on
//! pages of the basic files, each row triggered): the rows offered with their
//! counts, the files selected after each select row and the files left on the
//! page after each remove row.

use std::sync::Arc;

use hydrus_core::{HashId, ServiceKey};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_search::{FileSearchContext, LocationContext};
use hydrus_store::Store;
use serde_json::Value;

use super::media_support::{find, group_rows};

fn names(store: &Store, files: &[HashId]) -> Vec<String> {
    let hashes = store
        .read(|c| hydrus_store::master::hashes(c, files))
        .unwrap();
    files.iter().map(|f| hashes[f].to_hex()).collect()
}

fn ids(store: &Store, hashes: &Value) -> Vec<HashId> {
    hashes
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            store
                .read(|c| hydrus_store::master::hash_id(c, &h.as_str().unwrap().parse().unwrap()))
                .unwrap()
                .unwrap()
        })
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

struct Opened {
    store: Arc<Store>,
    page: Rc<RefCell<SearchPage>>,
    ui: MainWindow,
    _bound: hydrus_gui::Bound,
    _dirs: [tempfile::TempDir; 1],
}

use std::cell::RefCell;
use std::rc::Rc;

fn open(page: &Value, selected: &Value) -> Opened {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let files = ids(&store, &page["files"]);
    let location = if page["page"] == "all local files" {
        LocationContext::default()
    } else {
        LocationContext::single(ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
        ))
    };
    let search = SearchPage::restored(
        store.clone(),
        FileSearchContext {
            location,
            ..Default::default()
        },
        false,
        None,
        files,
    );
    let ui = MainWindow::new().unwrap();
    slint::ComponentHandle::show(&ui).unwrap();
    let bound = bind(&ui, Pages::single(search));
    let current = bound.current.borrow().clone();
    let picked = ids(&store, selected);
    if !picked.is_empty() {
        current.borrow_mut().select_files(&picked);
    }
    Opened {
        store,
        page: current,
        ui,
        _bound: bound,
        _dirs: [dir],
    }
}

// leaf: audit-media-context-selection
#[test]
fn select_and_remove_rows_do_what_the_references_do() {
    let recorded = hydrus_testkit::fixture_json("thumbnail_select_remove.json");
    let _windows = headless::init();
    let mut done = 0;
    for page in recorded["pages"].as_array().unwrap() {
        for case in page["cases"].as_array().unwrap() {
            let label = format!("{} / {}", page["page"], case["selection"]);
            // the rows offered, with their counts, in order
            let o = open(page, &case["selected"]);
            o.ui.invoke_thumbnail_menu_requested(-1);
            let menu = o.ui.get_thumbnail_menu();
            let select: Vec<String> = group_rows(&menu.select).into_iter().map(|r| r.0).collect();
            let remove: Vec<String> = group_rows(&menu.remove).into_iter().map(|r| r.0).collect();
            assert_eq!(
                select,
                strings(&case["select_rows"]),
                "{label}: select rows"
            );
            assert_eq!(
                remove,
                strings(&case["remove_rows"]),
                "{label}: remove rows"
            );
            // each select row, then each remove row, on a fresh page
            for row in &select {
                let o = open(page, &case["selected"]);
                o.ui.invoke_thumbnail_menu_requested(-1);
                let rows = group_rows(&o.ui.get_thumbnail_menu().select);
                o.ui.invoke_menu_chosen(find(&rows, row));
                let mut got = names(&o.store, &o.page.borrow().selected_files());
                got.sort();
                assert_eq!(got, strings(&case["select"][row]), "{label}: select {row}");
                done += 1;
            }
            for row in &remove {
                let o = open(page, &case["selected"]);
                o.ui.invoke_thumbnail_menu_requested(-1);
                let rows = group_rows(&o.ui.get_thumbnail_menu().remove);
                o.ui.invoke_menu_chosen(find(&rows, row));
                let left = names(&o.store, o.page.borrow().results());
                assert_eq!(left, strings(&case["remove"][row]), "{label}: remove {row}");
                done += 1;
            }
        }
    }
    assert!(done >= 100, "{done}");
}
