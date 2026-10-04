//! Popup new-page insertion, real chooser cancellation and frozen destinations.
use hydrus_core::pages::{Page, PageContent, PageKey, Session};
use hydrus_gui::{MainWindow, Pages, bind, headless, page_chooser::NewPage};
use hydrus_store::{Store, sessions};
use slint::{ComponentHandle as _, Model as _};
use std::sync::Arc;

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

fn notebook(name: &str, children: Vec<Page>) -> Page {
    Page {
        key: PageKey::random(),
        name: name.into(),
        content: PageContent::Pages(children),
    }
}

fn source() -> Vec<Page> {
    ["first", "second", "third"]
        .iter()
        .map(|name| notebook(name, vec![]))
        .collect()
}

fn choose(ui: &MainWindow, label: &str) {
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = (0..lines.row_count())
        .find(|&i| lines.row_data(i).unwrap().label == label)
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 200.0, 100.0, 10.0);
}

fn pick_notebook(ui: &MainWindow) {
    ui.invoke_chooser_pressed(6);
    ui.invoke_chooser_pressed(8);
}

fn insertion_option(
    ui: &MainWindow,
    bound: &hydrus_gui::Bound,
) -> (hydrus_gui::OptionsWindow, i32) {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    choose(ui, "options\u{2026}");
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let pages = options.get_pages();
    let page = (0..pages.row_count())
        .find(|&i| pages.row_data(i).unwrap().text == "gui pages")
        .unwrap();
    let page = i32::try_from(page).unwrap();
    options.set_page(page);
    options.invoke_page_chosen(page);
    let rows = options.get_rows();
    let row = (0..rows.row_count())
        .find(|&i| rows.row_data(i).unwrap().label == "Put new page tabs on: ")
        .unwrap();
    (options, i32::try_from(row).unwrap())
}

#[test]
fn popup_new_page_and_here_replay_real_reference_order_selection_and_cancellation() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let fixture = hydrus_testkit::fixture_json("tab_new_page.json");
    for step in fixture["steps"].as_array().unwrap() {
        let original = source();
        let selected =
            original[usize::try_from(step["initial_selected"].as_u64().unwrap()).unwrap()].key;
        let mode = hydrus_store::settings::PageInsertion::from_code(step["mode"].as_i64().unwrap())
            .unwrap();
        let session = Session {
            name: sessions::LAST_SESSION.into(),
            pages: original,
        };
        store
            .write(move |ctx| {
                sessions::save(ctx.conn(), &session, 1)?;
                sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&selected))?;
                hydrus_store::settings::set(ctx.conn(), &mode)
            })
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::open(store.clone()).unwrap());
        ui.invoke_tab_menu_requested(0, 1, 30.0, 55.0);
        choose(
            &ui,
            if step["here"] == true {
                "new page here"
            } else {
                "new page"
            },
        );
        assert!(ui.get_chooser_labels().row_count() > 0);
        if step["cancel"] == true {
            ui.invoke_chooser_cancel();
        } else {
            pick_notebook(&ui);
        }
        assert_eq!(ui.get_chooser_labels().row_count(), 0);
        let pages = bound.pages.borrow();
        let names: Vec<_> = pages
            .session()
            .pages
            .iter()
            .map(|page| page.name.clone())
            .collect();
        assert_eq!(serde_json::json!(names), step["names"]);
        let children: Vec<Vec<_>> = pages
            .session()
            .pages
            .iter()
            .map(|page| match &page.content {
                PageContent::Pages(children) => {
                    children.iter().map(|child| child.name.clone()).collect()
                }
                _ => panic!("notebook"),
            })
            .collect();
        assert_eq!(serde_json::json!(children), step["children"]);
        assert_eq!(
            pages.tabs()[0].selected as u64,
            step["selected"].as_u64().unwrap()
        );
        let tree = pages.session().pages.clone();
        let shown = pages.shown().key;
        drop(pages);
        (bound.sync)();
        let reopened = Pages::open(store.clone()).unwrap();
        assert_eq!(reopened.session().pages, tree);
        assert_eq!(reopened.shown().key, shown);
    }
}

