//! Actual storage-tag list defaults, independent of write autocomplete settings.
#[path = "../support/tag_dialog_preferences.rs"]
pub(super) mod fixture;
use hydrus_gui_model::{
    manage_tags::ManageTags,
    options::{Editor, Row, Settings},
    tag_relationships::{RelationKind, Relationships},
};
use hydrus_store::{settings, tag_editing::TagEditingSettings};
use serde_json::{Value, json};

const CONTROLS: [(&str, &str); 4] = [
    (
        "listbook",
        "Use listbook instead of tabbed notebook for tag service panels: ",
    ),
    (
        "parents",
        "Show parent info by default on edit/write taglists: ",
    ),
    (
        "expanded",
        "Show parents expanded by default on edit/write taglists: ",
    ),
    (
        "siblings",
        "Show sibling info by default on edit/write taglists: ",
    ),
];

fn rows(model: &ManageTags) -> Value {
    let mut rows = Vec::<Value>::new();
    for row in model
        .display_rows()
        .into_iter()
        .filter(|row| row.tag.starts_with("parity:"))
    {
        if row.parent_row {
            assert_eq!(row.colour_tag, row.label.trim());
            assert_eq!(rows.last().unwrap()["tag"], row.tag);
            rows.last_mut().unwrap()["rows"]
                .as_array_mut()
                .unwrap()
                .push(json!(row.label));
        } else {
            assert_eq!(row.colour_tag, row.tag);
            rows.push(json!({"tag":row.tag,"rows":[row.label]}));
        }
    }
    fixture::canonical(json!(rows))
}

#[test]
fn options_defaults_stage_cancel_apply_and_preserve_autocomplete_preferences() {
    let recorded = hydrus_testkit::fixture_json("tag_dialog_preferences.json");
    let (_directory, store, _files) = fixture::seed(&recorded);
    let before = store.read(Settings::load).unwrap();
    assert_eq!(
        fixture::preferences(&before.tag_editing),
        recorded["options"]["controls"]
    );
    let mut editor = Editor::new(before.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "tag editing")
        .unwrap();
    editor.show_page(page);
    for (key, label) in CONTROLS {
        let row = editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Opt { option, .. } if option.label == label))
            .unwrap();
        editor.check(row, recorded["options"]["applied"][key].as_bool().unwrap());
    }
    assert_eq!(
        fixture::preferences(&store.read::<TagEditingSettings>(settings::get).unwrap()),
        recorded["options"]["before_apply"]
    );
    // A cancelled options editor simply drops the draft and reopening sees the persisted defaults.
    assert_eq!(
        fixture::preferences(&Editor::new(before.clone()).applied().0.tag_editing),
        recorded["options"]["cancelled"]
    );
    let (after, original, problems) = editor.applied();
    assert!(problems.is_empty());
    assert_eq!(
        fixture::preferences(&after.tag_editing),
        recorded["options"]["applied"]
    );
    assert_eq!(
        after.tag_editing.autocomplete_show_parents,
        before.tag_editing.autocomplete_show_parents
    );
    assert_eq!(
        after.tag_editing.autocomplete_expand_parents,
        before.tag_editing.autocomplete_expand_parents
    );
    assert_eq!(
        after.tag_editing.autocomplete_show_siblings,
        before.tag_editing.autocomplete_show_siblings
    );
    let original = original.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &original))
        .unwrap();
    assert_eq!(
        fixture::preferences(&store.read::<TagEditingSettings>(settings::get).unwrap()),
        recorded["options"]["applied"]
    );
}

#[test]
fn storage_rows_and_service_topologies_replay_all_reference_combinations() {
    let recorded = hydrus_testkit::fixture_json("tag_dialog_preferences.json");
    let (_directory, store, files) = fixture::seed(&recorded);
    for case in recorded["cases"].as_array().unwrap() {
        fixture::set_preferences(&store, &case["preferences"]);
        let model = ManageTags::new(store.clone(), files.clone()).unwrap();
        assert_eq!(
            fixture::preferences(model.dialog_preferences()),
            case["preferences"]
        );
        assert_eq!(json!(model.service_names()), case["services"]);
        assert_eq!(rows(&model), fixture::canonical(case["rows"].clone()));
        let expected_listbook = case["topology"]["manage"] == "ListBook";
        assert_eq!(model.dialog_preferences().use_listbook, expected_listbook);
        for kind in [RelationKind::Siblings, RelationKind::Parents] {
            assert_eq!(
                Relationships::new(store.clone(), kind)
                    .unwrap()
                    .use_listbook(),
                expected_listbook
            );
        }
        assert!(model.autocomplete_options().autocomplete_show_parents);
        assert!(model.autocomplete_options().autocomplete_expand_parents);
        assert!(model.autocomplete_options().autocomplete_show_siblings);
    }
    let before = ManageTags::new(store.clone(), files.clone()).unwrap();
    let captured = rows(&before);
    fixture::set_preferences(&store, &recorded["cases"][0]["preferences"]);
    assert_eq!(
        rows(&before),
        captured,
        "these are opening defaults, not a live reset of an editor"
    );
    let reopened = ManageTags::new(store.clone(), files.clone()).unwrap();
    assert_eq!(
        rows(&reopened),
        fixture::canonical(recorded["cases"][0]["rows"].clone())
    );
    fixture::set_preferences(&store, &recorded["cases"][15]["preferences"]);
    let mut draft = ManageTags::new(store.clone(), files.clone()).unwrap();
    let activation = &recorded["cases"][15]["parent_activation"];
    assert_eq!(activation["activated"], true);
    assert_eq!(
        activation["passed_tags"],
        json!([activation["logical_tag"]])
    );
    let logical_tag = activation["logical_tag"].as_str().unwrap();
    let parent = draft
        .display_rows()
        .iter()
        .position(|row| row.tag == logical_tag && row.parent_row)
        .unwrap();
    draft.toggle_row(parent);
    assert!(draft.has_changes());
    assert_eq!(
        rows(&ManageTags::new(store.clone(), files.clone()).unwrap()),
        fixture::canonical(recorded["cases"][15]["rows"].clone()),
        "unapplied parent-row activation must preserve saved mappings"
    );
    draft.apply().unwrap();
    let after = ManageTags::new(store.clone(), files).unwrap();
    let expected_parent = after
        .display_rows()
        .iter()
        .find(|row| row.tag == logical_tag && !row.parent_row)
        .unwrap()
        .label
        .clone();
    let recorded_label = activation["rows_after_activation"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["tag"] == logical_tag)
        .unwrap()["rows"][0]
        .as_str()
        .unwrap();
    assert_eq!(expected_parent, recorded_label);
    assert_eq!(
        rows(&after),
        fixture::canonical(activation["rows_after_activation"].clone())
    );
    assert!(
        !after
            .display_rows()
            .iter()
            .any(|row| !row.parent_row
                && matches!(row.tag.as_str(), "category:colour" | "series:root")),
        "activating an implied row edits its originating child, not the implied tag"
    );
}
