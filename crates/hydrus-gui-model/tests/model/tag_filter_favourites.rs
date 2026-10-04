//! Replay the actual reference favourite actions, including rejected changes.
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::tag_filter_editor::{
    FAVOURITE_NAME, FavouriteTagFilters, NO_FAVOURITES, TagFilterEditor, delete_favourite,
    export_favourite, import_favourite, overwrite_favourite,
};
use hydrus_store::Store;
use serde_json::{Value, json};

fn rules(filter: &TagFilter) -> Value {
    json!(
        filter
            .rules()
            .map(|(s, r)| (s, if r == FilterRule::Blacklist { 1 } else { 0 }))
            .collect::<Vec<_>>()
    )
}
fn names(saved: &FavouriteTagFilters) -> Vec<String> {
    saved.0.iter().map(|(n, _)| n.clone()).collect()
}

#[test]
fn reference_favourite_workflow_preserves_cancelled_drafts_and_saves_immediately() {
    let f = hydrus_testkit::fixture_json("tag_filter_favourites.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let initial = TagFilter::new().with_rule("goblin", FilterRule::Blacklist);
    let replacement = TagFilter::new().with_rule(":", FilterRule::Blacklist);
    let mut editor = TagFilterEditor::new(&initial, false, &[]);
    assert_eq!(f["empty_menu"], json!([NO_FAVOURITES]));
    for event in f["events"].as_array().unwrap() {
        let action = event["action"].as_str().unwrap();
        let mut questions = Vec::new();
        match action {
            "save_cancel" => questions.push(FAVOURITE_NAME.to_owned()),
            "save" | "save_no" | "save_yes" => {
                if action != "save" {
                    editor.set_value(&replacement);
                }
                questions.push(FAVOURITE_NAME.to_owned());
                if action != "save" {
                    questions.push(overwrite_favourite("zebra"));
                }
                let changed = FavouriteTagFilters::save(
                    &store,
                    "zebra".into(),
                    editor.value(),
                    action == "save_yes",
                )
                .unwrap();
                assert_eq!(changed, action != "save_no");
            }
            "load" => {
                editor.set_value(&initial);
                let saved = FavouriteTagFilters::load(&store).unwrap();
                assert_eq!(f["load_menu"], json!(names(&saved)));
                editor.set_value(&saved.get("zebra").unwrap());
            }
            "import_cancel" | "import_no" | "import" | "import_yes" => {
                let imported = import_favourite(f["dirty_payload"].as_str().unwrap()).unwrap();
                questions.push(FAVOURITE_NAME.to_owned());
                if action != "import_cancel" {
                    let name = if action == "import" { "apple" } else { "zebra" };
                    if action != "import" {
                        questions.push(overwrite_favourite(name));
                    }
                    if FavouriteTagFilters::save(
                        &store,
                        name.into(),
                        imported.clone(),
                        action == "import_yes",
                    )
                    .unwrap()
                    {
                        editor.set_value(&imported);
                    }
                }
            }
            "import_invalid" => {
                assert!(
                    import_favourite("not json")
                        .unwrap_err()
                        .contains("JSON-serialised Tag Filter object")
                );
                assert_eq!(
                    f["export_menu"],
                    json!(["this tag filter", "zebra", "apple"])
                );
                assert_eq!(
                    import_favourite(&export_favourite(&editor.value())).unwrap(),
                    import_favourite(f["payload"].as_str().unwrap()).unwrap()
                );
            }
            "import_wrong_type" => assert!(import_favourite("[26, 3, []]").is_err()),
            "delete_no" | "delete_yes" => {
                questions.push(delete_favourite("apple"));
                if action == "delete_no" {
                    assert_eq!(
                        f["delete_menu"],
                        json!(names(&FavouriteTagFilters::load(&store).unwrap()))
                    );
                }
                if action == "delete_yes" {
                    FavouriteTagFilters::delete(&store, "apple".into()).unwrap();
                }
            }
            _ => panic!("unknown reference action {action}"),
        }
        assert_eq!(event["questions"], json!(questions), "{action}");
        assert_eq!(event["draft"], rules(&editor.value()), "{action}");
        let saved = FavouriteTagFilters::load(&store).unwrap();
        let actual: serde_json::Map<String, Value> = saved
            .0
            .iter()
            .map(|(name, filter)| (name.clone(), rules(filter)))
            .collect();
        assert_eq!(event["favourites"], Value::Object(actual), "{action}");
    }
    drop(editor); // cancelling the caller never rolls back shared favourites
    let reopened = Store::open(dir.path()).unwrap();
    let saved = FavouriteTagFilters::load(&reopened).unwrap();
    assert_eq!(f["reopen_menu"], json!(names(&saved)));
    let mut loaded = saved.get("zebra").unwrap();
    loaded.set_rule("changed", FilterRule::Blacklist);
    assert_ne!(saved.get("zebra").unwrap(), loaded);
}

#[test]
fn concurrent_favourite_saves_merge_and_invalid_exchange_cannot_change_settings() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let stale = FavouriteTagFilters::load(&store).unwrap();
    FavouriteTagFilters::save(&store, "A".into(), TagFilter::new(), false).unwrap();
    assert!(stale.get("A").is_none());
    FavouriteTagFilters::save(&store, "a".into(), TagFilter::new(), false).unwrap();
    assert_eq!(
        names(&FavouriteTagFilters::load(&store).unwrap()),
        ["A", "a"]
    );
    for invalid in [
        "[44, 2, []]",
        "[44, 1, [[\"goblin\", 2]]]",
        "[44, 1, [[null, 1]]]",
        "[44, 1, [[\"valid\", 1], [\"invalid\"]]]",
    ] {
        assert!(import_favourite(invalid).is_err());
    }
    assert_eq!(
        names(&FavouriteTagFilters::load(&store).unwrap()),
        ["A", "a"]
    );
    FavouriteTagFilters::delete(&store, "A".into()).unwrap();
    assert_eq!(names(&FavouriteTagFilters::load(&store).unwrap()), ["a"]);
}
