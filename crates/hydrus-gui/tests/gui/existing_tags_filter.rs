//! Actual importer owner, detached filter acceptance, persistence and retirement.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::queues;
use slint::{ComponentHandle as _, Model as _};

// leaf: import-existing-tags-filter
#[test]
fn existing_tag_filter_dialog_stages_enables_persists_and_retires_with_its_importer_owner() {
    let fixture = hydrus_testkit::fixture_json("existing_tags_filter.json");
    let (_dirs, store) = super::subscriptions::store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    let queue = bound.current.borrow().borrow().importer().unwrap().queue;
    ui.invoke_importer_import_options();
    let owner = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    owner.invoke_kind_clicked(4);
    owner.set_custom_index(1);
    owner.invoke_changed();
    let mine = i32::try_from(
        owner
            .get_tag_services()
            .iter()
            .position(|service| service.name == "my tags")
            .unwrap(),
    )
    .unwrap();
    owner.invoke_tag_service_toggled(mine, "get-tags".into(), true);
    let additional: Vec<&str> = fixture["additional"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tag| tag.as_str().unwrap())
        .collect();
    crate::import_locations::set_additional_tags(&owner, mine, &additional);
    owner.invoke_edit_existing_tags_filter(mine);
    let cancelled = hydrus_gui::tag_filter_window::last_opened().unwrap();
    assert_eq!(
        cancelled.get_window_title(),
        fixture["events"][0]["dialog"]["title"].as_str().unwrap()
    );
    assert_eq!(
        cancelled.get_message(),
        hydrus_gui_model::import_options_editor::EXISTING_TAGS_FILTER_MESSAGE
    );
    assert_eq!(cancelled.get_tabs().row_count(), 3);
    assert!(owner.get_tag_child_open());
    cancelled.invoke_global_clicked(0, 1);
    owner.invoke_apply();
    assert!(bound.folders.import_options.borrow().is_some());
    assert!(
        store
            .read(|conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options
            .is_empty()
    );
    cancelled.invoke_cancel();
    cancelled.invoke_apply();
    assert!(!owner.get_tag_child_open());
    assert!(
        !owner
            .get_tag_services()
            .row_data(usize::try_from(mine).unwrap())
            .unwrap()
            .only_existing
    );
    owner.invoke_edit_existing_tags_filter(-1);
    assert!(hydrus_gui::tag_filter_window::last_opened().is_none());
    owner.invoke_edit_existing_tags_filter(mine);
    let child = hydrus_gui::tag_filter_window::last_opened().unwrap();
    child.invoke_global_clicked(0, 1);
    // Parent changes cannot alter the captured target while its modal child waits.
    owner.invoke_tag_service_toggled(mine, "get-tags".into(), false);
    let last = (0..100)
        .take_while(|&index| windows.get(index).is_some())
        .last()
        .unwrap();
    let pixels = headless::render(&windows.get(last).unwrap(), 900, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("existing_tags_filter.png"),
        &pixels,
        900,
        760,
    )
    .unwrap();
    child.invoke_apply();
    assert!(
        owner
            .get_tag_services()
            .row_data(usize::try_from(mine).unwrap())
            .unwrap()
            .only_existing
    );
    owner.invoke_apply();
    let saved = store
        .read(|conn| queues::queue(conn, queue))
        .unwrap()
        .unwrap()
        .options;
    let key = hex::encode(hydrus_core::service::builtin_keys::MY_TAGS);
    let service = saved.tags.as_ref().unwrap().service(&key).unwrap();
    assert!(service.only_add_existing_tags);
    assert!(service.get_tags);
    assert!(
        !service
            .only_add_existing_tags_filter
            .tag_ok("creator:new", false)
    );
    assert!(
        service
            .only_add_existing_tags_filter
            .tag_ok("new plain", false)
    );
    assert_eq!(
        hydrus_store::Store::open(store.dir())
            .unwrap()
            .read(|conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options,
        saved
    );
    ui.invoke_importer_import_options();
    let retired_owner = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    retired_owner.invoke_kind_clicked(4);
    retired_owner.invoke_edit_existing_tags_filter(mine);
    let retired_child = hydrus_gui::tag_filter_window::last_opened().unwrap();
    retired_owner.invoke_cancel();
    assert!(hydrus_gui::tag_filter_window::last_opened().is_none());
    ui.invoke_importer_import_options();
    let successor = bound
        .folders
        .import_options
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    retired_owner.invoke_edit_existing_tags_filter(mine);
    retired_child.invoke_global_clicked(0, 1);
    retired_child.invoke_apply();
    retired_owner.invoke_apply();
    assert!(bound.folders.import_options.borrow().is_some());
    assert!(hydrus_gui::tag_filter_window::last_opened().is_none());
    assert_eq!(
        store
            .read(|conn| queues::queue(conn, queue))
            .unwrap()
            .unwrap()
            .options,
        saved
    );
    successor.invoke_cancel();
}
