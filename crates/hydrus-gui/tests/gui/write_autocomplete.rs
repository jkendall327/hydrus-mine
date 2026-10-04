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
        service.key.clone(),
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

fn choose_write_tag_menu(window: &hydrus_gui::WriteTagsWindow, path: &[&str]) {
    for (pane, label) in path.iter().enumerate() {
        let lines = window.get_tag_menu_panes().row_data(pane).unwrap().lines;
        let line = lines.iter().position(|row| row.label == *label).unwrap();
        window.invoke_tag_menu_clicked(
            i32::try_from(pane).unwrap(),
            i32::try_from(line).unwrap(),
            100.0,
            50.0,
            10.0,
        );
    }
}

#[test]
fn shared_tag_menu_favourites_questions_copy_launch_and_owner_lifetime() {
    use hydrus_core::{Tag, search::context::LocationContext, search::predicate::Predicate};
    use std::cell::Cell;
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let applied = Rc::new(Cell::new(0));
    let copies = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| copies.borrow_mut().push(clip.clone())
    });
    let launched = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::write_tag_menu::install_search_launcher(Rc::new({
        let launched = launched.clone();
        move |location, context, predicates, duplicate| {
            launched
                .borrow_mut()
                .push((location, context, predicates, duplicate));
        }
    }));
    let w = hydrus_gui::write_tag_window::open(
        &store,
        key.clone(),
        &[],
        "edit tags",
        &slot,
        Rc::new({
            let applied = applied.clone();
            move |_| applied.set(applied.get() + 1)
        }),
        Rc::new(|| {}),
    )
    .unwrap();
    w.invoke_edited("parity:menu new".into());
    w.invoke_context_menu(0, 10.0, 10.0);
    choose_write_tag_menu(&w, &["copy", "menu_new"]);
    assert_eq!(
        *copies.borrow(),
        vec![hydrus_gui::Clip::Text("menu_new".into())]
    );
    w.invoke_context_menu(0, 10.0, 10.0);
    choose_write_tag_menu(&w, &["favourites", "add \"parity:menu new\" to favourites"]);
    let favourites: settings::FavouriteTags = store.read(settings::get).unwrap();
    assert!(favourites.0.iter().any(|t| t == "parity:menu new"));
    w.invoke_tab_chosen(1);
    let row = w
        .get_suggestions()
        .iter()
        .position(|r| r.text == "parity:menu new")
        .unwrap();
    w.invoke_context_menu(i32::try_from(row).unwrap(), 10.0, 10.0);
    choose_write_tag_menu(
        &w,
        &["favourites", "remove \"parity:menu new\" from favourites"],
    );
    assert_eq!(
        w.get_tag_menu_question(),
        "Remove \"parity:menu new\" from the favourites list?"
    );
    w.invoke_apply();
    assert!(slot.borrow().is_some());
    assert_eq!(applied.get(), 0);
    w.invoke_tag_menu_answered(false);
    assert!(w.get_tag_menu_question().is_empty());
    let favourites: settings::FavouriteTags = store.read(settings::get).unwrap();
    assert!(favourites.0.iter().any(|t| t == "parity:menu new"));
    w.invoke_context_menu(i32::try_from(row).unwrap(), 10.0, 10.0);
    choose_write_tag_menu(&w, &["open", "open a new search page for parity:menu new"]);
    let defaults: settings::SearchDefaults = store.read(settings::get).unwrap();
    assert_eq!(launched.borrow().len(), 1);
    let actual = launched.borrow()[0].clone();
    assert_eq!(actual.0, LocationContext::default());
    assert_eq!(actual.1.service, defaults.tag_service);
    assert_eq!(
        actual.2,
        vec![Predicate::Tag {
            tag: Tag::new("parity:menu new").unwrap(),
            inclusive: true
        }]
    );
    assert!(!actual.3);
    w.invoke_context_menu(i32::try_from(row).unwrap(), 10.0, 10.0);
    choose_write_tag_menu(
        &w,
        &["favourites", "remove \"parity:menu new\" from favourites"],
    );
    w.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    w.invoke_tag_menu_answered(true);
    w.invoke_apply();
    w.invoke_context_menu(0, 10.0, 10.0);
    w.invoke_tag_menu_clicked(0, 0, 100.0, 50.0, 10.0);
    assert!(slot.borrow().is_none());
    assert_eq!(applied.get(), 0);
    assert_eq!(launched.borrow().len(), 1);
    let favourites: settings::FavouriteTags = store.read(settings::get).unwrap();
    assert!(favourites.0.iter().any(|t| t == "parity:menu new"));
    // Reopen, then a confirmed removal re-reads concurrent settings instead of replacing them.
    let w = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "edit tags",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    w.invoke_tab_chosen(1);
    let row = w
        .get_suggestions()
        .iter()
        .position(|r| r.text == "parity:menu new")
        .unwrap();
    w.invoke_context_menu(i32::try_from(row).unwrap(), 10.0, 10.0);
    choose_write_tag_menu(
        &w,
        &["favourites", "remove \"parity:menu new\" from favourites"],
    );
    store
        .write(|ctx| {
            let mut favourites: settings::FavouriteTags = settings::get(ctx.conn())?;
            favourites.0.push("parity:other window".into());
            settings::set(ctx.conn(), &favourites)
        })
        .unwrap();
    w.invoke_tag_menu_answered(true);
    let favourites: settings::FavouriteTags = store.read(settings::get).unwrap();
    assert!(!favourites.0.iter().any(|t| t == "parity:menu new"));
    assert!(favourites.0.iter().any(|t| t == "parity:other window"));
    assert!(
        !w.get_suggestions()
            .iter()
            .any(|r| r.text == "parity:menu new")
    );
    w.invoke_cancel();
    hydrus_gui::write_tag_menu::clear_search_launcher();
}

