//! Real Qt sorted favourite rows, duplicate/cancellation and legacy persistence.
use hydrus_gui_model::regex_favourites::{Editor, RegexFavourites, load, validity};
use serde_json::{Value, json};

fn favourites(value: &Value) -> RegexFavourites {
    RegexFavourites(serde_json::from_value(value.clone()).unwrap())
}
fn check(editor: &Editor, state: &Value) {
    assert_eq!(json!(editor.rows()), state["rows"]);
    assert_eq!(json!(editor.value().0), state["value"]);
    assert_eq!(json!(editor.selected()), state["selected"]);
}

#[test]
fn favourite_rows_and_input_boundaries_match_real_qt() {
    let fixture = hydrus_testkit::fixture_json("regex_favourites.json");
    let mut editor = Editor::new(&favourites(&fixture["initial"]));
    let steps = fixture["steps"].as_array().unwrap();
    check(&editor, &steps[0]["state"]);
    for step in steps.iter().skip(1) {
        let action = step["do"].as_array().unwrap();
        let kind = action[0].as_str().unwrap();
        if kind == "add" || kind == "edit" {
            let p = if kind == "edit" { 2 } else { 1 };
            let phrase = action[p].as_str().unwrap();
            assert_eq!(validity(phrase).is_ok(), step["opened"][0]["valid"]);
            if kind == "edit" {
                let row: (String, String) = serde_json::from_value(action[1].clone()).unwrap();
                let index = editor.rows().iter().position(|v| v == &row).unwrap();
                editor.click(index, false, false);
            }
            let accepted = action.last().unwrap() == true && !action[p + 1].is_null();
            if accepted {
                let description = action[p + 1].as_str().unwrap().to_owned();
                if kind == "add" {
                    let result = editor.add(phrase.to_owned(), description);
                    let warning = step["said"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find_map(|v| v.get("warning"));
                    assert_eq!(result.err().map(|e| json!(e)), warning.cloned());
                } else {
                    let (id, _) = editor.editing().unwrap();
                    editor.replace(id, phrase.to_owned(), description);
                }
            }
        } else {
            for (i, row) in action[1].as_array().unwrap().iter().enumerate() {
                let row: (String, String) = serde_json::from_value(row.clone()).unwrap();
                let index = editor.rows().iter().position(|v| v == &row).unwrap();
                editor.click(index, i > 0, false);
            }
            assert_eq!(step["said"][0]["asked"], "Remove all selected?");
            if action[2] == true {
                editor.delete();
            }
        }
        check(&editor, &step["state"]);
    }
    assert_eq!(
        json!(
            editor
                .value()
                .0
                .iter()
                .map(|row| row.0.clone())
                .collect::<Vec<_>>()
        ),
        fixture["copied"]
    );
}

#[test]
fn legacy_favourites_load_until_native_values_including_empty_are_saved() {
    let fixture = hydrus_testkit::fixture_json("regex_favourites.json");
    assert_eq!(json!(RegexFavourites::default().0), fixture["defaults"]);
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT); CREATE TABLE legacy_objects(source TEXT, dump TEXT);").unwrap();
    let legacy = "regex_favourites:\n- !!python/tuple ['[', 'fragment']\n- !!python/tuple ['a+', 'letters']\n";
    conn.execute("INSERT INTO legacy_objects VALUES ('options', ?)", [legacy])
        .unwrap();
    assert_eq!(
        load(&conn).unwrap().0,
        vec![
            ("[".into(), "fragment".into()),
            ("a+".into(), "letters".into())
        ]
    );
    hydrus_store::settings::set(&conn, &RegexFavourites(Vec::new())).unwrap();
    assert!(load(&conn).unwrap().0.is_empty());
    let reopened = load(&conn).unwrap();
    assert_eq!(
        reopened,
        hydrus_store::settings::get::<RegexFavourites>(&conn).unwrap()
    );
}

