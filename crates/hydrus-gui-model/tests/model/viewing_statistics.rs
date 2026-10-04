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

#[test]
fn completed_view_policy_matches_real_manager_caps_minima_durations_and_filter_switches() {
    use hydrus_gui_model::viewing_statistics::{Completed, completed};
    let oracle = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    for event in oracle["timing_events"].as_array().unwrap() {
        let settings = FileViewingStatistics {
            archive_delete: event["archive_delete"].as_bool().unwrap(),
            duplicates: event["duplicates"].as_bool().unwrap(),
            media_min_ms: event["minimum_ms"].as_u64(),
            media_max_ms: event["maximum_ms"].as_u64(),
            ..Default::default()
        };
        let canvas =
            CanvasType::from_code(u8::try_from(event["canvas"].as_u64().unwrap()).unwrap())
                .unwrap();
        let row = &event["row"];
        let expected = (row[1][1] == 1).then(|| Completed {
            canvas: CanvasType::from_code(u8::try_from(row[0].as_u64().unwrap()).unwrap()).unwrap(),
            elapsed_ms: row[1][2].as_u64().unwrap(),
        });
        assert_eq!(
            completed(
                &settings,
                canvas,
                event["duration_ms"].as_u64(),
                event["elapsed_ms"].as_u64().unwrap()
            ),
            expected,
            "{event}"
        );
        let disabled = FileViewingStatistics {
            active: false,
            ..settings
        };
        assert_eq!(
            completed(
                &disabled,
                canvas,
                event["duration_ms"].as_u64(),
                event["elapsed_ms"].as_u64().unwrap()
            ),
            None
        );
    }
}

#[test]
fn tracker_counts_once_on_change_or_close_reads_live_policy_and_preserves_latest_start() {
    use hydrus_gui_model::viewing_statistics::Tracker;
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let directory = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &directory.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    let oracle = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    let hash = oracle["file"].as_str().unwrap().parse().unwrap();
    let file = store
        .read(|conn| hydrus_store::master::hash_id(conn, &hash))
        .unwrap()
        .unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &FileViewingStatistics {
                    media_min_ms: None,
                    media_max_ms: Some(1000),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    store
        .write_content(move |w| w.set_views(file, CanvasType::MediaViewer, None, 0, 0))
        .unwrap();
    let stats = || {
        store
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[file]))
            .unwrap()
            .into_iter()
            .find(|s| s.canvas == CanvasType::MediaViewer)
            .unwrap()
    };
    let mut first = Tracker::new(store.clone(), CanvasType::MediaViewer);
    first.show(Some(file), 123000).unwrap();
    first.show(Some(file), 123050).unwrap();
    first.close(125000).unwrap();
    let recorded = stats();
    assert_eq!(
        (
            recorded.views,
            recorded.viewtime_ms,
            recorded.last_viewed.unwrap().0
        ),
        (1, 1000, 123000)
    );
    first.close(130000).unwrap();
    first.show(Some(file), 140000).unwrap();
    assert_eq!(
        stats(),
        recorded,
        "closed owner cannot restart or count twice"
    );
    let mut older = Tracker::new(store.clone(), CanvasType::ArchiveDeleteFilter);
    let mut newer = Tracker::new(store.clone(), CanvasType::MediaViewer);
    older.show(Some(file), 150000).unwrap();
    newer.show(Some(file), 151000).unwrap();
    newer.close(153000).unwrap();
    older.close(154000).unwrap();
    assert_eq!(
        (
            stats().views,
            stats().viewtime_ms,
            stats().last_viewed.unwrap().0
        ),
        (3, 3000, 151000)
    );
    let mut disabled = Tracker::new(store.clone(), CanvasType::MediaViewer);
    disabled.show(Some(file), 160000).unwrap();
    store
        .write(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &FileViewingStatistics {
                    active: false,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    disabled.close(165000).unwrap();
    assert_eq!(stats().views, 3, "policy is read when interval finishes");
}
