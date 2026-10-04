//! Per-service deleted mappings are a private preview, display is a live setting.
#[path = "../support/manage_tag_counts.rs"]
pub(super) mod fixture;
use hydrus_gui_model::manage_tags::ManageTags;

#[test]
fn deleted_counts_status_rows_and_global_toggle_replay_apply_cancel_and_reopen() {
    let recorded = hydrus_testkit::fixture_json("manage_tag_counts_incremental.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    let committed = fixture::tags(
        &store,
        &files,
        "my tags",
        hydrus_core::ContentStatus::Current,
    );
    let mut model = ManageTags::new(store.clone(), files.clone()).unwrap();
    let mut second = ManageTags::new(store.clone(), files.clone()).unwrap();
    let states = recorded["deleted"].as_array().unwrap();
    assert_eq!(fixture::snapshot(&mut model), states[0]["state"]);
    model.flip_show_deleted().unwrap();
    assert_eq!(fixture::snapshot(&mut model), states[1]["state"]);
    assert_eq!(
        fixture::snapshot(&mut second),
        states[1]["other_open_owner"]
    );
    model.enter("checkpoint:live").unwrap();
    assert_eq!(fixture::snapshot(&mut model), states[2]["state"]);
    model.enter("checkpoint:old").unwrap();
    assert_eq!(fixture::snapshot(&mut model), states[3]["state"]);
    assert_eq!(
        fixture::snapshot(&mut second),
        states[1]["state"],
        "staged changes do not leak to another owner"
    );
    assert_eq!(
        fixture::tags(
            &store,
            &files,
            "my tags",
            hydrus_core::ContentStatus::Current
        ),
        committed
    );
    drop(model);
    let mut reopened = ManageTags::new(store.clone(), files.clone()).unwrap();
    assert_eq!(fixture::snapshot(&mut reopened), states[4]["state"]);
    reopened.enter("checkpoint:old").unwrap();
    reopened.enter("checkpoint:live").unwrap();
    reopened.apply().unwrap();
    let mut saved = ManageTags::new(store.clone(), files.clone()).unwrap();
    assert_eq!(fixture::snapshot(&mut saved), states[5]["state"]);
    saved.flip_show_deleted().unwrap();
    drop(saved);
    let reopened_store = hydrus_store::Store::open(store.dir()).unwrap();
    assert!(
        !ManageTags::new(reopened_store, files)
            .unwrap()
            .show_deleted()
    );
}
