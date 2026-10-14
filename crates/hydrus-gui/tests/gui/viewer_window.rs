//! The media viewer's window as hydrus frames it: fullscreen (and
//! maximised beneath) by hydrus's default, F switching between fullscreen
//! and the window it was, and its size and place kept as it closes only
//! if hydrus's option says to.

use slint::ComponentHandle as _;

use hydrus_core::windows::{FrameLocation, WindowSettings};
use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

#[test]
fn the_viewer_opens_fullscreen_and_f_switches() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let kept = || -> WindowSettings { store.read(hydrus_store::settings::get).unwrap() };
    // hydrus's own frames come across
    assert_eq!(kept(), WindowSettings::default());

    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    // (as the client places its main window: maximised)
    hydrus_gui::windows::place(ui.window(), &kept().main_gui);
    assert!(ui.window().is_maximized());
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let viewer = || -> MediaViewerWindow {
        bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .unwrap()
    };
    ui.invoke_thumbnail_activated(0);
    let shown = viewer();
    // (never fullscreen on macOS, as the reference: there it is maximised,
    // and f does nothing)
    let fullscreens = !cfg!(target_os = "macos");
    assert_eq!(shown.window().is_fullscreen(), fullscreens);
    // (opened both maximised and fullscreen, it is fullscreen and not
    // maximised, as Qt's showFullScreen leaves it)
    assert_eq!(shown.window().is_maximized(), !fullscreens);
    // f: out of fullscreen, back to maximised; and in again
    shown.invoke_toggle_fullscreen();
    assert!(!shown.window().is_fullscreen());
    assert!(shown.window().is_maximized());
    shown.invoke_toggle_fullscreen();
    assert_eq!(shown.window().is_fullscreen(), fullscreens);
    // hydrus's default doesn't keep the viewer's frame as it closes
    shown.invoke_close_requested();
    assert_eq!(kept(), WindowSettings::default());

    // with the option on, a windowed viewer opens at its last size and
    // keeps the size it closes at
    let windowed = WindowSettings {
        media_viewer: FrameLocation {
            maximised: false,
            fullscreen: false,
            ..FrameLocation::media_viewer()
        },
        save_media_viewer_on_close: true,
        ..WindowSettings::default()
    };
    let saved = windowed.clone();
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &saved))
        .unwrap();
    ui.invoke_thumbnail_activated(0);
    let shown = viewer();
    assert!(!shown.window().is_fullscreen() && !shown.window().is_maximized());
    let scale = shown.window().scale_factor();
    assert_eq!(
        shown.window().size().to_logical(scale),
        slint::LogicalSize::new(640.0, 480.0)
    );
    shown
        .window()
        .set_size(slint::LogicalSize::new(900.0, 650.0));
    shown.invoke_close_requested();
    assert_eq!(kept().media_viewer.last_size, Some((900, 650)));
    assert!(!kept().media_viewer.fullscreen);
    // and the frame's own close button closes it as escape does
    ui.invoke_thumbnail_activated(0);
    assert!(bound.viewer.borrow().is_some());
    viewer()
        .window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    assert!(bound.viewer.borrow().is_none());
}

