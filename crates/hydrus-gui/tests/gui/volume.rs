//! The media viewer's volume control, as the reference's
//! (`ClientGUIMediaControls`): shown beside the scanbar of a file with
//! sound; the global mute (ctrl+g, in the viewer or the main window), the
//! viewer's own, and the volume the slider moves (the global one unless
//! the viewer uses its own), each kept.

use slint::ComponentHandle as _;

use hydrus_core::media_viewer::AudioSettings;
use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;

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

// leaf: audit-media-viewer-audio
#[test]
fn the_viewer_s_volume_control() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // hydrus's own: 70, nothing muted
    let kept = || -> AudioSettings { store.read(hydrus_store::settings::get).unwrap() };
    assert_eq!(kept(), AudioSettings::default());

    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let page = bound.current.borrow().clone();
    let index_of = |name: &str| {
        let id = store
            .read(|c| hydrus_store::master::hash_id(c, &hash_of(name)))
            .unwrap()
            .unwrap();
        let index = page.borrow().results().iter().position(|&f| f == id);
        i32::try_from(index.unwrap()).unwrap()
    };
    let viewer = || -> MediaViewerWindow {
        bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .unwrap()
    };

    // a silent animation mpv plays has no volume control; a file with
    // sound has
    ui.invoke_thumbnail_activated(index_of("gif_animated.gif"));
    assert!(viewer().get_scanbar_shown());
    assert!(!viewer().get_volume_shown());
    viewer().invoke_close_requested();
    ui.invoke_thumbnail_activated(index_of("audio.mp3"));
    let shown = viewer();
    assert!(shown.get_volume_shown());
    assert_eq!(shown.get_volume(), 70);
    assert!(!shown.get_global_muted() && !shown.get_viewer_muted());
    // drawn at the scanbar's right; with the pointer on it, the volume and
    // the viewer's mute show above it
    let drawn = windows.get(windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    let (right, bottom) = (
        shown.get_media_x() + shown.get_media_width(),
        shown.get_media_y() + shown.get_media_height(),
    );
    // (near the bar, which grows to its full height, then on the control)
    for up in [30.0, 10.0] {
        shown
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(right - 12.0, bottom - up),
            });
    }
    // (the scanbar stays full meanwhile)
    assert!(shown.get_volume_open());
    let pixels = headless::render(&drawn, 800, 600);
    let shots = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"));
    headless::save_png(&shots.join("volume.png"), &pixels, 800, 600).unwrap();
    // a click on the track's top (the open control is 160px: the track,
    // 10px either side, and the two mutes) sets the loudest
    shown
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(right - 12.0, bottom - 160.0 + 10.0),
            button: slint::platform::PointerEventButton::Left,
        });
    shown
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(right - 12.0, bottom - 160.0 + 10.0),
            button: slint::platform::PointerEventButton::Left,
        });
    assert_eq!(kept().global_volume, 100);
    shown.invoke_volume_changed(70);

    // the global mute, from the viewer and from the main window (ctrl+g)
    shown.invoke_flip_global_mute();
    assert!(kept().global_mute);
    assert!(shown.get_global_muted());
    ui.invoke_flip_global_mute();
    assert!(!kept().global_mute);
    assert!(!shown.get_global_muted());

    // the slider moves the global volume, as the viewer doesn't use its own
    shown.invoke_volume_changed(40);
    assert_eq!(kept().global_volume, 40);
    assert_eq!(kept().viewer_volume, 70);
    assert_eq!(shown.get_volume(), 40);
    // and the viewer's own mute
    shown.invoke_flip_viewer_mute();
    assert!(kept().viewer_mute);
    assert!(shown.get_viewer_muted());
    assert!(kept().viewer_muted());

    // a viewer using its own volume plays at it, and its slider moves it
    let own = AudioSettings {
        viewer_uses_its_own_volume: true,
        ..kept()
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &own))
        .unwrap();
    shown.invoke_audio_changed();
    assert_eq!(shown.get_volume(), 70);
    shown.invoke_volume_changed(25);
    assert_eq!(kept().viewer_volume, 25);
    assert_eq!(kept().global_volume, 40);
    assert_eq!(shown.get_volume(), 25);
    shown
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Control.into(),
        });
    shown
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: "g".into() });
    shown
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: "g".into() });
    shown
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Control.into(),
        });
    assert!(kept().global_mute, "ctrl+g in the viewer");
    shown.invoke_close_requested();
}
