//! The dialogs still open when the last clone of a binding is dropped are
//! hidden and freed with it, not kept by their own callbacks. (Closing a
//! dialog frees it already; this is for those left open, at a test's end or
//! the client's exit.)

use std::cell::RefCell;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;

thread_local! {
    static STASH: RefCell<Option<Bound>> = const { RefCell::new(None) };
}

#[test]
fn the_last_binding_frees_the_dialogs_left_open() {
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().as_weak();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().as_weak();
    assert!(manage.upgrade().unwrap().window().is_visible());
    assert!(viewer.upgrade().unwrap().window().is_visible());

    // a clone that is not the last leaves them open
    let last = bound.clone();
    drop(bound);
    assert!(manage.upgrade().unwrap().window().is_visible());
    assert!(viewer.upgrade().unwrap().window().is_visible());
    assert!(last.manage_tags.borrow().is_some());

    // the last one hides and frees them, though the collector and the main
    // window are still alive
    drop(last);
    assert!(
        manage.upgrade().is_none(),
        "the manage tags dialog is freed"
    );
    assert!(viewer.upgrade().is_none(), "the media viewer is freed");
    assert!(
        ui.window().is_visible(),
        "the main window is not the binding's"
    );
    drop(ui);
    drop(windows);
}

/// A binding kept by something a thread-local owns is dropped while the
/// thread's thread-locals are destroyed, after Slint's own: its teardown
/// must leave the windows rather than call into Slint (a panic in a
/// thread-local destructor aborts the whole test binary).
#[test]
fn a_binding_dropped_as_its_thread_ends_does_not_abort() {
    let ended = std::thread::spawn(|| {
        // (made before Slint's thread-locals, so destroyed after them)
        STASH.with(|_| ());
        let (_dirs, store) = crate::subscriptions::store();
        let windows = headless::init();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        crate::common::main_menu::choose(&ui, &["help"], "about");
        assert!(bound.about.borrow().is_some());
        let about = bound.about.borrow().as_ref().unwrap().clone_strong();
        STASH.with(|stash| *stash.borrow_mut() = Some(bound));
        drop(ui);
        drop(windows);
        // (shown again after the collector hid it, so the teardown would
        // have a visible window to hide)
        about.show().unwrap();
        drop(about);
    })
    .join();
    assert!(ended.is_ok());
}

type Alive = Box<dyn Fn() -> bool>;

/// Remember how to tell whether `w` is still alive.
fn track<T: slint::ComponentHandle + 'static>(alive: &mut Vec<Alive>, w: &T) {
    let weak = w.as_weak();
    alive.push(Box::new(move || weak.upgrade().is_some()));
}

/// The children of a dialog (and theirs) left open when the last binding
/// goes are freed too: the services editor and its service editor, the
/// network session browser and its editor, and the downloader definitions
/// list with a definition editor.
#[test]
fn the_last_binding_frees_the_nested_dialogs_left_open() {
    use hydrus_gui::downloader_definitions_window as definitions;
    use hydrus_gui::network_sessions_window as sessions;

    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());

    // network sessions: the browser and an editor child
    let browser = sessions::open(&store, &bound.network_sessions, false).unwrap();
    browser.invoke_add_clicked();
    let edit = sessions::last_edit_opened().unwrap();
    let mut alive: Vec<Box<dyn Fn() -> bool>> = Vec::new();
    track(&mut alive, &browser);
    track(&mut alive, &edit);
    drop((browser, edit));

    // downloader definitions: the list and a definition editor
    let list = definitions::open(&store, &bound.downloader_definitions, true).unwrap();
    track(&mut alive, &list);
    list.invoke_action("add".into());
    track(
        &mut alive,
        bound
            .downloader_definitions
            .class_edit
            .borrow()
            .as_ref()
            .unwrap(),
    );
    drop(list);

    // the services editor
    let titles = ui.get_menu_titles();
    let index = titles.iter().position(|t| t.label == "services").unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(index).unwrap(), 10.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let index = lines
        .iter()
        .position(|l| l.label.starts_with("edit"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    track(
        &mut alive,
        bound.services_editor.manage.borrow().as_ref().unwrap(),
    );

    assert!(alive.iter().all(|a| a()));
    drop(bound);
    for (n, a) in alive.iter().enumerate() {
        assert!(!a(), "nested dialog {n} is freed");
    }
}

/// The recursive slot types, each opened from the top and one child down:
/// the parser editors (list, page, a subsidiary page, its content parser,
/// and a formula below that) and the search OR editor.
#[test]
fn the_last_binding_frees_the_recursive_dialogs_left_open() {
    use hydrus_gui::parser_editors_window as parsers;

    type Case = (
        &'static str,
        fn(&MainWindow, &Bound, &Arc<Store>) -> Vec<Alive>,
    );
    let cases: [Case; 2] = [
        ("parser editors", |_, bound, store| {
            let mut alive = Vec::new();
            let list = parsers::open(store, &bound.parser_editors, false).unwrap();
            list.invoke_row_clicked(0, false, false);
            list.invoke_action("edit".into());
            let slots = &bound.parser_editors;
            let page = crate::parser_editors::child(&slots.page);
            page.invoke_action("add-subsidiary".into());
            let owned = (**slots.child.borrow().as_ref().unwrap()).clone();
            let subsidiary = crate::parser_editors::child(&owned.page);
            subsidiary.invoke_action("add-content".into());
            let content = crate::parser_editors::child(&owned.content);
            content.invoke_action("formula".into());
            let formula = owned
                .formula
                .formula
                .borrow()
                .as_ref()
                .unwrap()
                .clone_strong();
            track(&mut alive, &list);
            track(&mut alive, &page);
            track(&mut alive, &subsidiary);
            track(&mut alive, &content);
            track(&mut alive, &formula);
            alive
        }),
        ("search OR", |ui, bound, _| {
            let mut alive = Vec::new();
            ui.invoke_search_or_action(3);
            assert!(ui.get_search_or_open());
            track(&mut alive, bound.search_or.borrow().as_ref().unwrap());
            alive
        }),
    ];
    for (name, open) in cases {
        let (_dir, store, _) = crate::parser_editors::setup();
        let _windows = headless::init();
        let ui = MainWindow::new().unwrap();
        ui.show().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        let alive = open(&ui, &bound, &store);
        assert!(alive.iter().all(|a| a()), "{name}: opened");
        drop(bound);
        for (n, a) in alive.iter().enumerate() {
            assert!(!a(), "{name}: dialog {n} is freed");
        }
    }
}
