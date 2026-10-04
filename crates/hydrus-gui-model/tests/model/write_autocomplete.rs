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

#[test]
fn tag_menu_copy_decorations_favourites_and_launch_replay_real_qt_actions() {
    use hydrus_core::ServiceKey;
    use hydrus_gui_model::write_tag_menu::{Action, Entry};
    fn action(entries: &[Entry], label: &str) -> Option<Action> {
        entries.iter().find_map(|entry| match entry {
            Entry::Item(text, action) | Entry::Check(text, action, _) if text == label => {
                Some(action.clone())
            }
            Entry::Menu(_, entries) => action(entries, label),
            _ => None,
        })
    }
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &settings::FavouriteTags(vec![
                    "parity:root".into(),
                    "parity:amber old".into(),
                    "parity:favorite new".into(),
                ]),
            )
        })
        .unwrap();
    let original: TagEditingSettings = store.read(settings::get).unwrap();
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), LocationContext::default());
    input.set_text("parity:amber old");
    input.fetch();
    for event in fixture["menus"].as_array().unwrap() {
        if event["action"] == "open" {
            fn paths(entries: &[Entry], prefix: &[String], out: &mut Vec<Vec<String>>) {
                for entry in entries {
                    match entry {
                        Entry::Item(label, _) | Entry::Check(label, _, _) => {
                            let mut path = prefix.to_vec();
                            path.push(label.clone());
                            out.push(path);
                        }
                        Entry::Menu(label, entries) => {
                            let mut path = prefix.to_vec();
                            path.push(label.clone());
                            paths(entries, &path, out);
                        }
                        Entry::Separator => {}
                    }
                }
            }
            let row = input
                .rows()
                .iter()
                .position(|r| r.tag == "parity:amber old")
                .unwrap();
            let mut actual = Vec::new();
            paths(&input.menu(row), &[], &mut actual);
            for path in event["paths"].as_array().unwrap() {
                let first = path[0].as_str().unwrap();
                if first.contains("siblings") || first.contains("parents") {
                    let path: Vec<_> = path
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|s| s.as_str().unwrap().to_owned())
                        .collect();
                    assert!(
                        actual.contains(&path),
                        "missing recorded relationship path {path:?}"
                    );
                }
            }
            continue;
        }
        let favourite = event["action"] == "favourite";
        if favourite {
            input.set_text("parity:menu new");
        } else if event["action"] == "decorator" {
            input.set_text("parity:amber old");
        }
        let row = input
            .rows()
            .iter()
            .position(|row| {
                row.tag
                    == if favourite {
                        "parity:menu new"
                    } else {
                        "parity:amber old"
                    }
            })
            .unwrap();
        let chosen = action(&input.menu(row), event["label"].as_str().unwrap()).unwrap();
        match chosen {
            Action::Copy(text) => {
                if event["label"].as_str().unwrap().ends_with("and 2 parents") {
                    // Qt stores parent tags as a set; only that set's order is unspecified.
                    let mut actual: Vec<_> = text.lines().collect();
                    let expected = event["copied"][0].as_str().unwrap();
                    let mut expected: Vec<_> = expected.lines().collect();
                    assert_eq!(actual[0], expected[0]);
                    actual[1..].sort_unstable();
                    expected[1..].sort_unstable();
                    assert_eq!(actual, expected);
                } else {
                    assert_eq!(json!([text]), event["copied"]);
                }
            }
            Action::Domain(..) | Action::Locations(..) => {
                panic!("unexpected domain action in tag menu replay")
            }
            Action::Relationship { .. } => panic!("unexpected relationship action in menu replay"),
            Action::Launch {
                location,
                context,
                predicates,
                duplicate,
            } => {
                let actual = &event["launched"][0];
                assert_eq!(
                    json!(
                        location
                            .current()
                            .iter()
                            .map(ServiceKey::to_hex)
                            .collect::<Vec<_>>()
                    ),
                    actual["current"]
                );
                assert_eq!(
                    json!(
                        location
                            .deleted()
                            .iter()
                            .map(ServiceKey::to_hex)
                            .collect::<Vec<_>>()
                    ),
                    actual["deleted"]
                );
                assert_eq!(
                    json!(if duplicate {
                        "new_page_duplicates"
                    } else {
                        "new_page_query"
                    }),
                    actual["topic"]
                );
                assert_eq!(
                    predicates,
                    vec![hydrus_core::search::predicate::Predicate::Tag {
                        tag: Tag::new("parity:amber old").unwrap(),
                        inclusive: true
                    }]
                );
                let defaults: settings::SearchDefaults = store.read(settings::get).unwrap();
                assert_eq!(context.service, defaults.tag_service);
            }
            Action::Decorate { tab, kind, value } => {
                input.decorate(tab, kind, value);
                let labels: Vec<_> = input
                    .menu(row.min(input.rows().len() - 1))
                    .into_iter()
                    .filter_map(|e| {
                        if let Entry::Item(label, _) = e {
                            Some(label)
                        } else {
                            None
                        }
                    })
                    .collect();
                for path in event["paths"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|p| p.as_array().unwrap().len() == 1)
                {
                    let label = path[0].as_str().unwrap();
                    if label.contains("decorators") || label.contains("parent rows") {
                        assert!(labels.iter().any(|s| s == label), "{event}");
                    }
                }
                assert_eq!(
                    store.read(settings::get::<TagEditingSettings>).unwrap(),
                    original
                );
            }
            action @ Action::Favourite { .. } => {
                let asked = event["asked"].as_array().unwrap();
                assert_eq!(
                    action.question(),
                    asked.first().and_then(|q| q["message"].as_str())
                );
                if action.question().is_none() || event["answer"] == true {
                    action.persist(&store).unwrap();
                }
                let favourites: settings::FavouriteTags = store.read(settings::get).unwrap();
                assert_eq!(json!(favourites.0), event["favourites"]);
                let tabs: settings::TagAutocompleteTabs = store.read(settings::get).unwrap();
                assert_eq!(
                    json!(
                        tabs.most_used
                            .get(&key.to_hex())
                            .cloned()
                            .unwrap_or_default()
                    ),
                    event["most_used"]
                );
            }
        }
    }
    let mut reopened = WriteAutocomplete::new(store.clone(), key, LocationContext::default());
    reopened.set_text("parity:amber old");
    reopened.fetch();
    assert!(reopened.rows().iter().any(|r| r.parent_row));
    assert!(reopened.rows().iter().any(|r| r.label.contains('→')));
}

