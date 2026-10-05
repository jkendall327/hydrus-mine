//! All real Qt RGB roles, staged defaults, field merges and imported backcompat.
use hydrus_gui_model::options::{Editor, Row, Settings as Options};
use hydrus_store::{
    Store,
    gui_colours::{self, Settings},
    services::Rgb,
    settings,
};
use serde_json::{Value, json};
use std::sync::Arc;
const OVERRIDE: &str = "override what is set in the stylesheet with the colours on this page: ";
const CURRENT: &str = "current colourset: ";
fn values(settings: &Settings) -> Value {
    json!({"override":settings.override_stylesheet,"current":gui_colours::SET_NAMES[settings.current.min(1)],"sets":{
        "default":settings.sets[0].map(|rgb|rgb.0),"darkmode":settings.sets[1].map(|rgb|rgb.0)}})
}
fn fixture() -> (tempfile::TempDir, Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    (dir, store)
}
fn editor(store: &Store) -> Editor {
    let mut editor = Editor::new(store.read(Options::load).unwrap());
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "colours")
        .unwrap();
    editor.show_page(page);
    editor
}
fn row(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(|row| matches!(row,Row::Opt{option,..}if option.label==label))
        .unwrap()
}
fn save(store: &Store, editor: &Editor) {
    let (after, before, problems) = editor.applied();
    assert!(problems.is_empty(), "{problems:?}");
    let before = before.clone();
    store
        .write(move |tx| after.save(tx.conn(), &before))
        .unwrap();
}
#[test]
fn all_twenty_six_rgb_roles_and_independent_tabs_stage_cancel_apply_and_reopen_like_qt() {
    let qt = hydrus_testkit::fixture_json("gui_coloursets.json");
    let (dir, store) = fixture();
    assert_eq!(
        values(&store.read(gui_colours::load).unwrap()),
        qt["initial"]
    );
    assert_eq!(values(&Settings::default()), qt["initial"]);
    assert_eq!(
        serde_json::from_value::<Settings>(json!({})).unwrap(),
        Settings::default()
    );
    assert_eq!(json!(gui_colours::SET_NAMES), qt["choices"]);
    for label in gui_colours::ROW_LABELS {
        assert!(
            qt["labels"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == label)
        );
    }
    let mut draft = editor(&store);
    draft.check(row(&draft, OVERRIDE), true);
    draft.choose(row(&draft, CURRENT), 1);
    assert_eq!(
        qt["reopened"]["selected_tab"], 0,
        "active darkmode does not choose the editing tab"
    );
    let edits = qt["role_edits"].as_array().unwrap();
    assert_eq!(edits.len(), 26);
    for edit in edits {
        let set = usize::from(edit["set"] == "darkmode");
        let role = usize::try_from(edit["role"].as_u64().unwrap()).unwrap();
        draft.set_gui_colour(
            set,
            role,
            Rgb(serde_json::from_value(edit["colour"].clone()).unwrap()),
        );
    }
    assert_eq!(values(&draft.edited_gui_colours()), qt["saved"]);
    assert_eq!(
        values(&store.read(gui_colours::load).unwrap()),
        qt["before_update"]
    );
    save(&store, &draft);
    assert_eq!(values(&store.read(gui_colours::load).unwrap()), qt["saved"]);
    assert_eq!(
        values(&editor(&store).edited_gui_colours()),
        qt["round_trip"]
    );
    let mut cancelled = editor(&store);
    cancelled.check(row(&cancelled, OVERRIDE), false);
    cancelled.choose(row(&cancelled, CURRENT), 0);
    assert!(!cancelled.edited_gui_colours().override_stylesheet);
    drop(cancelled);
    assert_eq!(
        values(&store.read(gui_colours::load).unwrap()),
        qt["cancel_after"]
    );
    drop(draft);
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(
        values(&reopened.read(gui_colours::load).unwrap()),
        qt["saved"]
    );
}
#[test]
fn staged_role_changes_merge_with_live_help_switch_and_other_saved_rgb_roles() {
    let (_dir, store) = fixture();
    let mut draft = editor(&store);
    draft.check(row(&draft, OVERRIDE), true);
    draft.set_gui_colour(0, 9, Rgb([11, 37, 211]));
    store
        .write(|tx| {
            let mut latest = gui_colours::load(tx.conn())?;
            latest.flip();
            latest.sets[1][12] = Rgb([223, 17, 47]);
            settings::set(tx.conn(), &latest)
        })
        .unwrap();
    save(&store, &draft);
    let latest = store.read(gui_colours::load).unwrap();
    assert!(latest.override_stylesheet);
    assert_eq!(latest.current, 1);
    assert_eq!(latest.sets[0][9], Rgb([11, 37, 211]));
    assert_eq!(latest.sets[1][12], Rgb([223, 17, 47]));
    let unchanged = editor(&store);
    store
        .write(|tx| {
            let mut latest = gui_colours::load(tx.conn())?;
            latest.flip();
            settings::set(tx.conn(), &latest)
        })
        .unwrap();
    save(&store, &unchanged);
    assert_eq!(store.read(gui_colours::load).unwrap().current, 0);
}
#[test]
fn old_imported_store_keeps_retained_legacy_rgb_until_native_values_take_precedence() {
    let (dir, store) = fixture();
    store
        .write(|tx| {
            let kind = u32::from(hydrus_legacy::serialisable::SerialisableType::CLIENT_OPTIONS.0);
            let (_, mut dump) = hydrus_store::legacy::singleton(tx.conn(), kind)?.unwrap();
            for (from, to) in [
                (
                    r#"[[0, "override_stylesheet_colours"], [0, false]]"#,
                    r#"[[0, "override_stylesheet_colours"], [0, true]]"#,
                ),
                (
                    r#"[[0, "current_colourset"], [0, "default"]]"#,
                    r#"[[0, "current_colourset"], [0, "darkmode"]]"#,
                ),
            ] {
                assert!(dump.contains(from));
                dump = dump.replace(from, to);
            }
            let start = dump.find(r#"[[0, "colours"]"#).unwrap();
            let (before, colours) = dump.split_at(start);
            let old = "[2, [26, 3, [[0, 255], [0, 255], [0, 255]]]]";
            assert!(colours.contains(old));
            let updated = format!(
                "{before}{}",
                colours.replacen(old, "[2, [26, 3, [[0, 7], [0, 19], [0, 31]]]]", 1)
            );
            tx.conn().execute(
                "UPDATE legacy_objects SET dump=?1 WHERE source='json_dumps' AND type_id=?2",
                rusqlite::params![updated, kind],
            )?;
            tx.conn()
                .execute("DELETE FROM settings WHERE key='gui_coloursets'", [])?;
            Ok(())
        })
        .unwrap();
    let legacy = store.read(gui_colours::load).unwrap();
    assert!(legacy.override_stylesheet);
    assert_eq!(legacy.current, 1);
    assert_eq!(legacy.sets[0][0], Rgb([7, 19, 31]));
    assert_eq!(legacy.sets[1], Settings::default().sets[1]);
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.read(gui_colours::load).unwrap(), legacy);
    let mut native = Settings::default();
    native.sets[0][0] = Rgb([97, 53, 13]);
    let expected = native.clone();
    store
        .write(move |tx| settings::set(tx.conn(), &native))
        .unwrap();
    assert_eq!(store.read(gui_colours::load).unwrap(), expected);
}
