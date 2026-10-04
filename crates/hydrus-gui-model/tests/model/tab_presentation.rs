//! Actual GUI Pages choices, recursive hiding and Qt middle-elided tab names.
use hydrus_gui_model::{
    options::{Editor, Kind, Row, Settings, Value},
    tab_presentation::middle_name,
};
use hydrus_store::{
    Store,
    settings::{self, TabAlignment, TabPresentationSettings},
};

const ALIGN: &str = "Notebook tab alignment: ";
const TREE: &str = "EXPERIMENTAL: Show tab tree view: ";
const HIDE: &str = "EXPERIMENTAL: Hide main page navigation tabs: ";
const ELIDE: &str = "When there are too many tabs to fit, '...' elide their names so they fit: ";

fn editor(settings: Settings) -> Editor {
    let mut editor = Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "gui pages")
        .unwrap();
    editor.show_page(page);
    editor
}
fn row(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(|row| matches!(row, Row::Opt {option,..} if option.label==label))
        .unwrap()
}
fn recorded(value: &serde_json::Value) -> TabPresentationSettings {
    TabPresentationSettings {
        alignment: TabAlignment::from_code(value["alignment"].as_i64().unwrap()).unwrap(),
        tree_alignment: value["tree"].as_i64().and_then(TabAlignment::from_code),
        hide_navigation_tabs: value["hide"].as_bool().unwrap(),
        elide_names: value["elide"].as_bool().unwrap(),
    }
}

#[test]
fn legacy_defaults_and_staged_controls_replay_actual_options_then_reopen() {
    let fixture = hydrus_testkit::fixture_json("tab_presentation.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let defaults = recorded(&fixture["defaults"]);
    assert_eq!(
        store
            .read(settings::get::<TabPresentationSettings>)
            .unwrap(),
        defaults
    );
    assert_eq!(defaults, TabPresentationSettings::default());
    assert_eq!(
        serde_json::from_str::<TabPresentationSettings>("{}").unwrap(),
        defaults
    );
    assert!(TabAlignment::from_code(-1).is_none());
    assert!(TabAlignment::from_code(4).is_none());
    assert!(
        !TabPresentationSettings {
            tree_alignment: Some(TabAlignment::Top),
            hide_navigation_tabs: true,
            ..defaults
        }
        .tabs_hidden()
    );
    let before = store.read(Settings::load).unwrap();
    let mut draft = editor(before.clone());
    let mut group = "";
    for row in draft.rows() {
        match row {
            Row::Title { title, .. } => group = title,
            Row::Opt { option, value, .. } => match option.label {
                ALIGN => {
                    assert_eq!(group, "navigation and drag-and-drop");
                    assert_eq!(
                        option.kind,
                        Kind::Choice(&["top", "left", "right", "bottom"])
                    );
                    assert_eq!(value, &Value::Choice(0));
                }
                TREE => {
                    assert_eq!(group, "navigation and drag-and-drop");
                    assert_eq!(option.kind, Kind::Choice(&["disable", "left", "right"]));
                }
                HIDE => assert_eq!(group, "navigation and drag-and-drop"),
                ELIDE => assert_eq!(group, "page tab names"),
                _ => {}
            },
        }
    }
    draft.choose(row(&draft, ALIGN), 3);
    draft.choose(row(&draft, TREE), 2);
    draft.check(row(&draft, HIDE), true);
    draft.check(row(&draft, ELIDE), false);
    assert_eq!(
        store.read(Settings::load).unwrap(),
        before,
        "Cancel discards the detached editor"
    );
    assert_eq!(recorded(&fixture["cancelled"]), defaults);
    draft.choose(row(&draft, ALIGN), 999);
    let (invalid, _, problems) = draft.applied();
    assert!(!problems.is_empty());
    assert_eq!(invalid.tab_presentation.alignment, defaults.alignment);
    for step in fixture["alignments"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["hide"].as_array().unwrap())
        .chain(fixture["elide"].as_array().unwrap())
    {
        let before = store.read(Settings::load).unwrap();
        let expected = recorded(&step["settings"]);
        let mut draft = editor(before.clone());
        draft.choose(
            row(&draft, ALIGN),
            usize::try_from(expected.alignment.code()).unwrap(),
        );
        draft.choose(
            row(&draft, TREE),
            usize::try_from(expected.tree_side()).unwrap(),
        );
        draft.check(row(&draft, HIDE), expected.hide_navigation_tabs);
        draft.check(row(&draft, ELIDE), expected.elide_names);
        let (after, _, problems) = draft.applied();
        assert!(problems.is_empty());
        assert_eq!(after.tab_presentation, expected);
        assert_eq!(
            expected.tabs_hidden(),
            step["root"]["hidden"].as_bool().unwrap()
        );
        assert_eq!(
            expected.tabs_hidden(),
            step["nested"]["hidden"].as_bool().unwrap()
        );
        assert_eq!(store.read(Settings::load).unwrap(), before);
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            reopened
                .read(settings::get::<TabPresentationSettings>)
                .unwrap(),
            expected
        );
        let reopened = editor(reopened.read(Settings::load).unwrap());
        let (after, _, problems) = reopened.applied();
        assert!(problems.is_empty());
        assert_eq!(after.tab_presentation, expected);
    }
    assert_eq!(
        store
            .read(settings::get::<TabPresentationSettings>)
            .unwrap(),
        recorded(&fixture["reopened"])
    );
}

#[test]
fn middle_names_replay_actual_qt_paint_without_changing_stored_names() {
    let fixture = hydrus_testkit::fixture_json("tab_presentation.json");
    let mut elided = 0;
    for step in fixture["alignments"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["elide"].as_array().unwrap())
    {
        for area in ["root", "nested"] {
            for tab in step[area]["tabs"].as_array().unwrap() {
                let stored = tab["stored"].as_str().unwrap();
                let paint = tab["paint"].as_str().unwrap();
                let kept = if paint.contains('…') {
                    elided += 1;
                    paint.chars().count() - 1
                } else {
                    stored.chars().count()
                };
                assert_eq!(middle_name(stored, kept), paint);
                assert_eq!(middle_name(stored, stored.chars().count()), stored);
            }
        }
    }
    assert!(
        elided >= 20,
        "real constrained horizontal and vertical Qt labels"
    );
    assert_eq!(middle_name("漢😀字🙂末", 2), "漢…末");
    assert_eq!(middle_name("漢😀字🙂末", 3), "漢😀…末");
    assert_eq!(middle_name("abc", 0), "…");
}
