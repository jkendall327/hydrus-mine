//! Zooming and panning in the media viewer, as the reference's media
//! container does it with its default options and shortcuts: z switches
//! between 100% and canvas fit, + and - (and ctrl and the wheel) step
//! through the zooms about the pointer, shift and the arrow keys pan, and
//! dragging moves the file (in the media viewer; in the archive/delete
//! filter a click decides).

// (zooms and positions are exact, as the reference's are)
#![allow(clippy::float_cmp)]

use std::sync::Arc;

use hydrus_core::Mime;
use hydrus_core::media_viewer::{MediaViewerSettings, ZoomCentre, ZoomType};
use hydrus_gui::zoom::Zoom;
use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, LogicalPosition, SharedString};

fn big_jpeg() -> Zoom {
    Zoom::new(
        MediaViewerSettings::default(),
        Mime::ImageJpeg,
        Some((4000, 3000)),
        (1000, 750),
        1.0,
    )
}

#[test]
fn zooming_steps_about_the_pointer_and_switches() {
    // a big image opens fitted, centred
    let mut zoom = big_jpeg();
    assert_eq!(zoom.zoom(), 0.25);
    assert_eq!(zoom.rect(), (0, 0, 1000, 750));
    // zooming in about the pointer, a quarter of the way across and half
    // way down, keeps that point still
    zoom.zoom_in(Some((250, 375)));
    assert_eq!(zoom.zoom(), 0.3);
    assert_eq!(zoom.rect(), (-50, -75, 1200, 900));
    zoom.zoom_out(Some((250, 375)));
    assert_eq!(zoom.zoom(), 0.25);
    assert_eq!(zoom.rect(), (0, 0, 1000, 750));
    // without the pointer over the window, about its centre
    zoom.zoom_in(None);
    assert_eq!(zoom.rect(), (-100, -75, 1200, 900));
    // switching goes to 100%, about the pointer (the window's top left
    // stays on the same pixel), then back to fitting, centred
    zoom.switch(Some((0, 0)));
    assert_eq!(zoom.zoom(), 1.0);
    assert_eq!(zoom.rect(), (-333, -250, 4000, 3000));
    zoom.switch(None);
    assert_eq!(zoom.rect(), (0, 0, 1000, 750));
    // panning: a twelfth of the smaller of the file and the window
    zoom.pan(1, 0);
    zoom.pan(0, -2);
    assert_eq!(zoom.rect(), (83, -124, 1000, 750));
    zoom.drag((-83, 124));
    assert_eq!(zoom.rect(), (0, 0, 1000, 750));
    // nothing is ever bigger than 32000 pixels a side
    for _ in 0..30 {
        zoom.zoom_in(None);
    }
    assert_eq!(zoom.zoom(), 8.0);
    assert_eq!(zoom.rect().2, 32000);
    // resizing the window fits it again
    zoom.resize((500, 500), 1.0);
    assert_eq!(zoom.zoom(), 0.125);
    assert_eq!(zoom.rect(), (0, 62, 500, 375));
}

#[test]
fn a_file_dragged_away_is_rescued_when_zoomed() {
    let mut zoom = big_jpeg();
    zoom.drag((-5000, 0));
    assert_eq!(zoom.rect().0, -5000);
    // zoomed, it would be wholly off the window: a fifth of it comes back
    zoom.zoom_in(None);
    let (x, _, width, _) = zoom.rect();
    assert_eq!(width, 1200);
    assert_eq!(x + width - 1, width / 5, "{:?}", zoom.rect());
}

#[test]
fn zooms_follow_the_settings() {
    // a small image fits the window, as the defaults scale up
    let small = |settings: MediaViewerSettings| {
        Zoom::new(settings, Mime::ImagePng, Some((300, 200)), (1000, 750), 1.0)
    };
    let mut zoom = small(MediaViewerSettings::default());
    assert_eq!(zoom.zoom_of(ZoomType::Canvas), 1000.0 / 300.0);
    assert_eq!(zoom.rect(), (0, 41, 1000, 667));
    // centring on the media's top left instead
    let mut settings = MediaViewerSettings {
        zoom_centre: ZoomCentre::MediaTopLeft,
        ..MediaViewerSettings::default()
    };
    let mut top_left = small(settings.clone());
    top_left.switch(Some((999, 749)));
    assert_eq!(top_left.rect(), (350, 275, 300, 200), "recentred, fitting");
    top_left.zoom_out(Some((999, 749)));
    assert_eq!(top_left.rect(), (350, 275, 270, 180), "the top left stays");
    // a default zoom of 100% (shown at its size), and other steps
    settings.default_zoom_type = ZoomType::Full;
    settings.media_zooms = vec![0.5, 1.0, 4.0];
    let mut full = small(settings);
    assert_eq!(full.rect(), (350, 275, 300, 200));
    full.zoom_in(None);
    assert_eq!(full.zoom(), 1000.0 / 300.0, "canvas fit is a step");
    full.zoom_in(None);
    assert_eq!(full.zoom(), 4.0);
    full.zoom_in(None);
    assert_eq!(full.zoom(), 4.0);
    // a device pixel ratio of 2: 100% is half as many logical pixels
    zoom.resize((1000, 750), 2.0);
    zoom.switch(None);
    assert_eq!(zoom.zoom(), 1.0);
    assert_eq!(zoom.rect(), (425, 325, 150, 100));
    // what isn't shown (a PDF's thumbnail) fills the window, unzoomed
    let mut pdf = Zoom::new(
        MediaViewerSettings::default(),
        Mime::ApplicationPdf,
        Some((600, 800)),
        (1000, 750),
        1.0,
    );
    pdf.zoom_in(None);
    pdf.drag((10, 10));
    assert_eq!(pdf.rect(), (0, 0, 1000, 750));
}

