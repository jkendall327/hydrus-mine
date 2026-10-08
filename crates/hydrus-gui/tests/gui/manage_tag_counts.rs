//! Real thumbnail Manage Tags controls, global display memory and closed owners.
#[path = "../../../hydrus-gui-model/tests/support/manage_tag_counts.rs"]
pub(super) mod fixture;
use hydrus_gui::{MainWindow, ManageTagsWindow, Pages, SearchPage, bind, headless};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};

fn snapshot(window: &ManageTagsWindow) -> Value {
    let mut services = Vec::new();
    for (i, name) in window.get_service_names().iter().enumerate() {
        window.invoke_service_chosen(i32::try_from(i).unwrap());
        let rows: Vec<_> = window
            .get_tags()
            .iter()
            .filter(|row| row.text.starts_with("checkpoint:"))
            .map(|row| {
                let label = row.text.to_string();
                json!({"tag":label.split(" (").next().unwrap(),"label":label})
            })
            .collect();
        services.push(json!({"service":name.as_str(),"label":window.get_deleted_count_label().as_str(),"visible":window.get_deleted_count_visible(),"tooltip":if window.get_show_deleted(){"hide deleted mappings"}else{"show deleted mappings"},"rows":rows}));
    }
    let mine = window
        .get_service_names()
        .iter()
        .position(|name| name == "my tags")
        .unwrap();
    window.invoke_service_chosen(i32::try_from(mine).unwrap());
    json!({"show_deleted":window.get_show_deleted(),"services":services})
}
/// The remove button on one selected tag, confirmed (the reference's `RemoveTags`).
fn remove(window: &ManageTagsWindow, tag: &str) {
    let row = window
        .get_tags()
        .iter()
        .position(|row| row.text.starts_with(tag))
        .unwrap();
    window.invoke_tag_clicked(i32::try_from(row).unwrap(), false, false);
    window.invoke_remove_pressed();
    window.invoke_tag_menu_answered(true);
}
fn enter(window: &ManageTagsWindow, tag: &str) {
    window.invoke_text_edited(tag.into());
    window.invoke_entered();
}

// leaf: audit-media-tags-missing-deleted
#[test]
fn thumbnail_dialog_replays_deleted_counts_and_retained_callbacks_cannot_mutate_reopened_owner() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let committed = fixture::tags(
        &store,
        &files,
        "my tags",
        hydrus_core::ContentStatus::Current,
    );
    let mut model = hydrus_gui::manage_tags::ManageTags::new(store.clone(), files.clone()).unwrap();
    assert_eq!(
        fixture::snapshot(&mut model),
        recorded["deleted"][0]["state"]
    );
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "tag-count corpus",
            None,
            files.clone(),
        )),
    );
    ui.invoke_select_all();
    let rendered = windows.count();
    ui.invoke_manage_tags_selected();
    let window = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    let states = recorded["deleted"].as_array().unwrap();
    assert_eq!(snapshot(&window), states[0]["state"]);
    let other_ui = MainWindow::new().unwrap();
    other_ui.show().unwrap();
    let other_bound = bind(
        &other_ui,
        Pages::single(SearchPage::fixed(store.clone(), "same corpus", None, files)),
    );
    other_ui.invoke_select_all();
    other_ui.invoke_manage_tags_selected();
    let other = other_bound
        .manage_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    window.invoke_flip_show_deleted();
    std::thread::sleep(std::time::Duration::from_millis(250));
    slint::platform::update_timers_and_animations();
    assert!(
        other.get_show_deleted(),
        "another live owner follows the global toggle"
    );
    assert_eq!(snapshot(&window), states[1]["state"]);
    assert_eq!(snapshot(&other), states[1]["other_open_owner"]);
    remove(&window, "checkpoint:live");
    assert_eq!(snapshot(&window), states[2]["state"]);
    enter(&window, "checkpoint:old");
    assert_eq!(snapshot(&window), states[3]["state"]);
    assert_eq!(snapshot(&other), states[1]["state"]);
    let pixels = headless::render(&windows.get(rendered).unwrap(), 720, 680);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("manage_deleted_mappings.png"),
        &pixels,
        720,
        680,
    )
    .unwrap();
    assert_eq!(
        fixture::tags(
            &store,
            model.files(),
            "my tags",
            hydrus_core::ContentStatus::Current
        ),
        committed
    );
    window.invoke_cancel();
    other.invoke_cancel();
    ui.invoke_manage_tags_selected();
    let reopened = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(snapshot(&reopened), states[4]["state"]);
    window.invoke_flip_show_deleted();
    window.invoke_text_edited("stale:tag".into());
    window.invoke_entered();
    window.invoke_apply();
    assert_eq!(
        snapshot(&reopened),
        states[4]["state"],
        "old callbacks cannot change tags or display preference"
    );
    assert!(bound.manage_tags.borrow().is_some());
    enter(&reopened, "checkpoint:old");
    remove(&reopened, "checkpoint:live");
    reopened.invoke_apply();
    assert!(bound.manage_tags.borrow().is_none());
    ui.invoke_manage_tags_selected();
    let saved = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(snapshot(&saved), states[5]["state"]);
    saved.invoke_cancel();
}
