//! Real-store display/search windows, nested ownership and all-page refresh.
use hydrus_core::Tag;
use hydrus_gui::{MainWindow, Pages, SearchPage, TagDisplayWindow, bind, headless};
use hydrus_store::Store;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;
use slint::{ComponentHandle as _, Model as _};

fn open(ui: &MainWindow, bound: &hydrus_gui::Bound, application: bool) -> TagDisplayWindow {
    let top = ui
        .get_menu_titles()
        .iter()
        .position(|r| r.label == "tags")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(top).unwrap(), 0.0, 22.0);
    let pane = ui.get_menu_panes().row_data(0).unwrap();
    let name = if application {
        "advanced"
    } else {
        "display/search"
    };
    let row = pane
        .lines
        .iter()
        .position(|r| r.label.starts_with(name))
        .unwrap();
    if application {
        ui.invoke_menu_line_hovered(0, i32::try_from(row).unwrap(), 200.0, 22.0, 0.0);
        ui.invoke_menu_line_clicked(1, 0, 0.0, 0.0, 0.0);
    } else {
        ui.invoke_menu_line_clicked(0, i32::try_from(row).unwrap(), 0.0, 0.0, 0.0);
    }
    bound.tag_display.borrow().as_ref().unwrap().clone_strong()
}
fn mine(w: &TagDisplayWindow) {
    let at = w
        .get_services()
        .iter()
        .position(|s| s == "my tags")
        .unwrap();
    w.invoke_service_chosen(i32::try_from(at).unwrap());
}

#[test]
fn dialogs_cancel_nested_editors_persist_and_refresh_locked_pages_and_viewer() {
    let windows = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let daemon = Store::open(dir.path()).unwrap();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let file = page.borrow().results()[0];
    let snapshot = store.snapshot();
    let own = snapshot.services.by_name("my tags").unwrap().id;
    let other = snapshot.services.by_name("downloader tags").unwrap().id;
    store
        .write_content(move |w| {
            let tag =
                hydrus_store::master::intern_tag(w.conn(), &Tag::new("display lane old").unwrap())?;
            w.update_mappings(
                own,
                &hydrus_store::content::MappingAction::Add,
                tag,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
    for (service, good) in [(own, "display lane own"), (other, "display lane other")] {
        tag_relations::apply(
            &store,
            RelationKind::Siblings,
            vec![RelationUpdate {
                service,
                left: Tag::new("display lane old").unwrap(),
                right: Tag::new(good).unwrap(),
                action: RelationAction::Add,
            }],
        )
        .unwrap();
    }
    page.borrow_mut().refresh_tags();
    page.borrow_mut().lock_search();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    let w = open(&ui, &bound, false);
    mine(&w);
    w.invoke_filter(false);
    let nested = hydrus_gui::tag_filter_window::last_opened().unwrap();
    assert!(w.get_child_open());
    let chosen = w.get_service_index();
    let revision = store.snapshot().revision;
    w.invoke_apply();
    w.invoke_service_chosen(i32::from(chosen == 0));
    w.invoke_rule_changed(4, true);
    assert_eq!(w.get_service_index(), chosen);
    assert_eq!(store.snapshot().revision, revision);
    assert!(!w.get_fetch_all());
    assert!(bound.tag_display.borrow().is_some());
    nested.invoke_typed(2, "display lane own".into());
    nested.invoke_apply();
    assert!(!w.get_child_open());
    assert!(w.get_single_label().contains("display lane own"));
    w.invoke_filter(true);
    let cancelled_child = hydrus_gui::tag_filter_window::last_opened().unwrap();
    cancelled_child.invoke_typed(2, "display lane own".into());
    w.invoke_cancel();
    assert!(!cancelled_child.window().is_visible());
    cancelled_child.invoke_apply();
    w.invoke_apply();
    assert!(
        viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "display lane own")
    );
    let w2 = open(&ui, &bound, false);
    mine(&w2);
    // Old parent close callbacks cannot remove a replacement window.
    w.invoke_cancel();
    assert!(bound.tag_display.borrow().is_some());
    w2.invoke_filter(false);
    let nested = hydrus_gui::tag_filter_window::last_opened().unwrap();
    nested.invoke_typed(2, "display lane own".into());
    nested.invoke_apply();
    w2.set_fetch_automatically(false);
    w2.set_threshold(0);
    w2.invoke_options_changed();
    w2.invoke_rule_changed(4, true);
    let pixels = headless::render(&windows.get(windows.count() - 2).unwrap(), 820, 660);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag-display-search.png"),
        &pixels,
        820,
        660,
    )
    .unwrap();
    w2.invoke_apply();
    assert!(
        !viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "display lane own")
    );
    let w3 = open(&ui, &bound, false);
    mine(&w3);
    assert!(!w3.get_fetch_automatically());
    assert_eq!(w3.get_threshold(), 0);
    assert!(w3.get_fetch_all());
    // Clearing a native filter must remove the persisted blacklist.
    w3.invoke_location();
    let location = hydrus_gui::locations_window::last_opened().unwrap();
    assert!(w3.get_child_open());
    w3.invoke_apply();
    assert!(bound.tag_display.borrow().is_some());
    location.invoke_cancel();
    assert!(!w3.get_child_open());
    w3.invoke_filter(false);
    let nested = hydrus_gui::tag_filter_window::last_opened().unwrap();
    assert_eq!(nested.get_exclude_rows().row_count(), 1);
    nested.invoke_row_activated(2, 0);
    nested.invoke_apply();
    w3.invoke_apply();
    assert!(
        viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "display lane own")
    );
    let w4 = open(&ui, &bound, true);
    mine(&w4);
    assert!(w4.get_services().iter().all(|s| s != "all known tags"));
    w4.invoke_source_click(false, 0);
    w4.invoke_source_change(false, 0);
    let at = w4
        .get_source_services()
        .iter()
        .position(|s| s == "downloader tags")
        .unwrap();
    w4.set_source_index(i32::try_from(at).unwrap());
    w4.invoke_source_add(false);
    let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 820, 660);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tag-display-application.png"),
        &pixels,
        820,
        660,
    )
    .unwrap();
    // Refresh a locked page even while another page is shown.
    bound.pages.borrow_mut().new_search_page();
    w4.invoke_apply();
    assert!(
        page.borrow()
            .tag_rows()
            .iter()
            .any(|s| s.starts_with("display lane other"))
    );
    assert!(
        viewer
            .get_tags()
            .iter()
            .any(|r| r.text == "display lane other")
    );
    assert!(daemon.refresh_if_changed().unwrap());
    let w5 = open(&ui, &bound, true);
    mine(&w5);
    assert_eq!(
        w5.get_sibling_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "downloader tags"
    );
    w5.invoke_source_click(false, 0);
    w5.invoke_source_change(false, 0);
    w5.invoke_apply();
    let w6 = open(&ui, &bound, true);
    mine(&w6);
    assert_eq!(w6.get_sibling_rows().row_count(), 0);
    w6.invoke_cancel();
    let w7 = open(&ui, &bound, false);
    mine(&w7);
    w7.invoke_location();
    let child = hydrus_gui::locations_window::last_opened().unwrap();
    w7.invoke_cancel();
    assert!(!child.window().is_visible());
    child.invoke_apply();
    assert!(bound.tag_display.borrow().is_none());
    viewer.invoke_close_requested();
}
