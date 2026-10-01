//! Each file's info lines, and the media viewer's top line, are the
//! reference's (`oracle/record_info_lines.py`, at a fixed "now"), with a new
//! client's options and with the info line options turned the other way.

use std::sync::Arc;

use hydrus_core::media_viewer::InfoLineSettings;
use hydrus_gui::info_lines::{InfoLine, info_lines, top_line};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::{Value, json};

fn described(lines: &[InfoLine]) -> Value {
    Value::Array(
        lines
            .iter()
            .map(|line| match &line.submenu {
                Some(sub) => json!({
                    "submenu": line.text,
                    "interesting": line.interesting,
                    "lines": described(sub),
                }),
                None => json!({ "text": line.text, "interesting": line.interesting }),
            })
            .collect(),
    )
}

fn settings_of(phase: &Value) -> InfoLineSettings {
    let flag = |name: &str| phase["booleans"][name].as_bool().unwrap();
    InfoLineSettings {
        archived_interesting: flag("file_info_line_consider_archived_interesting"),
        archived_time_interesting: flag("file_info_line_consider_archived_time_interesting"),
        file_services_interesting: flag("file_info_line_consider_file_services_interesting"),
        file_services_import_times_interesting: flag(
            "file_info_line_consider_file_services_import_times_interesting",
        ),
        trash_time_interesting: flag("file_info_line_consider_trash_time_interesting"),
        trash_reason_interesting: flag("file_info_line_consider_trash_reason_interesting"),
        hide_uninteresting_modified_time: flag("hide_uninteresting_modified_time"),
        nice_resolutions: flag("use_nice_resolution_strings"),
        has_audio_label: phase["has_audio_label"].as_str().unwrap().to_owned(),
    }
}

#[test]
fn info_lines_are_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("info_lines.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    // (a new client's are what migrates without options of its own)
    assert_eq!(
        settings_of(&fixture["phases"]["defaults"]),
        InfoLineSettings::default()
    );
    let now_ms = fixture["now"].as_i64().unwrap() * 1000;
    let snapshot = store.snapshot();
    for phase in ["defaults", "changed"] {
        let expected = &fixture["phases"][phase];
        let settings = settings_of(expected);
        let files = expected["files"].as_object().unwrap();
        assert!(files.len() > 30);
        for (hash, theirs) in files {
            let id = store
                .read(|c| hydrus_store::master::hash_id(c, &hash.parse().unwrap()))
                .unwrap()
                .unwrap();
            let media = store
                .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[id]))
                .unwrap()
                .results
                .remove(0);
            let lines = info_lines(&media, &snapshot.services, &settings, now_ms, false);
            assert_eq!(described(&lines), theirs["lines"], "{phase}: {hash}");
            assert_eq!(
                top_line(&media, &snapshot.services, &settings, now_ms),
                theirs["top"].as_str().unwrap(),
                "{phase}: {hash}"
            );
        }
    }
}

#[test]
fn the_viewer_shows_the_top_line_near_its_top() {
    use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
    use slint::ComponentHandle as _;
    use slint::platform::WindowEvent;

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:inbox".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let file = bound.current.borrow().borrow().results()[0];
    let snapshot = store.snapshot();
    let line = |store: &Store| {
        let media = store
            .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
            .unwrap()
            .results
            .remove(0);
        top_line(
            &media,
            &snapshot.services,
            &InfoLineSettings::default(),
            hydrus_core::TimestampMs::now().0,
        )
    };
    assert_eq!(viewer.get_info_line(), line(&store).as_str());
    assert!(!viewer.get_info_line().contains("archived"));
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    let window = viewer.window();
    let at = |x: f32, y: f32| slint::LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerMoved {
        position: at(400.0, 300.0),
    });
    assert!(!viewer.get_info_showing());
    window.dispatch_event(WindowEvent::PointerMoved {
        position: at(400.0, 10.0),
    });
    assert!(viewer.get_info_showing());
    let pixels = headless::render(&drawn, 800, 600);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("info_line.png"), &pixels, 800, 600).unwrap();
    // archived (F7), the line says so
    viewer.invoke_archive();
    assert!(
        viewer.get_info_line().ends_with(" | archived: now"),
        "{}",
        viewer.get_info_line()
    );
    assert_eq!(viewer.get_info_line(), line(&store).as_str());
}
