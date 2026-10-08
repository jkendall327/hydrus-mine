//! Real reference list rows/questions plus frozen selection and staged isolation.
use hydrus_core::{
    import_options::{CallerType, ImportOptionsManager, ImportOptionsSlice, NoteImportOptions},
    service::builtin_keys,
    url::{UrlClass, UrlType},
};
use hydrus_gui_model::import_options_panel::{Editor, List, Reset, Target, Value};
use hydrus_store::settings::ImportOptionsUiSettings;

fn name(key: &str) -> String {
    if hex::decode(key).unwrap() == builtin_keys::DOWNLOADER_TAGS {
        "downloader tags".into()
    } else {
        "my files".into()
    }
}
fn classes() -> Vec<UrlClass> {
    [
        (1, "alpha post", UrlType::Post),
        (2, "beta watch", UrlType::Watchable),
        (3, "gamma gallery", UrlType::Gallery),
        (4, "excluded file", UrlType::File),
    ]
    .into_iter()
    .map(|(key, name, url_type)| UrlClass {
        key: vec![key; 32],
        name: name.into(),
        url_type,
        ..UrlClass::default()
    })
    .collect()
}
fn value() -> Value {
    let mut manager = ImportOptionsManager::default();
    manager.favourites.clear();
    Value {
        manager,
        ui: ImportOptionsUiSettings::default(),
    }
}
fn cells(editor: &Editor, list: List) -> serde_json::Value {
    serde_json::json!(
        editor
            .rows(list, &name)
            .iter()
            .map(|row| row.cells.clone())
            .collect::<Vec<_>>()
    )
}
fn select(editor: &mut Editor, list: List, names: &serde_json::Value) {
    let rows = editor.rows(list, &name);
    let mut first = true;
    for (index, row) in rows.iter().enumerate() {
        if names
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name.as_str() == Some(row.cells[0].as_str()))
        {
            editor.click(list, index, !first, false, &name);
            first = false;
        }
    }
}