#[test]
fn switching_files_keeps_the_zoom_as_the_duplicate_filter_does() {
    let settings = MediaViewerSettings::default();
    let at = |resolution: (u32, u32)| {
        Zoom::new(
            settings.clone(),
            Mime::ImagePng,
            Some(resolution),
            (1000, 750),
            1.0,
        )
    };
    // both landscape: as tall as the first was, where it was
    let mut zoom = at((4000, 3000));
    zoom.switch_to(Mime::ImagePng, Some((2000, 1000)));
    assert_eq!(zoom.zoom(), 0.75);
    assert_eq!(zoom.rect(), (0, 0, 1500, 750));
    zoom.pan(1, 1);
    zoom.switch_to(Mime::ImagePng, Some((4000, 3000)));
    assert_eq!(zoom.zoom(), 0.25);
    assert_eq!(zoom.rect(), (83, 62, 1000, 750));
    // both portrait: as wide, unless, at the default zoom filling the
    // canvas's height, that would spill a little over the bottom
    let mut zoom = at((600, 750));
    assert_eq!(zoom.rect(), (200, 0, 600, 750));
    zoom.switch_to(Mime::ImagePng, Some((600, 780)));
    assert_eq!(zoom.zoom(), 750.0 / 780.0);
    assert_eq!(zoom.rect(), (200, 0, 577, 750));
    let mut zoom = at((600, 750));
    zoom.switch_to(Mime::ImagePng, Some((300, 500)));
    assert_eq!(zoom.zoom(), 2.0, "as wide");
    // one of each: whichever side differs less (the first fills the
    // window, 1000x750)
    let mut zoom = at((800, 600));
    zoom.switch_to(Mime::ImagePng, Some((600, 900)));
    assert_eq!(zoom.zoom(), 1000.0 / 600.0, "as wide");
    let mut zoom = at((800, 600));
    zoom.switch_to(Mime::ImagePng, Some((400, 700)));
    assert_eq!(zoom.zoom(), 750.0 / 700.0, "as tall");
    // a file without a resolution takes its default zoom, where it was
    let mut zoom = at((4000, 3000));
    zoom.pan(-1, 0);
    zoom.switch_to(Mime::AudioMp3, None);
    assert_eq!(zoom.zoom(), 1.0);
    assert_eq!(zoom.rect(), (-83, 0, 360, 240));
}

#[test]
fn video_renders_at_the_size_shown_up_to_twice_the_window() {
    let mut video = Zoom::new(
        MediaViewerSettings::default(),
        Mime::VideoMp4,
        Some((3840, 2160)),
        (1000, 750),
        1.0,
    );
    assert_eq!(video.render_size(), (1000, 562));
    video.switch(None);
    assert_eq!(video.render_size(), (2000, 1125));
    // (what plays is never bigger than 8000 a side)
    for _ in 0..30 {
        video.zoom_in(None);
    }
    assert!(video.rect().2 <= 8000);
}

