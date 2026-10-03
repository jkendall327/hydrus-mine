//! The thumbnails' right-click menu is the reference's
//! (`oracle/record_thumbnail_menu.py`, its own `GetMenu` on pages of the
//! `basic` fixture's files with several selections), for the entries
//! hydrus-rs has so far: the reference's menu is compared with the others
//! left out.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::media_viewer::InfoLineSettings;
use hydrus_gui::thumbnail_menu::{
    GROUPS, Slots, facts, info_menu, menu, open_menu, rearrange_menu, share_menu, url_facts,
    urls_menu,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::{Value, json};

use crate::common::menus::{as_recorded, described, pruned, tidy, unescaped};

#[test]
fn the_menu_is_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_menu.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let snapshot = store.snapshot();
    let now_ms = fixture["now"].as_i64().unwrap() * 1000;
    let ids = |hashes: &Value| -> Vec<HashId> {
        hashes
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                store
                    .read(|c| {
                        hydrus_store::master::hash_id(c, &h.as_str().unwrap().parse().unwrap())
                    })
                    .unwrap()
                    .unwrap()
            })
            .collect()
    };
    let mut checked = 0;
    for page in fixture["pages"].as_array().unwrap() {
        let in_page = ids(&page["files"]);
        let files = facts(&store, &in_page);
        for case in page["menus"].as_array().unwrap() {
            let selected: HashSet<HashId> = ids(&case["selected"]).into_iter().collect();
            let in_order = ids(&case["selected"]);
            let info = info_menu(
                &store,
                in_order.first().copied(),
                (&in_order, hydrus_gui::status::Items::files(in_order.len())),
                &InfoLineSettings::default(),
                now_ms,
            );
            let share = (!selected.is_empty())
                .then(|| share_menu(&store, &files, in_order.first().copied(), &in_order));
            let open = open_menu(&store, in_order.first().copied(), in_order.len());
            let urls = (!selected.is_empty())
                .then(|| urls_menu(&url_facts(&store, in_order.first().copied(), &in_order)))
                .flatten();
            // (the recorder selects with a click, then ctrl+clicks, which
            // don't move the focus by hydrus's default: the first file
            // selected is focused)
            let rearrange = rearrange_menu(&in_page, &selected, in_order.first().copied());
            let entries = menu(
                &snapshot.services,
                &files,
                &selected,
                info,
                urls,
                open,
                share,
                rearrange,
                // (the basic fixture's files have no notes)
                None,
            );
            let ours = described(&entries);
            let recorded_ours = as_recorded(&ours);
            // (and the window's template shows it as it is)
            let slots = Slots::new(&entries);
            assert!(slots.select.len() <= GROUPS && slots.remove.len() <= GROUPS);
            assert_eq!(described(&slots.entries()), ours);
            let mut theirs = unescaped(case["menu"].as_array().unwrap());
            // (the selection's info first, less what hydrus-rs doesn't have)
            let info = (!selected.is_empty()).then(|| theirs.remove(0));
            let mut theirs = pruned(&theirs);
            if let Some(mut info) = info {
                if let Some(entries) = info.get_mut("entries") {
                    let kept: Vec<Value> = entries
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| *e != "show detailed embedded file metadata")
                        .cloned()
                        .collect();
                    *entries = Value::Array(tidy(kept));
                }
                theirs.insert(0, info);
                theirs.insert(1, json!("---"));
                theirs = tidy(theirs);
            }
            assert!(
                recorded_ours == theirs,
                "{}: {}\n{}\n!=\n{}",
                page["page"],
                case["selection"],
                serde_json::to_string_pretty(&recorded_ours).unwrap(),
                serde_json::to_string_pretty(&theirs).unwrap(),
            );
            checked += 1;
        }
    }
    assert!(checked >= 10, "{checked}");
}