#[test]
fn tag_menu_launches_native_search_and_duplicate_pages_with_recorded_predicates() {
    use hydrus_core::{Tag, pages::PageContent, search::predicate::Predicate};
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let window = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "edit tags",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    *slot.borrow_mut() = Some(window.clone_strong());
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let launches = fixture["menus"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["action"] == "launch");
    let mut count = bound.pages.borrow().session().pages.len();
    for event in launches {
        let expected = &event["launched"][0];
        let tag = expected["predicates"][0].as_str().unwrap();
        window.invoke_edited(tag.into());
        let row = window
            .get_suggestions()
            .iter()
            .position(|row| row.text.starts_with(tag))
            .unwrap();
        window.invoke_context_menu(i32::try_from(row).unwrap(), 10.0, 10.0);
        // This store has no parent links; the action's payload is the same recorded child tag.
        let duplicate = expected["topic"] == "new_page_duplicates";
        let label = format!(
            "open a new {} page for {tag}",
            if duplicate {
                "duplicate filter"
            } else {
                "search"
            }
        );
        choose_write_tag_menu(&window, &["open", &label]);
        count += 1;
        let pages = bound.pages.borrow();
        assert_eq!(pages.session().pages.len(), count);
        assert_eq!(pages.shown().name, expected["page_name"].as_str().unwrap());
        let predicate = Predicate::Tag {
            tag: Tag::new(tag).unwrap(),
            inclusive: true,
        };
        let context = match &pages.shown().content {
            PageContent::Search { search, .. } => search,
            PageContent::Duplicates { duplicates, .. } => {
                assert_eq!(duplicates.search.search_1, duplicates.search.search_2);
                &duplicates.search.search_1
            }
            content => panic!("unexpected launched page: {content:?}"),
        };
        assert_eq!(context.predicates, vec![predicate]);
        assert_eq!(
            context.tags.service,
            store
                .read(settings::get::<settings::SearchDefaults>)
                .unwrap()
                .tag_service
        );
        assert_eq!(
            context
                .location
                .current()
                .iter()
                .map(|key| hex::encode(key.as_bytes()))
                .collect::<Vec<_>>(),
            expected["current"]
                .as_array()
                .unwrap()
                .iter()
                .map(|key| key.as_str().unwrap())
                .collect::<Vec<_>>()
        );
    }
    window.invoke_cancel();
    window.invoke_context_menu(0, 10.0, 10.0);
    window.invoke_tag_menu_clicked(0, 0, 100.0, 50.0, 10.0);
    assert_eq!(bound.pages.borrow().session().pages.len(), count);
    bound.pages.borrow_mut().sync(1_700_200_000).unwrap();
    let reopened = Pages::open(store).unwrap();
    assert_eq!(reopened.session().pages.len(), count);
    assert!(matches!(
        reopened.shown().content,
        PageContent::Duplicates { .. }
    ));
}

