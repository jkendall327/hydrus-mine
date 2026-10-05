//! Advanced extraction rules replayed from actual Qt handlers and child answers.
use hydrus_gui_model::filename_rules::{DELETE_QUESTION, Editor};
use hydrus_parse::folders::FilenameTagging;

#[test]
fn advanced_lists_children_and_filename_consumers_match_real_reference() {
    let fixture = hydrus_testkit::fixture_json("filename_rules.json");
    let original = FilenameTagging::default();
    let mut options = original.clone();
    let mut editor = Editor::new(&options);
    for step in fixture["steps"].as_array().unwrap() {
        let action = step["action"].as_str().unwrap();
        if let Some(selected) = step["selected"].as_array() {
            // Click a selected row again without Ctrl first to clear old selection.
            if let Some(first) = selected.first() {
                editor.click(
                    action.starts_with("quick"),
                    usize::try_from(first.as_u64().unwrap()).unwrap(),
                    false,
                    false,
                );
                for index in selected.iter().skip(1) {
                    editor.click(
                        action.starts_with("quick"),
                        usize::try_from(index.as_u64().unwrap()).unwrap(),
                        true,
                        false,
                    );
                }
            }
        }
        match action {
            "quick_add" | "quick_edit" => {
                let initial = editor.begin(action == "quick_edit").unwrap();
                assert_eq!(serde_json::json!(initial), step["calls"][0]["initial"]);
                if let Some(attempts) = step["attempts"].as_array() {
                    for attempt in attempts {
                        let before = editor.quick_rows();
                        let result = editor
                            .accept(attempt[0].as_str().unwrap(), attempt[1].as_str().unwrap());
                        if let Err(error) = result {
                            assert!(editor.asking());
                            assert_eq!(editor.quick_rows(), before);
                            assert!(
                                error == "Please enter something for the namespace."
                                    || error.starts_with("That regex would not compile!\n\n")
                            );
                        }
                    }
                } else {
                    editor.cancel();
                }
            }
            "quick_delete" => {
                assert!(editor.request_delete());
                assert_eq!(DELETE_QUESTION, step["calls"][0]["text"]);
                editor.answer(step["answer"].as_bool().unwrap());
            }
            "regex_add" => {
                editor.input = step["text"].as_str().unwrap().into();
                let before = editor.regex_rows();
                if let Err(error) = editor.add_regex() {
                    assert!(error.starts_with("That regex would not compile!\n\n"));
                    assert_eq!(editor.regex_rows(), before);
                }
            }
            "regex_remove" => editor.remove_regexes(),
            "quick_sort_regex" => editor.sort(1, false),
            "quick_sort_namespace" => editor.sort(0, true),
            _ => unreachable!(),
        }
        editor.update(&mut options);
        let expected = &step["state"];
        assert_eq!(
            serde_json::json!(options.quick_namespaces),
            expected["quick"],
            "{step}"
        );
        assert_eq!(
            serde_json::json!(options.regexes),
            expected["regexes"],
            "{step}"
        );
        assert_eq!(
            serde_json::json!(
                editor
                    .quick_rows()
                    .into_iter()
                    .filter(|row| row.selected)
                    .map(|row| row.value)
                    .collect::<Vec<_>>()
            ),
            expected["quick_selected"],
            "{step}"
        );
        assert_eq!(
            serde_json::json!(
                editor
                    .regex_rows()
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| row.selected.then_some(i))
                    .collect::<Vec<_>>()
            ),
            expected["regex_selected"],
            "{step}"
        );
        assert_eq!(editor.input, expected["input"].as_str().unwrap());
        let tags: Vec<Vec<_>> = fixture["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| options.tags(path.as_str().unwrap()).into_iter().collect())
            .collect();
        assert_eq!(serde_json::json!(tags), expected["tags"], "{step}");
        assert_eq!(original, FilenameTagging::default());
    }
    let mut reopened = FilenameTagging::default();
    Editor::new(&options).update(&mut reopened);
    assert_eq!(
        serde_json::json!(reopened.quick_namespaces),
        fixture["reopened"]["quick"]
    );
    assert_eq!(
        serde_json::json!(reopened.regexes),
        fixture["reopened"]["regexes"]
    );
    assert_eq!(fixture["draft_isolated"], true);
}

#[test]
fn child_identity_is_frozen_and_unrelated_callbacks_preserve_it() {
    let options = FilenameTagging {
        quick_namespaces: vec![
            ("first".into(), "one".into()),
            ("second".into(), "two".into()),
        ],
        ..FilenameTagging::default()
    };
    let mut editor = Editor::new(&options);
    editor.click(true, 0, false, false);
    editor.begin(true).unwrap();
    editor.click(true, 1, false, false);
    assert!(!editor.request_delete());
    editor.answer(true);
    assert!(editor.asking());
    editor.accept("renamed", "one").unwrap();
    assert_eq!(
        editor
            .quick_rows()
            .iter()
            .find(|row| row.selected)
            .unwrap()
            .value
            .0,
        "renamed"
    );
    assert!(
        editor
            .quick_rows()
            .iter()
            .any(|row| row.value.0 == "second")
    );
    editor.accept("stale", "three").unwrap();
    assert!(!editor.quick_rows().iter().any(|row| row.value.0 == "stale"));
}

#[test]
fn both_original_regex_buttons_offer_the_recorded_help_components_and_favourites() {
    use hydrus_gui_model::filename_rules::{RegexAction, regex_menu};
    use hydrus_gui_model::main_menu::{Command, Entry};
    fn describe(entries: &[Entry], actions: &[RegexAction]) -> serde_json::Value {
        serde_json::json!(entries.iter().map(|entry| match entry {
            Entry::Separator => serde_json::json!({"kind":"separator"}),
            Entry::Menu { label, entries, .. } => serde_json::json!({"kind":"menu", "label":label, "rows":describe(entries, actions)}),
            Entry::Item { label, command: Some(Command::Popup(index)), enabled } => {
                let outputs = match &actions[*index] {
                    RegexAction::Copy(value) => vec![serde_json::json!({"kind":"copy", "value":value})],
                    RegexAction::Help(value) => vec![serde_json::json!({"kind":"url", "value":value})],
                    _ => vec![],
                };
                serde_json::json!({"kind":"item", "label":label, "enabled":enabled, "outputs":outputs})
            }
            _ => unreachable!(),
        }).collect::<Vec<_>>())
    }
    let fixture = hydrus_testkit::fixture_json("filename_rules.json");
    let value = hydrus_gui_model::regex_favourites::RegexFavourites(
        serde_json::from_value(fixture["menu_favourites"].clone()).unwrap(),
    );
    let (entries, actions) = regex_menu(&value);
    let actual = describe(&entries, &actions);
    let mut expected = fixture["regex_menus"][0].clone();
    let filename = hydrus_gui_model::regex_favourites::regex_tools(1)
        .pop()
        .unwrap();
    let recorded_filename = expected[2]["rows"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    recorded_filename["label"] = serde_json::json!(filename.0);
    recorded_filename["outputs"][0]["value"] = serde_json::json!(filename.1);
    assert_eq!(actual, expected);
    assert_eq!(fixture["regex_menus"][0], fixture["regex_menus"][1]);
}
