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
    Entry, GROUPS, Slots, facts, info_menu, menu, open_menu, share_menu,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::{Value, json};

/// Which of the reference's entries hydrus-rs has.
enum Kept {
    No,
    /// The entry, and all of a submenu's entries.
    All,
    /// A submenu with just these of its entries.
    Only(&'static [&'static str]),
}

fn kept(label: &str) -> Kept {
    match label {
        "manage" => Kept::Only(&["tags"]),
        "open" => Kept::Only(&[
            "in a new page",
            "in a new duplicate filter page",
            "similar files in a new page",
            "using Default OS File Launch",
            "in web browser",
            "focused file using Default OS File Launch",
            "focused file in web browser",
        ]),
        "share" => Kept::Only(&[
            "copy paths",
            "copy hashes",
            "copy file ids",
            "copy path",
            "copy hash",
            "copy file id (",
        ]),
        "select"
        | "remove"
        | "delete"
        | "delete selected"
        | "refresh"
        | "archive/delete filter"
        | "archive"
        | "archive selected"
        | "re-inbox"
        | "re-inbox selected"
        | "delete trash physically now"
        | "delete physically now"
        | "delete selected physically now"
        | "undelete"
        | "undelete selected" => Kept::All,
        l if l.starts_with("delete from ") => Kept::All,
        _ => Kept::No,
    }
}

/// Separators as Qt shows them: none first or last, none doubled.
fn tidy(entries: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for e in entries {
        if e == "---" && out.last().is_none_or(|l| *l == "---") {
            continue;
        }
        out.push(e);
    }
    while out.last().is_some_and(|l| *l == "---") {
        out.pop();
    }
    out
}

fn pruned(entries: &[Value]) -> Vec<Value> {
    tidy(
        entries
            .iter()
            .filter_map(|e| {
                if e == "---" {
                    return Some(e.clone());
                }
                if let Some(text) = e.as_str() {
                    return match kept(text) {
                        Kept::No => None,
                        _ => Some(e.clone()),
                    };
                }
                let label = e["menu"].as_str().unwrap();
                let only = match kept(label) {
                    Kept::No => return None,
                    Kept::All => None,
                    Kept::Only(only) => Some(only),
                };
                let inner: Vec<Value> = e["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|x| {
                        // (a pattern ending "(" takes labels it starts)
                        let label = x.as_str().or_else(|| x["menu"].as_str()).unwrap_or("");
                        *x == "---"
                            || only.is_none_or(|o| {
                                o.iter().any(|p| {
                                    label == *p || (p.ends_with('(') && label.starts_with(p))
                                })
                            })
                    })
                    .map(|x| match x["menu"].as_str() {
                        // (less the distance chooser)
                        Some(sub @ "similar files in a new page") => {
                            let inner: Vec<Value> = x["entries"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|y| *y != "custom")
                                .cloned()
                                .collect();
                            json!({ "menu": sub, "entries": tidy(inner) })
                        }
                        _ => x.clone(),
                    })
                    .collect();
                Some(json!({ "menu": label, "entries": tidy(inner) }))
            })
            .collect(),
    )
}

fn described(entries: &[Entry]) -> Vec<Value> {
    tidy(
        entries
            .iter()
            .map(|e| match e {
                Entry::Separator => json!("---"),
                Entry::Item(label, _) | Entry::Label(label) => json!(label),
                Entry::Menu(label, inner) => json!({ "menu": label, "entries": described(inner) }),
            })
            .collect(),
    )
}

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
        let files = facts(&store, &ids(&page["files"]));
        for case in page["menus"].as_array().unwrap() {
            let selected: HashSet<HashId> = ids(&case["selected"]).into_iter().collect();
            let in_order = ids(&case["selected"]);
            let info = info_menu(
                &store,
                in_order.first().copied(),
                &in_order,
                &InfoLineSettings::default(),
                now_ms,
            );
            let share = (!selected.is_empty())
                .then(|| share_menu(&store, &files, in_order.first().copied(), &in_order));
            let open = open_menu(&store, in_order.first().copied(), in_order.len());
            let entries = menu(&snapshot.services, &files, &selected, info, open, share);
            let ours = described(&entries);
            // (and the window's template shows it as it is)
            let slots = Slots::new(&entries);
            assert!(slots.select.len() <= GROUPS && slots.remove.len() <= GROUPS);
            assert_eq!(described(&slots.entries()), ours);
            let mut theirs = case["menu"].as_array().unwrap().clone();
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
                ours == theirs,
                "{}: {}\n{}\n!=\n{}",
                page["page"],
                case["selection"],
                serde_json::to_string_pretty(&ours).unwrap(),
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
    let at = slint::LogicalPosition::new(300.0 + 4.0 + 154.0 + 75.0, 4.0 + 75.0);
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
