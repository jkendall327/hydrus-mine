//! Native ± child reads the ordered selection and accepts only into its live parent.
#[path = "../../../hydrus-gui-model/tests/support/manage_tag_counts.rs"]
mod fixture;
use hydrus_core::ContentStatus;
use hydrus_gui::{IncrementalTaggingWindow, MainWindow, Pages, SearchPage, bind, headless};
use serde_json::{Value, json};
use slint::{ComponentHandle as _, Model as _};
fn fields(window: &IncrementalTaggingWindow) -> Value {
    json!({"namespace":window.get_namespace().as_str(),"prefix":window.get_prefix().as_str(),"suffix":window.get_suffix().as_str(),"start":window.get_start(),"step":window.get_step(),"reverse":window.get_reverse(),"summary":window.get_summary().as_str()})
}
fn expected(value: &Value) -> Value {
    let mut value = value.clone();
    value.as_object_mut().unwrap().remove("updates");
    value
}
#[test]
fn real_incremental_child_replays_cancel_apply_memory_and_blocks_parent_mutations() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let mut prior = hydrus_gui::manage_tags::ManageTags::new(store.clone(), files.clone()).unwrap();
    fixture::mine(&mut prior);
    prior.enter("checkpoint:old").unwrap();
    prior.enter("checkpoint:live").unwrap();
    prior.apply().unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store.clone(),
            "ordered incremental corpus",
            None,
            files.clone(),
        )),
    );
    for case in recorded["incremental"].as_array().unwrap() {
        ui.invoke_select_all();
        ui.invoke_manage_tags_selected();
        let parent = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
        let mine = parent
            .get_service_names()
            .iter()
            .position(|name| name == "my tags")
            .unwrap();
        parent.invoke_service_chosen(i32::try_from(mine).unwrap());
        assert!(parent.get_incremental_available());
        parent.invoke_incremental_tags();
        let child = bound
            .incremental_tags
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        assert!(parent.get_incremental_open());
        assert_eq!(fields(&child), expected(&case["child"][0]["state"]));
        let service = parent.get_service_index();
        let rows: Vec<_> = parent
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        parent.invoke_service_chosen(
            (service + 1) % i32::try_from(parent.get_service_names().row_count()).unwrap(),
        );
        parent.invoke_text_edited("blocked:tag".into());
        parent.invoke_entered();
        parent.invoke_apply();
        parent.invoke_incremental_tags();
        assert_eq!(parent.get_service_index(), service);
        assert_eq!(
            parent
                .get_tags()
                .iter()
                .map(|row| row.text.to_string())
                .collect::<Vec<_>>(),
            rows
        );
        assert!(bound.manage_tags.borrow().is_some());
        assert_eq!(
            bound
                .incremental_tags
                .borrow()
                .as_ref()
                .unwrap()
                .get_summary(),
            child.get_summary()
        );
        for (field, key) in ["namespace", "prefix", "suffix"].into_iter().enumerate() {
            child.invoke_text_edited(
                i32::try_from(field).unwrap(),
                case["input"][key].as_str().unwrap().into(),
            );
        }
        child.invoke_numbers_edited(
            i32::try_from(case["input"]["start"].as_i64().unwrap()).unwrap(),
            i32::try_from(case["input"]["step"].as_i64().unwrap()).unwrap(),
            case["input"]["reverse"].as_bool().unwrap(),
        );
        assert_eq!(fields(&child), expected(&case["child"][1]["state"]));
        let valid = fields(&child);
        child.invoke_numbers_edited(10_000_001, 1, false);
        assert!(!child.get_error().is_empty());
        assert_eq!(fields(&child), valid);
        let before = fixture::tags(&store, &files, "my tags", ContentStatus::Current);
        let pixels = headless::render(&windows.get(windows.count() - 1).unwrap(), 640, 480);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("incremental_tagging.png"),
            &pixels,
            640,
            480,
        )
        .unwrap();
        if case["input"]["accept"].as_bool().unwrap() {
            child.invoke_apply();
        } else {
            child.invoke_cancel();
        }
        assert!(bound.incremental_tags.borrow().is_none());
        assert!(!parent.get_incremental_open());
        assert_eq!(
            fixture::tags(&store, &files, "my tags", ContentStatus::Current),
            before,
            "child Apply has not committed mappings"
        );
        let after: Vec<_> = parent
            .get_tags()
            .iter()
            .map(|row| row.text.to_string())
            .collect();
        child.invoke_text_edited(0, "stale namespace".into());
        child.invoke_numbers_edited(1, 1, false);
        child.invoke_apply();
        child.invoke_cancel();
        assert_eq!(
            parent
                .get_tags()
                .iter()
                .map(|row| row.text.to_string())
                .collect::<Vec<_>>(),
            after
        );
        if case["input"]["name"]
            .as_str()
            .unwrap()
            .ends_with("parent_apply")
        {
            parent.invoke_apply();
        } else {
            parent.invoke_cancel();
        }
        assert!(bound.manage_tags.borrow().is_none());
        let memory: hydrus_store::tag_editing::ManageTagsSettings =
            store.read(hydrus_store::settings::get).unwrap();
        assert_eq!(
            json!({"namespace":memory.incremental_namespace,"prefix":memory.incremental_prefix,"suffix":memory.incremental_suffix}),
            case["preferences"]
        );
        assert_eq!(
            json!(fixture::tags(
                &store,
                &files,
                "my tags",
                ContentStatus::Current
            )),
            case["reopened_tags"]
        );
    }
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let parent = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    parent.invoke_incremental_tags();
    let child = bound
        .incremental_tags
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let memory: hydrus_store::tag_editing::ManageTagsSettings =
        store.read(hydrus_store::settings::get).unwrap();
    parent.invoke_cancel();
    assert!(bound.incremental_tags.borrow().is_none());
    child.invoke_text_edited(0, "closed:owner".into());
    child.invoke_apply();
    assert_eq!(
        store
            .read::<hydrus_store::tag_editing::ManageTagsSettings>(hydrus_store::settings::get)
            .unwrap(),
        memory
    );
    let _ = fixture::snapshot(&mut prior);
    // Single-file dialogs omit the actual ± launcher, rather than fabricating a child.
    let ui = MainWindow::new().unwrap();
    let bound = bind(
        &ui,
        Pages::single(SearchPage::fixed(
            store,
            "single file",
            None,
            vec![files[0]],
        )),
    );
    ui.invoke_select_all();
    ui.invoke_manage_tags_selected();
    let parent = bound.manage_tags.borrow().as_ref().unwrap().clone_strong();
    assert!(!parent.get_incremental_available());
    parent.invoke_incremental_tags();
    assert!(bound.incremental_tags.borrow().is_none());
    parent.invoke_cancel();
}
