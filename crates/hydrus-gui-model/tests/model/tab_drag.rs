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
