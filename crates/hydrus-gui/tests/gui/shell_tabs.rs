//! Selecting pages from the main window: clicking a tab and the
//! ctrl+page up / ctrl+page down keys, which the reference's
//! `MoveSelection` handles (the deepest notebook first, unless one above
//! it moved in the last three seconds).

use std::sync::Arc;

use slint::platform::{Key, WindowEvent};
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

fn page(name: &str, content: PageContent) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content,
    }
}

fn search(name: &str) -> Page {
    page(
        name,
        PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
            lock: None,
            collect: None,
        },
    )
}

// leaf: audit-options-tabs-selection
#[test]
fn clicking_a_tab_and_ctrl_page_keys_select_pages_and_restore_their_state() {
    let (_dirs, store) = store();
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("a"),
            page("n", PageContent::Pages(vec![search("b"), search("c")])),
            search("d"),
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    ui.show().unwrap();
    let window = windows.get(0).unwrap();
    headless::render(&window, 1100, 700);
    let shown = || bound.pages.borrow().shown().name.clone();
    // (focus the window as a click on it would)
    let position = slint::LogicalPosition::new(600.0, 400.0);
    for event in [
        WindowEvent::PointerMoved { position },
        WindowEvent::PointerPressed {
            position,
            button: slint::platform::PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        },
    ] {
        ui.window().dispatch_event(event);
    }
    let tap = |k: Key, control: bool| {
        if control {
            ui.window().dispatch_event(WindowEvent::KeyPressed {
                text: Key::Control.into(),
            });
        }
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: k.into() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text: k.into() });
        if control {
            ui.window().dispatch_event(WindowEvent::KeyReleased {
                text: Key::Control.into(),
            });
        }
        headless::render(&window, 1100, 700);
    };

    // the reference's steps (tab_selection.json, recorded by
    // oracle/record_tab_selection.py: setCurrentIndex for a click on a tab,
    // MoveSelection(+-1) for the keys), replayed with real clicks on the
    // tabs and real key events; after each, the page shown and the selected
    // tab of the top row and of n's row
    let recorded = hydrus_testkit::fixture_json("tab_selection.json");
    let top_names = ["a", "n", "d"];
    for step in recorded["steps"].as_array().unwrap() {
        let do_ = step["do"].as_array().unwrap();
        match do_[0].as_str().unwrap() {
            "click" => {
                let name = top_names[do_[1].as_u64().unwrap() as usize];
                widgets::click_nth(ui.window(), name, 0);
            }
            "click inner" => {
                widgets::click(ui.window(), ["b", "c"][do_[1].as_u64().unwrap() as usize]);
            }
            "key" => match do_[1].as_str().unwrap() {
                "ctrl+page down" => tap(Key::PageDown, true),
                "ctrl+page up" => tap(Key::PageUp, true),
                other => panic!("{other}"),
            },
            other => panic!("{other}"),
        }
        headless::render(&window, 1100, 700);
        let rows = ui.get_tab_rows();
        assert_eq!(shown(), step["shown"].as_str().unwrap(), "after {do_:?}");
        assert_eq!(
            i64::from(rows.row_data(0).unwrap().selected),
            step["top"].as_i64().unwrap(),
            "after {do_:?}: top tab"
        );
        // (the row of n's tabs is there exactly while one of its pages is shown)
        let in_n = step["shown"] == "b" || step["shown"] == "c";
        assert_eq!(
            rows.row_count(),
            if in_n { 2 } else { 1 },
            "after {do_:?}: rows"
        );
        if in_n {
            assert_eq!(
                i64::from(rows.row_data(1).unwrap().selected),
                step["nested"].as_i64().unwrap(),
                "after {do_:?}: n's tab"
            );
        }
    }
}

/// A page locked to the first `n` of `files`.
fn locked_page(store: &Arc<Store>, name: &str, files: &[hydrus_core::HashId], n: usize) -> Page {
    use hydrus_core::search::predicate::{FileHashes, Predicate, SystemPredicate};
    let hashes = store
        .read(|c| hydrus_store::master::hashes(c, &files[..n]))
        .unwrap();
    let search = FileSearchContext {
        predicates: vec![Predicate::System(SystemPredicate::Hash {
            hashes: FileHashes::Sha256(hashes.values().copied().collect()),
            inclusive: true,
        })],
        ..FileSearchContext::default()
    };
    page(
        name,
        PageContent::Search {
            search,
            synchronised: true,
            sort: None,
            lock: Some(hydrus_core::pages::HashLock::default()),
            collect: None,
        },
    )
}

