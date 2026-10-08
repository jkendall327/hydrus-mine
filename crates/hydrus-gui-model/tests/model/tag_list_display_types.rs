//! Recorded opening defaults and the staged four-choice option values.
use super::tag_dialog_preferences::fixture;
use hydrus_core::tag_presentation::{TagDisplayType, TagPresentation};
use hydrus_gui_model::options::{Editor, Kind, Row, Settings, Value};
use hydrus_store::settings;
use serde_json::json;

const LABELS: [&str; 2] = [
    "Tag display type for new page sidebar taglists: ",
    "Tag display type for new media viewer taglists: ",
];
fn editor(store: &hydrus_store::Store) -> (Editor, [usize; 2]) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "tag presentation")
        .unwrap();
    editor.show_page(page);
    let rows = LABELS.map(|label| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Opt {option,..} if option.label == label))
            .unwrap()
    });
    (editor, rows)
}
fn values(settings: &TagPresentation) -> serde_json::Value {
    json!({"sidebar":settings.sidebar_display_type.code(),"viewer":settings.viewer_display_type.code()})
}
// leaf: audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-media-viewer-taglists
// leaf: audit-options-tag-presentation-default-taglist-display-type-advanced-tag-display-type-for-new-page-sidebar-taglists
#[test]
fn exact_choices_stage_cancel_apply_reopen_and_preserve_old_payload_defaults() {
    let recorded = hydrus_testkit::fixture_json("tag_list_display_types.json");
    let (_directory, store, _files) = fixture::seed(&recorded);
    let before: TagPresentation = store.read(settings::get).unwrap();
    assert_eq!(values(&before), recorded["options"]["shown"]);
    let mut old = serde_json::to_value(&before).unwrap();
    for field in ["sidebar_display_type", "viewer_display_type"] {
        old.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(
        serde_json::from_value::<TagPresentation>(old).unwrap(),
        before
    );
    let (mut cancelled, rows) = editor(&store);
    for (row, field) in rows.into_iter().zip(["sidebar", "viewer"]) {
        let state = cancelled.rows();
        let Row::Opt { option, value, .. } = &state[row] else {
            panic!("missing display choice")
        };
        let Kind::Choice(labels) = option.kind else {
            panic!("not a choice")
        };
        let Value::Choice(selected) = value else {
            panic!("not a selected index")
        };
        assert_eq!(
            json!(
                labels
                    .iter()
                    .zip(TagDisplayType::CHOICES)
                    .map(|(label, mode)| (*label, mode.code()))
                    .collect::<Vec<_>>()
            ),
            recorded["options"]["choices"][field]
        );
        assert_eq!(
            TagDisplayType::CHOICES[*selected].code(),
            recorded["options"]["shown"][field]
        );
        cancelled.choose(row, TagDisplayType::Storage.choice());
    }
    assert_eq!(
        store.read::<TagPresentation>(settings::get).unwrap(),
        before
    );
    drop(cancelled);
    assert_eq!(
        values(&store.read(settings::get).unwrap()),
        recorded["options"]["cancelled"]
    );
    for (sidebar, viewer) in TagDisplayType::CHOICES
        .into_iter()
        .flat_map(|sidebar| TagDisplayType::CHOICES.map(|viewer| (sidebar, viewer)))
    {
        let (mut edit, rows) = editor(&store);
        edit.choose(rows[0], sidebar.choice());
        edit.choose(rows[1], viewer.choice());
        let prior: TagPresentation = store.read(settings::get).unwrap();
        assert_eq!(
            values(&edit.applied().0.tag_presentation),
            json!({"sidebar":sidebar.code(),"viewer":viewer.code()})
        );
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), prior);
        let (accepted, original, errors) = edit.applied();
        assert!(errors.is_empty());
        let original = original.clone();
        store
            .write(move |ctx| accepted.save(ctx.conn(), &original))
            .unwrap();
        assert_eq!(
            store.read::<TagPresentation>(settings::get).unwrap(),
            TagPresentation {
                sidebar_display_type: sidebar,
                viewer_display_type: viewer,
                ..prior
            }
        );
        let (opened, rows) = editor(&store);
        for (row, mode) in rows.into_iter().zip([sidebar, viewer]) {
            assert!(
                matches!(opened.rows()[row], Row::Opt {value:Value::Choice(index),..} if *index == mode.choice())
            );
        }
    }
    assert_eq!(TagDisplayType::from_choice(usize::MAX), None);
}
