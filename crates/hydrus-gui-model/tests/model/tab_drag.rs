//! Real Qt defaults/independent Shift checkboxes, staged save and legacy import.
use hydrus_gui_model::options::{Editor, Row, Settings};
use hydrus_store::{
    Store,
    settings::{self, TabDragSettings},
};
fn values(s: TabDragSettings) -> serde_json::Value {
    serde_json::json!({"page_drop_chase_normally":s.chase,"page_drop_chase_with_shift":s.chase_shift,"page_drag_change_tab_normally":s.navigate,"page_drag_change_tab_with_shift":s.navigate_shift,"wheel_scrolls_tab_bar":s.wheel_scroll,"disable_page_tab_dnd":s.disabled})
}
#[test]
fn independent_drag_preferences_replay_exact_qt_options_cancel_save_and_reopen() {
    let fixture = hydrus_testkit::fixture_json("tab_drag.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(
        values(store.read(settings::get).unwrap()),
        fixture["defaults"]
    );
    assert_eq!(values(TabDragSettings::default()), fixture["defaults"]);
    let before = store.read(Settings::load).unwrap();
    let mut editor = Editor::new(before.clone());
    let page = editor
        .page_names()
        .iter()
        .position(|n| *n == "gui pages")
        .unwrap();
    editor.show_page(page);
    let rows = editor.rows();
    let start=rows.iter().position(|row|matches!(row,Row::Opt{option,..} if option.label==fixture["labels"][0].as_str().unwrap())).unwrap();
    for offset in 0..6 {
        let Row::Opt { option, .. } = &rows[start + offset] else {
            panic!("checkbox")
        };
        assert_eq!(option.label, fixture["labels"][offset].as_str().unwrap());
    }
    for offset in 0..6 {
        let key = fixture["keys"][offset].as_str().unwrap();
        editor.check(start + offset, fixture["saved"][key].as_bool().unwrap());
    }
    let (after, _, errors) = editor.applied();
    assert!(errors.is_empty());
    assert_eq!(values(after.tab_drag), fixture["saved"]);
    assert_eq!(store.read(Settings::load).unwrap(), before);
    editor.check(usize::MAX, true);
    assert_eq!(values(editor.applied().0.tab_drag), fixture["saved"]);
    assert_eq!(
        values(store.read(settings::get).unwrap()),
        fixture["cancelled"]
    );
    store
        .write(move |ctx| after.save(ctx.conn(), &before))
        .unwrap();
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        values(reopened.read(settings::get).unwrap()),
        fixture["reopened"]
    );
    let partial: TabDragSettings = serde_json::from_str("{\"chase\":false}").unwrap();
    assert!(!partial.chase);
    assert!(partial.navigate);
    assert!(partial.navigate_shift);
    assert!(!partial.chase_shift);
}

#[test]
fn real_drop_order_and_independent_shift_decisions_preserve_the_source_object() {
    use hydrus_gui_model::tab_drag::{Edge, insertion};
    let fixture = hydrus_testkit::fixture_json("tab_drag.json");
    let mut names = vec![
        "alpha".to_owned(),
        "beta".into(),
        "gamma".into(),
        "omega".into(),
        "nested".into(),
    ];
    for step in fixture["drops"].as_array().unwrap() {
        assert_eq!(serde_json::json!(names), step["before"]["order"]);
        let source = names.iter().position(|name| name == "gamma").unwrap();
        let target = names.iter().position(|name| name == "alpha").unwrap();
        let index = insertion(Some(source), Some(target), Edge::Body, names.len()).unwrap();
        let moving = names.remove(source);
        names.insert(index, moving);
        assert_eq!(serde_json::json!(names), step["after"]["order"]);
        let shift = step["shift"].as_bool().unwrap();
        let chase = step["chase"].as_bool().unwrap();
        let prefs = if shift {
            TabDragSettings {
                chase_shift: chase,
                chase: !chase,
                ..TabDragSettings::default()
            }
        } else {
            TabDragSettings {
                chase,
                chase_shift: !chase,
                ..TabDragSettings::default()
            }
        };
        assert_eq!(prefs.chase(shift), chase);
    }
    assert_eq!(insertion(Some(2), Some(2), Edge::Right, 5), None);
    assert_eq!(insertion(Some(1), Some(2), Edge::Left, 5), None);
    assert_eq!(insertion(Some(1), None, Edge::Body, 5), Some(4));
    assert_eq!(insertion(None, Some(0), Edge::Right, 2), Some(1));
}
#[test]
fn drag_threshold_disabled_and_cancel_are_owned_and_deterministic() {
    use hydrus_core::pages::PageKey;
    use hydrus_gui_model::tab_drag::Pointer;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let first = PageKey::random();
    let second = PageKey::random();
    let mut a = Pointer::default();
    let mut b = Pointer::default();
    a.start(first, None, now);
    b.start(second, Some(first), now);
    assert!(a.moved(now + Duration::from_millis(99), false).is_none());
    assert!(a.release().is_none());
    a.start(first, None, now);
    assert_eq!(
        a.moved(now + Duration::from_millis(100), false)
            .unwrap()
            .key,
        first
    );
    assert_eq!(
        b.moved(now + Duration::from_millis(100), false)
            .unwrap()
            .key,
        second
    );
    a.cancel();
    assert!(a.release().is_none());
    assert!(b.moved(now + Duration::from_millis(101), true).is_none());
    assert!(b.release().is_none());
    a.start(first, None, now);
    a.moved(now + Duration::from_millis(110), false);
    assert_eq!(a.release().unwrap().key, first);
    assert!(a.release().is_none());
}
