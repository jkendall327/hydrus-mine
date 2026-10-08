//! The real connector is presentation only: saved mappings and query tags stay raw.
use super::tag_dialog_preferences::fixture;
use hydrus_core::{
    ServiceKey, search::context::LocationContext, tag_presentation::TagPresentation,
};
use hydrus_gui_model::{
    domains::Choice,
    manage_tags::ManageTags,
    options::{Editor, Row, Settings},
    write_autocomplete::WriteAutocomplete,
};
use hydrus_store::settings;
use serde_json::{Value, json};

const LABEL: &str = "Sibling connecting string: ";

// leaf: audit-options-tag-presentation-other-rendering-sibling-connecting-string
#[test]
fn connector_options_replay_stage_cancel_apply_and_live_consumers() {
    let recorded = hydrus_testkit::fixture_json("sibling_connector.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    fixture::set_preferences(
        &store,
        &json!({"listbook":false,"parents":true,"expanded":true,"siblings":true}),
    );
    let old: TagPresentation = serde_json::from_value(json!({"namespace_connector":":"})).unwrap();
    assert_eq!(old.sibling_connector, recorded["options"]["shown"]);
    let before: TagPresentation = store.read(settings::get).unwrap();
    assert_eq!(before.sibling_connector, recorded["options"]["shown"]);
    let manage = ManageTags::new(store.clone(), files.clone()).unwrap();
    let key = ServiceKey::from_hex(recorded["tag_service"].as_str().unwrap()).unwrap();
    let location = LocationContext::single(
        ServiceKey::from_hex(recorded["file_context"][0].as_str().unwrap()).unwrap(),
    );
    let mut input = WriteAutocomplete::new(store.clone(), key.clone(), location.clone());
    input.choose_domain(Choice::Tags(key));
    input.choose_domain(Choice::Location(location));
    let make_editor = || {
        let mut editor = Editor::new(store.read(Settings::load).unwrap());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "tag presentation")
            .unwrap();
        editor.show_page(page);
        let row = editor
            .rows()
            .iter()
            .position(|row| matches!(row,Row::Opt{option,..} if option.label==LABEL))
            .unwrap();
        (editor, row)
    };
    let (mut cancelled, row) = make_editor();
    cancelled.text(row, recorded["options"]["applied"].as_str().unwrap());
    assert_eq!(
        store.read::<TagPresentation>(settings::get).unwrap(),
        before
    );
    drop(cancelled);
    assert_eq!(
        store
            .read::<TagPresentation>(settings::get)
            .unwrap()
            .sibling_connector,
        recorded["options"]["cancelled"]
    );
    for case in recorded["cases"].as_array().unwrap() {
        let (mut editor, row) = make_editor();
        let prior: TagPresentation = store.read(settings::get).unwrap();
        editor.text(row, case["connector"].as_str().unwrap());
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), prior);
        let (after, original, problems) = editor.applied();
        assert!(problems.is_empty());
        let original = original.clone();
        store
            .write(move |ctx| after.save(ctx.conn(), &original))
            .unwrap();
        let saved: TagPresentation = store.read(settings::get).unwrap();
        assert_eq!(saved.sibling_connector, case["connector"]);
        assert_eq!(
            saved,
            TagPresentation {
                sibling_connector: saved.sibling_connector.clone(),
                ..prior
            }
        );
        let rows: Vec<Value> = manage
            .display_rows()
            .into_iter()
            .filter(|row| row.tag.starts_with("parity:connector"))
            .map(|row| json!({"tag":row.tag,"rows":[row.label]}))
            .collect();
        assert_eq!(json!(rows), case["storage"]);
        input.set_text(recorded["query"].as_str().unwrap());
        let rows: Vec<Value> = input
            .rows()
            .iter()
            .map(|row| json!({"tag":row.tag,"rows":[row.label]}))
            .collect();
        assert_eq!(json!(rows), case["write"]);
        // Reopening reads saved text; presentation never stages a tag mapping.
        let reopened = ManageTags::new(store.clone(), files.clone()).unwrap();
        assert_eq!(reopened.display_rows(), manage.display_rows());
        assert!(!manage.has_changes());
        assert_eq!(
            fixture::preferences(manage.dialog_preferences()),
            json!({"listbook":false,"parents":true,"expanded":true,"siblings":true})
        );
    }
}