// leaf: audit-options-import-options-favourites-profiles-delete
#[test]
fn three_lists_and_staged_actions_replay_the_reference() {
    let fixture = hydrus_testkit::fixture_json("import_options_panel.json");
    let original = value();
    let mut editor = Editor::new(&original, &classes());
    for (list, key) in [
        (List::Defaults, "defaults"),
        (List::UrlClasses, "urls"),
        (List::Favourites, "favourites"),
    ] {
        assert_eq!(cells(&editor, list), fixture["initial"][key]);
    }
    let incoming = ImportOptionsSlice {
        notes: Some(NoteImportOptions::default()),
        ..ImportOptionsSlice::default()
    };
    for step in fixture["steps"].as_array().unwrap() {
        for (list, key) in [
            (List::Defaults, "defaults"),
            (List::UrlClasses, "urls"),
            (List::Favourites, "favourites"),
        ] {
            select(&mut editor, list, &step["selection"][key]);
        }
        let calls = step["calls"].as_array().unwrap();
        match step["action"].as_str().unwrap() {
            "_SeeDefaultStack" | "_SeeURLClassStack" => {
                let list = if step["action"] == "_SeeDefaultStack" {
                    List::Defaults
                } else {
                    List::UrlClasses
                };
                assert_eq!(
                    editor.stack(&editor.one(list, &name).unwrap()).unwrap(),
                    calls[0]["information"]
                );
            }
            "_ShowTLDR" => assert_eq!(
                hydrus_gui_model::import_options_panel::TLDR,
                calls[0]["information"]
            ),
            "_EditDefault" | "_EditURLClass" => {
                if calls[0]["accepted"] == true {
                    let list = if step["action"] == "_EditDefault" {
                        List::Defaults
                    } else {
                        List::UrlClasses
                    };
                    let target = editor.one(list, &name).unwrap();
                    assert!(editor.set(&target, incoming.clone()));
                }
            }
            "_ClearDefault" | "_ClearURLClass" => {
                let list = if step["action"] == "_ClearDefault" {
                    List::Defaults
                } else {
                    List::UrlClasses
                };
                match editor.clear_request(list, &name) {
                    Err(message) => assert_eq!(message, calls[0]["information"]),
                    Ok(Some(request)) => {
                        assert_eq!(request.message, calls[0]["question"]);
                        if calls[0]["answer"] == true {
                            editor.clear(&request.targets);
                        }
                    }
                    Ok(None) => panic!("recorded clear has no selection"),
                }
            }
            "_AddFavourite" => {
                if calls[0]["accepted"] == true {
                    editor.save_favourite(None, "notes", incoming.clone());
                }
            }
            "_EditFavourite" => {
                let Target::Favourite(original) = editor.one(List::Favourites, &name).unwrap()
                else {
                    panic!()
                };
                editor.save_favourite(Some(&original), "notes (1)", incoming.clone());
            }
            "_DeleteFavourite" => {
                let request = editor.delete_request(&name).unwrap();
                if step["error"].is_null() {
                    assert_eq!(request.message, calls[0]["question"]);
                    if calls[0]["answer"] == true {
                        editor.delete(&request.targets);
                    }
                } else {
                    assert!(
                        request
                            .message
                            .starts_with("Delete the favourite/profile named")
                    );
                }
            }
            "_ResetDefaultToDefault" => {
                let reset =
                    Reset::ALL[usize::try_from(calls[0]["answer"].as_u64().unwrap()).unwrap()];
                assert_eq!(reset.question(), calls[1]["question"]);
                for (row, reset) in calls[0]["choices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(Reset::ALL)
                {
                    assert_eq!(
                        row,
                        &serde_json::json!([reset.label(), reset.key(), reset.description()])
                    );
                }
                if calls[1]["answer"] == true {
                    editor.reset(reset);
                }
            }
            action => panic!("unhandled action {action}"),
        }
        assert_eq!(cells(&editor, List::Defaults), step["state"]["defaults"]);
        assert_eq!(cells(&editor, List::UrlClasses), step["state"]["urls"]);
        if step["action"] == "_ResetDefaultToDefault" && calls[0]["answer"] == 2 {
            assert_eq!(
                cells(&editor, List::Favourites),
                fixture["reopened_favourites"]
            );
        } else {
            assert_eq!(
                cells(&editor, List::Favourites),
                step["state"]["favourites"]
            );
        }
    }
    assert!(fixture["draft_isolated"].as_bool().unwrap());
    assert_eq!(original, value());
    editor.ui.simple = false;
    let reopened = Editor::new(&editor.value(), &classes());
    assert_eq!(reopened.ui.simple, fixture["reopened_simple"]);
    assert_eq!(
        cells(&reopened, List::Favourites),
        fixture["reopened_favourites"]
    );
}

#[test]
fn sorted_selection_frozen_clear_and_profile_identity_are_independent() {
    let original = value();
    let mut editor = Editor::new(&original, &classes());
    let notes = ImportOptionsSlice {
        notes: Some(NoteImportOptions::default()),
        ..ImportOptionsSlice::default()
    };
    editor.set(&Target::Caller(CallerType::PostUrls), notes.clone());
    let index = editor
        .rows(List::Defaults, &name)
        .iter()
        .position(|row| row.target == Target::Caller(CallerType::PostUrls))
        .unwrap();
    editor.click(List::Defaults, index, false, false, &name);
    let request = editor
        .clear_request(List::Defaults, &name)
        .unwrap()
        .unwrap();
    editor.sort(List::Defaults, 0, false);
    assert_eq!(
        editor.one(List::Defaults, &name),
        Some(Target::Caller(CallerType::PostUrls))
    );
    editor.click(List::Defaults, 0, false, false, &name);
    editor.clear(&request.targets);
    assert_eq!(
        editor.own(&Target::Caller(CallerType::PostUrls)).unwrap(),
        ImportOptionsSlice::default()
    );
    assert_eq!(
        editor.own(&Target::Caller(CallerType::Global)),
        original.manager.caller_default(CallerType::Global).cloned()
    );
    assert!(!editor.set(&Target::Url("missing".into()), notes.clone()));
    assert!(!editor.set(&Target::Favourite("missing".into()), notes.clone()));
    let first = editor.save_favourite(None, "profile", notes.clone());
    editor.save_favourite(None, "profile", notes.clone());
    editor.click(List::Favourites, 0, false, false, &name);
    let renamed = editor.save_favourite(Some(&first), "profile (1)", notes);
    assert_eq!(renamed, "profile (1) (1)");
    assert_eq!(
        editor.one(List::Favourites, &name),
        Some(Target::Favourite(renamed))
    );
    editor.click(List::Favourites, 1, false, true, &name);
    assert_eq!(editor.selected(List::Favourites, &name).len(), 2);
    let request = editor.delete_request(&name).unwrap();
    editor.click(List::Favourites, 0, false, false, &name);
    editor.delete(&request.targets);
    assert!(editor.rows(List::Favourites, &name).is_empty());
    assert_eq!(original, value());
}
