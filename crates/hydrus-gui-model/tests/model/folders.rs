//! The manage import folders and export folders dialogs against the
//! reference's: the lists' rows (`oracle/record_folders_lists.py`), and the
//! edit dialogs' fields as they open and what "apply" refuses, warns of,
//! asks and gives back (`oracle/record_folders_dialogs.py`).

use serde_json::{Value as Json, json};

use hydrus_core::search::context::FileSearchContext;
use hydrus_gui_model::folders::{
    ACTION_CHOICES, DELETE_QUESTION, DELETE_WARNING, ImportFolderEdit, Named, Sensitive,
    action_index, check_export_folder, check_import_folder, export_folder_row, import_folder_row,
    new_export_folder,
};
use hydrus_parse::folders::{ExportFolder, ExportType, FolderAction, ImportFolderSettings};
use hydrus_search::{TextContext, parse_api_search, predicate_text};

fn import_folder(case: &Json) -> ImportFolderEdit {
    ImportFolderEdit {
        id: None,
        name: case["name"].as_str().unwrap().into(),
        paused: case["paused"].as_bool().unwrap(),
        settings: ImportFolderSettings {
            path: case["path"].as_str().unwrap().into(),
            check_regularly: case["check_regularly"].as_bool().unwrap(),
            period: case["period"].as_i64().unwrap(),
            ..ImportFolderSettings::default()
        },
    }
}

fn export_folder(case: &Json) -> ExportFolder {
    let tags = &case["tags"];
    let predicates = if tags.as_array().unwrap().is_empty() {
        Vec::new()
    } else {
        parse_api_search(tags).unwrap()
    };
    ExportFolder {
        name: case["name"].as_str().unwrap().into(),
        path: case["path"].as_str().unwrap().into(),
        export_type: if case["type"] == "synchronise" {
            ExportType::Synchronise
        } else {
            ExportType::Regular
        },
        delete_from_client_after_export: case["delete"].as_bool().unwrap(),
        run_regularly: case["run_regularly"].as_bool().unwrap(),
        period: case["period"].as_i64().unwrap(),
        phrase: case["phrase"].as_str().unwrap().into(),
        run_now: case["run_now"].as_bool().unwrap(),
        last_error: case["last_error"].as_str().unwrap().into(),
        ..new_export_folder(
            String::new(),
            FileSearchContext {
                predicates,
                ..FileSearchContext::default()
            },
        )
    }
}

fn predicates(folder: &ExportFolder) -> Vec<String> {
    let text = TextContext::default();
    folder
        .search
        .predicates
        .iter()
        .map(|p| predicate_text(p, &text))
        .collect()
}

#[test]
fn the_lists_rows_are_the_references() {
    let recorded = hydrus_testkit::fixture_json("folders_lists.json");
    assert_eq!(
        recorded["import_columns"],
        json!(hydrus_gui_model::folders::IMPORT_COLUMNS)
    );
    assert_eq!(
        recorded["export_columns"],
        json!(hydrus_gui_model::folders::EXPORT_COLUMNS)
    );
    for case in recorded["import_folders"].as_array().unwrap() {
        let folder = import_folder(&case["folder"]);
        let row = import_folder_row(&folder.name, folder.paused, &folder.settings);
        assert_eq!(json!(row), case["row"], "{}", folder.name);
    }
    for case in recorded["export_folders"].as_array().unwrap() {
        let folder = export_folder(&case["folder"]);
        let row = export_folder_row(&folder, &predicates(&folder));
        assert_eq!(json!(row), case["row"], "{}", folder.name);
    }
}

fn action_name(action: &FolderAction) -> &'static str {
    match action {
        FolderAction::Delete => "delete",
        FolderAction::Ignore => "ignore",
        FolderAction::Move(_) => "move",
    }
}

fn import_cases() -> Vec<(String, ImportFolderEdit)> {
    let lists = hydrus_testkit::fixture_json("folders_lists.json");
    let mut cases: Vec<(String, ImportFolderEdit)> = lists["import_folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let f = import_folder(&c["folder"]);
            (f.name.clone(), f)
        })
        .collect();
    cases.push(("new".into(), ImportFolderEdit::new_folder()));
    cases
}

