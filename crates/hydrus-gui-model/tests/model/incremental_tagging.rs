//! Exact Qt child summaries/pairs, live text memory and per-file staged Apply.
#[path = "../support/manage_tag_counts.rs"]
mod fixture;
use hydrus_core::ContentStatus;
use hydrus_gui_model::{incremental_tagging::IncrementalTagging, manage_tags::ManageTags};
use serde_json::{Value, json};
fn state(editor: &IncrementalTagging, files: &[hydrus_core::HashId]) -> Value {
    let updates:Vec<_>=editor.pairs().into_iter().map(|(file,tag)|json!({"service":"my tags","action":0,"tag":tag,"files":[files.iter().position(|f|*f==file).unwrap()]})).collect();
    json!({"namespace":editor.namespace,"prefix":editor.prefix,"suffix":editor.suffix,"start":editor.start,"step":editor.step,"reverse":editor.reverse,"summary":editor.summary(),"updates":updates})
}
#[test]
fn actual_child_defaults_conflicts_reverse_pairs_and_nested_cancellation_replay() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let mut prior = ManageTags::new(store.clone(), files.clone()).unwrap();
    prior.enter("checkpoint:old").unwrap();
    prior.enter("checkpoint:live").unwrap();
    prior.apply().unwrap();
    for case in recorded["incremental"].as_array().unwrap() {
        let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
        fixture::mine(&mut parent);
        let before = fixture::tags(&store, &files, "my tags", ContentStatus::Current);
        let mut editor = parent.incremental().unwrap();
        assert_eq!(state(&editor, &files), case["child"][0]["state"]);
        let input = &case["input"];
        for (field, key) in ["namespace", "prefix", "suffix"].into_iter().enumerate() {
            editor
                .set_text(field, input[key].as_str().unwrap())
                .unwrap();
        }
        editor
            .set_numbers(
                i32::try_from(input["start"].as_i64().unwrap()).unwrap(),
                i32::try_from(input["step"].as_i64().unwrap()).unwrap(),
                input["reverse"].as_bool().unwrap(),
            )
            .unwrap();
        assert_eq!(state(&editor, &files), case["child"][1]["state"]);
        let valid = state(&editor, &files);
        assert!(editor.set_numbers(10_000_001, 1, false).is_err());
        assert!(editor.set_numbers(1, -10_001, false).is_err());
        assert!(editor.set_text(99, "invalid field").is_err());
        assert_eq!(
            state(&editor, &files),
            valid,
            "invalid controls preserve the accepted preview"
        );
        if input["accept"].as_bool().unwrap() {
            parent
                .apply_incremental(parent.service(), editor.cleaned_pairs().unwrap())
                .unwrap();
            assert_eq!(
                fixture::tags(&store, &files, "my tags", ContentStatus::Current),
                before,
                "child Apply is staged"
            );
            if input["name"].as_str().unwrap().ends_with("parent_apply") {
                parent.apply().unwrap();
            }
        } else {
            assert!(!parent.has_changes());
        }
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
        drop(parent);
    }
    let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
    let editor = parent.incremental().unwrap();
    let service = parent.service();
    let other = parent
        .service_names()
        .iter()
        .position(|name| name == "second tags")
        .unwrap();
    parent.choose_service(other).unwrap();
    assert!(
        parent
            .apply_incremental(service, editor.cleaned_pairs().unwrap())
            .is_err()
    );
    assert!(!parent.has_changes());
    let single = ManageTags::new(store, vec![files[0]]).unwrap();
    assert!(single.incremental().is_none());
    let _ = fixture::snapshot(&mut parent);
}
#[test]
fn add_then_remove_retains_the_reference_deleted_mapping_in_private_preview() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
    parent.enter("checkpoint:old").unwrap();
    parent.enter("checkpoint:live").unwrap();
    parent.apply().unwrap();
    let mut parent = ManageTags::new(store.clone(), files.clone()).unwrap();
    parent.flip_show_deleted().unwrap();
    parent.enter("checkpoint:cycle").unwrap();
    parent.enter("checkpoint:cycle").unwrap();
    assert_eq!(
        parent.has_changes(),
        recorded["fresh_cycle"]["has_changes"].as_bool().unwrap()
    );
    assert_eq!(
        fixture::snapshot(&mut parent),
        recorded["fresh_cycle"]["state"]
    );
    assert!(
        !fixture::tags(&store, &files, "my tags", ContentStatus::Deleted)
            .iter()
            .any(|tags| tags.iter().any(|tag| tag == "checkpoint:cycle"))
    );
    parent.apply().unwrap();
    assert!(
        fixture::tags(&store, &files, "my tags", ContentStatus::Deleted)
            .iter()
            .all(|tags| tags.iter().any(|tag| tag == "checkpoint:cycle"))
    );
}
