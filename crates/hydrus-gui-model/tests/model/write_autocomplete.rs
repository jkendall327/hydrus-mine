//! Real write suggestions and paste decisions replayed from the running Qt client.
use std::sync::Arc;

use hydrus_core::{Sha256, Tag, search::context::LocationContext};
use hydrus_gui_model::{
    manage_tags::ManageTags,
    write_autocomplete::{Paste, WriteAutocomplete, paste},
};
use hydrus_store::{Store, settings, tag_editing::TagEditingSettings};
use serde_json::{Value, json};

fn seeded(fixture: &Value) -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let corpus = fixture["corpus"].clone();
    store
        .write_content(move |w| {
            for row in corpus.as_array().unwrap() {
                let tag = hydrus_store::master::intern_tag(
                    w.conn(),
                    &Tag::new(row["tag"].as_str().unwrap()).unwrap(),
                )?;
                let files = row["hashes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|h| {
                        let hash: Sha256 = h.as_str().unwrap().parse().unwrap();
                        hydrus_store::master::hash_id(w.conn(), &hash).map(Option::unwrap)
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                w.update_mappings(
                    service,
                    &hydrus_store::content::MappingAction::Add,
                    tag,
                    &files,
                )?;
            }
            Ok(())
        })
        .unwrap();
    use hydrus_store::{
        content::tag_relations::{self, RelationAction, RelationUpdate},
        display::RelationKind,
    };
    for (kind, pairs) in [
        (
            RelationKind::Siblings,
            vec![("parity:amber old", "parity:amber")],
        ),
        (
            RelationKind::Parents,
            vec![
                ("parity:amber", "parity:colour"),
                ("parity:colour", "parity:root"),
            ],
        ),
    ] {
        tag_relations::apply(
            &store,
            kind,
            pairs
                .into_iter()
                .map(|(left, right)| RelationUpdate {
                    service,
                    left: Tag::new(left).unwrap(),
                    right: Tag::new(right).unwrap(),
                    action: RelationAction::Add,
                })
                .collect(),
        )
        .unwrap();
    }
    (dir, store)
}

#[test]
fn exact_rows_counts_domains_decorations_and_first_selection_match_reference() {
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), LocationContext::default());
    for query in fixture["queries"].as_array().unwrap() {
        let query_copy = query.clone();
        let display_key = key.clone();
        let count_key = store
            .snapshot()
            .services
            .by_name(query["service"].as_str().unwrap())
            .unwrap()
            .key
            .clone();
        store
            .write(move |ctx| {
                let mut settings: TagEditingSettings = settings::get(ctx.conn())?;
                settings.select_first_with_count =
                    query_copy["first_with_count"].as_bool().unwrap();
                settings.autocomplete_expand_parents = query_copy["expanded"].as_bool().unwrap();
                settings.autocomplete_show_parents = query_copy["parents"].as_bool().unwrap();
                settings.autocomplete_show_siblings = query_copy["siblings"].as_bool().unwrap();
                settings::set(ctx.conn(), &settings)?;
                let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                    settings::get(ctx.conn())?;
                let mut options = widgets.options(&display_key);
                options.write_tag_service = count_key;
                widgets.services.insert(display_key.to_hex(), options);
                settings::set(ctx.conn(), &widgets)
            })
            .unwrap();
        input.set_text(query["text"].as_str().unwrap());
        let mut actual: Vec<Value> = Vec::new();
        for row in input.rows() {
            if row.parent_row {
                actual.last_mut().unwrap()["rows"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(row.label));
            } else {
                actual.push(json!({"tag":row.tag,"count":row.counted,"rows":[row.label]}));
            }
        }
        assert_eq!(json!(actual), query["suggestions"], "{query}");
        assert_eq!(json!([input.chosen(None).unwrap()]), query["selected"]);
    }
    // Fetch-as-you-type disabled must suppress suggestions, while Ctrl+Space still fetches.
    store
        .write(move |ctx| {
            let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                settings::get(ctx.conn())?;
            let mut options = widgets.options(&key);
            options.fetch_automatically = false;
            widgets.services.insert(key.to_hex(), options);
            settings::set(ctx.conn(), &widgets)
        })
        .unwrap();
    input.set_text("parity:amb");
    // The raw typed tag is still available for entry, as in WriteFetch's disabled path.
    assert_eq!(input.suggestions().len(), 1);
    input.fetch();
    assert_eq!(input.rows().iter().filter(|r| !r.parent_row).count(), 3);
}

