//! Saving sessions (pages > sessions > save) against the reference's,
//! recorded by `oracle/record_sessions_menu.py`: each recorded save,
//! answered as recorded, asks what the reference asked (each dialog's
//! kind, message, title and buttons, in turn) and saves the session the
//! reference saved, or none.

use serde_json::{Value as Json, json};

use hydrus_gui_model::session_saving::{NAME_MESSAGE, Saving, Step};

/// Did this step save `name`: newly listed, or saved once more.
fn saved(before: &Json, after: &Json) -> Option<String> {
    after
        .as_object()
        .unwrap()
        .iter()
        .find_map(|(name, count)| (before.get(name) != Some(count)).then(|| name.clone()))
}

#[test]
fn saving_sessions_asks_and_saves_as_the_reference() {
    let recorded = hydrus_testkit::fixture_json("sessions_menu.json");
    let steps = recorded["steps"].as_array().unwrap();
    let mut checked = 0;
    for (i, step) in steps.iter().enumerate().skip(1) {
        let what = step["do"].as_str().unwrap();
        if !what.starts_with("save") {
            continue;
        }
        let before = &steps[i - 1]["sessions"];
        let existing: Vec<String> = before.as_object().unwrap().keys().cloned().collect();
        let theirs = step["asked"].as_array().unwrap();
        let mut answers = theirs.iter().filter(|a| a["kind"] != "warning");
        let mut ours = Vec::new();
        let (mut saving, mut next) = if what == "save new" {
            Saving::new_session(existing)
        } else {
            Saving::over(step["name"].as_str().unwrap())
        };
        let mut done = None;
        loop {
            next = match next {
                Step::AskName { warning } => {
                    if let Some(warning) = warning {
                        ours.push(json!({ "kind": "warning", "message": warning }));
                    }
                    let answer = answers.next().expect("an answer to the name");
                    ours.push(json!({
                        "kind": "text",
                        "message": NAME_MESSAGE,
                        "default": "",
                        "answer": answer["answer"],
                    }));
                    saving.named(answer["answer"].as_str())
                }
                Step::Ask(question) => {
                    let answer = answers.next().expect("an answer to the question");
                    ours.push(json!({
                        "kind": "yes/no",
                        "message": question.message,
                        "title": question.title,
                        "yes": question.yes,
                        "no": question.no,
                        "answer": answer["answer"],
                    }));
                    saving.answered(answer["answer"].as_bool())
                }
                Step::Save(name) => {
                    done = Some(name);
                    break;
                }
                Step::Stop => break,
            };
        }
        assert_eq!(ours, *theirs, "step {i}: {what}");
        assert_eq!(done, saved(before, &step["sessions"]), "step {i}: {what}");
        checked += 1;
    }
    assert_eq!(checked, 8);
}

#[test]
fn clear_and_load_asks_as_the_references_does() {
    use hydrus_gui_model::session_saving::{
        CLEAR_AND_LOAD_TITLE, clear_and_load_question, close_all_question,
    };
    let recorded: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/fixtures/sessions_menu.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let steps: Vec<&serde_json::Value> = recorded["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["do"] == "clear and load")
        .collect();
    assert!(!steps.is_empty());
    for step in &steps {
        let first = &step["asked"][0];
        assert_eq!(
            first["message"],
            clear_and_load_question(step["name"].as_str().unwrap())
        );
        assert_eq!(first["title"], CLEAR_AND_LOAD_TITLE);
    }
    // the recorded pages' objections: two local imports, then a watcher
    // page
    let importing = "This page is still importing.".to_owned();
    let both = close_all_question(&[
        (importing.clone(), "files".into()),
        (importing, "files".into()),
    ]);
    assert_eq!(serde_json::json!(both), steps[3]["asked"][1]["message"]);
    let watchers =
        close_all_question(&[("2 watchers are still importing.".into(), "files".into())]);
    assert_eq!(serde_json::json!(watchers), steps[5]["asked"][1]["message"]);
    assert_eq!(close_all_question(&[]), None);
}

#[test]
fn backup_menu_labels_match_the_reference() {
    fn find<'a>(
        entries: &'a [hydrus_gui_model::main_menu::Entry],
        label: &str,
    ) -> &'a [hydrus_gui_model::main_menu::Entry] {
        entries
            .iter()
            .find_map(|entry| match entry {
                hydrus_gui_model::main_menu::Entry::Menu {
                    label: name,
                    entries,
                    ..
                } if name == label => Some(entries.as_slice()),
                _ => None,
            })
            .unwrap()
    }
    let fixture = hydrus_testkit::fixture_json("session_backups.json");
    assert_eq!(
        hydrus_gui_model::session_saving::backup_timestamp(
            fixture["timestamp"].as_i64().unwrap(),
            &jiff::tz::TimeZone::UTC
        ),
        fixture["label_utc"].as_str().unwrap()
    );
    let facts = hydrus_gui_model::main_menu::Facts {
        session_backups: vec![(
            "backup test".into(),
            vec![fixture["timestamp"].as_i64().unwrap()],
        )],
        ..hydrus_gui_model::main_menu::Facts::default()
    };
    let pages = hydrus_gui_model::main_menu::menubar(&facts);
    let sessions = find(find(&pages, "&pages"), "sessions");
    let names = find(sessions, "append backup");
    let timestamps = find(names, "backup test");
    assert!(timestamps[0].usable());
    let hydrus_gui_model::main_menu::Entry::Item { command, .. } = &timestamps[0] else {
        panic!("timestamp action")
    };
    assert_eq!(
        command,
        &Some(hydrus_gui_model::main_menu::Command::AppendSessionBackup {
            name: "backup test".into(),
            timestamp: fixture["timestamp"].as_i64().unwrap(),
        })
    );
}
