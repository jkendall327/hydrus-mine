//! Actual Qt bounds/staging with changed-only concurrent saves.
use hydrus_gui_model::options::{Editor, Row, Settings, Value};
use hydrus_store::{
    Store,
    ffmpeg_policy::{self, FfmpegPolicy},
    settings,
};
const LABEL: &str = "FFMPEG call timeout:";
fn editor(store: &Store) -> (Editor, usize) {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|p| *p == "media playback")
        .unwrap();
    editor.show_page(page);
    let row = editor
        .rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..} if option.label==LABEL))
        .unwrap();
    assert!(
        matches!(&editor.rows()[row+1],Row::Opt{option,..} if option.label=="Apply image ICC Profile colour adjustments:")
    );
    (editor, row)
}
fn apply(editor: &Editor, store: &Store) {
    let (after, before, errors) = editor.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store.write(move |c| after.save(c.conn(), &before)).unwrap();
}
#[test]
fn qt_options_cancel_bounds_reopen_and_concurrent_changes() {
    let fixture = hydrus_testkit::fixture_json("ffmpeg_timeout.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        store.read(ffmpeg_policy::load).unwrap().seconds,
        fixture["default"].as_i64().unwrap()
    );
    let (mut cancel, row) = editor(&store);
    cancel.number(row, 7);
    drop(cancel);
    assert_eq!(
        store.read(ffmpeg_policy::load).unwrap().seconds,
        fixture["cancel"]["saved"]
    );
    for case in fixture["cases"].as_array().unwrap() {
        let (mut draft, row) = editor(&store);
        let before = store.read(ffmpeg_policy::load).unwrap();
        draft.number(row, case["requested"].as_i64().unwrap());
        assert_eq!(store.read(ffmpeg_policy::load).unwrap(), before);
        store
            .write(|c| {
                settings::set(
                    c.conn(),
                    &hydrus_store::image_colour::ImageColour {
                        normalise_icc: false,
                    },
                )
            })
            .unwrap();
        apply(&draft, &store);
        assert_eq!(
            store.read(ffmpeg_policy::load).unwrap().seconds,
            case["saved"]
        );
        assert!(
            !store
                .read(hydrus_store::image_colour::load)
                .unwrap()
                .normalise_icc
        );
        assert_eq!(
            Store::open(dir.path())
                .unwrap()
                .read(ffmpeg_policy::load)
                .unwrap()
                .seconds,
            case["reopened"]
        );
    }
    for raw in fixture["raw"].as_array().unwrap() {
        let imported = raw["imported"].as_i64().unwrap();
        store
            .write(move |c| settings::set(c.conn(), &FfmpegPolicy { seconds: imported }))
            .unwrap();
        let (draft, row) = editor(&store);
        assert!(
            matches!(&draft.rows()[row],Row::Opt{value:Value::Int(shown),..} if *shown==raw["shown"].as_i64().unwrap())
        );
        assert_eq!(store.read(ffmpeg_policy::load).unwrap().seconds, imported);
        apply(&draft, &store);
        assert_eq!(
            store.read(ffmpeg_policy::load).unwrap().seconds,
            raw["saved"]
        );
    }
    let (unchanged, _) = editor(&store);
    store
        .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 23 }))
        .unwrap();
    apply(&unchanged, &store);
    assert_eq!(store.read(ffmpeg_policy::load).unwrap().seconds, 23);
    store
        .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 0 }))
        .unwrap();
    let (implicit, _) = editor(&store);
    store
        .write(|c| settings::set(c.conn(), &FfmpegPolicy { seconds: 24 }))
        .unwrap();
    apply(&implicit, &store);
    assert_eq!(store.read(ffmpeg_policy::load).unwrap().seconds, 24);
}
