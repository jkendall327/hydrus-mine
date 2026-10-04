//! Native paste confirmation, add-only staging, option consumers and owner cancellation.
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{settings, tag_editing::TagEditingSettings};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};

#[test]
fn paste_confirmation_skip_and_list_height_are_consumed_by_manage_tags() {
    let f = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let clipboard = Rc::new(RefCell::new(
        f["paste"][0]["text"].as_str().unwrap().to_owned(),
    ));
    hydrus_gui::set_paster({
        let clipboard = clipboard.clone();
        move || clipboard.borrow().clone()
    });
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(w.get_autocomplete_height(), 11);
    w.invoke_text_edited("caller draft".into());
    assert!(w.invoke_paste_requested(false));
    assert!(
        w.get_question()
            .starts_with("You have pasted multiple lines of content.")
    );
    w.invoke_apply(); // A pending question must not accidentally apply/close its owner.
    assert!(bound.manage_tags.borrow().is_some());
    w.invoke_paste_answered(false);
    assert_eq!(w.get_text(), "caller draft");
    assert!(!w.get_tags().iter().any(|r| r.text == "parity:new"));
    assert!(w.invoke_paste_requested(false));
    w.invoke_paste_answered(true);
    assert!(w.get_question().is_empty());
    assert!(w.get_tags().iter().any(|r| r.text == "parity:new"));
    // Repeating a paste must retain a tag, rather than toggle it off.
    assert!(w.invoke_paste_requested(true));
    assert!(w.get_tags().iter().any(|r| r.text == "parity:new"));
    w.invoke_cancel();
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert!(!w.get_tags().iter().any(|r| r.text == "parity:new"));
    store
        .write(|ctx| {
            let mut options: TagEditingSettings = settings::get(ctx.conn())?;
            options.skip_multiline_paste_confirmation = true;
            options.autocomplete_list_height = 3;
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
    *clipboard.borrow_mut() = f["paste"][2]["text"].as_str().unwrap().to_owned();
    assert!(w.invoke_paste_requested(false));
    assert!(w.get_question().is_empty());
    assert_eq!(w.get_autocomplete_height(), 3);
    assert!(w.get_tags().iter().any(|r| r.text == "parity:skip a"));
    // Closing a pending paste invalidates its answer and all stale write callbacks.
    store
        .write(|ctx| {
            let mut options: TagEditingSettings = settings::get(ctx.conn())?;
            options.skip_multiline_paste_confirmation = false;
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
    assert!(w.invoke_paste_requested(false));
    w.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    w.invoke_paste_answered(true);
    w.invoke_apply();
    assert!(bound.manage_tags.borrow().is_none());
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert!(
        !w.get_tags()
            .iter()
            .any(|r| r.text.starts_with("parity:skip"))
    );
    *clipboard.borrow_mut() = "parity:single".into();
    assert!(!w.invoke_paste_requested(false));
    w.invoke_cancel();
}
