//! Actual Qt choices, preview-specific geometry, staged persistence and legacy precedence.
use hydrus_core::{
    Mime,
    media_viewer::{MediaViewerSettings, ZoomType, canvas_zooms},
};
use hydrus_gui_model::{
    options::{Editor, Row, Settings, Value},
    preview_zoom,
};
use hydrus_store::{
    Store,
    preview_zoom::{self as saved, Settings as Preview},
    settings,
};
const LABEL: &str = "Preview Viewer default zoom:";
fn store() -> (tempfile::TempDir, std::sync::Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    (directory, store)
}
fn editor(store: &Store) -> Editor {
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media playback")
        .unwrap();
    editor.show_page(page);
    editor
}
fn row(editor: &Editor) -> usize {
    editor
        .rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..}if option.label==LABEL))
        .unwrap()
}
// leaf: audit-options-media-playback-zoom-and-position-preview-viewer-default-zoom
#[test]
fn actual_qt_choices_staged_cancel_save_reopen_and_independent_viewer_policy() {
    let qt = hydrus_testkit::fixture_json("preview_default_zoom.json");
    let (_directory, store) = store();
    assert_eq!(
        store.read(saved::load).unwrap().default_zoom as i64,
        qt["default"]
    );
    for (index, control) in qt["controls"].as_array().unwrap().iter().enumerate() {
        let mut draft = editor(&store);
        let before = store.read(saved::load).unwrap();
        let rows = draft.rows();
        let Row::Opt {
            option,
            value: Value::Choice(_),
            enabled,
            ..
        } = &rows[row(&draft)]
        else {
            panic!("preview choice");
        };
        assert!(*enabled);
        if let hydrus_gui_model::options::Kind::Choice(choices) = &option.kind {
            assert_eq!(
                serde_json::json!(choices),
                serde_json::json!(
                    qt["choices"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| c["label"].as_str().unwrap())
                        .collect::<Vec<_>>()
                )
            );
        } else {
            panic!("choice topology");
        }
        draft.choose(row(&draft), index);
        assert_eq!(
            store.read(saved::load).unwrap(),
            before,
            "draft/Cancel is not a write"
        );
        let (after, original, errors) = draft.applied();
        assert!(errors.is_empty());
        assert_eq!(after.preview_zoom.default_zoom as i64, control["saved"]);
        let original = original.clone();
        // A separate viewer setting changed after opening must survive.
        store
            .write(|ctx| {
                let mut viewer: MediaViewerSettings = settings::get(ctx.conn())?;
                viewer.default_zoom_type = ZoomType::FillY;
                settings::set(ctx.conn(), &viewer)
            })
            .unwrap();
        store
            .write(move |ctx| after.save(ctx.conn(), &original))
            .unwrap();
        assert_eq!(
            store.read(saved::load).unwrap().default_zoom as i64,
            control["reopened"]
        );
        assert_eq!(
            store
                .read(settings::get::<MediaViewerSettings>)
                .unwrap()
                .default_zoom_type,
            ZoomType::FillY
        );
    }
    let unchanged = editor(&store);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preview {
                    default_zoom: ZoomType::Full,
                },
            )
        })
        .unwrap();
    let (after, before, errors) = unchanged.applied();
    assert!(errors.is_empty());
    let before = before.clone();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(saved::load).unwrap().default_zoom,
        ZoomType::Full,
        "untouched preview choice preserves concurrent change"
    );
}
// leaf: audit-options-media-playback-zoom-and-position-preview-viewer-default-zoom
#[test]
fn exact_real_preview_geometry_uses_preview_rules_and_keeps_full_viewer_zooms() {
    let qt = hydrus_testkit::fixture_json("preview_default_zoom.json");
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &qt["legacy_options"].to_string(),
    )
    .unwrap();
    let legacy = hydrus_legacy::objects::ClientOptions::from_object(&object).unwrap();
    let settings = legacy.media_viewer_settings();
    let mime = Mime::from_code(qt["mime"].as_u64().unwrap() as u8).unwrap();
    for case in qt["cases"].as_array().unwrap() {
        let canvas: [u32; 2] = serde_json::from_value(case["canvas"].clone()).unwrap();
        let resolution: [u32; 2] = serde_json::from_value(case["resolution"].clone()).unwrap();
        let code = ZoomType::from_code(case["code"].as_i64().unwrap()).unwrap();
        let rect = preview_zoom::rect(
            &settings,
            code,
            mime,
            Some((resolution[0], resolution[1])),
            (canvas[0], canvas[1]),
            case["dpr"].as_f64().unwrap(),
        );
        assert_eq!(
            serde_json::json!([rect.0, rect.1, rect.2, rect.3]),
            case["rect"],
            "{case}"
        );
        let full = canvas_zooms(
            &settings,
            mime,
            Some((resolution[0], resolution[1])),
            (canvas[0], canvas[1]),
            case["dpr"].as_f64().unwrap(),
        );
        assert_eq!(
            full[&ZoomType::DefaultForFiletype].to_bits(),
            1.0_f64.to_bits(),
            "full viewer rules remain independent"
        );
    }
    for case in qt["dpi_cases"].as_array().unwrap() {
        let canvas: [u32; 2] = serde_json::from_value(case["canvas"].clone()).unwrap();
        let resolution: [u32; 2] = serde_json::from_value(case["resolution"].clone()).unwrap();
        let code = ZoomType::from_code(case["code"].as_i64().unwrap()).unwrap();
        let rect = preview_zoom::rect(
            &settings,
            code,
            mime,
            Some((resolution[0], resolution[1])),
            (canvas[0], canvas[1]),
            case["dpr"].as_f64().unwrap(),
        );
        assert_eq!(
            serde_json::json!([rect.2, rect.3]),
            case["size"],
            "DPR2: {case}"
        );
    }
}
#[test]
fn retained_legacy_fallback_and_native_wins_survive_store_reopen() {
    let (directory, store) = store();
    let qt = hydrus_testkit::fixture_json("preview_default_zoom.json");
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &qt["legacy_options"].to_string(),
    )
    .unwrap();
    let legacy = hydrus_legacy::objects::ClientOptions::from_object(&object).unwrap();
    assert_eq!(Preview::from_legacy(&legacy).default_zoom, ZoomType::Canvas);
    store
        .write(|ctx| {
            let kind = u32::from(hydrus_legacy::serialisable::SerialisableType::CLIENT_OPTIONS.0);
            let (_, dump) = hydrus_store::legacy::singleton(ctx.conn(), kind)?.unwrap();
            let old = r#"[[0, "preview_default_zoom_type_override"], [0, 0]]"#;
            assert!(dump.contains(old));
            let changed = dump.replace(
                old,
                r#"[[0, "preview_default_zoom_type_override"], [0, 3]]"#,
            );
            ctx.conn().execute(
                "UPDATE legacy_objects SET dump=?1 WHERE source='json_dumps' AND type_id=?2",
                rusqlite::params![changed, kind],
            )?;
            ctx.conn()
                .execute("DELETE FROM settings WHERE key='preview_default_zoom'", [])?;
            Ok(())
        })
        .unwrap();
    assert_eq!(
        store.read(saved::load).unwrap().default_zoom,
        ZoomType::FillX
    );
    drop(store);
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(
        store.read(saved::load).unwrap().default_zoom,
        ZoomType::FillX
    );
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &Preview {
                    default_zoom: ZoomType::Full,
                },
            )
        })
        .unwrap();
    assert_eq!(
        store.read(saved::load).unwrap().default_zoom,
        ZoomType::Full
    );
}