#[test]
fn the_import_folder_dialog_is_the_references() {
    let recorded = hydrus_testkit::fixture_json("folders_dialogs.json");
    let cases = import_cases();
    for opened in recorded["import_opened"].as_array().unwrap() {
        let name = opened["folder"].as_str().unwrap();
        let folder = &cases.iter().find(|c| c.0 == name).unwrap().1;
        let s = &folder.settings;
        let action = |a: &FolderAction| {
            json!({
                "choices": ACTION_CHOICES,
                "action": action_name(a),
                "location": match a { FolderAction::Move(l) => l.as_str(), _ => "" },
                "location_enabled": action_index(a) == 2,
            })
        };
        let ours = json!({
            "name": folder.name,
            "path": s.path,
            "search_subdirectories": s.search_subdirectories,
            "paused": folder.paused,
            "check_regularly": s.check_regularly,
            "period": s.period,
            "period_enabled": s.check_regularly,
            "skip": s.last_modified_time_skip_period,
            "check_now": s.check_now,
            "popup": s.show_working_popup,
            "popup_button": s.publish_files_to_popup_button,
            "page": s.publish_files_to_page,
            "actions": {
                "successful": action(&s.actions.successful_and_new),
                "redundant": action(&s.actions.successful_but_redundant),
                "deleted": action(&s.actions.deleted),
                "failed": action(&s.actions.error),
            },
            "filename_tagging": [],
            "import_options": hydrus_gui_model::edit_subscription::import_options_label(
                &hydrus_core::import_options::ImportOptionsSlice::default(),
            ),
        });
        assert_eq!(ours, opened["fields"], "{name}");
    }

    let existing = recorded["existing"].as_str().unwrap().to_owned();
    let exists = move |p: &str| p == existing;
    for applied in recorded["import_applied"].as_array().unwrap() {
        let name = applied["folder"].as_str().unwrap();
        let mut folder = cases.iter().find(|c| c.0 == name).unwrap().1.clone();
        let s = &mut folder.settings;
        for (field, value) in applied["edits"].as_object().unwrap() {
            match field.as_str() {
                "name" => folder.name = value.as_str().unwrap().into(),
                "path" => s.path = value.as_str().unwrap().into(),
                "paused" => folder.paused = value.as_bool().unwrap(),
                "check_regularly" => s.check_regularly = value.as_bool().unwrap(),
                "period" => s.period = value.as_i64().unwrap(),
                "check_now" => s.check_now = value.as_bool().unwrap(),
                "search_subdirectories" => s.search_subdirectories = value.as_bool().unwrap(),
                "skip" => s.last_modified_time_skip_period = value.as_i64().unwrap(),
                "popup" => s.show_working_popup = value.as_bool().unwrap(),
                "popup_button" => s.publish_files_to_popup_button = value.as_bool().unwrap(),
                "page" => s.publish_files_to_page = value.as_bool().unwrap(),
                "actions" => {
                    for (which, set) in value.as_object().unwrap() {
                        let action = match set[0].as_str().unwrap() {
                            "delete" => FolderAction::Delete,
                            "ignore" => FolderAction::Ignore,
                            _ => FolderAction::Move(set[1].as_str().unwrap().into()),
                        };
                        let slot = match which.as_str() {
                            "successful" => &mut s.actions.successful_and_new,
                            "redundant" => &mut s.actions.successful_but_redundant,
                            "deleted" => &mut s.actions.deleted,
                            _ => &mut s.actions.error,
                        };
                        *slot = action;
                    }
                }
                other => panic!("{other}"),
            }
        }
        let at = format!("{name} {}", applied["edits"]);
        match check_import_folder(&folder, &Sensitive::default(), &exists) {
            Err(veto) => assert_eq!(applied["veto"], veto.as_str(), "{at}"),
            Ok(warnings) => {
                let asked: Vec<&str> = applied["asked"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a["message"].as_str().unwrap())
                    .collect();
                assert_eq!(asked, warnings, "{at}");
                let value = &applied["value"];
                let s = &folder.settings;
                assert_eq!(value["name"], folder.name.as_str(), "{at}");
                assert_eq!(value["path"], s.path.as_str(), "{at}");
                assert_eq!(value["period"], s.period, "{at}");
                assert_eq!(value["paused"], folder.paused, "{at}");
                assert_eq!(value["check_now"], s.check_now, "{at}");
                assert_eq!(value["skip"], s.last_modified_time_skip_period, "{at}");
                assert_eq!(value["page"], s.publish_files_to_page, "{at}");
                assert_eq!(
                    value["actions"]["failed"],
                    action_name(&s.actions.error),
                    "{at}"
                );
            }
        }
    }
}