// leaf: audit-options-tabs-selection
#[test]
fn pages_not_yet_shown_carry_their_file_counts_and_open_with_that_many_files() {
    // oracle/record_tab_selection.py: file pages loaded from a session (a:3,
    // n[b:5, c:2], d:4 files), none shown: the reference's tabs say what they
    // will hold, and every page does hold that many once loaded
    let recorded = hydrus_testkit::fixture_json("tab_selection.json");
    let lazy = &recorded["lazy"];
    let size = |name: &str| lazy["sizes"][name].as_u64().unwrap() as usize;
    let loaded = &lazy["steps"][1];
    let (_dirs, store) = store();
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(hydrus_testkit::fixture_path(
            "legacy_db/basic.manifest.json",
        ))
        .unwrap(),
    )
    .unwrap();
    let files: Vec<hydrus_core::HashId> = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| {
            std::path::Path::new(f["name"].as_str().unwrap())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
        })
        .take(5)
        .map(|f| {
            let hash: hydrus_core::Sha256 = f["hash"].as_str().unwrap().parse().unwrap();
            store
                .read(|c| hydrus_store::master::hash_id(c, &hash))
                .unwrap()
                .unwrap()
        })
        .collect();
    let (a, b, c, d) = (
        locked_page(&store, "a", &files, size("a")),
        locked_page(&store, "b", &files, size("b")),
        locked_page(&store, "c", &files, size("c")),
        locked_page(&store, "d", &files, size("d")),
    );
    let keys = [a.key, b.key, c.key, d.key];
    let counts = ["a", "b", "c", "d"].map(size);
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![a, page("n", PageContent::Pages(vec![b, c])), d],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 0)?;
            // (what the session kept of each page's files when it was saved)
            for (key, n) in keys.iter().zip(counts) {
                sessions::set_page_files(ctx.conn(), key, &files[..n])?;
            }
            Ok(())
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let names = |row: usize| -> Vec<String> {
        ui.get_tab_rows()
            .row_data(row)
            .unwrap()
            .names
            .iter()
            .map(|n| n.to_string())
            .collect()
    };
    // the tabs of the loaded session carry the reference's counts and
    // names (n sums its pages and says it holds more)
    assert_eq!(
        names(0),
        loaded["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    // selecting each opens it with that many files
    for (tab, name) in [(0, "a"), (2, "d")] {
        ui.invoke_tab_chosen(0, tab);
        assert_eq!(bound.pages.borrow().shown().name, name);
        assert_eq!(
            bound.current.borrow().borrow().results().len(),
            loaded["pages"][name]["files"].as_u64().unwrap() as usize,
            "{name}"
        );
    }
    ui.invoke_tab_chosen(0, 1);
    for (tab, name) in [(0, "b"), (1, "c")] {
        ui.invoke_tab_chosen(1, tab);
        assert_eq!(bound.pages.borrow().shown().name, name);
        assert_eq!(
            bound.current.borrow().borrow().results().len(),
            loaded["pages"][name]["files"].as_u64().unwrap() as usize,
            "{name}"
        );
    }
    assert_eq!(
        names(1),
        loaded["inner tabs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
}

// leaf: audit-options-tabs-context-action-2120-selectable-page
#[test]
fn tab_menu_pages_entry_shows_the_chosen_page() {
    let (_dirs, store) = store();
    let session = Session {
        name: LAST_SESSION.into(),
        pages: vec![
            search("a"),
            page("n", PageContent::Pages(vec![search("b"), search("c")])),
        ],
    };
    store
        .write(move |ctx| sessions::save(ctx.conn(), &session, 0))
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    let choose = |pane: i32, label: &str| {
        let lines = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
        let index = (0..lines.row_count())
            .find(|&i| lines.row_data(i).unwrap().label.starts_with(label))
            .unwrap_or_else(|| panic!("{label}"));
        ui.invoke_menu_line_clicked(pane, index as i32, 200.0, 100.0, 10.0);
    };
    assert_eq!(bound.pages.borrow().shown().name, "a");
    // right-click the notebook's tab: "pages" lists the pages inside it,
    // and choosing one shows it (`ShowPage`)
    ui.invoke_tab_menu_requested(0, 1, 30.0, 55.0);
    choose(0, "pages");
    choose(1, "c");
    assert_eq!(bound.pages.borrow().shown().name, "c");
    assert_eq!(ui.get_menu_panes().row_count(), 0, "the menu closed");
    // a media page's tab lists every media page of its notebook
    ui.invoke_tab_menu_requested(0, 0, 30.0, 55.0);
    choose(0, "pages");
    choose(1, "b");
    assert_eq!(bound.pages.borrow().shown().name, "b");
    ui.invoke_tab_menu_requested(0, 1, 30.0, 55.0);
    choose(0, "pages");
    choose(1, "c");
    assert_eq!(bound.pages.borrow().shown().name, "c");
}
