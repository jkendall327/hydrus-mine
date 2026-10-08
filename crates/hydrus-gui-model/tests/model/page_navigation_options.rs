//! Reference history output and GUI Pages settings defaults/import/validation.
use hydrus_core::pages::PageKey;
use hydrus_gui_model::{
    main_menu::{self, Entry, Facts},
    options::{Editor, Row, Settings},
};
use hydrus_store::{
    Store,
    settings::{self, PageNavigationSettings},
};

// leaf: audit-options-gui-pages-navigation-and-drag-and-drop-maximum-entries-to-show-in-page-navigation-history
#[test]
fn history_limit_replays_actual_menu_without_discarding_backing_history() {
    let fixture = hydrus_testkit::fixture_json("page_navigation_options.json");
    let mut facts = Facts {
        history: Some(
            (1..=25)
                .map(|i| (PageKey::random(), format!("page {i:02}")))
                .collect(),
        ),
        ..Facts::default()
    };
    assert_eq!(
        facts.page_navigation.history_entries,
        u16::try_from(fixture["controls"]["history"].as_u64().unwrap()).unwrap()
    );
    for step in fixture["history"].as_array().unwrap() {
        facts.page_navigation.history_entries =
            u16::try_from(step["value"].as_u64().unwrap()).unwrap();
        let menus = main_menu::menubar(&facts);
        let Entry::Menu { entries, .. } = main_menu::entry_at(&menus, &[2, 2]).unwrap() else {
            panic!("Pages History");
        };
        assert_eq!(
            entries
                .iter()
                .filter(|entry| !matches!(entry, Entry::Separator))
                .map(Entry::label)
                .collect::<Vec<_>>(),
            step["labels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
        );
        assert_eq!(facts.history.as_ref().unwrap().len(), 25);
    }
}

// leaf: audit-options-gui-pages-navigation-and-drag-and-drop-maximum-entries-to-show-in-page-navigation-history
#[test]
fn imported_defaults_and_options_limits_replay_reference_spinbox_clamping() {
    let fixture = hydrus_testkit::fixture_json("page_navigation_options.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = store.read(settings::get::<PageNavigationSettings>).unwrap();
    assert_eq!(
        original,
        PageNavigationSettings {
            confirm_all_closes: fixture["controls"]["confirm"].as_bool().unwrap(),
            focus_search_on_change: fixture["controls"]["focus"].as_bool().unwrap(),
            history_entries: 100
        }
    );
    let mut editor = Editor::new(store.read(Settings::load).unwrap());
    let index = editor
        .page_names()
        .iter()
        .position(|name| *name == "gui pages")
        .unwrap();
    editor.show_page(index);
    let row=editor.rows().iter().position(|row| matches!(row,Row::Opt {option,..} if option.label=="Maximum entries to show in page navigation history: ")).unwrap();
    for step in fixture["history"].as_array().unwrap() {
        editor.number(row, step["input"].as_i64().unwrap());
        let (after, _, problems) = editor.applied();
        assert_eq!(
            u64::from(after.page_navigation.history_entries),
            step["value"].as_u64().unwrap()
        );
        assert!(problems.is_empty());
    }
    editor.number(row, 2);
    let (after, _, problems) = editor.applied();
    assert_eq!(after.page_navigation.history_entries, 2);
    assert!(problems.is_empty());
    assert_eq!(
        store.read(settings::get::<PageNavigationSettings>).unwrap(),
        original
    );
}
