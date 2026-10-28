//! The dialogs still open when the last clone of a binding is dropped are
//! hidden and freed with it, not kept by their own callbacks. (Closing a
//! dialog frees it already; this is for those left open, at a test's end or
//! the client's exit.)

use slint::ComponentHandle as _;

use hydrus_gui::{MainWindow, Pages, bind, headless};

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