#[test]
fn write_domain_buttons_query_counts_and_own_cancelled_location_child() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let id = store.snapshot().services.by_key(&key).unwrap().id;
    let file = store
        .read(|conn| {
            Ok(
                conn.query_row("SELECT hash_id FROM files LIMIT 1", [], |row| {
                    row.get::<_, hydrus_core::HashId>(0)
                })?,
            )
        })
        .unwrap();
    store
        .write_content(move |writer| {
            let tag = hydrus_store::master::intern_tag(
                writer.conn(),
                &hydrus_core::Tag::new("parity:domain counted").unwrap(),
            )?;
            writer.update_mappings(id, &hydrus_store::content::MappingAction::Add, tag, &[file])?;
            Ok(())
        })
        .unwrap();
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let applied = Rc::new(std::cell::Cell::new(0));
    let w = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "edit tags",
        &slot,
        Rc::new({
            let applied = applied.clone();
            move |_| applied.set(applied.get() + 1)
        }),
        Rc::new(|| {}),
    )
    .unwrap();
    w.invoke_edited("parity:domain counted".into());
    let counted = w.get_suggestions().iter().any(|r| r.text.contains('('));
    assert!(counted);
    w.invoke_domain_menu(true, 10.0, 10.0);
    choose_write_tag_menu(&w, &["all known tags"]);
    assert_eq!(w.get_tag_label(), "all known tags");
    w.invoke_domain_menu(false, 10.0, 10.0);
    choose_write_tag_menu(&w, &["all known files with tags"]);
    assert_eq!(w.get_file_label(), "all known files with tags");
    assert_eq!(w.get_tag_label(), "downloader tags");
    w.invoke_domain_menu(true, 10.0, 10.0);
    choose_write_tag_menu(&w, &["all known tags"]);
    assert_eq!(w.get_file_label(), "my files");
    w.invoke_domain_menu(false, 10.0, 10.0);
    choose_write_tag_menu(&w, &["multiple/deleted locations"]);
    let child = hydrus_gui::locations_window::last_opened().unwrap();
    w.invoke_apply();
    assert!(slot.borrow().is_some());
    assert_eq!(applied.get(), 0);
    child.invoke_cancel();
    assert_eq!(w.get_file_label(), "my files");
    w.invoke_domain_menu(false, 10.0, 10.0);
    choose_write_tag_menu(&w, &["multiple/deleted locations"]);
    let child = hydrus_gui::locations_window::last_opened().unwrap();
    // A valid empty domain reaches the live count query only after child Apply.
    let ticks = child.get_ticks();
    for i in 0..ticks.row_count() {
        if ticks.row_data(i).unwrap().checked {
            child.invoke_toggled(i32::try_from(i).unwrap(), false);
        }
    }
    assert!(w.get_suggestions().iter().any(|r| r.text.contains('(')));
    child.invoke_apply();
    assert_eq!(w.get_file_label(), "nothing");
    assert!(w.get_suggestions().iter().all(|r| !r.text.contains('(')));
    w.invoke_domain_menu(false, 10.0, 10.0);
    choose_write_tag_menu(&w, &["multiple/deleted locations"]);
    let child = hydrus_gui::locations_window::last_opened().unwrap();
    w.invoke_cancel();
    child.invoke_apply();
    w.invoke_apply();
    assert!(slot.borrow().is_none());
    assert_eq!(applied.get(), 0);
    assert!(hydrus_gui::locations_window::last_opened().is_none());
}

