//! Real file-viewing Options and consumer policies.
use hydrus_core::CanvasType;
use hydrus_gui_model::options::{Editor, Kind, Row, Settings, Value};
use hydrus_store::settings::{FileViewingStatistics, ViewingStatsMenuDisplay};

fn row(editor: &Editor, label: &str) -> usize {
    editor
        .rows()
        .iter()
        .position(|r| matches!(r, Row::Opt {option,..} if option.label == label))
        .unwrap()
}

#[test]
fn viewing_menu_controls_match_qt_and_preserve_the_parent_draft() {
    let oracle = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    let settings = store.read(Settings::load).unwrap();
    let original = settings.file_viewing.clone();
    let mut editor = Editor::new(settings);
    let page = editor
        .page_names()
        .iter()
        .position(|name| *name == "file viewing statistics")
        .unwrap();
    editor.show_page(page);
    let menu = row(&editor, "Show viewing stats on media right-click menus?:");
    let canvases = row(&editor, "Which views to show?:");
    let rows = editor.rows();
    let Row::Opt { option, value, .. } = &rows[menu] else {
        panic!("menu choice")
    };
    let Kind::Choice(labels) = &option.kind else {
        panic!("choice")
    };
    assert_eq!(serde_json::json!(labels), oracle["initial"]["menu_choices"]);
    assert_eq!(**value, Value::Choice(0));
    let Row::Opt { option, value, .. } = &rows[canvases] else {
        panic!("canvas list")
    };
    assert_eq!(option.kind, Kind::CanvasTicks);
    assert_eq!(
        **value,
        Value::Canvases(vec![CanvasType::MediaViewer, CanvasType::ClientApi])
    );
    assert_eq!(original.menu_display, ViewingStatsMenuDisplay::Combined);
    assert!(!editor.search("preview views").is_empty());
    editor.choose(menu, 1);
    editor.canvas(canvases, 0, false);
    editor.canvas(canvases, 1, true);
    editor.canvas(canvases, 2, false);
    editor.canvas(canvases, 99, true);
    let (after, changed, problems) = editor.applied();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(changed.file_viewing, original);
    assert_eq!(
        after.file_viewing.menu_display,
        ViewingStatsMenuDisplay::Stacked
    );
    assert_eq!(
        after.file_viewing.interesting_canvases,
        [CanvasType::Preview]
    );
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<FileViewingStatistics>)
            .unwrap(),
        original
    );
    editor.canvas(canvases, 1, false);
    assert!(
        editor
            .applied()
            .0
            .file_viewing
            .interesting_canvases
            .is_empty()
    );
}
