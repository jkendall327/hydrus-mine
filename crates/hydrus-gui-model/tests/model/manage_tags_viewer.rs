//! Manage Tags launched from the media viewer, replayed from
//! `manage_tags_viewer.json` (the reference's real `ManageTagsPanel` with
//! `immediate_commit` and a canvas key: `oracle/record_manage_tags_viewer.py`).
use hydrus_core::HashId;
use hydrus_gui_model::manage_tags::{Entered, ManageTags, Removal};
use hydrus_store::Store;
use serde_json::{Value, json};

use super::manage_tags_cog::seed;

fn rows(model: &ManageTags) -> Value {
    json!(
        model
            .rows()
            .into_iter()
            .filter(|(tag, _)| tag.starts_with("v:"))
            .map(|(_, label)| label)
            .collect::<Vec<_>>()
    )
}

/// The tags starting `v:` each file has, as the database has them.
fn stored(store: &Store, files: &[HashId]) -> Value {
    let snapshot = store.snapshot();
    let service = snapshot.services.by_name("my tags").unwrap().id;
    let batch = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
        .unwrap();
    json!(
        files
            .iter()
            .map(|file| {
                let media = batch.results.iter().find(|m| m.hash_id == *file).unwrap();
                let mut tags: Vec<String> = media
                    .tags
                    .get(&service)
                    .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                    .into_iter()
                    .flatten()
                    .map(|id| batch.tags[id].to_string())
                    .filter(|t| t.starts_with("v:"))
                    .collect();
                tags.sort();
                tags
            })
            .collect::<Vec<_>>()
    )
}

fn allow_remove(store: &Store, allow: bool) {
    store
        .write(move |ctx| {
            let mut o: hydrus_store::tag_editing::TagEditingSettings =
                hydrus_store::settings::get(ctx.conn())?;
            o.allow_remove_on_input = allow;
            o.confirm_remove = true;
            o.default_service =
                hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::MY_TAGS.to_vec());
            hydrus_store::settings::set(ctx.conn(), &o)
        })
        .unwrap();
}

// leaf: audit-media-tags-missing-viewer-follow
#[test]
fn viewer_dialog_commits_each_change_and_follows_the_file_shown() {
    let recorded = hydrus_testkit::fixture_json("manage_tags_viewer.json");
    let (_directory, store, files) = seed(&recorded);
    allow_remove(&store, false);
    let mut model = ManageTags::new_viewer(store.clone(), files[0]).unwrap();
    assert!(model.is_immediate());
    let step = |n: &str| -> Value {
        recorded["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["step"] == n)
            .unwrap()
            .clone()
    };
    let check = |model: &ManageTags, store: &Store, name: &str, asked: &[String]| {
        let expected = step(name);
        assert_eq!(rows(model), expected["rows"], "{name}: rows");
        assert_eq!(
            stored(store, &files),
            expected["stored"],
            "{name}: database"
        );
        assert_eq!(
            !model.staged_changes().is_empty(),
            expected["has_changes"].as_bool().unwrap(),
            "{name}: nothing is left waiting"
        );
        assert_eq!(json!(asked), expected["asked"], "{name}: questions");
    };
    check(&model, &store, "opened_on_first", &[]);

    // typed entry is written at once
    assert_eq!(
        model.add_tags(&["v:new".into()], false).unwrap(),
        Entered::Done
    );
    assert!(model.take_committed());
    check(&model, &store, "typed_add_commits_at_once", &[]);

    // the viewer moves on: the dialog is about the second file
    model.set_file(files[1]);
    check(&model, &store, "follows_to_second", &[]);
    model.add_tags(&["v:shared".into()], false).unwrap();
    assert!(!model.take_committed(), "nothing to write");
    check(
        &model,
        &store,
        "typed_existing_tag_does_nothing_by_default",
        &[],
    );
    allow_remove(&store, true);
    model.add_tags(&["v:shared".into()], false).unwrap();
    assert!(model.take_committed());
    check(
        &model,
        &store,
        "typed_existing_tag_removes_when_allowed",
        &[],
    );

    // removal asks, then writes
    let Removal::Confirm { message, tags } = model.remove_tags(&["v:b".into()]).unwrap() else {
        panic!("the confirmation is on");
    };
    assert!(!model.take_committed(), "not before the yes");
    model.confirm_removal(&tags).unwrap();
    assert!(model.take_committed());
    check(&model, &store, "remove_confirms_then_commits", &[message]);

    // the viewer shows the third file; going back to a file already shown
    // reads what was written
    model.set_file(files[2]);
    check(&model, &store, "follows_to_third", &[]);
    model.set_file(files[0]);
    assert_eq!(
        rows(&model),
        json!(["v:a (1)", "v:new (1)", "v:shared (1)"])
    );
}