#[test]
fn paste_questions_skip_and_add_only_staging_follow_reference() {
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    for event in fixture["paste"].as_array().unwrap() {
        let options = TagEditingSettings {
            skip_multiline_paste_confirmation: event["skip"].as_bool().unwrap(),
            ..TagEditingSettings::default()
        };
        let decision = paste(
            event["text"].as_str().unwrap(),
            event["button"].as_bool().unwrap(),
            &options,
        );
        let accepted = match decision {
            Paste::Text => {
                assert_eq!(event["consumed"], false);
                None
            }
            Paste::Tags(tags) => {
                assert!(event["asked"].as_array().unwrap().is_empty());
                Some(tags)
            }
            Paste::Confirm { message, tags } => {
                let recorded = event["asked"][0]["message"].as_str().unwrap();
                // Python's CleanTags is a set, so the question's tag order is unspecified.
                let (prefix, lines) = message.split_once("\n\n").unwrap();
                let (expected_prefix, expected_lines) = recorded.split_once("\n\n").unwrap();
                assert_eq!(prefix, expected_prefix);
                let mut expected: Vec<_> = expected_lines.lines().collect();
                expected.sort_unstable();
                assert_eq!(lines.lines().collect::<Vec<_>>(), expected);
                event["answer"].as_bool().unwrap().then_some(tags)
            }
        };
        assert_eq!(
            json!(accepted.into_iter().collect::<Vec<_>>()),
            event["pasted"]
        );
    }
    let file = store
        .read(|c| {
            Ok(c.query_row("SELECT hash_id FROM files LIMIT 1", [], |r| {
                r.get::<_, hydrus_core::HashId>(0)
            })?)
        })
        .unwrap();
    let mut editor = ManageTags::new(store.clone(), vec![file]).unwrap();
    editor
        .paste_tags(&["new staged tag".into(), "new staged tag".into()])
        .unwrap();
    editor.paste_tags(&["new staged tag".into()]).unwrap();
    assert!(editor.rows().iter().any(|(tag, _)| tag == "new staged tag"));
    drop(editor);
    assert!(
        !ManageTags::new(store, vec![file])
            .unwrap()
            .rows()
            .iter()
            .any(|(tag, _)| tag == "new staged tag")
    );
}

