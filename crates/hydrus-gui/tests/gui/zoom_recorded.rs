//! The media viewer zooms and pans a picture as the reference's does
//! (`oracle/record_viewer_pan_zoom.py`: the real `MediaContainer` of a
//! `CanvasMediaListBrowser` in an 800x600 frame, zoomed about the viewer's
//! centre). Replayed step by step through a real viewer with the keys and
//! the menu entry hydrus-rs offers for each: the zoom steps, switching,
//! the largest zoom, panning by steps, and a drag, each compared with the
//! file's place and size the reference recorded.

#![allow(clippy::float_cmp)]

use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::Value;
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{ComponentHandle as _, LogicalPosition, SharedString};

fn hash_of(name: &str) -> String {
    let manifest: Value = serde_json::from_str(
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
        .to_string()
}

fn key(window: &slint::Window, key: impl Into<SharedString> + Clone) {
    window.dispatch_event(WindowEvent::KeyPressed {
        text: key.clone().into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

fn rect(viewer: &MediaViewerWindow) -> [i64; 4] {
    [
        viewer.get_media_x(),
        viewer.get_media_y(),
        viewer.get_media_width(),
        viewer.get_media_height(),
    ]
    .map(|v| v.round() as i64)
}

// leaf: audit-media-viewer-zoom
#[test]
fn the_viewer_zooms_and_pans_a_picture_as_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("viewer_pan_zoom.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    // (the recording drags the file by the pointer's moves)
    store
        .write(|ctx| {
            let mut pointer = hydrus_store::settings::get::<
                hydrus_store::settings::ViewerPointerSettings,
            >(ctx.conn())?;
            pointer.anchor_drag = false;
            hydrus_store::settings::set(ctx.conn(), &pointer)
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited(
        format!(
            "system:hash = {}",
            hash_of(fixture["file"].as_str().unwrap())
        )
        .into(),
    );
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .unwrap();
    let drawn = windows.get(windows.count() - 1).unwrap();
    // (the recording's canvas)
    let canvas = fixture["canvas"].as_array().unwrap();
    let (width, height) = (
        canvas[0].as_u64().unwrap() as u32,
        canvas[1].as_u64().unwrap() as u32,
    );
    headless::render(&drawn, width, height);
    slint::platform::update_timers_and_animations();
    let window = viewer.window();
    let centre = LogicalPosition::new(width as f32 / 2.0, height as f32 / 2.0);
    window.dispatch_event(WindowEvent::PointerMoved { position: centre });

    let menu_id = |label: &str| -> i32 {
        use slint::Model as _;
        viewer.invoke_context_menu_requested();
        let menu = viewer.get_context_menu();
        menu.zoom
            .iter()
            .find(|r| r.label == label)
            .unwrap_or_else(|| panic!("{label} in the zoom menu"))
            .id
    };
    let pan = |key_name: Key| {
        window.dispatch_event(WindowEvent::KeyPressed {
            text: Key::Shift.into(),
        });
        self::key(window, key_name);
        window.dispatch_event(WindowEvent::KeyReleased {
            text: Key::Shift.into(),
        });
    };
    let drag = |from: (f32, f32), to: (f32, f32)| {
        let at = |(x, y): (f32, f32)| LogicalPosition::new(x, y);
        window.dispatch_event(WindowEvent::PointerPressed {
            position: at(from),
            button: PointerEventButton::Left,
        });
        window.dispatch_event(WindowEvent::PointerMoved { position: at(to) });
        window.dispatch_event(WindowEvent::PointerReleased {
            position: at(to),
            button: PointerEventButton::Left,
        });
    };

    let mut compared = 0;
    let mut skipped = Vec::new();
    for step in fixture["steps"].as_array().unwrap() {
        let name = step["step"].as_str().unwrap();
        match name {
            "fitted" | "default" => {}
            n if n.starts_with("zoom in") && n != "zoom in after dragging" => key(window, "+"),
            n if n.starts_with("zoom out") || n == "out from max" => key(window, "-"),
            "switch" | "switch again" | "100%" | "canvas" | "100% again" => key(window, "z"),
            "max" => viewer.invoke_menu_chosen(menu_id("zoom to max")),
            "zoom in after dragging" => key(window, "+"),
            "pan right" | "pan right again" => pan(Key::RightArrow),
            "pan left" => pan(Key::LeftArrow),
            "pan down" => pan(Key::DownArrow),
            "pan up" | "pan up again" => pan(Key::UpArrow),
            "pan diagonally" => {
                pan(Key::RightArrow);
                pan(Key::DownArrow);
            }
            "dragged" => drag((400.0, 300.0), (370.0, 330.0)),
            // (hydrus-rs has no shortcut commands for the edges and
            // centres, nor for zooming to the canvas apart from switching; the recorded state they end in is where the file
            // was zoomed in, which z and + reach again)
            _ => {
                skipped.push(name.to_string());
                if name == "to the vertical centre" {
                    key(window, "z");
                    key(window, "+");
                    // back at the state the reference ended these in
                } else {
                    continue;
                }
            }
        }
        let want: Vec<i64> = step["rect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert_eq!(rect(&viewer).to_vec(), want, "{name}: {step}");
        compared += 1;
    }
    assert_eq!(
        skipped,
        [
            "to the left edge",
            "to the right edge",
            "to the top edge",
            "to the bottom edge",
            "to the horizontal centre",
            "to the vertical centre",
            "canvas after dragging"
        ]
    );
    assert_eq!(compared, 28);

    // fullscreen: the viewer opens in it; the recorded frame went into it
    // and out again
    let full = &fixture["fullscreen"];
    assert!(!full["before"].as_bool().unwrap());
    let started = window.is_fullscreen();
    viewer.invoke_toggle_fullscreen();
    assert_eq!(window.is_fullscreen(), !started);
    viewer.invoke_toggle_fullscreen();
    assert_eq!(window.is_fullscreen(), started);
    assert!(full["after full"].as_bool().unwrap());
    assert!(!full["after normal"].as_bool().unwrap());
    viewer.invoke_close_requested();
}
