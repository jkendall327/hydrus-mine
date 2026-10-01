//! The scanbar under a file mpv plays, as the reference's animation bar:
//! how far through it is (by frame, for an animation of several), where a
//! click on it goes, and the seek shortcuts' steps; and in the viewer,
//! seeking by the bar and by ctrl and the arrows (where libmpv is
//! installed).

// (the reference's arithmetic gives exact values here)
#![allow(clippy::float_cmp)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use hydrus_gui::scanbar::Scanbar;
use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless, mpv};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::ComponentHandle as _;
use slint::platform::{Key, WindowEvent};

#[test]
fn the_bar_shows_and_seeks_as_the_reference_s_does() {
    assert_eq!(Scanbar::new(None, Some(10)), None);
    assert_eq!(Scanbar::new(Some(0), Some(10)), None);
    let animation = Scanbar::new(Some(9600), Some(240)).unwrap();
    assert_eq!(
        animation.at(480.0),
        (12.0 / 239.0, "13/240 - 0.480/9.600".to_owned())
    );
    assert_eq!(
        animation.at(20000.0),
        (1.0, "240/240 - 9.600/9.600".to_owned())
    );
    // (frames rounded as Python rounds, halves to even)
    let four = Scanbar::new(Some(1000), Some(4)).unwrap();
    assert_eq!(four.at(125.0).1, "1/4 - 0.125/1.000");
    assert_eq!(four.at(375.0).1, "3/4 - 0.375/1.000");
    // a video: by time
    let video = Scanbar::new(Some(9600), None).unwrap();
    assert_eq!(video.at(4800.0), (0.5, "4.800/9.600".to_owned()));
    let single = Scanbar::new(Some(9600), Some(1)).unwrap();
    assert_eq!(single.at(4800.0).1, "4.800/9.600");
    // clicks: the nub's middle goes where it is clicked
    assert_eq!(video.seek_to(5.0, 110.0), 0.0);
    assert_eq!(video.seek_to(60.0, 110.0), 5280.0);
    assert_eq!(video.seek_to(-20.0, 110.0), 0.0);
    assert_eq!(video.seek_to(200.0, 110.0), 9600.0);
    // ctrl and left back 2.5 seconds, never before the start; ctrl and
    // right on 5, past the end round to the start
    assert_eq!(video.seek_delta(1000.0, -1, 2500), 0.0);
    assert_eq!(video.seek_delta(3000.0, -1, 2500), 500.0);
    assert_eq!(video.seek_delta(1000.0, 1, 5000), 6000.0);
    assert_eq!(video.seek_delta(8000.0, 1, 5000), 0.0);
}

#[test]
fn a_file_that_plays_seeks_by_its_bar_and_by_key() {
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
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let basic = store
        .read(|c| hydrus_store::media::load_basic(c, &files))
        .unwrap();
    let index_of = |mime: hydrus_core::Mime| {
        basic
            .iter()
            .position(|b| b.info.as_ref().is_some_and(|i| i.mime == mime))
            .unwrap()
    };
    let viewer = |index: usize| {
        ui.invoke_thumbnail_activated(i32::try_from(index).unwrap());
        bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .unwrap()
    };
    // a still has no bar
    let still = viewer(index_of(hydrus_core::Mime::ImagePng));
    assert!(!still.get_scanbar_shown());
    still.invoke_close_requested();
    if !mpv::available() {
        eprintln!("libmpv is not installed here; the viewer part is skipped");
        return;
    }
    // (the fixture's videos aren't in its local files: one is imported)
    let importer = hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
    importer
        .import_path(
            &hydrus_testkit::fixture_path("media/mp4_h264.mp4"),
            &hydrus_import::FileImportOptions::default(),
        )
        .unwrap();
    ui.invoke_search_edited("system:filetype is mp4".into());
    ui.invoke_search_accepted();
    let video = viewer(0);
    assert!(video.get_scanbar_shown());
    assert!(
        video.get_scanbar_text().starts_with("1/"),
        "{}",
        video.get_scanbar_text()
    );
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    let settle = |done: &dyn Fn() -> bool| {
        let started = Instant::now();
        while !done() && started.elapsed() < Duration::from_secs(20) {
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    // the bar follows playing
    settle(&|| video.get_scanbar_progress() > 0.0);
    assert!(
        video.get_scanbar_progress() > 0.0,
        "{}",
        video.get_scanbar_text()
    );
    // pause, then seek half way along the bar: the bar shows where the
    // click went at once, and mpv is there once it has seeked
    let file = bound.current.borrow().borrow().results()[0];
    let info = store
        .read(|c| hydrus_store::media::load_basic(c, &[file]))
        .unwrap()
        .remove(0)
        .info
        .unwrap();
    let bar = Scanbar::new(info.duration_ms, info.num_frames).unwrap();
    video.invoke_toggle_pause();
    let width = video.get_media_width();
    video.invoke_scan(width / 2.0, width);
    let to = bar.seek_to(width / 2.0, width);
    assert_eq!(to, (info.duration_ms.unwrap() / 2) as f64);
    let (half, half_text) = bar.at(to);
    assert_eq!(video.get_scanbar_progress(), half);
    assert_eq!(video.get_scanbar_text(), half_text.as_str());
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(500) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(20));
    }
    let seeked = video.get_scanbar_progress();
    assert!((seeked - half).abs() < 0.15, "{}", video.get_scanbar_text());
    let text = video.get_scanbar_text().to_string();
    // ctrl and left: 2.5 seconds back (or the start), the file shown as it
    // was (zoomed in, here)
    let window = video.window();
    window.dispatch_event(WindowEvent::KeyPressed { text: "+".into() });
    window.dispatch_event(WindowEvent::KeyReleased { text: "+".into() });
    let zoomed = video.get_media_width();
    assert!(zoomed > width);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::LeftArrow.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::LeftArrow.into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    // (the fixture's video is a second long: back to the start)
    assert_eq!(video.get_media_width(), zoomed, "not shown again");
    assert_eq!(
        video.get_scanbar_progress(),
        0.0,
        "{text} then {}",
        video.get_scanbar_text()
    );
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(500) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        video.get_scanbar_progress(),
        0.0,
        "still paused at the start"
    );
    assert!(video.get_caption().starts_with("1/"), "not moved on");
    video.invoke_close_requested();
}
