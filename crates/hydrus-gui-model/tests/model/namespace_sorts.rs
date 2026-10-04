//! Real namespace prompt cleaning, staged queue order and advanced choices.
use hydrus_core::pages::{PageSort, PageSortBy};
use hydrus_gui_model::namespace_sorts::{self, Editor, Prompt};
fn data(value: &serde_json::Value) -> Vec<PageSort> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| PageSort {
            ascending: true,
            by: PageSortBy::Namespaces {
                namespaces: row[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect(),
                tag_display_type: row[1].as_i64().unwrap(),
            },
        })
        .collect()
}
#[test]
fn replay_actual_queue_add_edit_escaped_names_views_moves_and_delete_answers() {
    let fixture = hydrus_testkit::fixture_json("namespace_sorts.json");
    let original = data(&fixture["initial"]["data"]);
    let mut editor = Editor::new(&original);
    for step in fixture["steps"].as_array().unwrap() {
        if let Some(indices) = step["selection"].as_array() {
            editor.select(
                &indices
                    .iter()
                    .map(|i| usize::try_from(i.as_u64().unwrap()).unwrap())
                    .collect::<Vec<_>>(),
            );
        }
        match step["action"].as_str().unwrap() {
            "_Add" | "_Edit" => {
                let prompt = editor.begin(
                    step["action"] == "_Edit",
                    step["advanced"].as_bool().unwrap(),
                );
                let first = &step["calls"][0];
                assert_eq!(
                    prompt,
                    Prompt::Text(first["default"].as_str().unwrap().into())
                );
                assert_eq!(
                    namespace_sorts::TEXT_MESSAGE,
                    first["message"].as_str().unwrap()
                );
                let next = editor.text(step["text"].as_str());
                if next == Prompt::View {
                    let question = &step["calls"][1];
                    assert_eq!(
                        namespace_sorts::VIEW_TITLE,
                        question["title"].as_str().unwrap()
                    );
                    assert_eq!(
                        namespace_sorts::VIEW_MESSAGE,
                        question["message"].as_str().unwrap()
                    );
                    assert_eq!(
                        namespace_sorts::VIEWS
                            .iter()
                            .map(|(label, code)| serde_json::json!([label, code, label]))
                            .collect::<Vec<_>>(),
                        question["choices"].as_array().unwrap().clone()
                    );
                    editor.view(step["view"].as_u64().map(|v| usize::try_from(v).unwrap()));
                }
            }
            "_Up" => editor.move_selected(false),
            "_Down" => editor.move_selected(true),
            "_Delete" => {
                let (ids, message) = editor.delete_request().unwrap();
                assert_eq!(message, step["calls"][0]["message"].as_str().unwrap());
                if step["accepted"] == true {
                    editor.delete(&ids);
                }
            }
            _ => unreachable!(),
        }
        assert_eq!(editor.value(), data(&step["state"]["data"]), "{step}");
        let rows = editor.rows();
        assert_eq!(
            rows.iter()
                .map(namespace_sorts::Row::label)
                .collect::<Vec<_>>(),
            step["state"]["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            rows.iter()
                .enumerate()
                .filter_map(|(i, row)| row.selected.then_some(i))
                .collect::<Vec<_>>(),
            step["state"]["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(editor.value(), data(&fixture["applied"]));
    assert_eq!(editor.value(), data(&fixture["reopened"]));
    assert_ne!(original, editor.value());
}
#[test]
fn duplicate_ids_and_frozen_edits_survive_selection_changes_cancel_and_removal() {
    let sort = PageSort {
        ascending: true,
        by: PageSortBy::Namespaces {
            namespaces: vec!["creator-id".into()],
            tag_display_type: 3,
        },
    };
    let mut editor = Editor::new(&[sort.clone(), sort.clone()]);
    editor.select(&[1]);
    assert_eq!(
        editor.begin(true, true),
        Prompt::Text("creator\\-id".into())
    );
    editor.select(&[0]);
    assert_eq!(editor.text(Some("TITLE-PAGE")), Prompt::View);
    editor.view(Some(2));
    assert_eq!(editor.value()[0], sort);
    assert_eq!(
        editor.value()[1].by,
        PageSortBy::Namespaces {
            namespaces: vec!["title".into(), "page".into()],
            tag_display_type: 2
        }
    );
    let before = editor.value();
    editor.begin(true, true);
    editor.text(Some("new"));
    editor.view(None);
    assert_eq!(editor.value(), before);
    editor.begin(false, false);
    editor.text(Some("---"));
    assert_eq!(editor.value(), before);
    let (ids, _) = editor.delete_request().unwrap();
    editor.select(&[1]);
    editor.delete(&ids);
    assert_eq!(editor.value(), vec![before[1].clone()]);
    editor.click(99, false, false);
    assert_eq!(editor.rows().iter().filter(|row| row.selected).count(), 1);
}
