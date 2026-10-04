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

#[test]
fn detached_write_tag_child_commits_once_and_discards_closed_owner_answers() {
    use hydrus_gui::write_tag_window;
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let caller = Rc::new(RefCell::new(vec!["parity:caller initial".to_owned()]));
    let slot = write_tag_window::Slot::default();
    let commits = Rc::new(std::cell::Cell::new(0));
    let closed = Rc::new(std::cell::Cell::new(0));
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let open = || {
        write_tag_window::open(
            &store,
            service.clone(),
            &caller.borrow(),
            "edit additional tags",
            &slot,
            Rc::new({
                let caller = caller.clone();
                let commits = commits.clone();
                move |tags| {
                    *caller.borrow_mut() = tags;
                    commits.set(commits.get() + 1);
                }
            }),
            Rc::new({
                let closed = closed.clone();
                move || closed.set(closed.get() + 1)
            }),
        )
        .unwrap()
    };
    hydrus_gui::set_paster(|| "parity:caller initial\nparity:child new".into());
    let w = open();
    assert!(w.invoke_paste(false));
    w.invoke_answered(false);
    assert_eq!(w.get_tags().row_count(), 1);
    assert!(w.invoke_paste(true));
    assert_eq!(w.get_tags().row_count(), 2);
    assert!(w.invoke_paste(true));
    assert_eq!(w.get_tags().row_count(), 2);
    assert_eq!(caller.borrow().as_slice(), ["parity:caller initial"]);
    w.invoke_apply();
    assert_eq!(commits.get(), 1);
    assert_eq!(closed.get(), 1);
    assert!(slot.borrow().is_none());
    w.invoke_apply();
    assert_eq!(commits.get(), 1);
    let accepted = caller.borrow().clone();
    let w = open();
    w.invoke_edited("cancelled child tag".into());
    w.invoke_entered();
    assert!(w.invoke_paste(false));
    w.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    w.invoke_answered(true);
    w.invoke_apply();
    assert_eq!(*caller.borrow(), accepted);
    assert_eq!(commits.get(), 1);
    assert_eq!(closed.get(), 2);
    let w = open();
    assert!(
        !w.get_tags()
            .iter()
            .any(|row| row.text == "cancelled child tag")
    );
    w.invoke_cancel();
}

#[test]
fn import_whitelist_child_unlocks_on_cancel_and_closes_with_its_parent() {
    use slint::platform::WindowAdapter as _;
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(6);
    ui.invoke_page_import_options();
    let parent = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let kind = parent
        .get_labels()
        .iter()
        .position(|row| row.starts_with("default tag filtering"))
        .unwrap();
    parent.invoke_kind_clicked(i32::try_from(kind).unwrap());
    parent.set_custom_index(1);
    parent.invoke_changed();
    parent.set_tag_whitelist("parity:caller initial".into());
    parent.invoke_changed();
    let count = windows.count();
    parent.invoke_edit_whitelist();
    assert!(parent.get_tag_child_open());
    assert_eq!(windows.count(), count + 1);
    let child = windows.get(count).unwrap();
    child
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!parent.get_tag_child_open());
    assert_eq!(parent.get_tag_whitelist(), "parity:caller initial");
    parent.invoke_edit_whitelist();
    let child = windows.get(count + 1).unwrap();
    parent.invoke_apply();
    assert!(bound.folders.import_options.borrow().is_some());
    parent
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(bound.folders.import_options.borrow().is_none());
    assert!(!child.window().is_visible());
}

#[test]
fn favourite_children_tabs_and_applied_cap_feed_manage_tags_and_import_tag_child() {
    use hydrus_core::Tag;
    use hydrus_store::{
        content::tag_relations::{self, RelationAction, RelationUpdate},
        display::RelationKind,
    };
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let service = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .clone();
    let id = service.id;
    let file = store
        .read(|c| {
            Ok(c.query_row("SELECT hash_id FROM files LIMIT 1", [], |r| {
                r.get::<_, hydrus_core::HashId>(0)
            })?)
        })
        .unwrap();
    store
        .write_content(move |w| {
            let tag =
                hydrus_store::master::intern_tag(w.conn(), &Tag::new("parity:gui root").unwrap())?;
            w.update_mappings(id, &hydrus_store::content::MappingAction::Add, tag, &[file])?;
            Ok(())
        })
        .unwrap();
    tag_relations::apply(
        &store,
        RelationKind::Parents,
        [
            "parity:gui child1",
            "parity:gui child2",
            "parity:gui child3",
        ]
        .into_iter()
        .map(|tag| RelationUpdate {
            service: id,
            left: Tag::new(tag).unwrap(),
            right: Tag::new("parity:gui root").unwrap(),
            action: RelationAction::Add,
        })
        .collect(),
    )
    .unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &settings::FavouriteTags(vec!["parity:gui favourite".into()]),
            )
        })
        .unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let w = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    w.invoke_tab_chosen(1);
    assert_eq!(
        w.get_suggestions().row_data(0).unwrap().text,
        "parity:gui favourite"
    );
    w.invoke_suggestion_chosen(0);
    assert!(
        w.get_tags()
            .iter()
            .any(|row| row.text == "parity:gui favourite")
    );
    w.invoke_tab_chosen(2);
    assert_eq!(w.get_suggestions().row_count(), 3);
    // The real options dialog stages the cap, then the already-open consumer reads Apply.
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let index = pane
        .lines
        .iter()
        .position(|row| row.label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(index).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|row| row.text == "tag autocomplete tabs")
        .unwrap();
    options.invoke_page_chosen(i32::try_from(page).unwrap());
    let row = options
        .get_rows()
        .iter()
        .position(|row| row.label == "How many tags to show in the children tab: ")
        .unwrap();
    let control = options.get_rows().row_data(row).unwrap();
    assert_eq!(
        (control.minimum, control.maximum, control.number),
        (1, 1_000_000, 40)
    );
    assert_eq!(control.none_phrase, "show all");
    options.invoke_number_edited(i32::try_from(row).unwrap(), 1);
    w.invoke_fetch();
    assert_eq!(w.get_suggestions().row_count(), 3);
    options.invoke_apply();
    w.invoke_fetch();
    assert_eq!(w.get_suggestions().row_count(), 1);
    assert_eq!(
        w.get_suggestions().row_data(0).unwrap().text,
        "parity:gui child1"
    );
    w.invoke_cancel();
    let child_slot = hydrus_gui::write_tag_window::Slot::default();
    let child = hydrus_gui::write_tag_window::open(
        &store,
        service.key,
        &["parity:gui root".into()],
        "edit tags",
        &child_slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    child.invoke_tab_chosen(2);
    assert_eq!(child.get_suggestions().row_count(), 1);
    child.invoke_chosen(0);
    assert!(
        child
            .get_tags()
            .iter()
            .any(|row| row.text == "parity:gui child1")
    );
    child.invoke_tab_chosen(1);
    assert_eq!(
        child.get_suggestions().row_data(0).unwrap().text,
        "parity:gui favourite"
    );
    child.invoke_cancel();
}