#[test]
fn selected_batches_stage_in_shared_dialogs_and_closed_owners_ignore_callbacks() {
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let tags = vec![
        "parity:multi alpha".to_owned(),
        "parity:multi beta".to_owned(),
        "parity:multi gamma".to_owned(),
    ];
    let saved = tags.clone();
    store
        .write(move |ctx| settings::set(ctx.conn(), &settings::FavouriteTags(saved)))
        .unwrap();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let applied = Rc::new(RefCell::new(Vec::<String>::new()));
    let open = || {
        hydrus_gui::write_tag_window::open(
            &store,
            key.clone(),
            &[],
            "batch tags",
            &slot,
            Rc::new({
                let applied = applied.clone();
                move |tags| *applied.borrow_mut() = tags
            }),
            Rc::new(|| {}),
        )
        .unwrap()
    };
    let child = open();
    child.invoke_tab_chosen(1);
    child.invoke_selection_clicked(2, true, false);
    assert_eq!(
        child.get_selected().iter().collect::<Vec<_>>(),
        vec![true, false, true]
    );
    assert_eq!(child.get_tags().row_count(), 0);
    child.invoke_entered();
    assert_eq!(
        child
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect::<Vec<_>>(),
        vec![tags[0].clone(), tags[2].clone()]
    );
    child.invoke_cancel();
    child.invoke_selection_clicked(1, true, false);
    child.invoke_chosen(1);
    child.invoke_apply();
    assert!(applied.borrow().is_empty());
    let child = open();
    assert_eq!(child.get_tags().row_count(), 0);
    child.invoke_tab_chosen(1);
    child.invoke_selection_clicked(2, false, true);
    child.invoke_chosen(1); // Double-click an already selected row activates the batch.
    child.invoke_apply();
    assert_eq!(*applied.borrow(), tags);
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    manage.invoke_tab_chosen(1);
    manage.invoke_selection_clicked(2, false, true);
    manage.invoke_entered(); // Empty favourites text must enter choices, not apply the dialog.
    assert!(bound.manage_tags.borrow().is_some());
    for tag in &tags {
        assert!(
            manage
                .get_tags()
                .iter()
                .any(|row| row.text.starts_with(tag.as_str()))
        );
    }
    manage.invoke_cancel();
    manage.invoke_selection_clicked(0, false, false);
    manage.invoke_entered();
    manage.invoke_apply();
    ui.invoke_manage_tags_selected();
    let reopened = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    for tag in &tags {
        assert!(
            !reopened
                .get_tags()
                .iter()
                .any(|row| row.text.starts_with(tag.as_str()))
        );
    }
    reopened.invoke_cancel();
    // The separate relationships consumer accepts both sides as staged batches.
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let at = pane
        .lines
        .iter()
        .position(|row| row.label.starts_with("parents"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let relation = bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    relation.invoke_autocomplete_tab(false, 1);
    relation.invoke_autocomplete_clicked(false, 2, true, false);
    relation.invoke_enter_tags(false, "".into());
    assert_eq!(relation.get_left_tags().row_count(), 2);
    relation.invoke_autocomplete_tab(true, 1);
    relation.invoke_autocomplete_clicked(true, 2, false, true);
    relation.invoke_autocomplete_chosen(true, 1);
    assert_eq!(relation.get_right_tags().row_count(), 3);
    relation.invoke_cancel();
    relation.invoke_autocomplete_clicked(false, 1, true, false);
    relation.invoke_autocomplete_chosen(false, 1);
    relation.invoke_add();
    relation.invoke_apply();
    assert!(bound.tag_relationships.borrow().is_none());
}

#[test]
fn batch_context_menu_copies_and_launches_real_and_or_each_and_duplicate_pages() {
    use hydrus_core::{Tag, pages::PageContent, search::predicate::Predicate};
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("write_tag_selection.json");
    let tags: Vec<String> = serde_json::from_value(
        fixture["steps"].as_array().unwrap().last().unwrap()["entered"][0].clone(),
    )
    .unwrap();
    let saved = tags.clone();
    store
        .write(move |ctx| settings::set(ctx.conn(), &settings::FavouriteTags(saved)))
        .unwrap();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let child = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "batch menus",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    child.invoke_tab_chosen(1);
    child.invoke_selection_clicked(2, false, true);
    let copied = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                text.clone_into(&mut copied.borrow_mut());
            }
        }
    });
    child.invoke_context_menu(1, 10.0, 10.0);
    choose_write_tag_menu(&child, &["copy", "3 selected"]);
    assert_eq!(
        *copied.borrow(),
        fixture["menus"][1]["copied"][0].as_str().unwrap()
    );
    assert_eq!(child.get_tags().row_count(), 0);
    let predicates: Vec<_> = tags
        .iter()
        .map(|tag| Predicate::Tag {
            tag: Tag::new(tag).unwrap(),
            inclusive: true,
        })
        .collect();
    for (label, expected, duplicate) in [
        (
            "open a new search page for 3 selected",
            vec![predicates.clone()],
            false,
        ),
        (
            "open a new OR search page for 3 selected",
            vec![vec![Predicate::Or(predicates.clone())]],
            false,
        ),
        (
            "open new search pages for each in selection",
            predicates
                .iter()
                .cloned()
                .map(|predicate| vec![predicate])
                .collect(),
            false,
        ),
        (
            "open a new duplicate filter page for 3 selected",
            vec![predicates.clone()],
            true,
        ),
    ] {
        let before = bound.pages.borrow().session().pages.len();
        child.invoke_context_menu(1, 10.0, 10.0);
        choose_write_tag_menu(&child, &["open", label]);
        let pages = bound.pages.borrow();
        assert_eq!(pages.session().pages.len(), before + expected.len());
        for (page, wanted) in pages.session().pages[before..].iter().zip(expected) {
            match &page.content {
                PageContent::Search { search, .. } => {
                    assert!(!duplicate);
                    assert_eq!(search.predicates, wanted);
                }
                PageContent::Duplicates { duplicates, .. } => {
                    assert!(duplicate);
                    assert_eq!(duplicates.search.search_1.predicates, wanted);
                    assert_eq!(duplicates.search.search_1, duplicates.search.search_2);
                }
                content => panic!("unexpected launched content {content:?}"),
            }
        }
    }
    let before = bound.pages.borrow().session().pages.len();
    child.invoke_context_menu(1, 10.0, 10.0);
    child.invoke_cancel();
    child.invoke_tag_menu_clicked(0, 0, 10.0, 10.0, 10.0);
    child.invoke_context_menu(1, 10.0, 10.0);
    assert_eq!(child.get_tag_menu_panes().row_count(), 0);
    assert_eq!(bound.pages.borrow().session().pages.len(), before);
    hydrus_gui::set_clipper(|_| {});
}

