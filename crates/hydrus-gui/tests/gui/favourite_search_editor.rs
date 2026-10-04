//! Favourite searches expose the shared page controls, commit only accepted
//! owner drafts and cancel predicate children with their editor/manager.
use hydrus_core::{
    ServiceKey,
    pages::{PageCollect, PageSortBy},
};
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{
    Store,
    settings::{self, AdvancedMode, FavouriteSearches},
};
use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString};

fn index(model: ModelRc<SharedString>, text: &str) -> i32 {
    (0..model.row_count())
        .find(|&i| model.row_data(i).unwrap().contains(text))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or_else(|| panic!("missing choice {text}"))
}
fn service(store: &Store, name: &str) -> ServiceKey {
    store
        .snapshot()
        .services
        .all()
        .find(|s| s.name == name)
        .unwrap()
        .key
        .clone()
}

#[test]
fn favourite_domain_sort_collect_and_autocomplete_widgets_reach_saved_searches() {
    let f = hydrus_testkit::fixture_json("favourite_search_editor.json");
    let (_dirs, store) = crate::subscriptions::store();
    store
        .write(|ctx| settings::set(ctx.conn(), &AdvancedMode(true)))
        .unwrap();
    let rendered = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_favourites_menu_requested(0.0, 0.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manager = bound
        .favourites
        .list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let original: FavouriteSearches = store.read(settings::get).unwrap();
    manager.invoke_add();
    let w = bound
        .favourites
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    w.set_name("editable reference".into());
    w.invoke_save_sort_ticked(true);
    w.invoke_save_collect_ticked(true);
    w.invoke_location_chosen(index(w.get_location_choices(), "all known files"));
    assert_eq!(w.get_tags_label(), "downloader tags");
    w.invoke_tag_chosen(index(w.get_tag_choices(), "all known tags"));
    assert_eq!(w.get_location_label(), "my files");
    w.invoke_tag_chosen(index(
        w.get_tag_choices(),
        f["local_tag_service"].as_str().unwrap(),
    ));
    w.invoke_tag_status_ticked(false, false);
    w.invoke_tag_status_ticked(true, false);
    assert!(!w.get_include_current() && !w.get_include_pending());
    w.invoke_tag_status_ticked(false, true);
    w.invoke_tag_status_ticked(true, true);
    let before = w.get_location_label();
    w.invoke_location_chosen(index(w.get_location_choices(), "multiple/deleted"));
    assert!(w.get_selecting_locations());
    let ticks = w.get_location_ticks();
    let deleted = (0..ticks.row_count())
        .find(|&i| ticks.row_data(i).unwrap().label == "deleted from my files")
        .unwrap();
    w.invoke_location_ticked(i32::try_from(deleted).unwrap(), true);
    w.invoke_locations_answered(false);
    assert_eq!(w.get_location_label(), before);
    w.invoke_location_chosen(index(w.get_location_choices(), "multiple/deleted"));
    w.invoke_location_ticked(i32::try_from(deleted).unwrap(), true);
    w.invoke_locations_answered(true);
    assert!(w.get_location_label().contains("current and deleted"));
    assert_eq!(
        w.get_location_index(),
        index(w.get_location_choices(), "multiple/deleted")
    );
    w.invoke_location_chosen(index(w.get_location_choices(), "my files"));
    w.invoke_sort_chosen(index(w.get_sort_choices(), "dimensions: width"));
    w.invoke_sort_order_chosen(1);
    assert_eq!(w.get_sort_order(), 1);
    w.invoke_sort_chosen(index(w.get_sort_choices(), "tags: series-creator"));
    assert_eq!(w.get_sort_order(), 0);
    w.invoke_sort_chosen(index(w.get_sort_choices(), "rating: stars"));
    assert_eq!(w.get_sort_order(), 1);
    let choices = w.get_collect_choices();
    for label in ["creator", "stars"] {
        let i = (0..choices.row_count())
            .find(|&i| choices.row_data(i).unwrap().label == label)
            .unwrap();
        w.invoke_collect_ticked(i32::try_from(i).unwrap(), true);
    }
    w.invoke_collect_unmatched_ticked(false);
    w.invoke_collect_tag_chosen(index(
        w.get_tag_choices(),
        f["local_tag_service"].as_str().unwrap(),
    ));
    w.set_typed("blue".into());
    w.invoke_typed_edited();
    let suggested = index(w.get_suggestions(), "blue eyes");
    w.invoke_suggestion_chosen(suggested);
    assert!(w.get_predicates().iter().any(|p| p == "blue eyes"));
    let limit = index(w.get_suggestions(), "system:limit");
    w.invoke_suggestion_chosen(limit);
    assert!(w.get_child_open());
    let child = bound
        .favourites
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.invoke_number_edited(0, 1, 7);
    child.invoke_ok(0);
    assert!(!w.get_child_open());
    assert!(w.get_predicates().iter().any(|p| p == "system:limit is 7"));
    let adapter = rendered.get(rendered.count() - 2).unwrap();
    let pixels = headless::render(&adapter, 1100, 1000);
    assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
    // Child Apply only stages the favourite. The manager owns persistence.
    w.invoke_apply();
    assert_eq!(
        store.read(settings::get::<FavouriteSearches>).unwrap(),
        original
    );
    manager.invoke_apply();
    let saved: FavouriteSearches = store.read(settings::get).unwrap();
    let favorite = saved
        .0
        .iter()
        .find(|s| s.name == "editable reference")
        .unwrap();
    assert_eq!(
        favorite
            .search
            .location
            .current()
            .iter()
            .cloned()
            .collect::<Vec<_>>(),
        [service(&store, "my files")]
    );
    assert!(favorite.search.location.deleted().is_empty());
    assert_eq!(favorite.search.tags.service, service(&store, "my tags"));
    assert!(favorite.search.tags.include_current && favorite.search.tags.include_pending);
    assert_eq!(
        favorite.sort.as_ref().unwrap().by,
        PageSortBy::Rating(service(&store, "stars"))
    );
    assert!(!favorite.sort.as_ref().unwrap().ascending);
    assert_eq!(
        favorite.collect.as_ref().unwrap(),
        &PageCollect {
            namespaces: vec!["creator".into()],
            ratings: vec![service(&store, "stars")],
            collect_unmatched: false,
            tag_context: hydrus_search::TagContext::new(service(&store, "my tags"), true, true),
        }
    );
    // Reopen and cancel a pending predicate child; old child handles cannot
    // mutate favourites or close a subsequent editor's new predicate child.
    ui.invoke_favourites_menu_requested(0.0, 0.0);
    ui.invoke_menu_line_clicked(0, 0, 0.0, 0.0, 0.0);
    let manager = bound
        .favourites
        .list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    manager.invoke_add();
    let cancel = bound
        .favourites
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    cancel.invoke_suggestion_chosen(index(cancel.get_suggestions(), "system:limit"));
    let old_child = bound
        .favourites
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    cancel
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(!old_child.window().is_visible());
    assert!(bound.favourites.predicate_editor.borrow().is_none());
    manager.invoke_add();
    let next = bound
        .favourites
        .edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    next.invoke_suggestion_chosen(index(next.get_suggestions(), "system:limit"));
    let new_child = bound
        .favourites
        .predicate_editor
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    old_child.invoke_ok(0);
    assert!(new_child.window().is_visible());
    assert!(bound.favourites.predicate_editor.borrow().is_some());
    manager.invoke_cancel();
    assert!(!new_child.window().is_visible());
    assert_eq!(
        store.read(settings::get::<FavouriteSearches>).unwrap(),
        saved
    );
}
