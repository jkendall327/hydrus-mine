//! The duplicate metadata merge options editor, step by step as the
//! reference's (`oracle/record_merge_options_editor.py`), on the basic
//! fixture's services and merge options: its note, lists, choices, what it
//! asks and the options after each step.

use hydrus_core::ServiceKey;
use hydrus_core::notes::NoteConflict;
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::merge_options_editor::{
    self as editor, Choice, Choices, MergeOptionsEditor, Service,
};
use hydrus_store::Store;
use hydrus_store::duplicates::PairRelationship;
use hydrus_store::duplicates::merge::{DuplicateMergeSettings, MergeOptions};
use serde_json::{Value, json};

fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn filter(rules: &Value) -> TagFilter {
    let mut filter = TagFilter::new();
    for rule in rules.as_array().unwrap() {
        let rule = strings(rule);
        filter.set_rule(
            rule[0].clone(),
            if rule[1] == "black" {
                FilterRule::Blacklist
            } else {
                FilterRule::Whitelist
            },
        );
    }
    filter
}

fn rules(filter: &TagFilter) -> Value {
    let mut rules: Vec<Value> = filter
        .rules()
        .map(|(s, r)| {
            json!([
                s,
                if r == FilterRule::Blacklist {
                    "black"
                } else {
                    "white"
                }
            ])
        })
        .collect();
    rules.sort_by_key(ToString::to_string);
    Value::Array(rules)
}

/// What a select dialog shows, as the recording has it: the reference
/// gives the choices unsorted to a dialog that sorts them.
fn said_select<T>(choices: &Choices<T>) -> Value {
    if choices.choices.len() == 1 {
        return json!({ "auto": choices.title, "choices": [choices.choices[0].0] });
    }
    json!({
        "select": choices.title,
        "choices": choices.choices.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
        "sorted": true,
        "preselected": choices.preselected,
    })
}

/// The recording's said, its select choices sorted as its dialog shows them.
fn sorted_said(said: &Value) -> Vec<Value> {
    said.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let mut s = s.clone();
            if s.get("select").is_some()
                && let Some(choices) = s.get_mut("choices")
            {
                let mut sorted = strings(choices);
                sorted.sort();
                *choices = json!(sorted);
            }
            s
        })
        .collect()
}

/// Answer a select by label: `None` cancels.
fn pick<T: Clone>(choices: &Choices<T>, answer: &Value) -> Option<T> {
    if let Some(only) = choices.only() {
        return Some(only);
    }
    let answer = answer.as_str()?;
    choices
        .choices
        .iter()
        .find(|c| c.0 == answer)
        .map(|c| c.1.clone())
}

fn choice_json(choice: &Choice) -> Value {
    json!({
        "options": choice.options,
        "value": choice.options[choice.value],
        "enabled": choice.enabled,
    })
}

fn value_json(editor: &MergeOptionsEditor, name: &dyn Fn(&ServiceKey) -> String) -> Value {
    let v = editor.value();
    let mut tags: Vec<Value> = v
        .tags
        .iter()
        .map(|t| {
            json!([
                name(&t.service),
                editor::action_label(Some(t.action)),
                rules(&t.filter)
            ])
        })
        .collect();
    tags.sort_by_key(ToString::to_string);
    let mut ratings: Vec<Value> = v
        .ratings
        .iter()
        .map(|r| json!([name(&r.service), editor::action_label(Some(r.action))]))
        .collect();
    ratings.sort_by_key(ToString::to_string);
    let sync = |a: Option<hydrus_store::duplicates::merge::SyncAction>| {
        editor::action_label(a.map(|a| match a {
            hydrus_store::duplicates::merge::SyncAction::Copy => {
                hydrus_store::duplicates::merge::MergeAction::Copy
            }
            hydrus_store::duplicates::merge::SyncAction::TwoWay => {
                hydrus_store::duplicates::merge::MergeAction::TwoWay
            }
        }))
    };
    let note = v.note_merge.unwrap();
    json!({
        "tags": tags,
        "ratings": ratings,
        "archive": editor.archive().value,
        "urls": sync(v.urls),
        "file_modified": sync(v.file_modified),
        "notes": editor::action_label(v.notes),
        "note_extend": note.extend_existing,
        "note_conflict": note.conflict as i64,
    })
}

