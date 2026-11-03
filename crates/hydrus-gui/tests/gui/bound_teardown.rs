//! The dialogs still open when the last clone of a binding is dropped are
//! hidden and freed with it, not kept by their own callbacks. (Closing a
//! dialog frees it already; this is for those left open, at a test's end or
//! the client's exit.)

use std::cell::RefCell;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{Bound, MainWindow, Pages, SearchPage, bind, headless};

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

/// The children of a dialog (and theirs) left open when the last binding
/// goes are freed too: the services editor and its service editor, the
/// network session browser and its editor, the downloader definitions list
/// with a definition editor and the string converter opened from it.
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
    fn track<T: slint::ComponentHandle + 'static>(alive: &mut Vec<Box<dyn Fn() -> bool>>, w: &T) {
        let weak = w.as_weak();
        alive.push(Box::new(move || weak.upgrade().is_some()));
    }
    track(&mut alive, &browser);
    track(&mut alive, &edit);
    drop((browser, edit));

    // downloader definitions: the list and, if the store has a class, its
    // editor and a string converter below that
    let list = definitions::open(&store, &bound.downloader_definitions, true).unwrap();
    track(&mut alive, &list);
    list.invoke_action("add".into());
    if let Some(edit) = bound.downloader_definitions.class_edit.borrow().as_ref() {
        track(&mut alive, edit);
    }
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
