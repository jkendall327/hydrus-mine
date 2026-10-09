//! The archive/delete filter plays files as the media viewer does: the
//! client's own player for a ugoira or animated WebP with a scanbar that
//! seeks by frame (a drag pauses), the volume control beside the scanbar of
//! a file with sound, space to pause, and the tags and information frames
//! under the pointer. The scanbar and volume are compared with the media
//! viewer's, which show the same file the same way.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::media_viewer::AudioSettings;
use hydrus_gui::{
    ArchiveDeleteWindow, MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless,
};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn picture(window: &ArchiveDeleteWindow) -> Option<((u32, u32), Vec<u8>)> {
    let pixels = window.get_media().to_rgba8()?;
    Some((
        (pixels.width(), pixels.height()),
        pixels.as_bytes().to_vec(),
    ))
}

/// The distinct pictures shown over `time`.
fn watch(window: &ArchiveDeleteWindow, time: Duration) -> HashSet<((u32, u32), Vec<u8>)> {
    let mut seen = HashSet::new();
    let started = Instant::now();
    while started.elapsed() < time {
        slint::platform::update_timers_and_animations();
        seen.extend(picture(window));
        std::thread::sleep(Duration::from_millis(5));
    }
    seen
}

fn scanbar_reaches(window: &ArchiveDeleteWindow, wanted: &str) -> String {
    let started = Instant::now();
    loop {
        watch(window, Duration::from_millis(20));
        if window.get_scanbar_text().starts_with(wanted) {
            watch(window, Duration::from_millis(120));
            if window.get_scanbar_text().starts_with(wanted) {
                return window.get_scanbar_text().to_string();
            }
        }
        if started.elapsed() > Duration::from_secs(10) {
            return window.get_scanbar_text().to_string();
        }
    }
}

fn hash_of(name: &str) -> hydrus_core::Sha256 {
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(hydrus_testkit::fixture_path(
            "legacy_db/basic.manifest.json",
        ))
        .unwrap(),
    )
    .unwrap();
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn key(window: &ArchiveDeleteWindow, text: &str) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: text.into() });
}

