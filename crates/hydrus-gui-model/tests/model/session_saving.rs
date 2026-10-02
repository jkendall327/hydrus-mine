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
