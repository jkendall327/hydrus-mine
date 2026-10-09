//! An audio file in the media viewer, played through libmpv as the reference
//! plays it: its scanbar by time (no frames), moving while it plays, held
//! while paused, and going where it is seeked; and its volume control shown.
//! (Embedded cover art is not checked: nothing here tells it from the
//! thumbnail the viewer shows first.) Skipped where libmpv isn't installed. The headless
//! harness gives mpv a null audio output, which plays in real time with no
//! sound card.

use std::time::{Duration, Instant};

use hydrus_gui::scanbar::Scanbar;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless, mpv};
use hydrus_import::{FileImportOptions, FileImporter};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

fn settle(done: &dyn Fn() -> bool) -> bool {
    let started = Instant::now();
    while !done() && started.elapsed() < Duration::from_secs(20) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

fn wait(time: Duration) {
    let started = Instant::now();
    while started.elapsed() < time {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn an_audio_file_plays_in_the_viewer_with_its_scanbar() {
    if !mpv::available() {
        eprintln!("libmpv is not installed here; skipped");
        return;
    }
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let importer = FileImporter::new(store.clone(), MediaTools::new());
    for name in ["flac.flac", "flac_cover.flac"] {
        importer
            .import_path(
                &hydrus_testkit::fixture_path(format!("media/{name}")),
                &FileImportOptions::default(),
            )
            .unwrap();
    }
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:filetype is flac".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    assert!(files.len() >= 2, "{files:?}");
    for (at, &file) in files.iter().enumerate() {
        let info = store
            .read(|c| hydrus_store::media::load_basic(c, &[file]))
            .unwrap()
            .remove(0)
            .info
            .unwrap();
        let bar = Scanbar::new(info.duration_ms, info.num_frames).unwrap();
        ui.invoke_thumbnail_activated(i32::try_from(at).unwrap());
        let viewer = bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opened");
        // an audio file's bar is by time, not frames, and shows at once
        assert!(viewer.get_scanbar_shown());
        let text = || viewer.get_scanbar_text().to_string();
        assert!(!text().contains(" - "), "no frames: {}", text());
        assert!(text().starts_with("0.000/"), "{}", text());
        // it plays: the bar moves on
        let start = text();
        assert!(settle(&|| text() != start), "the bar never moved: {start}");
        assert!(viewer.get_scanbar_progress() > 0.0, "{}", text());
        // pause holds it; resuming moves it on
        viewer.invoke_toggle_pause();
        wait(Duration::from_millis(300));
        let held = text();
        wait(Duration::from_millis(500));
        assert_eq!(text(), held, "paused");
        viewer.invoke_toggle_pause();
        assert!(settle(&|| text() != held), "playing again: {held}");
        // a click half way along the bar goes there (the bar shows it at
        // once, as the reference's does)
        viewer.invoke_toggle_pause();
        let width = viewer.get_media_width().max(200.0);
        viewer.invoke_scan_started();
        viewer.invoke_scan(width / 2.0, width);
        viewer.invoke_scan_ended();
        let to = bar.seek_to(width / 2.0, width);
        assert_eq!(viewer.get_scanbar_text(), bar.at(to).1.as_str());
        wait(Duration::from_millis(800));
        let seeked = viewer.get_scanbar_progress();
        assert!((seeked - bar.at(to).0).abs() < 0.2, "{} {seeked}", text());
        // a file with sound has the volume control
        assert!(viewer.get_volume_shown());
        viewer.invoke_close_requested();
        assert!(bound.viewer.borrow().is_none());
    }
}