#[test]
fn regex_clipboard_components_match_real_qt() {
    use hydrus_gui_model::regex_favourites::regex_tools;
    let fixture = hydrus_testkit::fixture_json("regex_favourites.json");
    for (category, name) in ["regex components", "regex replacement groups"]
        .iter()
        .enumerate()
    {
        let category = category + 1;
        let mut expected = fixture["menus"][name].clone();
        if cfg!(windows) && category == 1 {
            let phrase = r"(?<=\\)[^\\]*?(?=\..*$)";
            *expected.as_array_mut().unwrap().last_mut().unwrap() =
                json!([format!("filename - {phrase}"), phrase]);
        }
        assert_eq!(json!(regex_tools(category)), expected);
    }
    assert!(regex_tools(3).is_empty());
}

#[test]
fn matcher_favourite_menu_matches_actual_qt_actions_and_copy_payloads() {
    use hydrus_gui_model::{
        main_menu::Entry,
        regex_favourites::{MenuAction, menu},
    };
    let fixture = hydrus_testkit::fixture_json("matcher_favourites.json");
    let value = favourites(&fixture["initial"]);
    let (entries, actions) = menu(&value);
    let rows = entries
        .iter()
        .map(|entry| match entry {
            Entry::Separator => json!({"separator": true}),
            _ => json!({"label": entry.label(), "enabled": entry.usable()}),
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(rows), fixture["menus"][0]);
    assert_eq!(actions[0], MenuAction::Manage);
    assert_eq!(actions[1], MenuAction::Instruction);
    assert_eq!(actions[2], MenuAction::Copy("a+".into()));
    assert_eq!(actions[3], MenuAction::Copy("[".into()));
    // The reference label is enabled but performs no clipboard action.
    assert!(entries[2].usable());
    let (empty, actions) = menu(&RegexFavourites(Vec::new()));
    assert_eq!(actions, [MenuAction::Manage, MenuAction::Instruction]);
    assert!(empty[0].usable());
}

#[test]
fn readonly_row_input_menu_uses_saved_pairs_without_recursive_management() {
    use hydrus_gui_model::regex_favourites::{MenuAction, input_menu};
    use serde_json::json;
    let f = hydrus_testkit::fixture_json("regex_options_editor.json");
    for case in f["descriptions"].as_array().unwrap() {
        let text = case["input"].as_str().unwrap();
        let actual = hydrus_gui_model::regex_favourites::description_validity(text);
        if let Some(error) = case["error"].as_str() {
            assert_eq!(actual.unwrap_err(), error);
        } else {
            assert!(actual.is_ok());
            assert_eq!(text, case["value"].as_str().unwrap());
        }
    }
    for step in f["steps"].as_array().unwrap() {
        for recorded in step["menus"].as_array().unwrap() {
            let saved = RegexFavourites(serde_json::from_value(recorded["saved"].clone()).unwrap());
            let (entries, actions) = input_menu(&saved);
            let actual = entries
                .iter()
                .map(|e| match e {
                    hydrus_gui_model::main_menu::Entry::Separator => json!({"separator":true}),
                    _ => json!({"label":e.label(),"enabled":e.usable()}),
                })
                .collect::<Vec<_>>();
            assert_eq!(json!(actual), recorded["rows"]);
            assert!(!actions.contains(&MenuAction::Manage));
            assert_eq!(actions[0], MenuAction::Instruction);
            let copied = actions
                .iter()
                .filter_map(|a| {
                    if let MenuAction::Copy(s) = a {
                        Some(s)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(json!(copied), step["copied"]);
            assert_ne!(
                json!(saved.0),
                step["after"]["draft"],
                "row menu reads saved preferences, not edited list rows"
            );
        }
    }
    let (entries, actions) = input_menu(&RegexFavourites(Vec::new()));
    assert_eq!(actions, [MenuAction::Instruction]);
    assert_eq!(entries.len(), 2);
    assert!(entries[0].usable());
}