#[test]
fn seeded_relationship_editors_replay_service_defaults_memory_and_cancel() {
    use hydrus_gui_model::tag_relationships::{RelationKind, Relationships};
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let my_tags = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    for event in fixture["seeded_dialogs"].as_array().unwrap() {
        let kind = if event["kind"] == "siblings" {
            RelationKind::Siblings
        } else {
            RelationKind::Parents
        };
        let key = my_tags.clone();
        store
            .write(move |ctx| {
                let mut prefs: TagEditingSettings = settings::get(ctx.conn())?;
                prefs.default_service = key;
                prefs.remember_service = true;
                settings::set(ctx.conn(), &prefs)
            })
            .unwrap();
        let mut model =
            Relationships::new_with_tags(store.clone(), kind, &["parity:amber old".into()])
                .unwrap();
        assert_eq!(
            model.service_names()[model.service()],
            event["initial"].as_str().unwrap()
        );
        for seed in event["seeds"].as_array().unwrap() {
            let service = model
                .service_names()
                .iter()
                .position(|s| s == seed["service"].as_str().unwrap())
                .unwrap();
            model.choose_service(service);
            assert_eq!(json!(model.inputs().0), seed["tags"]);
            assert!(model.inputs().1.is_empty());
        }
        let remembered = model
            .service_names()
            .iter()
            .position(|s| s == event["remembered"].as_str().unwrap())
            .unwrap();
        model.choose_service_remembered(remembered).unwrap();
        let prefs: TagEditingSettings = store.read(settings::get).unwrap();
        assert_eq!(
            prefs.default_service,
            model.service_key(remembered).unwrap()
        );
        store
            .write(|ctx| {
                let mut prefs: TagEditingSettings = settings::get(ctx.conn())?;
                prefs.remember_service = false;
                settings::set(ctx.conn(), &prefs)
            })
            .unwrap();
        let mine = model
            .service_names()
            .iter()
            .position(|s| s == "my tags")
            .unwrap();
        model.choose_service_remembered(mine).unwrap();
        let prefs: TagEditingSettings = store.read(settings::get).unwrap();
        assert_eq!(
            store
                .snapshot()
                .services
                .by_key(&prefs.default_service)
                .unwrap()
                .name,
            event["disabled_memory"].as_str().unwrap()
        );
        model.enter_tags(true, "parity:unapplied relation").unwrap();
        model.add(&[]).unwrap();
        drop(model);
        let reopened = Relationships::new(store.clone(), kind).unwrap();
        assert_eq!(
            reopened.service_names()[reopened.service()],
            event["disabled_memory"].as_str().unwrap()
        );
        assert!(
            reopened
                .rows()
                .iter()
                .all(|r| r.pair.1 != "parity:unapplied relation")
        );
    }
}

