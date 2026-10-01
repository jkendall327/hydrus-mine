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
    assert!(shown.window().is_maximized());
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
}
