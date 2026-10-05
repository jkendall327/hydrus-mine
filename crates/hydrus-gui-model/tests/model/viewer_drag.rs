//! Actual Qt anchored/touch drag deltas, warp requests, and detached Options.
use hydrus_gui_model::{
    options::{Editor, Row, Settings},
    viewer_drag::Drag,
};
use hydrus_store::{
    Store,
    settings::{self, ViewerPointerSettings},
};

fn pair(value: &serde_json::Value) -> (i32, i32) {
    (
        i32::try_from(value[0].as_i64().unwrap()).unwrap(),
        i32::try_from(value[1].as_i64().unwrap()).unwrap(),
    )
}
#[test]
fn anchored_and_touch_overridden_drags_replay_actual_qt_threshold_and_warps() {
    let fixture = hydrus_testkit::fixture_json("viewer_anchor_options.json");
    for case in fixture["cases"].as_array().unwrap() {
        let mut drag = Drag::default();
        drag.begin((120, 140));
        for step in case["moves"].as_array().unwrap() {
            let motion = drag.step(
                pair(&step["point"]),
                case["anchor"].as_bool().unwrap(),
                case["touch_override"].as_bool().unwrap(),
            );
            assert_eq!(
                motion.map_or((0, 0), |motion| motion.delta),
                pair(&step["delta"])
            );
            let warps: Vec<_> = motion
                .and_then(|motion| motion.warp)
                .into_iter()
                .map(|(x, y)| vec![x, y])
                .collect();
            assert_eq!(serde_json::json!(warps), step["warp"]);
            assert_eq!(drag.last(), Some(pair(&step["last"])));
            assert_eq!(drag.touch, step["touch"].as_bool().unwrap());
        }
        drag.end();
        assert!(drag.step((150, 150), true, true).is_none());
        drag.begin((120, 140));
        assert_eq!(drag.touch, case["new_drag_touch"].as_bool().unwrap());
    }
}
#[test]
fn anchor_options_remain_drafts_until_apply_and_survive_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let before = store.read(Settings::load).unwrap();
    assert_eq!(
        before.viewer_pointer.anchor_drag,
        !cfg!(target_os = "macos")
    );
    assert!(!before.viewer_pointer.touch_unanchors);
    let mut editor = Editor::new(before.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "media viewer")
        .unwrap();
    editor.show_page(page);
    let rows: Vec<_> = [
        "Anchor mouse cursor during media viewer drags:",
        "If set to anchor drags, undo on apparent touchscreen drag:",
    ]
    .iter()
    .map(|label| {
        editor
            .rows()
            .iter()
            .position(|row| matches!(row,Row::Opt{option,..} if option.label==*label))
            .unwrap()
    })
    .collect();
    editor.check(rows[0], true);
    editor.check(rows[1], true);
    let (after, _, problems) = editor.applied();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        store.read(Settings::load).unwrap(),
        before,
        "detached draft"
    );
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let reopened = Store::open(directory.path()).unwrap();
    let saved = reopened
        .read(settings::get::<ViewerPointerSettings>)
        .unwrap();
    assert!(saved.anchor_drag && saved.touch_unanchors);
}