#[test]
fn six_write_options_replay_recorded_defaults_and_stay_staged_until_apply() {
    use hydrus_gui_model::options::{Editor, Row, Settings, Value};
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let before = store.read(Settings::load).unwrap();
    let mut editor = Editor::new(before.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|n| *n == "tag editing")
        .unwrap();
    editor.show_page(page);
    let controls: Vec<_> = editor
        .rows()
        .iter()
        .enumerate()
        .filter_map(|(i, row)| match row {
            Row::Opt { option, value, .. }
                if option.label.contains("autocomplete")
                    || option.label == "Autocomplete list height: " =>
            {
                Some((i, (**value).clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(controls.len(), 6);
    for ((row, value), expected) in controls
        .iter()
        .take(5)
        .zip(fixture["controls"]["values"].as_array().unwrap())
    {
        assert_eq!(*value, Value::Check(expected.as_bool().unwrap()));
        editor.check(*row, !expected.as_bool().unwrap());
    }
    assert_eq!(
        controls[5].1,
        Value::Int(fixture["controls"]["height"].as_i64().unwrap())
    );
    editor.number(controls[5].0, 3);
    let (after, _, _) = editor.applied();
    assert!(after.tag_editing.select_first_with_count);
    assert!(after.tag_editing.skip_multiline_paste_confirmation);
    assert!(!after.tag_editing.autocomplete_show_parents);
    assert!(!after.tag_editing.autocomplete_expand_parents);
    assert!(!after.tag_editing.autocomplete_show_siblings);
    assert_eq!(after.tag_editing.autocomplete_list_height, 3);
    let stored: TagEditingSettings = store.read(settings::get).unwrap();
    assert_eq!(stored, before.tag_editing); // Discarding an options draft has no side effects.
}

#[test]
fn relationship_inputs_replay_real_add_only_paste_and_opposite_side_removal() {
    use hydrus_gui_model::tag_relationships::{RelationKind, Relationships};
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    for (name, kind) in [
        ("siblings", RelationKind::Siblings),
        ("parents", RelationKind::Parents),
    ] {
        let mut model = Relationships::new(store.clone(), kind).unwrap();
        let service = model
            .service_names()
            .iter()
            .position(|n| n == "my tags")
            .unwrap();
        model.choose_service(service);
        for event in fixture["relationship_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["kind"] == name)
        {
            let action = event["action"].as_str().unwrap();
            let (right, tags) = match action {
                "paste_left" | "repeat_left" => (
                    false,
                    vec!["parity:paste left a".into(), "parity:paste left b".into()],
                ),
                "paste_right" | "repeat_right" if kind == RelationKind::Siblings => {
                    (true, vec!["parity:paste right".into()])
                }
                "paste_right" | "repeat_right" => (
                    true,
                    vec!["parity:paste right a".into(), "parity:paste right b".into()],
                ),
                "move_to_right" => (true, vec!["parity:paste left a".into()]),
                _ => panic!("unknown event {action}"),
            };
            model.paste_tags(right, &tags).unwrap();
            let (left, right) = model.inputs();
            assert_eq!(json!(left), event["left"]);
            assert_eq!(json!(right), event["right"]);
        }
        drop(model);
        let reopened = Relationships::new(store.clone(), kind).unwrap();
        assert_eq!(reopened.inputs(), (Vec::new(), Vec::new()));
    }
}

#[test]
fn detached_import_tag_lists_replay_reference_and_never_mutate_caller() {
    use hydrus_gui_model::write_autocomplete::TagEntry;
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    for (kind, name) in [("additional", "my tags"), ("whitelist", "all known tags")] {
        let key = store.snapshot().services.by_name(name).unwrap().key.clone();
        let caller = vec!["parity:caller initial".to_owned()];
        let mut model = TagEntry::new(
            WriteAutocomplete::new(store.clone(), key, LocationContext::default()),
            &caller,
        );
        for event in fixture["detached_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["kind"] == kind)
        {
            match event["action"].as_str().unwrap() {
                "initial" | "cancel" => {}
                "paste" | "repeat_paste" => {
                    model.paste(&["parity:caller initial".into(), "parity:child new".into()])
                }
                "typed_toggle" => {
                    model.input.set_text("parity:caller initial");
                    model.enter(None);
                }
                action => panic!("unknown action {action}"),
            }
            assert_eq!(json!(model.tags()), event["tags"]);
            assert_eq!(json!(caller), event["caller"]);
        }
        drop(model);
        assert_eq!(caller, ["parity:caller initial"]);
    }
}

#[test]
fn favourite_and_count_ordered_children_tabs_replay_reference_caps_and_context() {
    use hydrus_gui_model::write_autocomplete::Tab;
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), LocationContext::default());
    for event in fixture["tabs"].as_array().unwrap() {
        let data = event.clone();
        let display = key.clone();
        let count_key = store
            .snapshot()
            .services
            .by_name(event["service"].as_str().unwrap_or("my tags"))
            .unwrap()
            .key
            .clone();
        store
            .write(move |ctx| {
                let mut widgets: hydrus_store::tag_display_config::AutocompleteWidgetSettings =
                    settings::get(ctx.conn())?;
                let mut options = widgets.options(&display);
                options.write_tag_service = count_key;
                widgets.services.insert(display.to_hex(), options);
                settings::set(ctx.conn(), &widgets)?;
                if data["tab"] == "favourites" {
                    settings::set(
                        ctx.conn(),
                        &settings::FavouriteTags(
                            serde_json::from_value(data["tags"].clone()).unwrap(),
                        ),
                    )?;
                } else {
                    let mut tabs: settings::TagAutocompleteTabs = settings::get(ctx.conn())?;
                    tabs.children_limit =
                        data["limit"].as_u64().map(|n| usize::try_from(n).unwrap());
                    settings::set(ctx.conn(), &tabs)?;
                }
                Ok(())
            })
            .unwrap();
        if event["tab"] == "favourites" {
            input.set_tab(Tab::Favourites);
        } else {
            input.set_context_tags(
                event["context"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|t| t.as_str().unwrap().to_owned()),
            );
            input.set_tab(Tab::Children);
        }
        let mut actual: Vec<Value> = Vec::new();
        for row in input.rows() {
            if row.parent_row {
                actual.last_mut().unwrap()["rows"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(row.label));
            } else {
                actual.push(json!({"tag":row.tag,"rows":[row.label]}));
            }
        }
        let mut expected = event["rows"].clone();
        // Favourite String terms store parents in Python sets. Their base row/order
        // is exact; the unordered expanded-parent group is compared as a sorted set.
        if event["tab"] == "favourites" {
            for row in &mut actual {
                let rows = row["rows"].as_array_mut().unwrap();
                rows[1..].sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            }
            for row in expected.as_array_mut().unwrap() {
                let rows = row["rows"].as_array_mut().unwrap();
                rows[1..].sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            }
        }
        assert_eq!(json!(actual), expected, "{event}");
        assert!(input.rows().iter().all(|row| !row.counted));
    }
}

#[test]
fn children_limit_option_matches_reference_and_persists_only_accepted_drafts() {
    use hydrus_gui_model::options::{Editor, Row, Settings, Value};
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let original = store.read(Settings::load).unwrap();
    assert_eq!(original.tag_autocomplete_tabs.children_limit, Some(40));
    let mut editor = Editor::new(original.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|n| *n == "tag autocomplete tabs")
        .unwrap();
    editor.show_page(page);
    let row = editor.rows().iter().position(|r| matches!(r, Row::Opt { option, .. } if option.label == "How many tags to show in the children tab: ")).unwrap();
    assert!(matches!(
        editor.rows()[row],
        Row::Opt {
            value: Value::Noneable(Some(40)),
            ..
        }
    ));
    editor.number(row, 1);
    assert_eq!(
        editor.applied().0.tag_autocomplete_tabs.children_limit,
        Some(1)
    );
    let stored: settings::TagAutocompleteTabs = store.read(settings::get).unwrap();
    assert_eq!(stored.children_limit, Some(40));
    editor.none(row, true);
    let (accepted, _, _) = editor.applied();
    assert_eq!(accepted.tag_autocomplete_tabs.children_limit, None);
    store
        .write(move |ctx| accepted.save(ctx.conn(), &original))
        .unwrap();
    let persisted: settings::TagAutocompleteTabs = store.read(settings::get).unwrap();
    assert_eq!(persisted.children_limit, None);
    assert_eq!(
        fixture["children_control"],
        json!({"value":40,"min":1,"max":1_000_000})
    );
}
