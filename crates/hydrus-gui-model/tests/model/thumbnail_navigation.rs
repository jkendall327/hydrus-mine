//! Replay real Qt staged controls and thumbnail keyboard/scroll consumers.
use hydrus_gui_model::{
    options::{Editor, Row, Settings},
    selection::{Move, Selection},
    thumbnail_navigation,
};
use hydrus_store::{
    Store,
    settings::{self, ThumbnailNavigation},
};
fn value(p: &ThumbnailNavigation) -> serde_json::Value {
    serde_json::json!({"shift":p.shift_moves_origin,"percent":p.visibility_percent,"rate":p.scroll_rate})
}
#[test]
fn staged_exact_controls_import_cancel_invalid_rate_save_and_reopen_match_qt() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_navigation.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        value(&store.read(settings::get).unwrap()),
        fixture["defaults"]
    );
    let before = store.read(Settings::load).unwrap();
    for event in fixture["events"].as_array().unwrap() {
        let mut editor = Editor::new(store.read(Settings::load).unwrap());
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "thumbnails")
            .unwrap();
        editor.show_page(page);
        let indices:Vec<usize> = fixture["labels"].as_array().unwrap().iter().map(|label| {
            editor.rows().iter().position(|row| matches!(row,Row::Opt{option,..} if option.label==label.as_str().unwrap())).unwrap()
        }).collect();
        let snapshot = store.read(Settings::load).unwrap();
        editor.check(indices[0], event["input"][0].as_bool().unwrap());
        editor.number(indices[1], event["input"][1].as_i64().unwrap());
        editor.text(indices[2], event["input"][2].as_str().unwrap());
        editor.check(usize::MAX, true);
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty());
        assert_eq!(value(&after.thumbnail_navigation), event["saved"]);
        assert_eq!(
            store.read(Settings::load).unwrap(),
            snapshot,
            "private draft"
        );
        if event == &fixture["events"][0] {
            assert_eq!(value(&snapshot.thumbnail_navigation), fixture["cancelled"]);
            assert_eq!(snapshot, before);
        }
        store
            .write(move |ctx| after.save(ctx.conn(), &snapshot))
            .unwrap();
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            value(&reopened.read(settings::get).unwrap()),
            event["reopened"]
        );
    }
    let compatible: ThumbnailNavigation = serde_json::from_str("{}").unwrap();
    assert_eq!(compatible, ThumbnailNavigation::default());
}
#[test]
fn optional_last_hit_origin_preserves_actual_ranges_focus_and_repeated_navigation() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_navigation.json");
    let files: Vec<hydrus_core::HashId> = (0..20).collect();
    for case in fixture["selections"].as_array().unwrap() {
        let enabled = case["enabled"].as_bool().unwrap();
        let mut selection = Selection::default();
        for step in case["steps"].as_array().unwrap() {
            let action = step["action"].as_str().unwrap();
            if action == "move" {
                selection.move_focus_with_last_hit(&files, Move::Right, false, 5, 4, enabled);
            } else {
                selection.hit(
                    &files,
                    Some(step["index"].as_i64().unwrap()),
                    false,
                    action == "shift",
                );
            }
            assert_eq!(
                serde_json::json!(selection.files(&files)),
                step["after"]["selected"]
            );
            assert_eq!(
                serde_json::json!(selection.focused()),
                step["after"]["focused"]
            );
            assert_eq!(
                serde_json::json!(selection.last_hit()),
                step["after"]["last_hit"]
            );
        }
    }
}
#[test]
fn exact_visibility_boundaries_and_rate_ties_negative_zero_and_errors_match_qt() {
    let fixture = hydrus_testkit::fixture_json("thumbnail_navigation.json");
    for step in fixture["scrolls"].as_array().unwrap() {
        let actual = thumbnail_navigation::scroll_target(
            step["top"].as_f64().unwrap(),
            step["span"].as_f64().unwrap(),
            step["view_y"].as_f64().unwrap(),
            step["view_height"].as_f64().unwrap(),
            fixture["geometry"]["content_height"].as_f64().unwrap(),
            step["percent"].as_u64().unwrap() as u8,
        );
        assert_eq!(
            actual.to_bits(),
            (step["after"].as_i64().unwrap() as f64).to_bits()
        );
    }
    for rate in fixture["rates"].as_array().unwrap() {
        let step = thumbnail_navigation::single_step(
            rate["span"].as_f64().unwrap(),
            rate["rate"].as_str().unwrap(),
            rate["before"].as_i64().unwrap() as i32,
        );
        assert_eq!(i64::from(step), rate["step"].as_i64().unwrap());
        let after = thumbnail_navigation::wheel_target(
            -60.0,
            step,
            fixture["geometry"]["viewport_height"].as_f64().unwrap(),
            0.0,
        );
        assert_eq!(
            after.to_bits(),
            (rate["wheel_after"].as_i64().unwrap() as f64).to_bits()
        );
    }
    assert!(thumbnail_navigation::parse_rate("1__0").is_none());
    assert!(thumbnail_navigation::parse_rate("_1").is_none());
    assert!(thumbnail_navigation::parse_rate("1_").is_none());
    assert_eq!(thumbnail_navigation::single_step(131.0, "０.５", 10), 66);
    assert_eq!(thumbnail_navigation::single_step(131.0, "1_0.0", 10), 1310);
}
