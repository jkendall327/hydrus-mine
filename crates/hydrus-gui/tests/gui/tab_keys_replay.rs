//! Selecting pages by a click on a tab, by Ctrl+PageUp / Ctrl+PageDown and
//! from the tab right-click menu's "pages" list, replayed against the
//! reference's (`oracle/fixtures/tab_keys.json`, from
//! `oracle/record_tab_keys.py`): a search page "a", a notebook "n" holding
//! "b" and "c", and a search page "d"; each step a real click, key press or
//! menu choice, and after each the page shown and every row of tabs (their
//! names and the one selected) compared. The three-second rule (Ctrl+Page
//! goes into the shown notebook first unless the notebook above moved in the
//! last three seconds) is replayed with the keys' clock held and moved on
//! where the recording waited.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use slint::platform::Key;
use slint::{ComponentHandle as _, Model as _};

use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::common::widgets;

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

/// The recorded layout as pages, each search page holding its files.
fn source(store: &Store, rows: &Value) -> Vec<Page> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let name = row["name"].as_str().unwrap();
            if let Some(children) = row.get("children") {
                return Page {
                    key: PageKey::random(),
                    name: name.into(),
                    content: PageContent::Pages(source(store, children)),
                };
            }
            let page = Page {
                key: PageKey::random(),
                name: name.into(),
                content: PageContent::Search {
                    search: FileSearchContext::default(),
                    synchronised: false,
                    sort: None,
                    lock: None,
                    collect: None,
                },
            };
            let key = page.key;
            let hashes: Vec<hydrus_core::Sha256> = row["hashes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|h| h.as_str().unwrap().parse().unwrap())
                .collect();
            store
                .write(move |ctx| {
                    let ids = hydrus_store::master::hash_ids(ctx.conn(), &hashes)?;
                    let files: Vec<_> = hashes.iter().map(|h| ids[h]).collect();
                    sessions::set_page_files(ctx.conn(), &key, &files)
                })
                .unwrap();
            page
        })
        .collect()
}

fn labels(ui: &MainWindow, pane: usize) -> Vec<String> {
    let lines = ui.get_menu_panes().row_data(pane).unwrap().lines;
    (0..lines.row_count())
        .map(|i| lines.row_data(i).unwrap().label.to_string())
        .collect()
}

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

// leaf: audit-options-tabs-selection
// leaf: audit-options-tabs-context-action-2120-selectable-page
#[test]
fn tabs_keys_and_the_pages_menu_select_pages_as_the_reference_did() {
    let recorded = hydrus_testkit::fixture_json("tab_keys.json");
    let (_dirs, store) = store();
    let pages = source(&store, &recorded["layout"]);
    let session = Session {
        name: LAST_SESSION.into(),
        pages,
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let windows = headless::init();
    headless::hold_clock();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let win = ui.window();
    widgets::lay_out(win, 1100.0, 700.0);
    let state = || {
        let rows = ui.get_tab_rows();
        let rows: Vec<Value> = (0..rows.row_count())
            .map(|i| {
                let row = rows.row_data(i).unwrap();
                json!({
                    "tabs": row.names.iter().map(|n| n.to_string()).collect::<Vec<_>>(),
                    "current": row.selected,
                })
            })
            .collect();
        json!({ "shown": bound.pages.borrow().shown().name, "rows": rows })
    };
    let steps = recorded["steps"].as_array().unwrap();
    assert_eq!(state(), steps[0]["state"], "start");
    for step in &steps[1..] {
        let action = step["do"].as_array().unwrap();
        let context = step["do"].to_string();
        headless::render(&windows.get(0).unwrap(), 1100, 700);
        match action[0].as_str().unwrap() {
            "click" => widgets::click(win, step["label"].as_str().unwrap()),
            "key" => {
                let key = if action[1] == "down" {
                    Key::PageDown
                } else {
                    Key::PageUp
                };
                widgets::ctrl_key(win, key);
            }
            "wait" => headless::advance_clock(Duration::from_secs(10)),
            "menu" | "choose" => {
                let name = action[1].as_str().unwrap();
                let tabs = ui.get_tab_rows();
                let place = (0..tabs.row_count()).find_map(|r| {
                    let row = tabs.row_data(r).unwrap();
                    let at = row.full_names.iter().position(|n| n == name)?;
                    Some((r, at))
                });
                // (a tab of a notebook that is not shown has no tab to
                // right-click: the row of its notebook is chosen later)
                let Some((row, at)) = place else {
                    continue;
                };
                ui.invoke_tab_menu_requested(
                    i32::try_from(row).unwrap(),
                    i32::try_from(at).unwrap(),
                    30.0,
                    55.0,
                );
                choose(&ui, 0, "pages");
                if action[0] == "menu" {
                    assert_eq!(json!(labels(&ui, 1)), step["pages"], "{context}");
                    // (closed again by choosing nothing: the shown page
                    // is as it was)
                    ui.invoke_menu_dismissed();
                } else {
                    choose(&ui, 1, action[2].as_str().unwrap());
                }
            }
            other => panic!("{other}"),
        }
        assert_eq!(state(), step["state"], "{context}");
    }
}
