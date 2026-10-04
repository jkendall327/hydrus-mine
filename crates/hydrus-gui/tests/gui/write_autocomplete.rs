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

#[test]
fn relationship_autocomplete_preserves_service_drafts_and_cancels_paste_with_owner() {
    fn open(
        ui: &MainWindow,
        bound: &hydrus_gui::Bound,
        name: &str,
    ) -> hydrus_gui::TagRelationshipsWindow {
        let top = ui
            .get_menu_titles()
            .iter()
            .position(|row| row.label == "tags")
            .unwrap();
        ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
        let pane = ui.get_menu_panes().row_data(0).unwrap();
        let index = pane
            .lines
            .iter()
            .position(|row| row.label.starts_with(name))
            .unwrap();
        ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
        bound
            .tag_relationships
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong()
    }
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    hydrus_gui::set_paster(|| "parity:paste left a\nparity:paste left b".into());
    for kind in ["siblings", "parents"] {
        let w = open(&ui, &bound, kind);
        let mine = w
            .get_service_names()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        let other = w
            .get_service_names()
            .iter()
            .position(|s| s == "downloader tags")
            .unwrap();
        w.invoke_service_chosen(i32::try_from(mine).unwrap());
        w.invoke_autocomplete_edited(false, "parity:caller draft".into());
        assert_eq!(w.get_left_suggestions().row_count(), 1);
        w.invoke_service_chosen(i32::try_from(other).unwrap());
        assert!(w.get_left_input().is_empty());
        w.invoke_autocomplete_edited(false, "different service draft".into());
        w.invoke_service_chosen(i32::try_from(mine).unwrap());
        assert_eq!(w.get_left_input(), "parity:caller draft");
        assert!(w.invoke_autocomplete_paste(false, false));
        w.invoke_answered(false);
        assert_eq!(w.get_left_input(), "parity:caller draft");
        assert_eq!(w.get_left_tags().row_count(), 0);
        assert!(w.invoke_autocomplete_paste(false, true));
        assert_eq!(w.get_left_tags().row_count(), 2);
        assert!(w.invoke_autocomplete_paste(false, true));
        assert_eq!(w.get_left_tags().row_count(), 2);
        assert!(w.invoke_autocomplete_paste(true, false));
        w.window()
            .dispatch_event(slint::platform::WindowEvent::CloseRequested);
        w.invoke_answered(true);
        w.invoke_apply();
        assert!(bound.tag_relationships.borrow().is_none());
        let w = open(&ui, &bound, kind);
        assert_eq!(w.get_left_tags().row_count(), 0);
        assert_eq!(w.get_right_tags().row_count(), 0);
        w.invoke_cancel();
    }
}
