//! Reference ImportingPanel drafts, bounds, apply/cancel and persistence.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    settings::{self, ImportWorkSlots},
};
use serde_json::{Value, json};
const LABELS: [&str; 5] = [
    "Number of gallery downloader file queues that can import at the same time:",
    "Number of gallery downloader searches that can run at the same time:",
    "Number of watcher page file queues that can run at the same time:",
    "Number of watcher page checkers that can run at the same time:",
    "Number of other paged importer jobs that can run at the same time:",
];
fn row(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(|row| matches!(row, Row::Opt{option,..} if option.label==label))
        .unwrap()
}
#[test]
fn reference_controls_stay_staged_and_saved_limits_survive_reopening() {
    let fixture = hydrus_testkit::fixture_json("import_work_slots.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        json!(store.read(settings::get::<ImportWorkSlots>).unwrap()),
        fixture["defaults"]
    );
    for event in fixture["events"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        assert_eq!(json!(before.import_work_slots), event["before"]);
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "importing")
            .unwrap();
        editor.show_page(page);
        for (label, value) in LABELS.iter().zip(event["input"].as_array().unwrap()) {
            editor.number(row(&editor, label), value.as_i64().unwrap());
        }
        assert_eq!(store.read(Settings::load).unwrap(), before);
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty());
        assert_eq!(json!(after.import_work_slots), event["draft"]);
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            json!(
                Store::open(dir.path())
                    .unwrap()
                    .read(settings::get::<ImportWorkSlots>)
                    .unwrap()
            ),
            event["reopened"]
        );
    }
    let before = store.read(Settings::load).unwrap();
    let mut cancelled = Editor::new(before.clone());
    let page = cancelled
        .page_names()
        .iter()
        .position(|name| *name == "importing")
        .unwrap();
    cancelled.show_page(page);
    for label in LABELS {
        cancelled.number(row(&cancelled, label), 1);
    }
    drop(cancelled);
    assert_eq!(store.read(Settings::load).unwrap(), before);
    assert_eq!(json!(before.import_work_slots), fixture["cancel_after"]);
}
#[test]
fn loaded_out_of_range_controls_normalize_only_on_unchanged_apply() {
    let fixture = hydrus_testkit::fixture_json("import_work_slots.json");
    let boundary = &fixture["loaded_boundaries"];
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let raw: ImportWorkSlots = serde_json::from_value(boundary["before"].clone()).unwrap();
    store
        .write(move |ctx| settings::set(ctx.conn(), &raw))
        .unwrap();
    let before = store.read(Settings::load).unwrap();
    let editor = Editor::new(before.clone());
    assert_eq!(
        json!(store.read(settings::get::<ImportWorkSlots>).unwrap()),
        boundary["before"]
    );
    let (after, _, errors) = editor.applied();
    assert!(errors.is_empty());
    assert_eq!(json!(after.import_work_slots), boundary["saved"]);
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        json!(store.read(settings::get::<ImportWorkSlots>).unwrap()),
        boundary["saved"]
    );
}

#[test]
fn missing_and_raw_legacy_keys_preserve_independent_defaults() {
    let defaults: ImportWorkSlots = serde_json::from_value(json!({})).unwrap();
    assert_eq!(defaults, ImportWorkSlots::default());
    let mut migrated = defaults;
    migrated.apply_legacy(
        &[
            ("thread_slots_gallery_files".into(), 0),
            ("thread_slots_watcher_check".into(), 900),
            ("thread_slots_misc".into(), 7),
        ]
        .into(),
    );
    assert_eq!(
        json!(migrated),
        json!({"gallery_files":0,"gallery_search":5,"watcher_files":15,"watcher_check":900,"misc":7})
    );
    let fixture: Value = hydrus_testkit::fixture_json("import_work_slots.json");
    assert_eq!(json!(ImportWorkSlots::default()), fixture["defaults"]);
}
