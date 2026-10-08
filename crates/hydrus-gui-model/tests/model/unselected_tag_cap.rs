//! Real options bounds and transactional nullable/zero tag computation preferences.
use super::tag_dialog_preferences::fixture;
use hydrus_core::tag_presentation::TagPresentation;
use hydrus_gui_model::options::{Editor, Kind, Row, Settings, Value};
use hydrus_store::settings;

const LABEL: &str = "Max number of thumbnails to compute tags for when none are selected: ";

// leaf: audit-options-tag-presentation-selection-tags-max-number-of-thumbnails-to-compute-tags-for-when-none-are-selected
#[test]
fn recorded_bounds_none_zero_cancel_and_saved_preferences() {
    let recorded = hydrus_testkit::fixture_json("unselected_tag_cap.json");
    let (_directory, store, _files) = fixture::seed(&recorded);
    let mut old = serde_json::to_value(TagPresentation::default()).unwrap();
    old.as_object_mut().unwrap().remove("unselected_tag_limit");
    assert_eq!(
        serde_json::from_value::<TagPresentation>(old)
            .unwrap()
            .unselected_tag_limit,
        Some(4096)
    );
    let before: TagPresentation = store.read(settings::get).unwrap();
    assert_eq!(
        serde_json::json!(before.unselected_tag_limit),
        recorded["options"]["shown"]
    );
    let make = || {
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
        let rows = editor.rows();
        let Row::Opt { option, .. } = &rows[row] else {
            panic!("missing cap row")
        };
        let Kind::Noneable {
            none_phrase,
            default,
            min,
            max,
            ..
        } = &option.kind
        else {
            panic!("cap must be nullable")
        };
        assert_eq!(
            *none_phrase,
            recorded["options"]["none_phrase"].as_str().unwrap()
        );
        assert_eq!(*default, recorded["options"]["shown"].as_i64().unwrap());
        assert_eq!(*min, recorded["options"]["min"].as_i64().unwrap());
        assert_eq!(*max, recorded["options"]["max"].as_i64().unwrap());
        (editor, row)
    };
    let (mut cancelled, row) = make();
    cancelled.number(row, 1);
    assert_eq!(
        store.read::<TagPresentation>(settings::get).unwrap(),
        before
    );
    drop(cancelled);
    assert_eq!(
        serde_json::json!(
            store
                .read::<TagPresentation>(settings::get)
                .unwrap()
                .unselected_tag_limit
        ),
        recorded["options"]["cancelled"]
    );
    for limit in [None, Some(0), Some(1), Some(10_000_000)] {
        let (mut editor, row) = make();
        editor.none(row, limit.is_none());
        if let Some(limit) = limit {
            editor.number(row, limit);
        }
        let prior: TagPresentation = store.read(settings::get).unwrap();
        assert_eq!(
            editor.applied().0.tag_presentation.unselected_tag_limit,
            limit.map(|limit| limit as u32)
        );
        assert_eq!(store.read::<TagPresentation>(settings::get).unwrap(), prior);
        let (accepted, original, problems) = editor.applied();
        assert!(problems.is_empty());
        let original = original.clone();
        store
            .write(move |ctx| accepted.save(ctx.conn(), &original))
            .unwrap();
        assert_eq!(
            store.read::<TagPresentation>(settings::get).unwrap(),
            TagPresentation {
                unselected_tag_limit: limit.map(|limit| limit as u32),
                ..prior
            }
        );
        let (reopened, row) = make();
        assert!(
            matches!(reopened.rows()[row],Row::Opt{value:Value::Noneable(value),..} if *value==limit)
        );
    }
}
