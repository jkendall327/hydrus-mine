//! Real percent constructor/Apply semantics and independent concurrent saves.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    animation_start::{self, Preferences},
    settings,
};

fn editor(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media playback")
        .unwrap();
    editor.show_page(page);
    let row = editor
        .rows()
        .iter()
        .position(
            |row| matches!(row,Row::Opt{option,..} if option.label=="Start animations this % in:"),
        )
        .unwrap();
    (editor, row)
}
#[test]
#[allow(clippy::float_cmp)]
fn recorded_raw_constructor_cancel_unchanged_apply_reopen_and_scoped_merge() {
    let fixture = hydrus_testkit::fixture_json("animation_start.json");
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    for case in fixture["controls"].as_array().unwrap() {
        let raw = Preferences {
            fraction: case["raw"].as_f64().unwrap(),
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &raw))
            .unwrap();
        let (mut draft, row) = editor(&store);
        let displayed = match draft.rows()[row] {
            Row::Opt {
                value: hydrus_gui_model::options::Value::Int(value),
                ..
            } => *value,
            _ => panic!("integer percent expected"),
        };
        assert_eq!(displayed, case["displayed"].as_i64().unwrap());
        if let Some(value) = case["typed"].as_i64() {
            draft.number(row, value);
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
            store.read(animation_start::load).unwrap().fraction,
            case["saved"].as_f64().unwrap()
        );
        assert_eq!(
            Store::open(directory.path())
                .unwrap()
                .read(animation_start::load)
                .unwrap()
                .percent(),
            case["reopened"].as_u64().unwrap() as u8
        );
    }
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { fraction: 0.119 }))
        .unwrap();
    let (draft, _) = editor(&store);
    store
        .write(|ctx| settings::set(ctx.conn(), &Preferences { fraction: 0.72 }))
        .unwrap();
    let (after, before, errors) = draft.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(animation_start::load).unwrap().percent(),
        72,
        "implicit normalization cannot overwrite newer policy"
    );
    let (mut draft, row) = editor(&store);
    draft.number(row, 35);
    store
        .write(|ctx| {
            let mut playback: settings::ViewerPlaybackSettings = settings::get(ctx.conn())?;
            playback.always_loop = false;
            settings::set(ctx.conn(), &playback)
        })
        .unwrap();
    let (after, before, errors) = draft.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(store.read(animation_start::load).unwrap().percent(), 35);
    assert!(
        !store
            .read(settings::get::<settings::ViewerPlaybackSettings>)
            .unwrap()
            .always_loop
    );
}
