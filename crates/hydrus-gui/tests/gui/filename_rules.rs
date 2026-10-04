//! Real advanced filename-rule controls, staged folder ownership and persisted consumers.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::import_folders;
use slint::{ComponentHandle as _, Model as _};

fn rows(rows: slint::ModelRc<hydrus_gui::TableRow>) -> Vec<Vec<String>> {
    (0..rows.row_count())
        .map(|i| {
            let row = rows.row_data(i).unwrap();
            (0..row.cells.row_count())
                .map(|j| row.cells.row_data(j).unwrap().to_string())
                .collect()
        })
        .collect()
}

fn selected(rows: slint::ModelRc<hydrus_gui::TableRow>) -> Vec<usize> {
    (0..rows.row_count())
        .filter(|&i| rows.row_data(i).unwrap().selected)
        .collect()
}

#[test]
fn real_lists_replay_reference_and_save_folder_consumers_without_stale_callbacks() {
    let windows = headless::init();
    let (_dirs, store) = crate::subscriptions::store();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    let watched = tempfile::tempdir().unwrap();
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_add();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.set_path(watched.path().to_string_lossy().into_owned().into());
    let mine = (0..folder.get_tagging_choices().row_count())
        .find(|&i| folder.get_tagging_choices().row_data(i).unwrap() == "my tags")
        .unwrap();
    folder.set_tagging_choice(i32::try_from(mine).unwrap());
    folder.invoke_tagging_add();
    let dialog = bound
        .folders
        .filename_tagging
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    dialog.set_advanced(true);
    let fixture = hydrus_testkit::fixture_json("filename_rules.json");
    for step in fixture["steps"].as_array().unwrap() {
        let action = step["action"].as_str().unwrap();
        if let Some(indices) = step["selected"].as_array() {
            for (index, value) in indices.iter().enumerate() {
                dialog.invoke_rule_clicked(
                    action.starts_with("quick"),
                    i32::try_from(value.as_u64().unwrap()).unwrap(),
                    index != 0,
                    false,
                );
            }
        }
        if action == "regex_add" {
            dialog.set_regex_input(step["text"].as_str().unwrap().into());
        }
        if action == "quick_sort_regex" {
            dialog.invoke_rule_sort(1, false);
        } else if action == "quick_sort_namespace" {
            dialog.invoke_rule_sort(0, true);
        } else {
            dialog.invoke_rule_action(action.into());
        }
        if action == "quick_add" || action == "quick_edit" {
            assert!(dialog.get_rule_child());
            assert_eq!(
                dialog.get_rule_namespace().as_str(),
                step["calls"][0]["initial"][0].as_str().unwrap()
            );
            assert_eq!(
                dialog.get_rule_regex().as_str(),
                step["calls"][0]["initial"][1].as_str().unwrap()
            );
            // The child freezes service, selection and parent Apply callbacks.
            let before = rows(dialog.get_quick_rows());
            dialog.invoke_service_chosen(99);
            dialog.invoke_rule_clicked(true, 99, false, false);
            dialog.invoke_apply();
            assert!(bound.folders.filename_tagging.borrow().is_some());
            assert_eq!(rows(dialog.get_quick_rows()), before);
            if let Some(attempts) = step["attempts"].as_array() {
                for attempt in attempts {
                    let before = rows(dialog.get_quick_rows());
                    dialog.set_rule_namespace(attempt[0].as_str().unwrap().into());
                    dialog.set_rule_regex(attempt[1].as_str().unwrap().into());
                    dialog.invoke_rule_entered();
                    if dialog.get_rule_child() {
                        assert!(!dialog.get_errors().is_empty());
                        assert_eq!(rows(dialog.get_quick_rows()), before);
                    }
                }
            } else {
                dialog.invoke_rule_cancel();
            }
            assert!(!dialog.get_rule_child());
        } else if action == "quick_delete" {
            assert!(dialog.get_rule_delete());
            assert_eq!(
                dialog.get_rule_message().as_str(),
                step["calls"][0]["text"].as_str().unwrap()
            );
            dialog.invoke_rule_answer(step["answer"].as_bool().unwrap());
        }
        if action == "regex_add" && !step["calls"].as_array().unwrap().is_empty() {
            assert!(
                dialog
                    .get_errors()
                    .starts_with("That regex would not compile!\n\n")
            );
        }
        let expected = &step["state"];
        assert_eq!(
            serde_json::json!(rows(dialog.get_quick_rows())),
            expected["quick"],
            "{step}"
        );
        assert_eq!(
            serde_json::json!(
                rows(dialog.get_regex_rows())
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
            ),
            expected["regexes"],
            "{step}"
        );
        let quick = rows(dialog.get_quick_rows());
        let chosen: Vec<_> = selected(dialog.get_quick_rows())
            .into_iter()
            .map(|i| quick[i].clone())
            .collect();
        assert_eq!(
            serde_json::json!(chosen),
            expected["quick_selected"],
            "{step}"
        );
        assert_eq!(
            serde_json::json!(selected(dialog.get_regex_rows())),
            expected["regex_selected"],
            "{step}"
        );
        assert_eq!(
            dialog.get_regex_input().as_str(),
            expected["input"].as_str().unwrap()
        );
        for (path, tags) in fixture["paths"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected["tags"].as_array().unwrap())
        {
            dialog.set_example(path.as_str().unwrap().into());
            dialog.invoke_changed();
            let expected = tags
                .as_array()
                .unwrap()
                .iter()
                .map(|tag| tag.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ");
            assert_eq!(dialog.get_example_tags().as_str(), expected, "{step}");
        }
    }
    let pixels = headless::render(&windows.get(3).unwrap(), 1000, 760);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("filename_rules.png"),
        &pixels,
        1000,
        760,
    )
    .unwrap();
    assert!(
        store
            .read(import_folders::import_folders)
            .unwrap()
            .is_empty()
    );
    dialog.invoke_apply();
    assert!(bound.folders.filename_tagging.borrow().is_none());
    dialog.invoke_rule_action("quick_add".into());
    dialog.invoke_rule_entered();
    dialog.invoke_apply();
    assert!(!dialog.get_rule_child());
    folder.invoke_apply();
    list.invoke_apply();
    let saved = store
        .read(import_folders::import_folders)
        .unwrap()
        .remove(0);
    let (service, options) = &saved.settings.filename_tagging[0];
    assert_eq!(
        service,
        &hex::encode(hydrus_core::service::builtin_keys::MY_TAGS)
    );
    assert_eq!(
        serde_json::json!(options.quick_namespaces),
        fixture["reopened"]["quick"]
    );
    assert_eq!(
        serde_json::json!(options.regexes),
        fixture["reopened"]["regexes"]
    );
    for (path, tags) in fixture["paths"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fixture["applied"]["tags"].as_array().unwrap())
    {
        assert_eq!(
            serde_json::json!(
                options
                    .tags(path.as_str().unwrap())
                    .into_iter()
                    .collect::<Vec<_>>()
            ),
            *tags
        );
    }
    crate::folders::open(&ui, "manage import folders\u{2026}");
    let list = bound
        .folders
        .import_list
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    list.invoke_row_clicked(0, false, false);
    list.invoke_edit();
    let folder = bound
        .folders
        .import_edit
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    folder.invoke_tagging_edit(0);
    let canceled = bound
        .folders
        .filename_tagging
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        serde_json::json!(rows(canceled.get_quick_rows())),
        fixture["reopened"]["quick"]
    );
    canceled.invoke_rule_action("quick_add".into());
    canceled.set_rule_namespace("discarded".into());
    canceled.set_rule_regex(".*".into());
    canceled.invoke_rule_entered();
    canceled.invoke_cancel();
    canceled.invoke_apply();
    folder.invoke_apply();
    list.invoke_apply();
    assert_eq!(
        store.read(import_folders::import_folders).unwrap()[0].settings,
        saved.settings
    );
}
