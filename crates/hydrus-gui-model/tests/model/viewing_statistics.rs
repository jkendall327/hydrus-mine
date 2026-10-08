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

// leaf: audit-options-file-viewing-statistics-show-viewing-stats-on-media-right-click-menus
// leaf: audit-options-file-viewing-statistics-which-views-to-show
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

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn preview_timed_policy_matches_actual_manager_and_keeps_its_own_canvas_category() {
    use hydrus_gui_model::viewing_statistics::{Completed, completed};
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    for event in fixture["policies"].as_array().unwrap() {
        let settings = FileViewingStatistics {
            preview_min_ms: event["minimum_ms"].as_u64(),
            preview_max_ms: event["maximum_ms"].as_u64(),
            // Deliberately incompatible media rules reveal misrouting.
            media_min_ms: Some(u64::MAX),
            media_max_ms: Some(0),
            archive_delete: false,
            duplicates: false,
            ..Default::default()
        };
        let expected = (event["row"][1][1] == 1).then(|| Completed {
            canvas: CanvasType::Preview,
            elapsed_ms: event["row"][1][2].as_u64().unwrap(),
        });
        assert_eq!(
            completed(
                &settings,
                CanvasType::Preview,
                event["duration_ms"].as_u64(),
                event["elapsed_ms"].as_u64().unwrap()
            ),
            expected,
            "{event}"
        );
        assert_eq!(
            completed(
                &FileViewingStatistics {
                    active: false,
                    ..settings
                },
                CanvasType::Preview,
                event["duration_ms"].as_u64(),
                event["elapsed_ms"].as_u64().unwrap()
            ),
            None
        );
    }
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-preview-viewer-to-count-as-a-view
#[test]
fn preview_options_replay_actual_none_bounds_conversion_and_cancelled_parent() {
    let fixture = hydrus_testkit::fixture_json("preview_viewing_intervals.json");
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    for event in fixture["controls"].as_array().unwrap() {
        assert_eq!(
            event["reopened_ms"], event["persisted_ms"],
            "actual typed Options transport preserves the value"
        );
        let mut settings = store.read(Settings::load).unwrap();
        let minimum = event["control"] == "preview_min_time";
        let requested = event["requested"]
            .as_f64()
            .map(|value| (value * 1000.0) as u64);
        if minimum {
            settings.file_viewing.preview_min_ms = requested;
        } else {
            settings.file_viewing.preview_max_ms = requested;
        }
        let mut editor = Editor::new(settings);
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "file viewing statistics")
            .unwrap();
        editor.show_page(page);
        let index = row(
            &editor,
            if minimum {
                "Min time to view on preview viewer to count as a view:"
            } else {
                "Cap any view on the preview viewer to this maximum time:"
            },
        );
        let rows = editor.rows();
        let Row::Opt { value, .. } = &rows[index] else {
            panic!("preview duration")
        };
        let Value::NoneableDuration { none, seconds } = **value else {
            panic!("duration value")
        };
        assert_eq!(none, event["value"].is_null());
        if let Some(expected) = event["value"].as_f64() {
            assert!((seconds - expected).abs() < 1e-9, "{event}");
        }
        let (applied, _, errors) = editor.applied();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            if minimum {
                applied.file_viewing.preview_min_ms
            } else {
                applied.file_viewing.preview_max_ms
            },
            event["persisted_ms"].as_u64(),
            "{event}"
        );
    }
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<FileViewingStatistics>)
            .unwrap(),
        FileViewingStatistics::default(),
        "opening/cancelling drafts never writes"
    );
}

// leaf: audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking-in-the-archive-delete-filter
// leaf: audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking-in-the-duplicate-filter
// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-media-viewer-to-count-as-a-view
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