#[test]
fn regeneration_question_repairs_counts_only_on_yes_and_invalidates_on_cancel() {
    use hydrus_core::{HashId, Tag};
    use hydrus_store::{content::MappingAction, schema::MappingTables};
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name("my tags").unwrap();
    let key = service.key.clone();
    let service_id = service.id;
    let domain = snapshot
        .services
        .of_type(hydrus_core::ServiceType::CombinedFile)
        .next()
        .unwrap()
        .id;
    let tag = store
        .write_content(move |writer| {
            let tag = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("parity:regen one").unwrap(),
            )?;
            let file: HashId =
                writer
                    .conn()
                    .query_row("SELECT hash_id FROM files LIMIT 1", [], |row| row.get(0))?;
            writer.update_mappings(service_id, &MappingAction::Add, tag, &[file])?;
            Ok(tag)
        })
        .unwrap();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &settings::FavouriteTags(vec!["parity:regen one".into()]),
            )
        })
        .unwrap();
    let corrupt = |value| {
        store
            .write(move |ctx| {
                let tables = MappingTables::new(service_id);
                for table in [tables.counts, tables.display_counts] {
                    ctx.conn().execute(
                        &format!("UPDATE {table} SET current=?1 WHERE tag_id=?2 AND domain_id=?3"),
                        rusqlite::params![value, tag, domain],
                    )?;
                }
                Ok(())
            })
            .unwrap();
    };
    let count = || {
        store
            .read(|conn| hydrus_store::counts::count(conn, service_id, domain, tag, false))
            .unwrap()
            .current
    };
    corrupt(77_i64);
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let child = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "repair tags",
        &slot,
        Rc::new(|_| {}),
        Rc::new(|| {}),
    )
    .unwrap();
    child.invoke_tab_chosen(1);
    let fixture = hydrus_testkit::fixture_json("write_tag_selection.json");
    let asked = &fixture["menus"].as_array().unwrap().last().unwrap()["asked"][0];
    child.invoke_context_menu(0, 10.0, 10.0);
    choose_write_tag_menu(&child, &["maintenance", "regenerate tag display"]);
    assert_eq!(
        child.get_tag_menu_question(),
        asked["message"].as_str().unwrap()
    );
    assert_eq!(
        child.get_tag_menu_question_title(),
        asked["title"].as_str().unwrap()
    );
    assert_eq!(
        child.get_tag_menu_yes_label(),
        asked["yes_label"].as_str().unwrap()
    );
    assert_eq!(
        child.get_tag_menu_no_label(),
        asked["no_label"].as_str().unwrap()
    );
    child.invoke_apply();
    assert!(slot.borrow().is_some());
    child.invoke_selection_clicked(0, true, false);
    assert_eq!(child.get_selected().iter().collect::<Vec<_>>(), vec![true]);
    child.invoke_tag_menu_answered(false);
    assert_eq!(count(), 77);
    child.invoke_context_menu(0, 10.0, 10.0);
    choose_write_tag_menu(&child, &["maintenance", "regenerate tag display"]);
    child.invoke_tag_menu_answered(true);
    assert_eq!(count(), 1);
    assert_eq!(child.get_tags().row_count(), 0); // Maintenance does not enter suggestions.
    assert!(child.get_error().is_empty());
    corrupt(99_i64);
    child.invoke_context_menu(0, 10.0, 10.0);
    choose_write_tag_menu(&child, &["maintenance", "regenerate tag display"]);
    child.invoke_cancel();
    child.invoke_tag_menu_answered(true);
    assert_eq!(count(), 99);
    assert!(slot.borrow().is_none());
}

