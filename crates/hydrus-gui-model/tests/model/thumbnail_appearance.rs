//! Staged Qt appearance controls, persistence and field-scoped concurrency.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{Store, settings, thumbnail_appearance::Preferences};

fn editor(store: &Store) -> Editor {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "thumbnails")
        .unwrap();
    editor.show_page(page);
    editor
}
fn row(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(|row| matches!(row, Row::Opt { option, .. } if option.label.trim() == label))
        .unwrap()
}
// leaf: audit-options-thumbnails-appearance-use-blurhash-missing-thumbnail-fallback
// leaf: audit-options-thumbnails-media-background-experimental-image-path-for-thumbnail-panel-background-image-set-blank-to-clear
#[test]
fn actual_defaults_cancel_raw_paths_reopen_and_current_field_merge() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_appearance.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    let saved: Preferences = store.read(settings::get).unwrap();
    assert_eq!(saved.fade, fixture["defaults"]["fade"].as_bool().unwrap());
    assert_eq!(
        saved.blurhash,
        fixture["defaults"]["blurhash"].as_bool().unwrap()
    );
    assert_eq!(
        saved.new_renderer,
        fixture["new_renderer_default"].as_bool().unwrap()
    );
    let mut cancelled = editor(&store);
    let fade = row(&cancelled, "Fade thumbnails:");
    cancelled.check(fade, !saved.fade);
    drop(cancelled);
    assert_eq!(store.read(settings::get::<Preferences>).unwrap(), saved);
    for case in fixture["cases"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        let mut draft = editor(&store);
        for (label, index) in [
            ("Fade thumbnails:", 0),
            ("Use blurhash missing thumbnail fallback:", 1),
        ] {
            let row = row(&draft, label);
            draft.check(row, case["input"][index].as_bool().unwrap());
        }
        let path = row(
            &draft,
            "EXPERIMENTAL: Image path for thumbnail panel background image (set blank to clear):",
        );
        draft.text(path, case["input"][2].as_str().unwrap());
        let (after, _, errors) = draft.applied();
        assert!(errors.is_empty());
        assert_eq!(
            serde_json::json!([
                after.thumbnail_appearance.fade,
                after.thumbnail_appearance.blurhash,
                after.thumbnail_appearance.background
            ]),
            serde_json::json!([
                case["saved"]["fade"],
                case["saved"]["blurhash"],
                case["saved"]["background"]
            ])
        );
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        let reopened = Store::open(directory.path()).unwrap();
        assert_eq!(
            reopened.read(settings::get::<Preferences>).unwrap(),
            store.read(settings::get).unwrap()
        );
    }
    let raw = Preferences {
        background: Some(String::new()),
        ..saved
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    let draft = editor(&store);
    let before = store.read(Settings::load).unwrap();
    let (after, _, errors) = draft.applied();
    assert!(errors.is_empty());
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(settings::get::<Preferences>).unwrap().background,
        None
    );
    let before = store.read(Settings::load).unwrap();
    let expected_blurhash = !before.thumbnail_appearance.blurhash;
    let mut draft = editor(&store);
    let index = row(&draft, "Fade thumbnails:");
    draft.check(index, !before.thumbnail_appearance.fade);
    store
        .write(|ctx| {
            let mut current: Preferences = settings::get(ctx.conn())?;
            current.background = Some("concurrent.png".into());
            current.blurhash = !current.blurhash;
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    let (after, _, errors) = draft.applied();
    assert!(errors.is_empty());
    let expected = after.thumbnail_appearance.fade;
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let current: Preferences = store.read(settings::get).unwrap();
    assert_eq!(current.fade, expected);
    assert_eq!(current.blurhash, expected_blurhash);
    assert_eq!(current.background.as_deref(), Some("concurrent.png"));

    let raw = Preferences {
        background: Some(String::new()),
        ..current
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    let before = store.read(Settings::load).unwrap();
    let draft = editor(&store);
    let (after, _, errors) = draft.applied();
    assert!(errors.is_empty());
    store
        .write(|ctx| {
            let mut current: Preferences = settings::get(ctx.conn())?;
            current.background = Some("replaced.png".into());
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store
            .read(settings::get::<Preferences>)
            .unwrap()
            .background
            .as_deref(),
        Some("replaced.png")
    );
}