// leaf: audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking-in-the-archive-delete-filter
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
    // set_views(None) intentionally keeps an imported timestamp, or uses now
    // for a new row. Place the held clock after that actual setup state so
    // latest-start preservation is tested independently of the fixture date.
    let held_origin = stats().last_viewed.unwrap().0;
    let at = |offset| held_origin + offset;
    let mut first = Tracker::new(store.clone(), CanvasType::MediaViewer);
    first.show(Some(file), at(123_000)).unwrap();
    first.show(Some(file), at(123_050)).unwrap();
    first.close(at(125_000)).unwrap();
    let recorded = stats();
    assert_eq!(
        (
            recorded.views,
            recorded.viewtime_ms,
            recorded.last_viewed.unwrap().0
        ),
        (1, 1000, at(123_000))
    );
    first.close(at(130_000)).unwrap();
    first.show(Some(file), at(140_000)).unwrap();
    assert_eq!(
        stats(),
        recorded,
        "closed owner cannot restart or count twice"
    );
    let mut older = Tracker::new(store.clone(), CanvasType::ArchiveDeleteFilter);
    let mut newer = Tracker::new(store.clone(), CanvasType::MediaViewer);
    older.show(Some(file), at(150_000)).unwrap();
    newer.show(Some(file), at(151_000)).unwrap();
    newer.close(at(153_000)).unwrap();
    older.close(at(154_000)).unwrap();
    assert_eq!(
        (
            stats().views,
            stats().viewtime_ms,
            stats().last_viewed.unwrap().0
        ),
        (3, 3000, at(151_000))
    );
    let mut disabled = Tracker::new(store.clone(), CanvasType::MediaViewer);
    disabled.show(Some(file), at(160_000)).unwrap();
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
    disabled.close(at(165_000)).unwrap();
    assert_eq!(stats().views, 3, "policy is read when interval finishes");
}

// leaf: audit-options-file-viewing-statistics-min-time-to-view-on-media-viewer-to-count-as-a-view
#[test]
#[allow(clippy::float_cmp)] // exact recorded control minima
fn timing_controls_replay_qt_bounds_none_and_millisecond_conversion() {
    use hydrus_gui_model::options::{duration_seconds, noneable_duration_fields};
    let oracle = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    let directory = tempfile::tempdir().unwrap();
    let store = hydrus_store::Store::open(directory.path()).unwrap();
    for event in oracle["duration_events"].as_array().unwrap() {
        let is_min = event["control"] == "media_min_time";
        let requested = event["requested"]
            .as_f64()
            .map(|n| (n * 1000.0).round() as u64);
        let mut settings = store.read(Settings::load).unwrap();
        if is_min {
            settings.file_viewing.media_min_ms = requested;
        } else {
            settings.file_viewing.media_max_ms = requested;
        }
        let mut editor = Editor::new(settings);
        let page = editor
            .page_names()
            .iter()
            .position(|name| *name == "file viewing statistics")
            .unwrap();
        editor.show_page(page);
        let at = row(
            &editor,
            if is_min {
                "Min time to view on media viewer to count as a view:"
            } else {
                "Cap any view on the media viewer to this maximum time:"
            },
        );
        let rows = editor.rows();
        let Row::Opt { option, value, .. } = &rows[at] else {
            panic!("timing control")
        };
        let Kind::NoneableDuration {
            units,
            min,
            none_phrase,
            ..
        } = &option.kind
        else {
            panic!("noneable duration")
        };
        let units = *units;
        let spec = &oracle["durations"][event["control"].as_str().unwrap()];
        assert_eq!(*min, spec["minimum"].as_f64().unwrap());
        assert_eq!(*none_phrase, spec["none_phrase"].as_str().unwrap());
        let Value::NoneableDuration { none, seconds } = **value else {
            panic!("time value")
        };
        assert_eq!(none, event["value"].is_null());
        let fields = noneable_duration_fields(seconds, units);
        assert_eq!(serde_json::json!(fields), event["fields"], "{event}");
        if !none {
            assert!(
                (duration_seconds(&fields, units) - event["value"].as_f64().unwrap()).abs() < 1e-9
            );
        }
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            if is_min {
                after.file_viewing.media_min_ms
            } else {
                after.file_viewing.media_max_ms
            },
            event["persisted_ms"].as_u64(),
            "{event}"
        );
        // A None checkbox preserves its numeric draft, and fields clamp to Qt ranges.
        editor.none(at, true);
        editor.none(at, false);
        editor.field(at, units.len() - 1, 1500);
        let Row::Opt { value, .. } = &editor.rows()[at] else {
            panic!("time draft")
        };
        let Value::NoneableDuration { seconds, .. } = **value else {
            panic!("time value")
        };
        assert_eq!(
            *hydrus_gui_model::options::duration_fields(seconds, units)
                .last()
                .unwrap(),
            999
        );
    }
    assert_eq!(
        store
            .read(hydrus_store::settings::get::<FileViewingStatistics>)
            .unwrap(),
        FileViewingStatistics::default(),
        "the parent draft never writes early"
    );
}