#[test]
fn normal_paste_replays_cursor_selection_and_accepted_tags_preserve_the_draft() {
    use hydrus_core::Tag;
    let (_dirs, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("write_tag_selection.json");
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    for case in fixture["normal_paste"].as_array().unwrap() {
        let slot = hydrus_gui::write_tag_window::Slot::default();
        let applied = Rc::new(RefCell::new(Vec::new()));
        let child = hydrus_gui::write_tag_window::open(
            &store,
            key.clone(),
            &[],
            "normal paste",
            &slot,
            Rc::new({
                let applied = applied.clone();
                move |tags| *applied.borrow_mut() = tags
            }),
            Rc::new(|| {}),
        )
        .unwrap();
        child.invoke_edited(case["text"].as_str().unwrap().into());
        let anchor = i32::try_from(case["anchor"].as_i64().unwrap()).unwrap();
        let cursor = anchor + i32::try_from(case["length"].as_i64().unwrap()).unwrap();
        child.invoke_select_input(anchor, cursor);
        let pasted = case["pasted"].as_str().unwrap().to_owned();
        headless::set_clipboard_text(&pasted);
        hydrus_gui::set_paster(move || pasted.clone());
        if child.invoke_paste(false) {
            let question = child.get_question();
            let (prefix, tags) = question.split_once("\n\n").unwrap();
            let (expected_prefix, expected_tags) = case["asked"][0]["message"]
                .as_str()
                .unwrap()
                .split_once("\n\n")
                .unwrap();
            assert_eq!(prefix, expected_prefix);
            // Qt joins CleanTags' set without sorting, so row order varies by process.
            let mut tags = tags.lines().collect::<Vec<_>>();
            let mut expected_tags = expected_tags.lines().collect::<Vec<_>>();
            tags.sort_unstable();
            expected_tags.sort_unstable();
            assert_eq!(tags, expected_tags);
            child.invoke_answered(case["answer"].as_bool().unwrap());
        } else {
            assert!(case["asked"].as_array().unwrap().is_empty());
            child.invoke_normal_paste(); // The native key handler propagates the unconsumed event.
        }
        let recorded = case["after"].as_str().unwrap();
        // Both the raw draft and its cleaned eventual tag must match Qt.
        assert_eq!(child.get_text(), recorded);
        assert_eq!(Tag::new(&child.get_text()), Tag::new(recorded));
        if case["asked"].as_array().unwrap().is_empty() || !case["answer"].as_bool().unwrap() {
            child.invoke_undo_input();
            assert_eq!(child.get_text(), case["undo"].as_str().unwrap());
            child.invoke_redo_input();
            assert_eq!(child.get_text(), case["redo"].as_str().unwrap());
        }
        let expected: Vec<String> = case["pasted_tags"]
            .as_array()
            .unwrap()
            .first()
            .map(|tags| serde_json::from_value(tags.clone()).unwrap())
            .unwrap_or_default();
        assert_eq!(
            child
                .get_tags()
                .iter()
                .map(|row| row.text.to_string())
                .collect::<Vec<_>>(),
            expected
        );
        child.invoke_cancel();
        child.invoke_answered(true);
        child.invoke_apply();
        assert!(applied.borrow().is_empty());
    }
    // The real Manage Tags consumer also retains accepted text while staging mappings.
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    manage.invoke_text_edited("caller draft".into());
    headless::set_clipboard_text("parity:paste one\nparity:paste two");
    hydrus_gui::set_paster(|| "parity:paste one\nparity:paste two".into());
    assert!(manage.invoke_paste_requested(false));
    manage.invoke_paste_answered(true);
    assert_eq!(manage.get_text(), "caller draft");
    assert!(
        manage
            .get_tags()
            .iter()
            .any(|row| row.text.starts_with("parity:paste one"))
    );
    manage.invoke_select_input(0, 6);
    assert!(manage.invoke_paste_requested(false));
    manage.invoke_paste_answered(false);
    assert_eq!(manage.get_text(), "parity:paste one\nparity:paste twodraft");
    manage.invoke_cancel();
    ui.invoke_manage_tags_selected();
    let manage = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert!(
        !manage
            .get_tags()
            .iter()
            .any(|row| row.text.starts_with("parity:paste one"))
    );
    manage.invoke_cancel();
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let at = pane
        .lines
        .iter()
        .position(|row| row.label.starts_with("parents"))
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let relation = bound
        .tag_relationships
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    relation.invoke_autocomplete_edited(false, "left draft".into());
    relation.invoke_select_input(false, 0, 4);
    assert!(relation.invoke_autocomplete_paste(false, false));
    relation.invoke_answered(false);
    assert_eq!(
        relation.get_left_input(),
        "parity:paste one\nparity:paste two draft"
    );
    assert_eq!(relation.get_left_tags().row_count(), 0);
    relation.invoke_autocomplete_edited(true, "right draft".into());
    assert!(relation.invoke_autocomplete_paste(true, false));
    relation.invoke_answered(true);
    assert_eq!(relation.get_right_input(), "right draft");
    assert_eq!(relation.get_right_tags().row_count(), 2);
    relation.invoke_autocomplete_edited(true, "cancelled draft".into());
    assert!(relation.invoke_autocomplete_paste(true, false));
    relation.invoke_cancel();
    relation.invoke_answered(false);
    assert_eq!(relation.get_right_input(), "cancelled draft");
    assert!(bound.tag_relationships.borrow().is_none());
}

#[test]
fn keyboard_result_selection_copy_and_native_text_copy_use_their_own_focus() {
    use hydrus_core::{Sha256, Tag};
    use hydrus_store::{
        content::tag_relations::{self, RelationAction, RelationUpdate},
        display::RelationKind,
    };
    use serde_json::json;
    use slint::platform::{Key, WindowEvent};

    let fixture = hydrus_testkit::fixture_json("write_tag_selection.json");
    let (_dirs, store) = crate::subscriptions::store();
    let windows = headless::init();
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name("my tags").unwrap();
    let id = service.id;
    let key = service.key.clone();
    let corpus = fixture["corpus"].clone();
    store
        .write_content(move |w| {
            for row in corpus.as_array().unwrap() {
                let tag = hydrus_store::master::intern_tag(
                    w.conn(),
                    &Tag::new(row["tag"].as_str().unwrap()).unwrap(),
                )?;
                let hashes = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|hash| {
                        let hash: Sha256 = hash.as_str().unwrap().parse().unwrap();
                        hydrus_store::master::hash_id(w.conn(), &hash).map(Option::unwrap)
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                w.update_mappings(id, &hydrus_store::content::MappingAction::Add, tag, &hashes)?;
            }
            Ok(())
        })
        .unwrap();
    for (kind, field) in [
        (RelationKind::Siblings, "siblings"),
        (RelationKind::Parents, "parents"),
    ] {
        tag_relations::apply(
            &store,
            kind,
            fixture[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| RelationUpdate {
                    service: id,
                    left: Tag::new(pair[0].as_str().unwrap()).unwrap(),
                    right: Tag::new(pair[1].as_str().unwrap()).unwrap(),
                    action: RelationAction::Add,
                })
                .collect(),
        )
        .unwrap();
    }
    let saved_key = key.clone();
    store
        .write(move |ctx| {
            let mut prefs: TagEditingSettings = settings::get(ctx.conn())?;
            prefs.select_first_with_count = false;
            prefs.autocomplete_show_parents = true;
            prefs.autocomplete_expand_parents = true;
            prefs.autocomplete_show_siblings = true;
            prefs.autocomplete_list_height = 3;
            settings::set(ctx.conn(), &prefs)?;
            let mut defaults: settings::SearchDefaults = settings::get(ctx.conn())?;
            defaults.local_location = hydrus_core::search::context::LocationContext::single(
                hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::MY_FILES),
            );
            settings::set(ctx.conn(), &defaults)?;
            let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                settings::get(ctx.conn())?;
            let mut options = widgets.options(&saved_key);
            options.write_tag_service = saved_key.clone();
            widgets.services.insert(saved_key.to_hex(), options);
            settings::set(ctx.conn(), &widgets)
        })
        .unwrap();
    let slot = hydrus_gui::write_tag_window::Slot::default();
    let applied = Rc::new(RefCell::new(Vec::<String>::new()));
    let child = hydrus_gui::write_tag_window::open(
        &store,
        key,
        &[],
        "keyboard tags",
        &slot,
        Rc::new({
            let applied = applied.clone();
            move |tags| *applied.borrow_mut() = tags
        }),
        Rc::new(|| {}),
    )
    .unwrap();
    child.invoke_edited("parity:multi".into());
    assert_eq!(
        json!(
            child
                .get_suggestions()
                .iter()
                .map(|row| row.text.to_string())
                .collect::<Vec<_>>()
        ),
        json!(
            fixture["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["text"].as_str().unwrap())
                .collect::<Vec<_>>()
        )
    );
    let native = windows.get(0).unwrap();
    headless::render(&native, 460, 600);
    let copies = Rc::new(RefCell::new(Vec::<String>::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copies.borrow_mut().push(text.clone());
            }
        }
    });
    // Selected editor text retains native Ctrl+C; the tag-list handler must not steal it.
    child.invoke_select_input(0, 6);
    headless::set_clipboard_text("before native text copy");
    native.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    native.dispatch_event(WindowEvent::KeyPressed { text: "c".into() });
    native.dispatch_event(WindowEvent::KeyReleased { text: "c".into() });
    native.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    assert_eq!(headless::clipboard_text().as_deref(), Some("parity"));
    assert!(copies.borrow().is_empty());
    child.invoke_focus_results();
    for step in fixture["keyboard"]["steps"].as_array().unwrap() {
        copies.borrow_mut().clear();
        if step["action"] == "key" {
            let ctrl = step["ctrl"].as_bool().unwrap();
            let shift = step["shift"].as_bool().unwrap();
            if ctrl {
                native.dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Control.into(),
                });
            }
            if shift {
                native.dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Shift.into(),
                });
            }
            let key: slint::SharedString = match step["key"].as_str().unwrap() {
                "Up" => Key::UpArrow.into(),
                "Down" => Key::DownArrow.into(),
                "Home" => Key::Home.into(),
                "End" => Key::End.into(),
                "PageUp" => Key::PageUp.into(),
                "PageDown" => Key::PageDown.into(),
                key => key.to_lowercase().into(),
            };
            native.dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
            native.dispatch_event(WindowEvent::KeyReleased { text: key });
            if shift {
                native.dispatch_event(WindowEvent::KeyReleased {
                    text: Key::Shift.into(),
                });
            }
            if ctrl {
                native.dispatch_event(WindowEvent::KeyReleased {
                    text: Key::Control.into(),
                });
            }
            assert_eq!(json!(*copies.borrow()), step["copied"], "{step}");
        }
        let mut selected = Vec::new();
        for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
            if child.get_selected().row_data(i).unwrap() {
                let tag = row["tag"].as_str().unwrap();
                if !selected.contains(&tag) {
                    selected.push(tag);
                }
            }
        }
        assert_eq!(json!(selected), step["selected"], "{step}");
        assert_eq!(child.get_text(), "parity:multi");
        assert_eq!(child.get_tags().row_count(), 0);
    }
    // First Escape consumes selection; it must not cancel its owner or alter the draft.
    native.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Escape.into(),
    });
    native.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Escape.into(),
    });
    assert!(slot.borrow().is_some());
    assert!(child.get_selected().iter().all(|selected| !selected));
    assert_eq!(child.get_text(), "parity:multi");
    // Show all physical rows for the recorded mouse path, independent of page-key sizing.
    store
        .write(|ctx| {
            let mut prefs: TagEditingSettings = settings::get(ctx.conn())?;
            prefs.autocomplete_list_height = 11;
            settings::set(ctx.conn(), &prefs)
        })
        .unwrap();
    child.invoke_edited("".into());
    child.invoke_edited("parity:multi".into());
    headless::render(&native, 460, 600);
    for step in fixture["drag"].as_array().unwrap() {
        let action = step["action"].as_str().unwrap();
        if action == "select_all" {
            native.dispatch_event(WindowEvent::KeyPressed {
                text: Key::Control.into(),
            });
            native.dispatch_event(WindowEvent::KeyPressed { text: "a".into() });
            native.dispatch_event(WindowEvent::KeyReleased { text: "a".into() });
            native.dispatch_event(WindowEvent::KeyReleased {
                text: Key::Control.into(),
            });
        } else if action != "initial" {
            let physical = step["physical"].as_u64().unwrap();
            let position = slint::LogicalPosition::new(
                child.get_results_x() + 10.0,
                child.get_results_y() + 2.0 + physical as f32 * 22.0 + 11.0,
            );
            let ctrl = step["ctrl"].as_bool().unwrap();
            if ctrl {
                native.dispatch_event(WindowEvent::KeyPressed {
                    text: Key::Control.into(),
                });
            }
            let event = match action {
                "press" => WindowEvent::PointerPressed {
                    position,
                    button: slint::platform::PointerEventButton::Left,
                },
                "release" => WindowEvent::PointerReleased {
                    position,
                    button: slint::platform::PointerEventButton::Left,
                },
                "drag" => WindowEvent::PointerMoved { position },
                _ => panic!("unrecorded mouse action {action}"),
            };
            native.dispatch_event(event);
            if ctrl {
                native.dispatch_event(WindowEvent::KeyReleased {
                    text: Key::Control.into(),
                });
            }
            headless::render(&native, 460, 600);
        }
        let mut selected = Vec::new();
        for (i, row) in fixture["rows"].as_array().unwrap().iter().enumerate() {
            if child.get_selected().row_data(i).unwrap() {
                let tag = row["tag"].as_str().unwrap();
                if !selected.contains(&tag) {
                    selected.push(tag);
                }
            }
        }
        assert_eq!(json!(selected), step["selected"], "{step}");
        assert_eq!(child.get_text(), "parity:multi");
        assert_eq!(child.get_tags().row_count(), 0);
    }
    let before = copies.borrow().clone();
    native.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Escape.into(),
    });
    native.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Escape.into(),
    });
    assert!(slot.borrow().is_some());
    assert!(child.get_selected().iter().all(|selected| !selected));
    native.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Escape.into(),
    });
    native.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Escape.into(),
    });
    child.invoke_navigate(0, false, false);
    assert!(!child.invoke_results_action(1));
    child.invoke_entered();
    child.invoke_apply();
    assert_eq!(*copies.borrow(), before);
    assert!(applied.borrow().is_empty());
    assert!(slot.borrow().is_none());
    hydrus_gui::set_clipper(|_| {});
}