#[test]
fn write_domain_menus_and_interlocks_replay_reference_without_persisting_options() {
    use hydrus_core::{ServiceKey, service::builtin_keys};
    use hydrus_gui_model::write_tag_menu::{Action, Entry};
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let (_dir, store) = seeded(&fixture);
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let initial = LocationContext::single(ServiceKey::new(builtin_keys::MY_FILES.to_vec()));
    let before = serde_json::to_value(
        store
            .read(settings::get::<hydrus_store::tag_display_config::AutocompleteWidgetSettings>)
            .unwrap(),
    )
    .unwrap();
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), initial.clone());
    for event in fixture["domains"].as_array().unwrap() {
        let entries = input.domain_menu(event["tags"].as_bool().unwrap());
        let actual: Vec<_> = entries
            .iter()
            .filter_map(|entry| {
                if let Entry::Check(label, _, checked) = entry {
                    Some(json!({"label":label,"checked":checked}))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(json!(actual), event["rows"]);
        if let Some(label) = event["choose"].as_str() {
            let choice = entries
                .into_iter()
                .find_map(|entry| {
                    if let Entry::Check(text, Action::Domain(choice), _) = entry {
                        (text == label).then_some(choice)
                    } else {
                        None
                    }
                })
                .unwrap();
            input.choose_domain(choice);
        }
        let domains = input.domains();
        let (files, tags) = input.domain_labels();
        assert_eq!(
            json!({"current":domains.location.current().iter().map(ServiceKey::to_hex).collect::<Vec<_>>(),"deleted":domains.location.deleted().iter().map(ServiceKey::to_hex).collect::<Vec<_>>(),"service":domains.tags.service.to_hex(),"display":domains.tags.display_service.to_hex(),"current_tags":domains.tags.include_current,"pending_tags":domains.tags.include_pending,"file_label":files,"tag_label":tags}),
            event["after"]
        );
        assert_eq!(
            serde_json::to_value(store
                .read(settings::get::<hydrus_store::tag_display_config::AutocompleteWidgetSettings>)
                .unwrap()).unwrap(),
            before
        );
    }
    // Per-service drafts survive switching a Manage Tags input; a new widget uses defaults.
    let previous = input.domains();
    let other = store
        .snapshot()
        .services
        .by_name("second tags")
        .unwrap()
        .key
        .clone();
    input.set_context(other, initial.clone());
    assert_ne!(input.domains(), previous);
    input.set_context(key.clone(), initial.clone());
    assert_eq!(input.domains(), previous);
    let mut reopened = WriteAutocomplete::new(store.clone(), key, initial);
    assert_ne!(reopened.domains(), previous);
    reopened.set_text("parity:amb");
    reopened.fetch();
    let counted = reopened.rows().iter().filter(|r| r.counted).count();
    assert!(counted > 0);
    reopened.choose_domain(hydrus_gui_model::domains::Choice::Location(
        LocationContext::new([], []),
    ));
    assert!(reopened.rows().iter().all(|row| !row.counted));
}

#[test]
fn favourite_options_replay_add_only_choices_and_parent_transaction() {
    use hydrus_gui_model::{
        options::{Editor, Kind, Settings},
        write_autocomplete::{Tab, TagEntry},
    };
    use hydrus_store::settings::FavouriteTags;
    let fixture = hydrus_testkit::fixture_json("write_tag_autocomplete.json");
    let recorded = &fixture["favourite_options"];
    let initial: Vec<String> = serde_json::from_value(recorded["initial"].clone()).unwrap();
    let (_directory, store) = seeded(&fixture);
    let initial_setting = FavouriteTags(initial.clone());
    store
        .write(move |ctx| settings::set(ctx.conn(), &initial_setting))
        .unwrap();
    let before = store.read(Settings::load).unwrap();
    let mut editor = Editor::new(before.clone());
    let pages = hydrus_gui_model::options::pages(&before);
    let page = pages
        .iter()
        .find(|page| page.name == "tag autocomplete tabs")
        .unwrap();
    assert!(
        page.options()
            .iter()
            .any(|option| option.kind == Kind::FavouriteTags)
    );
    let service = hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::COMBINED_TAG);
    let mut child = TagEntry::new(
        WriteAutocomplete::new(store.clone(), service.clone(), LocationContext::default()),
        &initial,
    )
    .additions_only();
    for event in recorded["events"].as_array().unwrap() {
        match event["action"].as_str().unwrap() {
            "initial" => {}
            "manual" | "repeat_manual" => {
                child.input.set_text("parity:favourite 1");
                child.enter(None);
            }
            "paste" | "repeat_paste" => {
                child.paste(&[
                    "parity:favourite 2".into(),
                    "parity:favourite 3".into(),
                    "parity:favourite pasted".into(),
                ]);
            }
            "remove" => {
                let index = child
                    .tags()
                    .iter()
                    .position(|tag| tag == "parity:favourite 2")
                    .unwrap();
                child.remove(index);
            }
            "apply" => {
                editor.set_favourite_tags(&child.tags());
            }
            action => panic!("unexpected recorded action {action}"),
        }
        assert_eq!(serde_json::json!(child.tags()), event["rows"]);
        assert_eq!(
            store.read(Settings::load).unwrap().favourite_tags,
            before.favourite_tags
        );
    }
    let (after, original, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(
        serde_json::json!(after.favourite_tags.0),
        recorded["events"][6]["saved"]
    );
    let original = original.clone();
    // A concurrently edited child cap is unrelated to this favourite-list draft.
    store
        .write(|ctx| {
            let mut tabs: settings::TagAutocompleteTabs = settings::get(ctx.conn())?;
            tabs.children_limit = Some(1);
            settings::set(ctx.conn(), &tabs)
        })
        .unwrap();
    store
        .write(move |ctx| after.save(ctx.conn(), &original))
        .unwrap();
    let reopened = store.read(Settings::load).unwrap();
    assert_eq!(
        serde_json::json!(reopened.favourite_tags.0),
        recorded["cancelled_saved"]
    );
    assert_eq!(reopened.tag_autocomplete_tabs.children_limit, Some(1));
    let mut consumer = WriteAutocomplete::new(store.clone(), service, LocationContext::default());
    consumer.set_tab(Tab::Favourites);
    let offered: std::collections::BTreeSet<_> =
        consumer.rows().iter().map(|row| row.tag.clone()).collect();
    assert_eq!(offered, reopened.favourite_tags.0.iter().cloned().collect());
    let mut cancelled = Editor::new(reopened.clone());
    cancelled.set_favourite_tags(&["parity:cancelled".into()]);
    assert_eq!(store.read(Settings::load).unwrap(), reopened);
    assert_eq!(
        Editor::new(reopened.clone()).edited_favourite_tags(),
        reopened.favourite_tags
    );
}
