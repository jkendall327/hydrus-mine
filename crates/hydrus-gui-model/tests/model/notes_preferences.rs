//! Live note copy choices, Options staging and Qt cursor/clipboard outputs.
use hydrus_gui_model::{
    notes_editor::{self, NotesEditor},
    options::{Editor, Row, Settings},
};
use hydrus_store::settings::{self, NotePreferences};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn values(preferences: &NotePreferences) -> Value {
    json!([
        preferences.copy_all,
        preferences.copy_json,
        preferences.start_at_end,
        preferences.hover_text_only
    ])
}

// leaf: audit-options-notes-start-editing-notes-with-the-text-cursor-at-the-end-of-the-document
// leaf: audit-options-notes-when-middle-clicking-a-note-hover-only-copy-the-text
#[test]
fn actual_cog_copy_variants_and_options_persistence_replay() {
    let fixture = hydrus_testkit::fixture_json("notes_preferences.json");
    let notes: BTreeMap<String, String> = serde_json::from_value(fixture["notes"].clone()).unwrap();
    let mut editor = NotesEditor::new(&notes, Some("note2"));
    for recorded in fixture["initial_cursors"].as_array().unwrap() {
        assert_eq!(
            editor
                .tabs
                .iter()
                .find(|(name, _)| name == recorded["name"].as_str().unwrap())
                .unwrap()
                .1,
            recorded["text"].as_str().unwrap()
        );
    }
    editor
        .paste(fixture["future_paste"].as_str().unwrap())
        .unwrap();
    assert_eq!(editor.tabs[editor.current].0, "future");
    editor.tabs[editor.current].1 = fixture["inserted"]["text"].as_str().unwrap().into();
    for event in fixture["copies"].as_array().unwrap() {
        editor.current = editor
            .tabs
            .iter()
            .position(|(name, _)| name == event["current"].as_str().unwrap())
            .unwrap();
        let (text, notice) = editor
            .copy_with(
                event["all"].as_bool().unwrap(),
                event["json"].as_bool().unwrap(),
            )
            .unwrap();
        assert_eq!(json!([text]), event["clipboard"]);
        assert_eq!(json!([notice]), event["notice"]);
    }
    for event in fixture["hover"].as_array().unwrap() {
        assert_eq!(
            json!([notes_editor::hover_copy(
                "note2",
                "two 🦊",
                event["text_only"].as_bool().unwrap()
            )]),
            event["clipboard"]
        );
    }
    let (_directory, store) =
        super::options_dialog::fixture_store(&hydrus_testkit::fixture_json("options_dialog.json"));
    let mut before = store.read(Settings::load).unwrap();
    assert_eq!(values(&before.note_preferences), fixture["initial"]);
    for event in fixture["options"].as_array().unwrap() {
        let mut options = Editor::new(before.clone());
        let page = options
            .page_names()
            .iter()
            .position(|name| *name == "notes")
            .unwrap();
        options.show_page(page);
        for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
            let row = options.rows().iter().position(|row| matches!(row,Row::Opt {option,..} if option.label == label.as_str().unwrap())).unwrap();
            options.check(row, event["input"][index].as_bool().unwrap());
        }
        let (after, baseline, problems) = options.applied();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(values(&after.note_preferences), event["saved"]);
        assert_eq!(
            store.read(Settings::load).unwrap(),
            before,
            "Options draft is not durable"
        );
        let (after, baseline) = (after.clone(), baseline.clone());
        store
            .write(move |writer| after.save(writer.conn(), &baseline))
            .unwrap();
        let reopened = hydrus_store::Store::open(store.dir()).unwrap();
        before = reopened.read(Settings::load).unwrap();
        assert_eq!(values(&before.note_preferences), event["round_trip"]);
    }
    // An open Options draft updates only its changed fields. A live cog copy
    // choice made meanwhile is retained in the same writer transaction.
    let baseline = before.clone();
    let mut after = before.clone();
    after.note_preferences.start_at_end = false;
    store
        .write(|writer| {
            let mut latest: NotePreferences = settings::get(writer.conn())?;
            latest.copy_all = false;
            latest.copy_json = false;
            latest.hover_text_only = true;
            settings::set(writer.conn(), &latest)
        })
        .unwrap();
    store
        .write(move |writer| after.save(writer.conn(), &baseline))
        .unwrap();
    assert_eq!(
        values(&store.read(settings::get::<NotePreferences>).unwrap()),
        fixture["after_cancel"]
    );
    assert_eq!(fixture["before_cancel"], fixture["after_cancel"]);
    assert_eq!(fixture["after_cancel"], fixture["final_round_trip"]);
}
