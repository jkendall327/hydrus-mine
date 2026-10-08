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

    assert_eq!(shown(), "a");
    // clicking the notebook's tab shows its remembered page
    ui.invoke_tab_chosen(0, 1);
    assert_eq!(shown(), "b");
    assert_eq!(ui.get_tab_rows().row_count(), 2, "its own row of tabs");
    // ctrl+page down: along the deepest notebook, then out of it
    tap(Key::PageDown, true);
    assert_eq!(shown(), "c");
    tap(Key::PageUp, true);
    assert_eq!(shown(), "b");
    // clicking a tab in the inner row selects it
    ui.invoke_tab_chosen(1, 1);
    assert_eq!(shown(), "c");
    // a tab of the top row, and back: the notebook remembers "c"
    ui.invoke_tab_chosen(0, 2);
    assert_eq!(shown(), "d");
    assert_eq!(ui.get_tab_rows().row_count(), 1);
    ui.invoke_tab_chosen(0, 1);
    assert_eq!(shown(), "c");
    // the keys never go round the end
    ui.invoke_tab_chosen(0, 0);
    tap(Key::PageUp, true);
    assert_eq!(shown(), "a");
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
