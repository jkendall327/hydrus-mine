//! The ordered custom file-deletion reasons list in the real options window,
//! driven through the sequence the reference's panel was recorded with
//! (`oracle/fixtures/files_trash.json`, `reason_queue`): add (cancelled, then
//! answered), edit the selected reason, move it up and down, delete (declined,
//! then accepted), then Apply and reopen.

use hydrus_gui::OptionsWindow;
use hydrus_store::settings::DeletionPreferences;
use slint::{ComponentHandle as _, Model as _};

use super::options_system_consumers::client;

fn reasons(window: &OptionsWindow) -> Vec<String> {
    let rows = window.get_reason_rows();
    (0..rows.row_count())
        .map(|i| rows.row_data(i).unwrap().cells.row_data(0).unwrap().into())
        .collect()
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

// leaf: audit-options-files-and-trash-advanced-file-deletion-and-custom-reasons-edit
#[test]
fn editing_a_reason_asks_with_its_text_replaces_it_in_place_and_survives_apply() {
    let fixture = hydrus_testkit::fixture_json("files_trash.json");
    let queue = &fixture["reason_queue"];
    let events = queue["events"].as_array().unwrap();
    let client = client();
    let initial = strings(&events[0]["rows"]);
    client
        .store
        .write({
            let initial = initial.clone();
            move |ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &DeletionPreferences {
                        advanced: true,
                        reasons: initial,
                        ..DeletionPreferences::default()
                    },
                )
            }
        })
        .unwrap();
    let window = client.options("files and trash");
    assert_eq!(reasons(&window), initial);
    let child = || {
        client
            .bound
            .options_reason_child
            .borrow()
            .as_ref()
            .expect("the Enter Text child")
            .clone_strong()
    };

    // add, backed out of
    window.invoke_reason_action("add".into());
    let asked = child();
    assert_eq!(asked.get_message(), events[0]["message"].as_str().unwrap());
    assert_eq!(asked.get_text(), events[0]["default"].as_str().unwrap());
    asked.invoke_cancelled();
    assert_eq!(reasons(&window), strings(&events[0]["rows"]));
    // add, answered twice (the second with nothing)
    for event in &events[1..3] {
        window.invoke_reason_action("add".into());
        let asked = child();
        assert_eq!(asked.get_message(), event["message"].as_str().unwrap());
        assert_eq!(asked.get_text(), event["default"].as_str().unwrap());
        asked.invoke_name_entered(event["answer"].as_str().unwrap().into());
        assert_eq!(reasons(&window), strings(&event["rows"]));
    }

    // edit the second reason ("beta", of two)
    let event = &events[3];
    window.invoke_reason_clicked(1, false, false);
    window.invoke_reason_action("edit".into());
    let asked = child();
    assert_eq!(asked.get_message(), event["message"].as_str().unwrap());
    assert_eq!(
        asked.get_text(),
        event["default"].as_str().unwrap(),
        "the question starts on the reason's own text"
    );
    asked.invoke_name_entered(event["answer"].as_str().unwrap().into());
    assert_eq!(reasons(&window), strings(&event["rows"]));
    assert!(client.bound.options_reason_child.borrow().is_none());

    // a declined edit leaves the list as it was
    window.invoke_reason_action("edit".into());
    child().invoke_cancelled();
    assert_eq!(reasons(&window), strings(&event["rows"]));

    // the edited reason keeps its place in the order the list is kept in
    window.invoke_reason_action("up".into());
    assert_eq!(reasons(&window), strings(&queue["up"]));
    window.invoke_reason_action("down".into());
    assert_eq!(reasons(&window), strings(&queue["down"]));
    window.invoke_reason_action("delete".into());
    let question = child();
    assert_eq!(
        question.get_message(),
        queue["delete_question"][0].as_str().unwrap()
    );
    question.invoke_answered(false);
    assert_eq!(reasons(&window), strings(&queue["cancel_rows"]));
    window.invoke_reason_action("delete".into());
    child().invoke_answered(true);
    assert_eq!(reasons(&window), strings(&queue["deleted_rows"]));

    window.invoke_apply();
    let saved = client.get::<DeletionPreferences>().reasons;
    assert_eq!(saved, strings(&queue["reopened"]));
    let again = client.options("files and trash");
    assert_eq!(reasons(&again), strings(&queue["reopened"]));

    // double-clicking a reason is the same edit, asked on that reason
    again.invoke_reason_activated(0);
    let asked = child();
    assert_eq!(asked.get_text(), "alpha");
    asked.invoke_name_entered("alpha two".into());
    assert_eq!(reasons(&again)[0], "alpha two");
}
