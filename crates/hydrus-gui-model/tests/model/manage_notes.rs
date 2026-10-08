//! The "manage notes" dialog, step by step as the reference's
//! (`oracle/record_manage_notes.py`).

use std::collections::BTreeMap;

use hydrus_gui_model::notes_editor::{self, NotesEditor};
use serde_json::Value;

fn notes(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn check(editor: &NotesEditor, state: &Value, at: &str) {
    let tabs: Vec<(String, String)> = state["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t[0].as_str().unwrap().to_owned(),
                t[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(editor.tabs, tabs, "{at}: tabs");
    let current = state["current"].as_i64().unwrap();
    if current >= 0 {
        assert_eq!(editor.current as i64, current, "{at}: current");
    }
    assert_eq!(
        editor.can_edit(),
        state["can_edit"].as_bool().unwrap(),
        "{at}"
    );
    let (value, deletees) = editor.value();
    assert_eq!(value, notes(&state["notes"]), "{at}: notes");
    assert_eq!(
        deletees.into_iter().collect::<Vec<_>>(),
        strings(&state["deletees"]),
        "{at}: deletees"
    );
    assert_eq!(
        editor.changed(),
        state["changed"].as_bool().unwrap(),
        "{at}: changed"
    );
}

// leaf: audit-media-notes-clipboard, audit-media-notes-urls, audit-media-notes-tabs
#[test]
fn notes_dialog_steps_as_the_reference() {
    let recorded = hydrus_testkit::fixture_json("manage_notes.json");
    for case in recorded["cases"].as_array().unwrap() {
        let start_on = case["start_on"].as_str();
        let mut editor = NotesEditor::new(&notes(&case["notes"]), start_on);
        let states = case["states"].as_array().unwrap();
        check(&editor, &states[0]["state"], "start");
        for recorded in &states[1..] {
            let step = recorded["step"].as_array().unwrap();
            let at = format!("{step:?}");
            let said = recorded["said"].as_array().unwrap();
            let clipboard = strings(&recorded["clipboard"]);
            match step[0].as_str().unwrap() {
                "add" => {
                    assert_eq!(said[0]["asked"], notes_editor::NAME_PROMPT);
                    editor.add(step[1].as_str().unwrap());
                }
                "rename" => {
                    let index = usize::try_from(step[1].as_u64().unwrap()).unwrap();
                    assert_eq!(said[0]["default"], editor.tabs[index].0.as_str(), "{at}");
                    editor.rename(index, step[2].as_str().unwrap());
                }
                "delete" => {
                    if editor.can_edit() {
                        assert_eq!(said[0]["asked"], notes_editor::DELETE_QUESTION);
                        editor.delete_current();
                    } else {
                        assert!(said.is_empty());
                    }
                }
                "type" => {
                    let index = usize::try_from(step[1].as_u64().unwrap()).unwrap();
                    // the text box gives back its text with "\r\n" made "\n"
                    editor.tabs[index].1 = step[2].as_str().unwrap().replace("\r\n", "\n");
                }
                "select" => {
                    editor.current = usize::try_from(step[1].as_u64().unwrap()).unwrap();
                }
                "paste" => match editor.paste(step[1].as_str().unwrap()) {
                    Ok(message) => assert_eq!(said[0]["notified"], message.as_str(), "{at}"),
                    Err(message) => {
                        let error = said[0]["parse error"].as_str().unwrap();
                        assert!(message.starts_with("Sorry, I could not understand"), "{at}");
                        if !error.starts_with("Expecting") {
                            assert!(message.ends_with(error), "{at}: {message}");
                        }
                    }
                },
                "copy" => {
                    let (text, message) = editor.copy().unwrap();
                    assert_eq!(clipboard, vec![text], "{at}");
                    assert_eq!(said[0]["notified"], message.as_str(), "{at}");
                }
                "urls" => {
                    let (text, message) = editor.copy_urls().unwrap();
                    assert_eq!(clipboard, text.into_iter().collect::<Vec<_>>(), "{at}");
                    assert_eq!(said[0]["notified"], message.as_str(), "{at}");
                }
                other => panic!("unknown step {other}"),
            }
            check(&editor, &recorded["state"], &at);
        }
    }
}

#[test]
fn the_menu_counts_the_focused_files_notes() {
    assert_eq!(notes_editor::menu_label(0), "notes");
    assert_eq!(notes_editor::menu_label(1234), "notes (1,234)");
}

#[test]
fn copies_as_python_writes_json() {
    let mut editor = NotesEditor::new(&BTreeMap::new(), None);
    editor.tabs[0].1 = "caf\u{e9} \u{1f600}\t\u{7f}".to_owned();
    assert_eq!(
        editor.copy().unwrap().0,
        r#"{"notes": "caf\u00e9 \ud83d\ude00\t\u007f"}"#
    );
}