#[test]
fn a_right_click_shows_the_menu_and_its_entries_act() {
    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use slint::Model as _;
    use slint::platform::{PointerEventButton, WindowEvent};

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let files = page.borrow().results().to_vec();
    let main_window = windows.get(0).unwrap();
    headless::render(&main_window, 1100, 700);

    // a right-click on the second thumbnail selects it, and shows the menu
    let at = slint::LogicalPosition::new(300.0 + 4.0 + 156.0 + 76.0, 4.0 + 63.0);
    main_window.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button: PointerEventButton::Right,
    });
    main_window.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button: PointerEventButton::Right,
    });
    assert_eq!(
        page.borrow()
            .selected_indices()
            .into_iter()
            .collect::<Vec<_>>(),
        [1]
    );
    let pixels = headless::render(&main_window, 1100, 700);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("thumbnail_menu.png"), &pixels, 1100, 700).unwrap();
    let menu = ui.get_thumbnail_menu();
    let labels = |rows: slint::ModelRc<hydrus_gui::MenuRow>| -> Vec<(String, i32)> {
        (0..rows.row_count())
            .map(|i| {
                let row = rows.row_data(i).unwrap();
                (row.label.to_string(), row.id)
            })
            .collect()
    };
    assert_eq!(labels(menu.head.clone())[0].0, "refresh");
    assert_eq!(labels(menu.manage.clone())[0].0, "tags");
    assert_eq!(
        labels(menu.open_b.clone())[0].0,
        "using Default OS File Launch"
    );
    let find = |rows: slint::ModelRc<hydrus_gui::MenuRow>, label: &str| {
        labels(rows)
            .into_iter()
            .find(|(l, _)| l.starts_with(label))
            .unwrap_or_else(|| panic!("{label}"))
            .1
    };
    // select → inbox
    let inbox = store
        .read(|c| hydrus_store::media::inboxed(c, &files))
        .unwrap();
    ui.invoke_menu_chosen(find(menu.select.g2.clone(), "inbox ("));
    let selected = page.borrow().selected_files();
    assert_eq!(selected.len(), inbox.len());
    assert!(selected.iter().all(|f| inbox.contains(f)));
    // open → in a new page: a new tab with them
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    let tabs_before = bound.pages.borrow().tabs()[0].names.len();
    ui.invoke_menu_chosen(find(menu.open_a.clone(), "in a new page"));
    assert_eq!(bound.pages.borrow().tabs()[0].names.len(), tabs_before + 1);
    let opened = bound.current.borrow().clone();
    assert_eq!(opened.borrow().results(), selected.as_slice());
    // remove → selected: they leave that page, not the store
    opened.borrow_mut().select_files(&selected[..2]);
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    ui.invoke_menu_chosen(find(menu.remove.g1.clone(), "selected (2)"));
    assert_eq!(opened.borrow().results(), &selected[2..]);
    // delete from a domain asks first
    opened.borrow_mut().select_files(&selected[2..3]);
    ui.invoke_thumbnail_menu_requested(-1);
    let menu = ui.get_thumbnail_menu();
    let deletes = labels(menu.delete.clone());
    assert!(deletes[0].0.starts_with("delete from "), "{deletes:?}");
    ui.invoke_menu_chosen(deletes[0].1);
    assert!(
        ui.get_question().starts_with("Delete this file from "),
        "{}",
        ui.get_question()
    );
    ui.invoke_answer(false);
    // open → similar files in a new page → exact match, on a still
    // image's thumbnail: a new tab searching for it, which finds itself
    let at = opened
        .borrow()
        .results()
        .iter()
        .position(|&f| hydrus_gui::thumbnail_menu::similar_search(&store, &[f], 0).is_some())
        .unwrap();
    let still = opened.borrow().results()[at];
    ui.invoke_thumbnail_menu_requested(i32::try_from(at).unwrap());
    let menu = ui.get_thumbnail_menu();
    assert_eq!(menu.open_similar_title, "similar files in a new page");
    let tabs_before = bound.pages.borrow().tabs()[0].names.len();
    ui.invoke_menu_chosen(find(menu.open_similar.clone(), "exact match"));
    assert_eq!(bound.pages.borrow().tabs()[0].names.len(), tabs_before + 1);
    let similar = bound.current.borrow().clone();
    assert!(!Rc::ptr_eq(&similar, &opened));
    assert_eq!(
        similar.borrow().predicates(),
        ["system:similar to 1 files with distance of 0"]
    );
    assert!(similar.borrow().results().contains(&still));
}

#[test]
fn the_urls_menu_opens_pages_of_a_url_and_asks_before_opening_several() {
    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use slint::Model as _;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    // a file with several URLs
    let (index, facts) = page
        .borrow()
        .results()
        .iter()
        .enumerate()
        .map(|(i, &f)| (i, url_facts(&store, Some(f), &[f])))
        .find(|(_, facts)| facts.focus.len() > 1)
        .unwrap();
    let file = page.borrow().results()[index];
    let rows = |groups: hydrus_gui::MenuGroups| -> Vec<(String, i32)> {
        [
            groups.g1, groups.g2, groups.g3, groups.g4, groups.g5, groups.g6,
        ]
        .into_iter()
        .flat_map(|g| (0..g.row_count()).map(move |i| g.row_data(i).unwrap()))
        .map(|r| (r.label.to_string(), r.id))
        .collect()
    };
    ui.invoke_thumbnail_menu_requested(i32::try_from(index).unwrap());
    let menu = ui.get_thumbnail_menu();
    assert!(menu.has_urls && menu.has_url_pages);
    let pages = rows(menu.urls_pages.clone());
    let (label, url) = &facts.focus[0];
    let first = pages
        .iter()
        .find(|(l, _)| *l == format!("files with {label}"))
        .unwrap()
        .1;
    // files with the URL: a "url search" page on all my files, finding it
    let tabs_before = bound.pages.borrow().tabs()[0].names.len();
    ui.invoke_menu_chosen(first);
    let names = bound.pages.borrow().tabs()[0].names.clone();
    assert_eq!(names.len(), tabs_before + 1);
    assert_eq!(names.last().unwrap(), "url search");
    let found = bound.current.borrow().clone();
    assert_eq!(
        found.borrow().predicates(),
        [format!("system:has url {url}")]
    );
    assert!(found.borrow().results().contains(&file));
    // back on the first page, opening all its URLs asks first
    ui.invoke_tab_chosen(0, 0);
    ui.invoke_thumbnail_menu_requested(i32::try_from(index).unwrap());
    let visit = rows(ui.get_thumbnail_menu().urls_visit);
    let all = visit
        .iter()
        .find(|(l, _)| l.starts_with("this file's ") && l.ends_with(" urls"))
        .unwrap()
        .1;
    ui.invoke_menu_chosen(all);
    assert_eq!(
        ui.get_question(),
        format!("Open the {} URLs in your web browser?", facts.focus.len())
    );
    ui.invoke_answer(false);
    assert_eq!(ui.get_question(), "");
}
