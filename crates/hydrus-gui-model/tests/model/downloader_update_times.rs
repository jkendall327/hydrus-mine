//! Real Qt control normalization and current-row-count strict deadline histories.
use hydrus_gui_model::{
    downloader_update_times::{Deadline, normalised},
    options::{Editor, Row, Settings, Value},
};
use hydrus_store::{Store, downloader_update_times::Preferences, settings};
fn raw(p: &Preferences) -> serde_json::Value {
    serde_json::json!([
        p.gallery_minimum_ms,
        p.gallery_denominator,
        p.watcher_minimum_ms,
        p.watcher_denominator
    ])
}
fn preferences(value: &serde_json::Value) -> Preferences {
    Preferences {
        gallery_minimum_ms: value[0].as_i64().unwrap(),
        gallery_denominator: value[1].as_i64().unwrap(),
        watcher_minimum_ms: value[2].as_i64().unwrap(),
        watcher_denominator: value[3].as_i64().unwrap(),
    }
}
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-gallery-importer-magic-update-time-denominator
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-minimum-gallery-importer-update-time
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-minimum-watcher-importer-update-time
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-watcher-importer-magic-update-time-denominator
#[test]
fn exact_constructor_fields_unchanged_apply_cancel_legacy_and_concurrent_field_merge() {
    let fixture = hydrus_testkit::fixture_json("downloader_update_times.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(
        raw(&store.read(settings::get).unwrap()),
        fixture["defaults"]["raw"]
    );
    for case in fixture["controls"].as_array().unwrap() {
        let imported = preferences(&case["imported"]);
        store
            .write(move |ctx| settings::set(ctx.conn(), &imported))
            .unwrap();
        let before = store.read(Settings::load).unwrap();
        let mut draft = Editor::new(before.clone());
        let page = draft
            .page_names()
            .iter()
            .position(|name| *name == "speed and memory")
            .unwrap();
        draft.show_page(page);
        let rows = draft.rows();
        for (index, label) in fixture["labels"].as_array().unwrap().iter().enumerate() {
            let Row::Opt{value,enabled,..}=rows.iter().find(|row|matches!(row,Row::Opt{option,..} if option.label==label.as_str().unwrap())).unwrap() else { panic!("control"); };
            assert!(*enabled);
            let displayed = match value {
                Value::Duration(number) => serde_json::json!(number),
                Value::Int(number) => serde_json::json!(number),
                _ => panic!("expected time/integer"),
            };
            assert_eq!(displayed, case["displayed"][index]["value"]);
        }
        let (after, _, errors) = draft.applied();
        assert!(errors.is_empty());
        assert_eq!(
            raw(&after.downloader_update_times),
            case["saved"],
            "unchanged Apply accepts displayed bounds"
        );
        assert_eq!(
            store.read(Settings::load).unwrap(),
            before,
            "cancel/drop cannot normalize raw imports"
        );
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        let reopened = Store::open(directory.path()).unwrap();
        assert_eq!(raw(&reopened.read(settings::get).unwrap()), case["saved"]);
    }
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &fixture["legacy_options"].to_string(),
    )
    .unwrap();
    let legacy = hydrus_legacy::objects::ClientOptions::from_object(&object).unwrap();
    let mut decoded = Preferences::default();
    decoded.apply_legacy(&legacy.integers);
    assert_eq!(
        (
            decoded.gallery_minimum_ms,
            decoded.gallery_denominator,
            decoded.watcher_minimum_ms,
            decoded.watcher_denominator
        ),
        (2000, 99, 2000, 99)
    );
    let before = Preferences {
        gallery_minimum_ms: 119,
        gallery_denominator: 0,
        ..Preferences::default()
    };
    let mut accepted = normalised(&before);
    accepted.watcher_denominator = 7;
    let concurrent = Preferences {
        gallery_minimum_ms: 800,
        gallery_denominator: 5,
        ..before.clone()
    };
    let saved_concurrent = concurrent.clone();
    store
        .write(move |ctx| {
            settings::set(ctx.conn(), &saved_concurrent)?;
            accepted.save_changed(ctx.conn(), &before, &normalised(&before))
        })
        .unwrap();
    assert_eq!(
        store.read(settings::get::<Preferences>).unwrap(),
        Preferences {
            watcher_denominator: 7,
            ..concurrent
        },
        "implicit normalization must not clobber a concurrent edit"
    );
    assert_eq!(
        serde_json::from_str::<Preferences>("{}").unwrap(),
        Preferences::default()
    );
}
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-gallery-importer-magic-update-time-denominator
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-minimum-gallery-importer-update-time
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-minimum-watcher-importer-update-time
// leaf: audit-options-speed-and-memory-download-pages-update-experimental-watcher-importer-magic-update-time-denominator
#[test]
fn strict_deadlines_ratios_minimum_fallback_saved_changes_and_forcing_match_qt() {
    let fixture = hydrus_testkit::fixture_json("downloader_update_times.json");
    for case in fixture["schedules"].as_array().unwrap() {
        let mut deadline = Deadline::default();
        // Establish the recorder's initial 100s deadline through its actual formula.
        assert!(deadline.advance(99.0, 1000, 30, 0));
        let mut minimum = case["minimum_ms"].as_i64().unwrap();
        let mut denominator = case["denominator"].as_i64().unwrap();
        for (index, event) in case["events"].as_array().unwrap().iter().enumerate() {
            if index == 4 {
                minimum = 2000;
                denominator = 99;
            }
            let at = event["at"].as_f64().unwrap();
            assert_eq!(serde_json::json!(deadline.next()), event["before"]);
            if event["action"] == "force" {
                deadline.force();
            }
            let advanced = deadline.advance(
                at,
                minimum,
                denominator,
                case["items"].as_u64().unwrap() as usize,
            );
            assert_eq!(advanced, event["before"] != event["after"], "{case}");
            assert_eq!(serde_json::json!(deadline.next()), event["after"], "{case}");
        }
    }
}