fn key(window: &slint::Window, key: impl Into<SharedString> + Clone) {
    window.dispatch_event(WindowEvent::KeyPressed {
        text: key.clone().into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

fn rect(viewer: &MediaViewerWindow) -> (f32, f32, f32, f32) {
    (
        viewer.get_media_x(),
        viewer.get_media_y(),
        viewer.get_media_width(),
        viewer.get_media_height(),
    )
}

#[test]
fn the_viewer_and_the_archive_delete_filter_zoom_and_pan() {
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
    ui.invoke_search_edited("system:filetype is jpeg".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let batch = store
        .read(|c| hydrus_store::media::load_basic(c, &files[..1]))
        .unwrap();
    let info = batch[0].info.as_ref().unwrap();
    let (width, height) = (info.width.unwrap() as f32, info.height.unwrap() as f32);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    slint::platform::update_timers_and_animations();
    let window = viewer.window();
    window.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(400.0, 300.0),
    });
    // fitted, centred
    let fitted = rect(&viewer);
    let fit = (800.0 / width).min(600.0 / height);
    assert!(
        (fitted.2 - (width * fit).floor()).abs() <= 1.0,
        "{fitted:?}"
    );
    assert!(fitted.2 == 800.0 || fitted.3 == 600.0, "{fitted:?}");
    // drawn sharply, once rendered: exactly the reference's resize of the
    // file to the pixels it covers
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    while !viewer.get_sharp_shown() && std::time::Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(viewer.get_sharp_shown(), "the sharp still arrived");
    let sharp = (
        viewer.get_sharp_x(),
        viewer.get_sharp_y(),
        viewer.get_sharp_width(),
        viewer.get_sharp_height(),
    );
    assert_eq!(sharp, fitted);
    let hash = &batch[0].hash;
    let path = store.snapshot().storage.file_path(hash, info.mime).unwrap();
    let still = hydrus_media::decode_image(&std::fs::read(path).unwrap()).unwrap();
    let plan = hydrus_gui::still::plan(
        (
            fitted.0 as i32,
            fitted.1 as i32,
            fitted.2 as i32,
            fitted.3 as i32,
        ),
        (800, 600),
        1.0,
        (still.width(), still.height()),
        &MediaViewerSettings::default().view(Mime::ImageJpeg).zoom,
    )
    .unwrap();
    let expected = hydrus_gui::still::render(&still, &plan);
    let pixels = headless::render(&drawn, 800, 600);
    let (x0, y0) = (fitted.0 as usize, fitted.1 as usize);
    let (columns, rows) = (expected.width() as usize, expected.height() as usize);
    for row in 0..rows {
        for column in 0..columns {
            let on_screen = &pixels[((y0 + row) * 800 + x0 + column) * 4..][..3];
            let rendered = &expected.data()[(row * columns + column) * 3..][..3];
            assert_eq!(on_screen, rendered, "at {column},{row}");
        }
    }
    // z: 100% (nothing to resize, so drawn as it is); z again: fitted
    key(window, "z");
    assert!(!viewer.get_sharp_shown());
    assert_eq!((rect(&viewer).2, rect(&viewer).3), (width, height));
    key(window, "z");
    assert_eq!(rect(&viewer), fitted);
    // + and -
    key(window, "+");
    assert!(rect(&viewer).2 > fitted.2);
    key(window, "-");
    assert_eq!(rect(&viewer), fitted);
    // ctrl and the wheel
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(400.0, 300.0),
        delta_x: 0.0,
        delta_y: 120.0,
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    let zoomed = rect(&viewer);
    assert!(zoomed.2 > fitted.2, "{zoomed:?}");
    assert_eq!(
        viewer.get_caption(),
        format!("1/{}", files.len()),
        "not moved on"
    );
    // shift and right pans right a twelfth of the window
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    key(window, Key::RightArrow);
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    let step = (zoomed.2.min(800.0) / 12.0).floor();
    assert_eq!(rect(&viewer).0, zoomed.0 + step);
    assert_eq!(
        viewer.get_caption(),
        format!("1/{}", files.len()),
        "not moved on"
    );
    // dragging moves it with the pointer
    let before = rect(&viewer);
    let at = |x: f32, y: f32| LogicalPosition::new(x, y);
    window.dispatch_event(WindowEvent::PointerPressed {
        position: at(400.0, 300.0),
        button: PointerEventButton::Left,
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: at(380.0, 310.0),
    });
    window.dispatch_event(WindowEvent::PointerMoved {
        position: at(370.0, 330.0),
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: at(370.0, 330.0),
        button: PointerEventButton::Left,
    });
    assert_eq!(rect(&viewer).0, before.0 - 30.0);
    assert_eq!(rect(&viewer).1, before.1 + 30.0);
    // the next file opens at its default zoom
    key(window, Key::RightArrow);
    assert_eq!(viewer.get_caption(), format!("2/{}", files.len()));
    let next = rect(&viewer);
    assert!(next.2 == 800.0 || next.3 == 600.0, "{next:?}");
    // a resized window fits it again
    headless::render(&drawn, 400, 300);
    slint::platform::update_timers_and_animations();
    let resized = rect(&viewer);
    assert!(resized.2 == 400.0 || resized.3 == 300.0, "{resized:?}");
    // enter closes the viewer, as escape does
    key(window, Key::Return);
    assert!(bound.viewer.borrow().is_none());

    // the archive/delete filter zooms and pans the same way
    ui.invoke_thumbnail_clicked(0, false, false);
    ui.invoke_archive_delete_filter();
    let filter = bound
        .archive_delete
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    slint::platform::update_timers_and_animations();
    let window = filter.window();
    window.dispatch_event(WindowEvent::PointerMoved {
        position: LogicalPosition::new(400.0, 300.0),
    });
    let rect = |f: &hydrus_gui::ArchiveDeleteWindow| {
        (
            f.get_media_x(),
            f.get_media_y(),
            f.get_media_width(),
            f.get_media_height(),
        )
    };
    assert_eq!(rect(&filter), fitted);
    key(window, "z");
    assert_eq!((rect(&filter).2, rect(&filter).3), (width, height));
    key(window, "z");
    key(window, "+");
    assert!(rect(&filter).2 > fitted.2);
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    window.dispatch_event(WindowEvent::PointerScrolled {
        position: LogicalPosition::new(400.0, 300.0),
        delta_x: 0.0,
        delta_y: -120.0,
    });
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    assert_eq!(rect(&filter), fitted, "zoomed back out");
    // shift and up pans, rather than skipping
    window.dispatch_event(WindowEvent::KeyPressed {
        text: Key::Shift.into(),
    });
    key(window, Key::UpArrow);
    window.dispatch_event(WindowEvent::KeyReleased {
        text: Key::Shift.into(),
    });
    assert_eq!(
        rect(&filter).1,
        fitted.1 - (fitted.3.min(600.0) / 12.0).floor()
    );
    assert_eq!(filter.get_caption(), "1/1");
    // with nothing decided, enter closes it
    key(window, Key::Return);
    assert!(bound.archive_delete.borrow().is_none());
}

#[test]
fn zooming_to_max_is_as_the_reference_s() {
    // (`ZoomMax`: the largest step, 2000%; a still may be up to 32000
    // logical pixels a side)
    let mut small = Zoom::new(
        MediaViewerSettings::default(),
        Mime::ImagePng,
        Some((300, 200)),
        (1000, 750),
        1.0,
    );
    assert!(!small.at_max());
    small.zoom_max();
    assert!((small.zoom() - 20.0).abs() < f64::EPSILON);
    assert!(small.at_max());
    // a big one stops at the largest size, which is its max too
    let mut big = big_jpeg();
    big.zoom_max();
    assert!(big.zoom() < 20.0, "{}", big.zoom());
    let (width, height) = big.size();
    assert!(width == 32000 || height == 32000, "{width}x{height}");
    assert!(big.at_max());
    // with exact zooms only, the largest doubling under the largest step
    let mut settings = MediaViewerSettings::default();
    for view in settings.media_view.values_mut() {
        view.zoom.exact_zooms_only = true;
    }
    let mut exact = Zoom::new(settings, Mime::ImagePng, Some((30, 20)), (1000, 750), 1.0);
    exact.zoom_max();
    assert!(
        (exact.zoom() - 16.0).abs() < f64::EPSILON,
        "{}",
        exact.zoom()
    );
}

#[test]
fn resizing_preserves_or_recenters_the_recorded_zoom_and_pan() {
    let fixture = hydrus_testkit::fixture_json("viewer_canvas_options.json");
    let resolution = &fixture["media_resolution"];
    for event in fixture["resizes"].as_array().unwrap() {
        let mut zoom = Zoom::new(
            MediaViewerSettings {
                media_zooms: vec![1.0, 2.0],
                ..MediaViewerSettings::default()
            },
            Mime::ImageJpeg,
            Some((
                resolution[0].as_u64().unwrap() as u32,
                resolution[1].as_u64().unwrap() as u32,
            )),
            (1000, 750),
            1.0,
        );
        zoom.switch(None);
        // A direct two-times step matches the reference's _TryToChangeZoom(2).
        zoom.zoom_in(None);
        assert_eq!(zoom.zoom(), event["before"]["zoom"].as_f64().unwrap());
        zoom.drag((37, -19));
        let recorded_rect = |key: &str| {
            let values = event[key]["rect"].as_array().unwrap();
            (
                values[0].as_i64().unwrap() as i32,
                values[1].as_i64().unwrap() as i32,
                values[2].as_i64().unwrap() as i32,
                values[3].as_i64().unwrap() as i32,
            )
        };
        assert_eq!(zoom.rect(), recorded_rect("before"));
        zoom.resize_with_policy((800, 600), 1.0, event["recenter"].as_bool().unwrap());
        assert_eq!(zoom.rect(), recorded_rect("after"));
        assert_eq!(zoom.zoom(), event["after"]["zoom"].as_f64().unwrap());
    }
}
