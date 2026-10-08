//! Raw Qt namespace queue values, tag grouping consumers and scoped saves.
use hydrus_core::tag_sort::{self, TagGroupBy, TagSort, TagSortType};
use hydrus_gui_model::{
    options::{Editor as Options, Settings},
    tag_namespace_order::{self, Editor},
};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
// leaf: audit-options-tag-sort-tag-sort-namespace-grouping-sort-add
// leaf: audit-options-tag-sort-tag-sort-namespace-grouping-sort-edit
#[test]
fn raw_allow_blank_queue_actions_and_all_tag_sorts_match_qt() {
    let fixture = hydrus_testkit::fixture_json("tag_namespace_order.json");
    let mut editor = Editor::new(&strings(&fixture["initial"]["data"]));
    for step in fixture["steps"].as_array().unwrap() {
        if let Some(indices) = step["selection"].as_array() {
            for (n, index) in indices.iter().enumerate() {
                editor.0.click(
                    usize::try_from(index.as_u64().unwrap()).unwrap(),
                    n != 0,
                    false,
                );
            }
        }
        match step["action"].as_str().unwrap() {
            "_Add" | "_Edit" => {
                let editing = if step["action"] == "_Edit" {
                    editor
                        .0
                        .editing()
                        .map(|(id, text)| (Some(id), text.to_owned()))
                } else {
                    Some((None, "namespace".into()))
                };
                let (key, default) = editing.unwrap();
                assert_eq!(default, step["calls"][0]["default"].as_str().unwrap());
                assert_eq!(
                    tag_namespace_order::MESSAGE,
                    step["calls"][0]["message"].as_str().unwrap()
                );
                assert_eq!(step["calls"][0]["allow_blank"], true);
                if let Some(raw) = step["answer"].as_str() {
                    if let Some(key) = key {
                        editor.0.replace(key, raw.into());
                    } else {
                        editor.0.add(raw.into());
                    }
                }
            }
            "_Up" => editor.0.move_selected(false),
            "_Down" => editor.0.move_selected(true),
            "_Delete" => {
                assert_eq!(
                    editor.0.removal_question().unwrap(),
                    step["calls"][0]["message"].as_str().unwrap()
                );
                if step["answer"] == true {
                    editor.0.remove_selected();
                }
            }
            _ => panic!("action"),
        }
        assert_eq!(json!(editor.0.values()), step["state"]["data"]);
        assert_eq!(
            json!(
                editor
                    .0
                    .rows()
                    .iter()
                    .map(|(_, text)| Editor::label(text))
                    .collect::<Vec<_>>()
            ),
            step["state"]["labels"]
        );
        assert_eq!(
            json!(
                editor
                    .0
                    .rows()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (key, _))| editor.0.selection.is_selected(*key).then_some(i))
                    .collect::<Vec<_>>()
            ),
            step["state"]["selected"]
        );
        assert_eq!(
            step["saved"], fixture["initial"]["data"],
            "the reference queue is staged"
        );
    }
    assert_eq!(json!(editor.0.values()), fixture["applied"]);
    for case in fixture["sorts"].as_array().unwrap() {
        let sort = TagSort {
            sort_type: match case["type"].as_u64().unwrap() {
                0 => TagSortType::Tag,
                1 => TagSortType::Subtag,
                2 => TagSortType::Count,
                _ => panic!("type"),
            },
            ascending: case["ascending"].as_bool().unwrap(),
            group_by: TagGroupBy::NamespaceUser,
        };
        let mut tags = strings(&fixture["tags"]);
        tag_sort::sort_tags(
            &sort,
            &mut tags,
            String::as_str,
            |tag| fixture["counts"][tag].as_u64().unwrap(),
            &editor.0.values(),
        );
        assert_eq!(json!(tags), case["tags"]);
    }
}
#[test]
fn namespace_changes_merge_with_concurrent_presentation_edits_and_unchanged_list() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    let mut editor = Options::new(before.clone());
    editor.set_tag_namespace_order(vec![
        String::new(),
        ":".into(),
        " CREATOR ".into(),
        "creator".into(),
        "creator".into(),
    ]);
    let (after, original, problems) = editor.applied();
    assert!(problems.is_empty());
    let original = original.clone();
    store
        .write(|writer| {
            let mut live: hydrus_core::tag_presentation::TagPresentation =
                settings::get(writer.conn())?;
            live.namespace_connector = " :: ".into();
            live.replace_underscores = true;
            live.search_page_sort = TagSort {
                sort_type: TagSortType::Count,
                ascending: false,
                group_by: TagGroupBy::NamespaceUser,
            };
            live.sidebar_display_type = hydrus_core::tag_presentation::TagDisplayType::Storage;
            live.viewer_display_type = hydrus_core::tag_presentation::TagDisplayType::Display;
            settings::set(writer.conn(), &live)
        })
        .unwrap();
    store
        .write(move |writer| after.save(writer.conn(), &original))
        .unwrap();
    let saved = store.read(Settings::load).unwrap();
    assert_eq!(saved.tag_presentation.namespace_connector, " :: ");
    assert!(saved.tag_presentation.replace_underscores);
    assert_eq!(
        saved.tag_presentation.search_page_sort,
        TagSort {
            sort_type: TagSortType::Count,
            ascending: false,
            group_by: TagGroupBy::NamespaceUser
        }
    );
    assert_eq!(
        saved.tag_presentation.sidebar_display_type,
        hydrus_core::tag_presentation::TagDisplayType::Storage
    );
    assert_eq!(
        saved.tag_presentation.viewer_display_type,
        hydrus_core::tag_presentation::TagDisplayType::Display
    );
    assert_eq!(
        saved.tag_presentation.user_namespaces,
        editor.edited_tag_namespace_order()
    );
    assert_eq!(
        Store::open(directory.path())
            .unwrap()
            .read(Settings::load)
            .unwrap(),
        saved
    );
    let mut untouched = before.clone();
    untouched.tag_presentation.show_namespaces = false;
    store
        .write(move |writer| untouched.save(writer.conn(), &before))
        .unwrap();
    let saved = store.read(Settings::load).unwrap();
    assert!(!saved.tag_presentation.show_namespaces);
    assert_eq!(
        saved.tag_presentation.user_namespaces,
        editor.edited_tag_namespace_order()
    );
    assert_eq!(saved.tag_presentation.namespace_connector, " :: ");
}