/// "Save media viewer window size and position on close", off and on, as the
/// reference's frame keeps them (`oracle/record_viewer_close_geometry.py`: a
/// real media viewer frame remembering its size, opened at 640x480, resized,
/// and closed at once, before its own delayed save can run): the real
/// viewer is opened and closed the same way, and the frame location it leaves
/// is the recorded one. (A viewer left a moment after the resize has already
/// kept it in the reference; hydrus-rs keeps the size only on close, see
/// DIFFERENCES.md.)
// leaf: audit-options-gui-frame-locations-save-media-viewer-window-size-and-position-on-close
#[test]
fn the_viewer_keeps_its_size_on_close_only_when_the_option_is_on() {
    let fixture = hydrus_testkit::fixture_json("viewer_close_geometry.json");
    let resized = fixture["resized_to"].as_array().unwrap();
    let resized = slint::LogicalSize::new(
        resized[0].as_f64().unwrap() as f32,
        resized[1].as_f64().unwrap() as f32,
    );
    let _windows = headless::init();
    let mut seen = 0;
    for run in fixture["runs"].as_array().unwrap() {
        if run["scenario"] != "closed at once after resizing" {
            continue;
        }
        let save_on_close = run["save_on_close"].as_bool().unwrap();
        let legacy = hydrus_testkit::legacy_fixture("basic");
        let native = tempfile::tempdir().unwrap();
        import_legacy(
            legacy.path(),
            &native.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = Store::open(native.path()).unwrap();
        let kept = || -> WindowSettings { store.read(hydrus_store::settings::get).unwrap() };
        // as the recording started: a window remembering its size, at 640x480
        let opened = &run["opened"];
        let size = |size: &serde_json::Value| -> Option<(i32, i32)> {
            let a = size.as_array()?;
            Some((a[0].as_i64()? as i32, a[1].as_i64()? as i32))
        };
        let windowed = WindowSettings {
            media_viewer: FrameLocation {
                maximised: false,
                fullscreen: false,
                last_size: size(&opened[2]["last_size"]),
                ..FrameLocation::media_viewer()
            },
            save_media_viewer_on_close: save_on_close,
            ..WindowSettings::default()
        };
        let saved = windowed.clone();
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &saved))
            .unwrap();
        let ui = MainWindow::new().unwrap();
        let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
        ui.invoke_search_edited("system:everything".into());
        ui.invoke_search_accepted();
        ui.invoke_thumbnail_activated(0);
        let shown = bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .unwrap();
        let scale = shown.window().scale_factor();
        let opened_size = shown.window().size().to_logical(scale);
        assert_eq!(
            (opened_size.width as u64, opened_size.height as u64),
            (opened[0].as_u64().unwrap(), opened[1].as_u64().unwrap()),
            "opens at the remembered size"
        );
        shown.window().set_size(resized);
        shown.invoke_close_requested();
        assert!(bound.viewer.borrow().is_none());
        assert_eq!(
            kept().media_viewer.last_size,
            size(&run["closed"]["last_size"]),
            "{run}"
        );
        assert_eq!(kept().media_viewer.maximised, run["closed"]["maximised"]);
        assert_eq!(kept().media_viewer.fullscreen, run["closed"]["fullscreen"]);
        seen += 1;
    }
    assert_eq!(seen, 2);
}

#[test]
fn the_viewer_s_browsing_shortcuts() {
    use slint::platform::{Key, PointerEventButton, WindowEvent};

    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().len();
    assert!(files > 3);
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let press = |key: slint::SharedString| {
        viewer
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
        viewer
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text: key });
    };
    // as the reference's default browser shortcuts: end and home, down and
    // up (as right and left), round the ends
    let at = |n: usize| format!("{n}/{files}");
    assert_eq!(viewer.get_caption(), at(1));
    press(Key::End.into());
    assert_eq!(viewer.get_caption(), at(files));
    press(Key::Home.into());
    assert_eq!(viewer.get_caption(), at(1));
    press(Key::DownArrow.into());
    assert_eq!(viewer.get_caption(), at(2));
    press(Key::UpArrow.into());
    press(Key::UpArrow.into());
    assert_eq!(viewer.get_caption(), at(files));
    // a middle click closes it
    viewer.window().dispatch_event(WindowEvent::PointerPressed {
        position: slint::LogicalPosition::new(300.0, 300.0),
        button: PointerEventButton::Middle,
    });
    assert!(bound.viewer.borrow().is_none());

    // and so does a double-click (one click doesn't; nor do two apart)
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let click = |x: f32| {
        let position = slint::LogicalPosition::new(x, 300.0);
        for event in [
            WindowEvent::PointerMoved { position },
            WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            },
            WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            },
        ] {
            viewer.window().dispatch_event(event);
        }
    };
    click(300.0);
    assert!(bound.viewer.borrow().is_some(), "one click leaves it open");
    click(400.0);
    assert!(bound.viewer.borrow().is_some(), "two clicks apart too");
    click(400.0);
    assert!(bound.viewer.borrow().is_none(), "a double-click closes it");
}

#[test]
fn closing_an_older_viewer_releases_its_canvas_without_closing_its_successor() {
    let (_directories, store) = crate::subscriptions::store();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store)));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let older = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    ui.invoke_thumbnail_activated(1);
    let successor = bound.viewer.borrow().as_ref().unwrap().clone_strong();
    assert!(older.window().is_visible());
    assert!(successor.window().is_visible());
    older.invoke_close_requested();
    assert!(!older.window().is_visible());
    assert!(!older.get_sharp_shown());
    assert!(successor.window().is_visible());
    assert!(std::ptr::eq(
        bound.viewer.borrow().as_ref().unwrap().window(),
        successor.window()
    ));
    // Holding the old Slint handle and invoking a late zoom must not bring
    // its sharp canvas back or clear a successor, even after repeated Close.
    older.invoke_zoom(1, false, 0.0, 0.0);
    older.invoke_close_requested();
    assert!(!older.get_sharp_shown());
    assert!(successor.window().is_visible());
    assert!(bound.viewer.borrow().is_some());
    successor.invoke_close_requested();
    assert!(!successor.window().is_visible());
    assert!(bound.viewer.borrow().is_none());
}