fn sorted_value(value: &Value) -> Value {
    let mut value = value.clone();
    for list in ["tags", "ratings"] {
        let mut items = value[list].as_array().unwrap().clone();
        items.sort_by_key(ToString::to_string);
        value[list] = Value::Array(items);
    }
    value
}

#[test]
fn the_editor_edits_merge_options_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("merge_options_editor.json");
    let (_dir, store) = store();
    let snapshot = store.snapshot();
    let services: Vec<Service> = snapshot
        .services
        .all()
        .filter(|s| s.service_type().is_real_tag_service() || s.service_type().is_rating_service())
        .map(|s| Service {
            key: s.key.clone(),
            name: s.name.clone(),
            kind: s.service_type(),
        })
        .collect();
    let key = |name: &str| {
        services
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .key
            .clone()
    };
    let name = |key: &ServiceKey| {
        services
            .iter()
            .find(|s| &s.key == key)
            .unwrap()
            .name
            .clone()
    };
    let client: DuplicateMergeSettings = store.read(hydrus_store::settings::get).unwrap();
    for case in recorded["cases"].as_array().unwrap() {
        let relationship = match case["decision"].as_str().unwrap() {
            "better" => PairRelationship::Better,
            "same quality" => PairRelationship::SameQuality,
            "alternates" => PairRelationship::Alternate,
            _ => PairRelationship::FalsePositive,
        };
        let start = if case["start"] == "client" {
            client
                .for_relationship(relationship)
                .cloned()
                .unwrap_or_default()
        } else {
            MergeOptions::default()
        };
        let mut editor = MergeOptionsEditor::new(
            relationship,
            &start,
            case["custom"].as_bool().unwrap(),
            services.clone(),
        );
        for state in case["states"].as_array().unwrap() {
            let step = &state["step"];
            let at = format!("{} {}: {step}", case["decision"], case["custom"]);
            let mut said: Vec<Value> = Vec::new();
            if let Some(step) = step.as_array() {
                match step[0].as_str().unwrap() {
                    kind @ ("add_tag" | "add_rating") => {
                        let tags = kind == "add_tag";
                        let services = if tags {
                            editor.add_tag_services()
                        } else {
                            editor.add_rating_services()
                        };
                        match services {
                            Err(warning) => said.push(json!({ "warning": warning })),
                            Ok(services) => 'asked: {
                                said.push(said_select(&services));
                                let Some(service) = pick(&services, &step[1]) else {
                                    break 'asked;
                                };
                                let actions = if tags {
                                    editor.tag_actions(&service, None)
                                } else {
                                    editor.rating_actions(&service, None)
                                };
                                let action = match &actions {
                                    Some(actions) => {
                                        said.push(said_select(actions));
                                        pick(actions, &step[2]).map(Some)
                                    }
                                    None => Some(None),
                                };
                                if let Some(action) = action {
                                    if tags {
                                        said.push(json!({ "dialog": editor::FILTER_TITLE }));
                                        said.push(json!({ "filter given": [] }));
                                        editor.add_tag(service, action, filter(&step[3]));
                                    } else {
                                        editor.add_rating(service, action);
                                    }
                                }
                            }
                        }
                    }
                    "edit_tag_action" => {
                        let service = key(step[1].as_str().unwrap());
                        let current = editor.tag_action(&service);
                        let actions = editor.tag_actions(&service, current).unwrap();
                        said.push(said_select(&actions));
                        let action = pick(&actions, &step[2]).unwrap();
                        editor.set_tag_action(&service, action);
                    }
                    "edit_tag_filter" => {
                        let service = key(step[1].as_str().unwrap());
                        said.push(json!({ "dialog": editor::FILTER_TITLE }));
                        said.push(
                            json!({ "filter given": rules(&editor.tag_filter(&service).unwrap()) }),
                        );
                        editor.set_tag_filter(&service, filter(&step[2]));
                    }
                    "edit_rating" => {
                        let service = key(step[1].as_str().unwrap());
                        let current = editor.rating_action(&service);
                        let actions = editor.rating_actions(&service, current).unwrap();
                        said.push(said_select(&actions));
                        let action = pick(&actions, &step[2]).unwrap();
                        editor.set_rating_action(&service, action);
                    }
                    kind @ ("delete_tags" | "delete_ratings") => {
                        said.push(json!({ "asked": editor::REMOVE_SELECTED }));
                        let keys: Vec<ServiceKey> =
                            strings(&step[1]).iter().map(|n| key(n)).collect();
                        if kind == "delete_tags" {
                            editor.delete_tags(&keys);
                        } else {
                            editor.delete_ratings(&keys);
                        }
                    }
                    "set" => {
                        let label = step[2].as_str().unwrap();
                        let (choice, set): (Choice, fn(&mut MergeOptionsEditor, usize)) =
                            match step[1].as_str().unwrap() {
                                "archive" => (editor.archive(), MergeOptionsEditor::set_archive),
                                "urls" => (editor.urls(), MergeOptionsEditor::set_urls),
                                "file_modified" => (
                                    editor.file_modified(),
                                    MergeOptionsEditor::set_file_modified,
                                ),
                                _ => (editor.notes(), MergeOptionsEditor::set_notes),
                            };
                        let index = choice.options.iter().position(|o| *o == label).unwrap();
                        set(&mut editor, index);
                    }
                    "note_settings" => {
                        let merge = editor.note_merge();
                        said.push(json!({ "dialog": editor::NOTE_SETTINGS_TITLE }));
                        said.push(json!({
                            "notes given": [merge.extend_existing, merge.conflict as i64],
                            "simple": true,
                        }));
                        let conflict = match step[2].as_str().unwrap() {
                            "replace" => NoteConflict::Replace,
                            "ignore" => NoteConflict::Ignore,
                            "append" => NoteConflict::Append,
                            _ => NoteConflict::Rename,
                        };
                        editor.set_note_merge(hydrus_core::notes::NoteMerge {
                            extend_existing: step[1].as_bool().unwrap(),
                            conflict,
                        });
                    }
                    other => panic!("{other}"),
                }
                // (our selects sorted, as the reference's dialog shows them)
                let said: Vec<Value> = said
                    .into_iter()
                    .map(|s| {
                        let mut s = s;
                        if s.get("select").is_some() {
                            let mut sorted = strings(&s["choices"]);
                            sorted.sort();
                            s["choices"] = json!(sorted);
                        }
                        s
                    })
                    .collect();
                assert_eq!(said, sorted_said(&state["said"]), "{at}");
            }
            let theirs = &state["state"];
            assert_eq!(editor.note(), theirs["note"].as_str().unwrap(), "{at}");
            let tag_rows: Vec<Vec<String>> = editor
                .tag_rows()
                .into_iter()
                .map(|(r, _)| r.to_vec())
                .collect();
            let their_tag_rows: Vec<Vec<String>> = theirs["tag_rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(strings)
                .collect();
            assert_eq!(tag_rows, their_tag_rows, "{at}");
            let rating_rows: Vec<Vec<String>> = editor
                .rating_rows()
                .into_iter()
                .map(|(r, _)| r.to_vec())
                .collect();
            let their_rating_rows: Vec<Vec<String>> = theirs["rating_rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(strings)
                .collect();
            assert_eq!(rating_rows, their_rating_rows, "{at}");
            assert_eq!(
                editor.edit_action_shown(),
                theirs["tag_edit_action_shown"].as_bool().unwrap(),
                "{at}"
            );
            assert_eq!(
                editor.edit_action_shown(),
                theirs["rating_edit_action_shown"].as_bool().unwrap(),
                "{at}"
            );
            assert_eq!(choice_json(&editor.archive()), theirs["archive"], "{at}");
            assert_eq!(
                choice_json(&editor.file_modified()),
                theirs["file_modified"],
                "{at}"
            );
            assert_eq!(choice_json(&editor.urls()), theirs["urls"], "{at}");
            assert_eq!(choice_json(&editor.notes()), theirs["notes"], "{at}");
            assert_eq!(
                editor.note_settings_enabled(),
                theirs["note_settings_enabled"].as_bool().unwrap(),
                "{at}"
            );
            assert_eq!(
                sorted_value(&value_json(&editor, &name)),
                sorted_value(&theirs["value"]),
                "{at}"
            );
        }
    }
}
