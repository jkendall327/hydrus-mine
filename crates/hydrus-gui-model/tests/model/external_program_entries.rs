//! The external programs list's choices and dialog starts, against
//! `oracle/fixtures/import_external_programs.json` (the reference's entry
//! editor over its executable manager).
use hydrus_core::external_calls::{ActualCall, Callable, Manager, Pipeline, Process};
use hydrus_core::import_options::ExternalProgramEntry;
use hydrus_gui_model::external_program_entries as model;

fn fixture() -> serde_json::Value {
    hydrus_testkit::fixture_json("import_external_programs.json")
}

fn call(n: u8, name: &str, pipeline: Pipeline) -> Callable {
    Callable {
        key: [n; 32],
        name: name.into(),
        pipeline,
        call: ActualCall::Process(Process::default()),
    }
}

fn manager() -> Manager {
    Manager {
        calls: vec![
            call(1, "Zulu URL", Pipeline::Url),
            call(2, "beta log", Pipeline::File),
            call(3, "Alpha 日本 log", Pipeline::File),
            call(4, "broken", Pipeline::File),
            call(5, "Default OS File Launch", Pipeline::File),
            call(6, "gamma file id", Pipeline::File),
        ],
    }
}

#[test]
fn choices_are_the_file_calls_by_name_and_the_dialog_starts_where_the_reference_does() {
    let fixture = fixture();
    let choices = model::choices(&manager());
    let recorded: Vec<(String, String)> = fixture["sorted_programs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p[0].as_str().unwrap().to_owned(),
                p[1].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        choices
            .iter()
            .map(|c| (c.name.clone(), c.key.clone()))
            .collect::<Vec<_>>(),
        recorded
    );
    // the dialog's "when" choices, as worded and valued
    let seen = &fixture["add_events"][1]["seen"];
    for (ours, theirs) in model::WHENS.iter().zip(seen["whens"].as_array().unwrap()) {
        assert_eq!(ours.0, theirs[0].as_str().unwrap());
        assert_eq!(
            [ours.1, ours.2],
            [theirs[1][0] == true, theirs[1][1] == true]
        );
    }
    // a new entry names the first call, for new files; its dialog starts there
    let entry = model::new_entry(&choices);
    assert_eq!(entry.key, choices[0].key);
    let editing = model::start_editing(&entry, &choices).unwrap();
    assert_eq!(
        choices[editing.program_index].key,
        seen["initial"]["program"].as_str().unwrap()
    );
    assert!(model::WHENS[editing.when_index].1);
    assert!(!model::WHENS[editing.when_index].2);
    assert!(editing.notice.is_none());
    // an entry whose call is not there: its notice, the first call chosen
    let missing = &fixture["edit_missing"];
    let ghost = ExternalProgramEntry {
        key: "09".repeat(32),
        name: "gone program".into(),
        on_new: true,
        on_already_in: false,
    };
    assert_eq!(ghost.summary(), missing["summary"].as_str().unwrap());
    let editing = model::start_editing(&ghost, &choices).unwrap();
    assert_eq!(editing.program_index, 0);
    assert_eq!(
        editing.notice,
        Some(missing["messages"][0][1].as_str().unwrap())
    );
    // with no calls: no dialog, and the reference's critical notice
    assert!(model::start_editing(&ghost, &[]).is_none());
    let none = &fixture["no_calls"]["messages"][0];
    assert_eq!(model::NO_CALLS_TITLE, none[1].as_str().unwrap());
    assert_eq!(model::no_calls_message(), none[2].as_str().unwrap());
    // what the dialog hands back
    let done = model::finished(&choices, 2, 3).unwrap();
    assert_eq!(
        (done.name.as_str(), done.on_new, done.on_already_in),
        (choices[2].name.as_str(), false, false)
    );
    assert!(model::finished(&choices, 99, 0).is_none());
    assert_eq!(model::delete_question(1), "Remove 1 selected?");
    assert_eq!(model::delete_question(1234), "Remove 1,234 selected?");
}
