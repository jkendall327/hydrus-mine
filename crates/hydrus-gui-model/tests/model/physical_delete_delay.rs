//! Actual staged fields, raw constructor normalization and concurrent merge.
use hydrus_gui_model::options::{Editor, Row, Settings, Unit, Value, duration_fields};
use hydrus_store::{
    Store,
    physical_delete::{self, Preferences},
    settings,
};
const LABEL: &str =
    "When maintenance physically deletes files, wait this long between each delete: ";
fn editor(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "files and trash")
        .unwrap();
    editor.show_page(page);
    let row = editor
        .rows()
        .iter()
        .position(|row| matches!(row,Row::Opt {option,..} if option.label==LABEL))
        .unwrap();
    (editor, row)
}
#[test]
fn real_constructor_cancel_acceptance_reopen_fields_and_concurrent_policy_replacement() {
    let fixture = hydrus_testkit::fixture_json("physical_delete_delay.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    for case in fixture["controls"].as_array().unwrap() {
        let raw = Preferences {
            wait_ms: case["raw"].as_i64().unwrap(),
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &raw))
            .unwrap();
        let (mut draft, row) = editor(&store);
        let seconds = match draft.rows()[row] {
            Row::Opt {
                value: Value::Duration(seconds),
                ..
            } => *seconds,
            _ => panic!("duration row"),
        };
        assert_eq!(
            duration_fields(seconds, &[Unit::Seconds, Unit::Milliseconds]),
            case["before"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect::<Vec<_>>()
        );
        if case["typed"].is_array() {
            for (field, value) in case["typed"].as_array().unwrap().iter().enumerate() {
                draft.field(row, field, value.as_i64().unwrap());
            }
        }
        if case["apply"].as_bool().unwrap() {
            let (after, before, errors) = draft.applied();
            assert!(errors.is_empty());
            let before = before.clone();
            store
                .write(move |ctx| after.save(ctx.conn(), &before))
                .unwrap();
        }
        assert_eq!(
            store.read(physical_delete::load).unwrap().wait_ms,
            case["saved"].as_i64().unwrap(),
            "{case}"
        );
        let (draft, row) = editor(&Store::open(directory.path()).unwrap());
        let seconds = match draft.rows()[row] {
            Row::Opt {
                value: Value::Duration(seconds),
                ..
            } => *seconds,
            _ => panic!("duration"),
        };
        assert_eq!(
            duration_fields(seconds, &[Unit::Seconds, Unit::Milliseconds]),
            case["reopened"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect::<Vec<_>>()
        );
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { wait_ms: 19 }))
        .unwrap();
    let (draft, _) = editor(&store);
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { wait_ms: 900 }))
        .unwrap();
    let (after, before, errors) = draft.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 900);
    let (mut draft, row) = editor(&store);
    draft.field(row, 1, 45);
    store
        .write(|ctx| {
            let mut folders: settings::FolderSettings = settings::get(ctx.conn())?;
            folders.delete_to_recycle_bin = false;
            settings::set(ctx.conn(), &folders)
        })
        .unwrap();
    let (after, before, errors) = draft.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(store.read(physical_delete::load).unwrap().wait_ms, 45);
    assert!(
        !store
            .read(settings::get::<settings::FolderSettings>)
            .unwrap()
            .delete_to_recycle_bin
    );
}