#[test]
fn the_export_folder_dialog_is_the_references() {
    let recorded = hydrus_testkit::fixture_json("folders_dialogs.json");
    let lists = hydrus_testkit::fixture_json("folders_lists.json");
    let mut cases: Vec<ExportFolder> = lists["export_folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| export_folder(&c["folder"]))
        .collect();
    let mut new = new_export_folder("{hash}".into(), FileSearchContext::default());
    new.name = "new".into();
    cases.push(new);
    for opened in recorded["export_opened"].as_array().unwrap() {
        let name = opened["folder"].as_str().unwrap();
        let folder = cases.iter().find(|c| c.name == name).unwrap();
        let fields = &opened["fields"];
        let synchronise = folder.export_type == ExportType::Synchronise;
        assert_eq!(
            fields["type"],
            if synchronise {
                "synchronise"
            } else {
                "regular"
            }
        );
        assert_eq!(fields["types"], json!(["regular", "synchronise"]));
        assert_eq!(fields["delete_enabled"], !synchronise, "{name}");
        assert_eq!(fields["period_enabled"], folder.run_regularly, "{name}");
        assert_eq!(fields["popup_enabled"], folder.run_regularly, "{name}");
        assert_eq!(fields["predicates"], json!(predicates(folder)), "{name}");
        assert_eq!(fields["period"], folder.period, "{name}");
        assert_eq!(fields["phrase"], folder.phrase.as_str(), "{name}");
        assert_eq!(fields["popup"], folder.show_working_popup, "{name}");
        assert_eq!(fields["symlinks"], folder.export_symlinks, "{name}");
        if name == "new" {
            assert_eq!(fields["name"], "export folder");
            assert!(folder.run_regularly);
        }
    }
    assert_eq!(
        recorded["export_synchronise"],
        json!({"delete": false, "delete_enabled": false})
    );

    for applied in recorded["export_applied"].as_array().unwrap() {
        let name = applied["folder"].as_str().unwrap();
        let mut folder = cases.iter().find(|c| c.name == name).unwrap().clone();
        let mut answer = None;
        for (field, value) in applied["edits"].as_object().unwrap() {
            match field.as_str() {
                "name" => folder.name = value.as_str().unwrap().into(),
                "path" => folder.path = value.as_str().unwrap().into(),
                "phrase" => folder.phrase = value.as_str().unwrap().into(),
                "period" => folder.period = value.as_i64().unwrap(),
                "delete" => folder.delete_from_client_after_export = value.as_bool().unwrap(),
                "answer" => answer = value.as_bool(),
                "run_regularly" => folder.run_regularly = value.as_bool().unwrap(),
                "run_now" => folder.run_now = value.as_bool().unwrap(),
                "symlinks" => folder.export_symlinks = value.as_bool().unwrap(),
                "popup" => folder.show_working_popup = value.as_bool().unwrap(),
                "overwrite_next" => {
                    folder.overwrite_sidecars_on_next_run = value.as_bool().unwrap();
                }
                "overwrite_always" => folder.always_overwrite_sidecars = value.as_bool().unwrap(),
                other => panic!("{other}"),
            }
        }
        let at = format!("{name} {}", applied["edits"]);
        let asked = applied["asked"].as_array().unwrap();
        if folder.delete_from_client_after_export {
            assert_eq!(asked[0]["message"], DELETE_WARNING, "{at}");
            assert_eq!(asked[1]["message"], DELETE_QUESTION, "{at}");
            if answer == Some(false) {
                assert_eq!(applied["refused"], true, "{at}");
                continue;
            }
        }
        if let Err(veto) = check_export_folder(&folder) {
            assert_eq!(applied["veto"], veto.as_str(), "{at}");
        } else {
            {
                let value = &applied["value"];
                assert_eq!(value["name"], folder.name.as_str(), "{at}");
                assert_eq!(value["phrase"], folder.phrase.as_str(), "{at}");
                assert_eq!(
                    value["delete"], folder.delete_from_client_after_export,
                    "{at}"
                );
                assert_eq!(
                    value["overwrite_always"], folder.always_overwrite_sidecars,
                    "{at}"
                );
            }
        }
    }
}

#[test]
fn the_lists_name_what_they_add_and_edit_uniquely() {
    let mut list = Named::new(vec![
        ImportFolderEdit::new_folder(),
        ImportFolderEdit {
            name: "Other".into(),
            ..ImportFolderEdit::new_folder()
        },
    ]);
    let added = list.add(ImportFolderEdit::new_folder());
    assert_eq!(list.get(added).unwrap().name, "import folder (1)");
    assert_eq!(list.one_selected(), Some(added));
    // renamed to another's name, casefolded: made unique
    list.replace(
        added,
        ImportFolderEdit {
            name: "OTHER".into(),
            ..ImportFolderEdit::new_folder()
        },
    );
    assert_eq!(list.get(added).unwrap().name, "OTHER (1)");
    let names: Vec<&str> = list
        .in_order()
        .iter()
        .map(|(_, f)| f.name.as_str())
        .collect();
    assert_eq!(names, ["import folder", "Other", "OTHER (1)"]);
    list.click(0, false, false);
    let deleted = list.delete_selected();
    assert_eq!(deleted[0].name, "import folder");
    assert_eq!(list.into_items().len(), 2);
}
