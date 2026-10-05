//! PopupPanel staging and persistence, replayed from the real Qt controls.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{Store, popup_width::PopupWidth, settings};
use serde_json::{Value, json};

const WIDTH: &str = "Approximate max width of popup messages (in characters): ";
const FIXED: &str = "BUGFIX: Force this width as the fixed width for all popup messages: ";
fn value(p: &PopupWidth) -> Value {
    json!({"characters":p.characters,"fixed":p.fixed})
}
fn row(e: &Editor, label: &str) -> usize {
    e.rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..} if option.label==label))
        .unwrap()
}
fn editor(s: Settings) -> Editor {
    let mut e = Editor::new(s);
    let page = e
        .page_names()
        .iter()
        .position(|p| *p == "popup notifications")
        .unwrap();
    e.show_page(page);
    e
}

#[test]
fn qt_bounds_staging_cancel_loaded_normalisation_and_durable_reopen() {
    let f = hydrus_testkit::fixture_json("popup_width.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        value(&store.read(settings::get::<PopupWidth>).unwrap()),
        f["defaults"]
    );
    assert_eq!(
        serde_json::from_str::<PopupWidth>("{}").unwrap(),
        PopupWidth::default()
    );
    for event in f["events"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        assert_eq!(value(&before.popup_width), event["before"]);
        let mut e = editor(before.clone());
        e.number(row(&e, WIDTH), event["input"][0].as_i64().unwrap());
        e.check(row(&e, FIXED), event["input"][1].as_bool().unwrap());
        assert_eq!(
            value(&store.read(settings::get::<PopupWidth>).unwrap()),
            event["staged"]
        );
        let (after, _, errors) = e.applied();
        assert!(errors.is_empty());
        assert_eq!(value(&after.popup_width), event["draft"]);
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            value(&store.read(settings::get::<PopupWidth>).unwrap()),
            event["saved"]
        );
    }
    for event in f["loaded"].as_array().unwrap() {
        let p: PopupWidth = serde_json::from_value(event["before"].clone()).unwrap();
        store
            .write(move |ctx| settings::set(ctx.conn(), &p))
            .unwrap();
        let before = store.read(Settings::load).unwrap();
        let (after, _, errors) = editor(before.clone()).applied();
        assert!(errors.is_empty());
        assert_eq!(value(&after.popup_width), event["saved"]);
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
    }
    let before = store.read(Settings::load).unwrap();
    let mut cancelled = editor(before.clone());
    cancelled.number(row(&cancelled, WIDTH), 80);
    cancelled.check(row(&cancelled, FIXED), false);
    drop(cancelled);
    assert_eq!(store.read(Settings::load).unwrap(), before);
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(reopened.read(Settings::load).unwrap(), before);
}

#[test]
fn legacy_raw_integers_and_independent_edited_fields_are_preserved() {
    use std::collections::BTreeMap;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for raw in [-1, 0, 16, 56, 256, 999] {
        let mut p = PopupWidth::default();
        p.apply_legacy(
            &BTreeMap::from([("popup_message_character_width".into(), raw)]),
            &BTreeMap::from([("popup_message_force_min_width".into(), true)]),
        );
        assert_eq!(p.characters, raw);
        assert!(p.fixed);
        assert_eq!(i64::from(p.effective_characters()), raw.clamp(16, 256));
    }
    let before = store.read(Settings::load).unwrap();
    let mut e = editor(before.clone());
    e.number(row(&e, WIDTH), 80);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &PopupWidth {
                    fixed: true,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let (after, _, _) = e.applied();
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    assert_eq!(
        store.read(settings::get::<PopupWidth>).unwrap(),
        PopupWidth {
            characters: 80,
            fixed: true
        }
    );
}