#[test]
fn nested_chooser_freezes_parent_and_anchor_and_cancel_clears_pending_position() {
    let _windows = headless::init();
    let (_dirs, store) = store();
    let children = source();
    let selected = children[2].key;
    let parent = notebook("parent", children);
    let parent_key = parent.key;
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: vec![parent, notebook("outside", vec![])],
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&selected))
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_tab_menu_requested(1, 1, 30.0, 55.0);
    choose(&ui, "new page here");
    bound.pages.borrow_mut().select(0, 1);
    pick_notebook(&ui);
    let pages = bound.pages.borrow();
    let PageContent::Pages(children) = &pages.session().pages[0].content else {
        unreachable!()
    };
    assert_eq!(
        children
            .iter()
            .map(|page| page.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "pages", "second", "third"]
    );
    assert_eq!(pages.session().pages.len(), 2);
    assert_eq!(pages.tabs()[0].selected, 0);
    assert_eq!(pages.tabs()[1].selected, 1);
    drop(pages);
    ui.invoke_tab_menu_requested(1, 2, 30.0, 55.0);
    choose(&ui, "new page here");
    ui.invoke_chooser_cancel();
    bound.pages.borrow_mut().select(0, 1);
    ui.invoke_tab_menu_requested(0, -1, 30.0, 55.0);
    choose(&ui, "new page");
    pick_notebook(&ui);
    assert_eq!(bound.pages.borrow().session().pages.len(), 3);
    assert_eq!(bound.pages.borrow().session().pages[2].name, "pages");
    assert_eq!(bound.pages.borrow().session().pages[0].key, parent_key);

    (bound.sync)();
    let mut pages = Pages::open(store.clone()).unwrap();
    assert!(pages.new_page_at(Some(PageKey::random()), None).is_err());
    assert!(
        pages
            .new_page_at(Some(parent_key), Some(pages.session().pages[2].key))
            .is_err()
    );
    pages.new_page_at(Some(parent_key), None).unwrap();
    pages.close(0, 0).unwrap();
    let count = store
        .read(|conn| {
            Ok(
                conn.query_row("SELECT count(*) FROM import_queues", [], |row| {
                    row.get::<_, i64>(0)
                })?,
            )
        })
        .unwrap();
    let before = pages.session().pages.clone();
    assert!(pages.new_page(&NewPage::Urls).is_err());
    assert_eq!(pages.session().pages, before);
    assert_eq!(
        store
            .read(|conn| Ok(
                conn.query_row("SELECT count(*) FROM import_queues", [], |row| row
                    .get::<_, i64>(0))?
            ))
            .unwrap(),
        count
    );
}

#[test]
fn insertion_option_imports_rejects_invalid_cancels_and_changes_real_chooser_consumer() {
    use hydrus_store::settings::{self, PageInsertion};
    let _windows = headless::init();
    let (_dirs, store) = store();
    assert_eq!(
        store.read(settings::get::<PageInsertion>).unwrap(),
        PageInsertion::FarRight
    );
    assert!(PageInsertion::from_code(-1).is_none());
    assert!(PageInsertion::from_code(4).is_none());
    let original = source();
    let selected = original[2].key;
    let session = Session {
        name: sessions::LAST_SESSION.into(),
        pages: original,
    };
    store
        .write(move |ctx| {
            sessions::save(ctx.conn(), &session, 1)?;
            sessions::set_shown(ctx.conn(), sessions::LAST_SESSION, Some(&selected))
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let (window, row) = insertion_option(&ui, &bound);
    window.invoke_choice_chosen(row, 999);
    window.invoke_apply();
    assert_eq!(
        store.read(settings::get::<PageInsertion>).unwrap(),
        PageInsertion::FarRight
    );
    let (window, row) = insertion_option(&ui, &bound);
    window.invoke_choice_chosen(row, 0);
    window.invoke_cancel();
    assert_eq!(
        store.read(settings::get::<PageInsertion>).unwrap(),
        PageInsertion::FarRight
    );
    let (window, row) = insertion_option(&ui, &bound);
    window.invoke_choice_chosen(row, 0);
    window.invoke_apply();
    assert_eq!(
        store.read(settings::get::<PageInsertion>).unwrap(),
        PageInsertion::FarLeft
    );
    ui.invoke_tab_menu_requested(0, 1, 30.0, 55.0);
    choose(&ui, "new page");
    pick_notebook(&ui);
    assert_eq!(bound.pages.borrow().session().pages[0].name, "pages");
    assert_eq!(bound.pages.borrow().session().pages[3].key, selected);
    (bound.sync)();
    let reopened = Pages::open(store.clone()).unwrap();
    assert_eq!(
        reopened.session().pages,
        bound.pages.borrow().session().pages
    );
    assert_eq!(
        store.read(settings::get::<PageInsertion>).unwrap(),
        PageInsertion::FarLeft
    );
}