// leaf: audit-media-archive-playback
#[test]
fn the_filter_plays_files_with_a_scanbar_and_volume_as_the_viewer_does() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    let mut imported = Vec::new();
    for name in ["ugoira_json.zip", "webp_anim.webp"] {
        let result = importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("media/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap();
        imported.push((name.to_owned(), result.hash.unwrap()));
    }
    imported.push(("audio.mp3".to_owned(), hash_of("audio.mp3")));
    let kept = || -> AudioSettings { store.read(hydrus_store::settings::get).unwrap() };

    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let results = bound.current.borrow().borrow().results().to_vec();
    let position = |hash: &hydrus_core::Sha256| {
        let id = store
            .read(|c| hydrus_store::master::hash_id(c, hash))
            .unwrap()
            .unwrap();
        i32::try_from(results.iter().position(|&r| r == id).unwrap()).unwrap()
    };
    let filter_on = |hash: &hydrus_core::Sha256| -> ArchiveDeleteWindow {
        ui.invoke_select_none();
        ui.invoke_thumbnail_clicked(position(hash), false, false);
        ui.invoke_archive_delete_filter();
        bound
            .archive_delete
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the filter opened")
    };
    let viewer_on = |hash: &hydrus_core::Sha256| -> MediaViewerWindow {
        ui.invoke_thumbnail_activated(position(hash));
        bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .unwrap()
    };

    for (name, hash) in imported.iter().take(2) {
        let window = filter_on(hash);
        // plays frame after frame, looping
        let shown = watch(&window, Duration::from_millis(1500));
        assert!(shown.len() >= 3, "{name}: {} pictures", shown.len());
        // space pauses; again plays on
        key(&window, " ");
        watch(&window, Duration::from_millis(50));
        assert_eq!(
            watch(&window, Duration::from_millis(400)).len(),
            1,
            "{name}"
        );
        key(&window, " ");
        assert!(
            watch(&window, Duration::from_millis(1000)).len() >= 2,
            "{name}"
        );

        // the scanbar, by frame; a drag pauses and goes to the frame under
        // the pointer
        assert!(window.get_scanbar_shown(), "{name}");
        assert!(!window.get_volume_shown(), "{name}");
        watch(&window, Duration::from_millis(200));
        let text = window.get_scanbar_text().to_string();
        let frames: usize = text.split(['/', ' ']).nth(1).unwrap().parse().unwrap();
        let width = 210.0;
        let x = 5.0 + 0.75 * (width - 10.0);
        let target = (0.75 * (frames - 1) as f64 + 0.5) as usize;
        window.invoke_scan_started();
        window.invoke_scan(x, width);
        let wanted = format!("{}/{frames} - ", target + 1);
        let there = scanbar_reaches(&window, &wanted);
        assert!(there.starts_with(&wanted), "{name}: {there}");
        assert_eq!(
            watch(&window, Duration::from_millis(300)).len(),
            1,
            "{name}: paused"
        );
        let progress = window.get_scanbar_progress();
        window.invoke_scan_ended();
        assert!(
            watch(&window, Duration::from_millis(1000)).len() >= 2,
            "{name}: playing again"
        );
        window.invoke_close_requested();
        // the viewer shows the same file's scanbar the same way
        let viewer = viewer_on(hash);
        viewer.invoke_scan_started();
        viewer.invoke_scan(x, width);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !viewer.get_scanbar_text().starts_with(&wanted) {
            assert!(Instant::now() < deadline, "{name}: viewer");
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(viewer.get_scanbar_text().to_string(), there, "{name}");
        assert!(
            (viewer.get_scanbar_progress() - progress).abs() < f32::EPSILON,
            "{name}"
        );
        viewer.invoke_close_requested();
    }

    // a file with sound has the volume control, kept as in the viewer
    let (_, audio) = &imported[2];
    let window = filter_on(audio);
    assert!(window.get_scanbar_shown());
    assert!(window.get_volume_shown());
    assert_eq!(window.get_volume(), 70);
    assert!(!window.get_global_muted() && !window.get_viewer_muted());
    window.invoke_flip_global_mute();
    assert!(kept().global_mute);
    assert!(window.get_global_muted());
    window.invoke_flip_global_mute();
    assert!(!kept().global_mute);
    window.invoke_volume_changed(40);
    assert_eq!(kept().global_volume, 40);
    assert_eq!(window.get_volume(), 40);
    window.invoke_flip_viewer_mute();
    assert!(kept().viewer_mute);
    assert!(window.get_viewer_muted());
    // the volume control opens under the pointer, beside the scanbar
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    let (right, bottom) = (
        window.get_media_x() + window.get_media_width(),
        window.get_media_y() + window.get_media_height(),
    );
    for up in [30.0, 10.0] {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(right - 12.0, bottom - up),
            });
    }
    assert!(window.get_volume_open());
    // a click on the track's top sets the loudest
    let top = slint::LogicalPosition::new(right - 12.0, bottom - 160.0 + 10.0);
    for event in [
        slint::platform::WindowEvent::PointerPressed {
            position: top,
            button: slint::platform::PointerEventButton::Left,
        },
        slint::platform::WindowEvent::PointerReleased {
            position: top,
            button: slint::platform::PointerEventButton::Left,
        },
    ] {
        window.window().dispatch_event(event);
    }
    assert_eq!(kept().global_volume, 100);
    // the scanbar seeks where clicked, and shows how far
    window.invoke_scan_started();
    window.invoke_scan(105.0, 210.0);
    window.invoke_scan_ended();
    assert!(window.get_scanbar_progress() > 0.4 && window.get_scanbar_progress() < 0.6);

    // the tags are shown by the pointer at the left, the information at the
    // top
    headless::render(&drawn, 800, 600);
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(10.0, 300.0),
        });
    assert_eq!(window.get_tags_showing(), window.get_tags().row_count() > 0);
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(400.0, 3.0),
        });
    assert!(window.get_info_showing());
}
