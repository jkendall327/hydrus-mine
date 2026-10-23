//! Help > debug > gui actions > "publish some sub files in five seconds",
//! against the reference's (`oracle/fixtures/debug_sub_files.json`, from
//! `oracle/record_debug_sub_files.py`): five seconds on, five random hashes
//! are published to the page named "example sub files", as a subscription
//! publishes its files; with no such page, one is opened in the current
//! notebook, searching all local files for a locked `system:hash` of them; a
//! second time, the page takes the new hashes; and files the client has,
//! published to that name, join it.

use std::{cell::Cell, rc::Rc, time::Duration};

use hydrus_core::pages::{Page, PageContent};
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_gui_model::page_chooser::NewPage;
use slint::{ComponentHandle as _, Model as _};

fn choose(ui: &MainWindow, pane: i32, label: &str) {
    let rows = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let index = rows.iter().position(|row| row.label == label).unwrap();
    assert!(rows.row_data(index).unwrap().usable);
    ui.invoke_menu_line_clicked(pane, index as i32, 0.0, 0.0, 0.0);
}

/// Every page as the recording lists them: name, notebook, index, whether a
/// notebook, depth first.
fn pages(pages: &[Page], parent: Option<&str>, out: &mut Vec<serde_json::Value>) {
    for (index, page) in pages.iter().enumerate() {
        let notebook = matches!(page.content, PageContent::Pages(_));
        out.push(serde_json::json!({
            "name": page.name, "notebook": parent, "index": index, "is_notebook": notebook,
        }));
        if let PageContent::Pages(children) = &page.content {
            self::pages(children, Some(&page.name), out);
        }
    }
}

// leaf: audit-options-help-debug-action-publish-some-sub-files-in-five-seconds
#[test]
fn sub_files_are_published_to_their_named_page_five_seconds_on() {
    let recorded = hydrus_testkit::fixture_json("debug_sub_files.json");
    assert_eq!(
        recorded["menu_path"],
        serde_json::json!([
            "help",
            "debug",
            "gui actions",
            "publish some sub files in five seconds"
        ])
    );
    for item in recorded["schedule"].as_array().unwrap() {
        assert_eq!(item["delay_seconds"], 5);
    }
    let (_dirs, store) = super::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let now = Rc::new(Cell::new(Duration::ZERO));
    bound.debug_long_popup.set_clock(Rc::new({
        let now = now.clone();
        move || now.get()
    }));

    // the recording's pages: its first page, then a notebook "nested" holding
    // "inner", shown
    let first = recorded["before"]["pages"][0]["name"].as_str().unwrap();
    let shown = bound.pages.borrow().shown().key;
    bound.pages.borrow_mut().rename_key(&shown, first);
    bound.pages.borrow_mut().new_page(&NewPage::Pages).unwrap();
    let nested = bound.pages.borrow().tabs()[1].parent.unwrap();
    bound.pages.borrow_mut().rename_key(&nested, "nested");
    let inner = bound.pages.borrow().shown().key;
    bound.pages.borrow_mut().rename_key(&inner, "inner");
    // the pages, the one shown, and the page named for the sub files
    let state = || {
        fn named(pages: &[Page]) -> Option<hydrus_core::pages::PageKey> {
            pages.iter().find_map(|page| match &page.content {
                PageContent::Pages(children) => named(children),
                _ if page.name == hydrus_gui::debug_long_popup::SUB_FILES_PAGE => Some(page.key),
                _ => None,
            })
        }
        let (listed, current, key) = {
            let now = bound.pages.borrow();
            let mut listed = Vec::new();
            pages(&now.session().pages, None, &mut listed);
            (
                listed,
                now.shown().name.clone(),
                named(&now.session().pages),
            )
        };
        let page = key.and_then(|key| bound.pages.borrow_mut().page(&key));
        (listed, current, page)
    };
    let (listed, current, named) = state();
    assert_eq!(serde_json::Value::from(listed), recorded["before"]["pages"]);
    assert_eq!(current, recorded["before"]["current"].as_str().unwrap());
    assert!(named.is_none());

    // the menu: nothing until five seconds on
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    choose(&ui, 0, "debug");
    choose(&ui, 1, "gui actions");
    choose(&ui, 2, "publish some sub files in five seconds");
    assert_eq!(bound.debug_long_popup.pending_sub_files(), 1);
    now.set(Duration::from_millis(4500));
    bound.debug_long_popup.tick();
    assert_eq!(
        state().2.is_some(),
        recorded["at_four_and_a_half_seconds"]["named"]
            .as_bool()
            .unwrap()
    );
    now.set(Duration::from_secs(5));
    bound.debug_long_popup.tick();
    assert_eq!(bound.debug_long_popup.pending_sub_files(), 0);

    let compare = |step: &str, real: &[String]| {
        let wanted = &recorded[step];
        let (listed, current, named) = state();
        assert_eq!(serde_json::Value::from(listed), wanted["pages"], "{step}");
        assert_eq!(current, wanted["current"].as_str().unwrap(), "{step}");
        let page = named.expect("the page");
        let page = page.borrow();
        let location = page.location();
        assert_eq!(
            serde_json::json!({
                "current": location.current().iter().map(|k| hex::encode(k.as_bytes())).collect::<Vec<_>>(),
                "deleted": location.deleted().iter().map(|k| hex::encode(k.as_bytes())).collect::<Vec<_>>(),
            }),
            wanted["named"]["location"],
            "{step}"
        );
        assert_eq!(
            serde_json::Value::from(page.predicates()),
            wanted["named"]["predicates"],
            "{step}"
        );
        // the files it shows: the reference's that the client has (it also shows
        // the random hashes it does not have as files: DIFFERENCES.md)
        let shown: Vec<String> = store
            .read(|c| hydrus_store::master::hashes(c, &page.files()))
            .unwrap()
            .into_values()
            .map(|h| h.to_hex())
            .collect();
        let mut shown = shown;
        shown.sort();
        let theirs: Vec<String> = wanted["named"]["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap().to_owned())
            .filter(|f| real.contains(f))
            .collect();
        assert_eq!(shown, theirs, "{step}");
    };
    compare("after_first", &[]);

    // again: the page takes the new hashes; no new page
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    choose(&ui, 0, "debug");
    choose(&ui, 1, "gui actions");
    choose(&ui, 2, "publish some sub files in five seconds");
    now.set(Duration::from_secs(10));
    bound.debug_long_popup.tick();
    compare("after_second", &[]);

    // the client's own files published to that name join the page
    let real: Vec<String> = recorded["real_files_published"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_owned())
        .collect();
    let hashes: Vec<hydrus_core::Sha256> = real.iter().map(|h| h.parse().unwrap()).collect();
    // (the recording opened an "all files" page first, to find them)
    bound
        .pages
        .borrow_mut()
        .present_files(&hashes, hydrus_gui::debug_long_popup::SUB_FILES_PAGE)
        .unwrap();
    let wanted = &recorded["after_real_files"];
    let (_, current, named) = state();
    assert_eq!(current, wanted["current"].as_str().unwrap());
    let page = named.unwrap();
    assert_eq!(
        serde_json::Value::from(page.borrow().predicates()),
        wanted["named"]["predicates"]
    );
    let mut shown: Vec<String> = store
        .read(|c| hydrus_store::master::hashes(c, &page.borrow().files()))
        .unwrap()
        .into_values()
        .map(|h| h.to_hex())
        .collect();
    shown.sort();
    let mut theirs: Vec<String> = wanted["named"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap().to_owned())
        .filter(|f| real.contains(f))
        .collect();
    theirs.sort();
    assert_eq!(shown, theirs);
}
