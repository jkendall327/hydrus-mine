//! The media playback page's zoom and position box against the real media
//! viewer: each option is changed in the options window, applied, and the
//! viewer that opens afterwards zooms as the saved value says.

// (zooms and positions are exact, as the reference's are)
#![allow(clippy::float_cmp)]

use slint::platform::WindowEvent;
use slint::{ComponentHandle as _, SharedString};

use hydrus_gui::{MediaViewerWindow, headless};

use crate::options_gui_support::{box_of, items, row, show_page};
use crate::options_media_support::Media;

pub(crate) const CANVAS: (f32, f32) = (800.0, 600.0);

/// Set a dropdown of `page` to the choice named `choice`, apply.
fn choose(client: &Media, label: &str, choice: &str, box_title: &str) {
    let options = client.open_options();
    show_page(&options, "media playback");
    let (i, found) = row(&options, label);
    assert_eq!(found.kind, 5, "{label:?} is a dropdown");
    assert_eq!(box_of(&options, label), box_title);
    let at = items(&found)
        .iter()
        .position(|c| c == choice)
        .unwrap_or_else(|| panic!("{choice:?} in {:?}", items(&found)));
    options.invoke_choice_chosen(i, at as i32);
    options.invoke_apply();
    options.hide().unwrap();
}

/// The viewer on the first small jpeg (smaller than the canvas both
/// ways), and that jpeg's size, with its window drawn once.
pub(crate) fn small_jpeg_viewer(client: &Media) -> (MediaViewerWindow, (f32, f32)) {
    // (a second identical search finds nothing: search once)
    if client.results().is_empty() {
        client.search("system:filetype is jpeg");
    }
    let files = client.results();
    let batch = client
        .store
        .read(|c| hydrus_store::media::load_basic(c, &files))
        .unwrap();
    let (at, size) = batch
        .iter()
        .enumerate()
        .find_map(|(i, m)| {
            let info = m.info.as_ref()?;
            let (w, h) = (info.width? as f32, info.height? as f32);
            (w < 400.0 && h < 300.0).then_some((i, (w, h)))
        })
        .unwrap_or_else(|| panic!("a small jpeg in {} files", files.len()));
    client.ui.invoke_thumbnail_activated(at as i32);
    let viewer = client
        .bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opens");
    let drawn = client.windows.get(client.windows.count() - 1).unwrap();
    headless::render(&drawn, 800, 600);
    slint::platform::update_timers_and_animations();
    (viewer, size)
}

pub(crate) fn rect(viewer: &MediaViewerWindow) -> (f32, f32, f32, f32) {
    (
        viewer.get_media_x(),
        viewer.get_media_y(),
        viewer.get_media_width(),
        viewer.get_media_height(),
    )
}

fn key(window: &slint::Window, key: impl Into<SharedString> + Clone) {
    window.dispatch_event(WindowEvent::KeyPressed {
        text: key.clone().into(),
    });
    window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

// leaf: audit-options-media-playback-zoom-and-position-media-viewer-default-zoom
#[test]
fn the_default_zoom_and_the_zoom_steps_are_the_viewer_s() {
    let client = Media::basic();
    // as the reference ships: a small file opens fitted to the canvas
    let (viewer, (w, h)) = small_jpeg_viewer(&client);
    let fitted = rect(&viewer);
    assert!(fitted.2 == CANVAS.0 || fitted.3 == CANVAS.1, "{fitted:?}");
    viewer.invoke_close_requested();

    // the dropdown has the reference's choices
    let options = client.open_options();
    show_page(&options, "media playback");
    let (_, found) = row(&options, "Media Viewer default zoom:");
    assert_eq!(
        items(&found),
        [
            "default for filetype",
            "100% zoom",
            "canvas fit",
            "fill horizontally",
            "fill vertically",
            "canvas fill"
        ]
    );
    options.hide().unwrap();

    choose(
        &client,
        "Media Viewer default zoom:",
        "100% zoom",
        "zoom and position",
    );
    let saved: hydrus_core::media_viewer::MediaViewerSettings = client.setting();
    assert_eq!(
        saved.default_zoom_type,
        hydrus_core::media_viewer::ZoomType::Full
    );
    let (viewer, _) = small_jpeg_viewer(&client);
    assert_eq!(rect(&viewer).2, w, "opens at 100%");
    assert_eq!(rect(&viewer).3, h);
    // the steps are the default ones: 100% to the next, then the fit
    let window = viewer.window();
    key(window, "+");
    let default_step = rect(&viewer).2;
    assert!(default_step > w, "{default_step}");
    viewer.invoke_close_requested();

    // a list of its own: 100% steps to 3x
    let options = client.open_options();
    show_page(&options, "media playback");
    let (i, found) = row(&options, "Media zooms:");
    assert_eq!(box_of(&options, "Media zooms:"), "zoom and position");
    assert_ne!(found.text, "0.5, 1, 3");
    options.invoke_text_edited(i, "0.5, 1, 3".into());
    options.invoke_apply();
    options.hide().unwrap();
    let (viewer, _) = small_jpeg_viewer(&client);
    let window = viewer.window();
    assert_eq!(rect(&viewer).2, w);
    key(window, "+");
    assert_eq!(rect(&viewer).2, 3.0 * w);
    key(window, "-");
    key(window, "-");
    assert_eq!(rect(&viewer).2, 0.5 * w);
    assert_ne!(default_step, 3.0 * w, "the default steps are not these");
}

#[test]
fn zooming_keeps_the_centerpoint_the_option_names() {
    const LABEL: &str = "Centerpoint for media zooming:";
    let client = Media::basic();
    choose(
        &client,
        "Media Viewer default zoom:",
        "100% zoom",
        "zoom and position",
    );
    let centre = |r: (f32, f32, f32, f32)| (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
    // (whole pixels: a half may round either way)
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() <= 1.0 && (a.1 - b.1).abs() <= 1.0;
    let zoom_in = |viewer: &MediaViewerWindow| {
        let before = rect(viewer);
        key(viewer.window(), "+");
        (before, rect(viewer))
    };

    // the reference's choices; about the viewer's centre
    let options = client.open_options();
    show_page(&options, "media playback");
    let (_, found) = row(&options, LABEL);
    assert_eq!(
        items(&found),
        [
            "viewer center",
            "mouse (or viewer center if mouse outside)",
            "media center",
            "media top-left"
        ]
    );
    assert_eq!(found.index, 1, "the default is the mouse");
    options.hide().unwrap();
    choose(&client, LABEL, "viewer center", "zoom and position");
    let (viewer, _) = small_jpeg_viewer(&client);
    let (before, after) = zoom_in(&viewer);
    assert!(after.2 > before.2);
    assert!(near(centre(before), (CANVAS.0 / 2.0, CANVAS.1 / 2.0)));
    assert!(
        near(centre(after), centre(before)),
        "the viewer's centre stays"
    );
    viewer.invoke_close_requested();

    // about the media's top left, that corner stays where it was
    choose(&client, LABEL, "media top-left", "zoom and position");
    let (viewer, _) = small_jpeg_viewer(&client);
    let (before, after) = zoom_in(&viewer);
    assert!(after.2 > before.2);
    assert_eq!(
        (after.0, after.1),
        (before.0, before.1),
        "{before:?} {after:?}"
    );
    viewer.invoke_close_requested();

    // about the media's centre, that stays
    choose(&client, LABEL, "media center", "zoom and position");
    let (viewer, _) = small_jpeg_viewer(&client);
    let (before, after) = zoom_in(&viewer);
    assert!(near(centre(after), centre(before)), "{before:?} {after:?}");
}

/// The viewer zooming in and out through the media zooms list, about the
/// centrepoint the option names, against the reference's real viewer
/// (oracle/record_viewer_zoom_steps.py): 140 runs of 27 steps.
// leaf: audit-options-media-playback-zoom-and-position-media-zooms
// leaf: audit-options-media-playback-zoom-and-position-centerpoint-for-media-zooming
#[test]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn zooming_steps_through_the_list_about_the_centrepoint_as_the_reference_does() {
    let recorded: serde_json::Value = hydrus_testkit::fixture_json("viewer_zoom_steps.json");
    let client = Media::basic();
    client.search("system:everything");
    let files = client.results();
    let at_of = |file: &serde_json::Value| -> usize {
        let hash = hex::decode(file["hash"].as_str().unwrap()).unwrap();
        let hash = hydrus_core::Sha256::from_slice(&hash).unwrap();
        let id = client
            .store
            .read(|c| hydrus_store::master::hash_id(c, &hash))
            .unwrap()
            .unwrap();
        files.iter().position(|f| *f == id).unwrap()
    };
    let positions: std::collections::HashMap<String, usize> = recorded["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["label"].as_str().unwrap().to_owned(), at_of(f)))
        .collect();
    let mut applied: Option<(String, i64)> = None;
    let mut compared = 0;
    for case in recorded["cases"].as_array().unwrap() {
        // the options, set in the Options window
        let zooms: Vec<f64> = serde_json::from_value(case["zooms"].clone()).unwrap();
        let text = zooms
            .iter()
            .map(|z| format!("{z}"))
            .collect::<Vec<_>>()
            .join(", ");
        let centre = case["centre"].as_i64().unwrap();
        if applied.as_ref() != Some(&(text.clone(), centre)) {
            let options = client.open_options();
            show_page(&options, "media playback");
            let (i, _) = row(&options, "Media zooms:");
            options.invoke_text_edited(i, text.clone().into());
            let (i, found) = row(&options, "Centerpoint for media zooming:");
            // the reference's constants: 1 viewer centre, 2 mouse, 0 media centre, 3 top-left
            let choice = [2, 0, 1, 3][usize::try_from(centre).unwrap()];
            if applied.is_none() {
                assert_eq!(found.index, 1, "the default is the mouse");
            }
            options.invoke_choice_chosen(i, choice);
            options.invoke_apply();
            options.hide().unwrap();
            applied = Some((text.clone(), centre));
        }
        client
            .ui
            .invoke_thumbnail_activated(positions[case["file"].as_str().unwrap()] as i32);
        let viewer = client
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opens");
        let drawn = client.windows.get(client.windows.count() - 1).unwrap();
        headless::render(&drawn, 800, 600);
        slint::platform::update_timers_and_animations();
        let what = format!(
            "{} with zooms {text}, centre {centre}, mouse {}, dragged {}",
            case["file"], case["mouse"], case["drag"]
        );
        let theirs = |step: &serde_json::Value| -> (f32, f32, f32, f32) {
            (
                step["x"].as_f64().unwrap() as f32,
                step["y"].as_f64().unwrap() as f32,
                step["w"].as_f64().unwrap() as f32,
                step["h"].as_f64().unwrap() as f32,
            )
        };
        let steps = case["steps"].as_array().unwrap();
        let drag = (
            case["drag"][0].as_f64().unwrap() as f32,
            case["drag"][1].as_f64().unwrap() as f32,
        );
        // (the recorded first step is after the file was dragged)
        let start = theirs(&steps[0]);
        assert_eq!(
            rect(&viewer),
            (start.0 - drag.0, start.1 - drag.1, start.2, start.3),
            "{what}: opened"
        );
        if drag != (0.0, 0.0) {
            viewer.invoke_drag(drag.0, drag.1);
        }
        assert_eq!(rect(&viewer), start, "{what}: dragged");
        let mouse = (
            case["mouse"][0].as_f64().unwrap() as f32,
            case["mouse"][1].as_f64().unwrap() as f32,
        );
        let inside = (0.0..800.0).contains(&mouse.0) && (0.0..600.0).contains(&mouse.1);
        for (n, step) in steps.iter().enumerate().skip(1) {
            let direction = if step["op"] == "in" { 1 } else { -1 };
            viewer.invoke_zoom(direction, inside, mouse.0, mouse.1);
            assert_eq!(
                rect(&viewer),
                theirs(step),
                "{what}: step {n} ({})",
                step["op"]
            );
            compared += 1;
        }
        viewer.invoke_close_requested();
    }
    assert!(compared > 3000);

    // the text of the box, as the reference's options panel reads it
    for parse in recorded["parses"].as_array().unwrap() {
        let text = parse["text"].as_str().unwrap();
        client
            .store
            .write(|ctx| {
                let mut viewer: hydrus_core::media_viewer::MediaViewerSettings =
                    hydrus_store::settings::get(ctx.conn())?;
                viewer.media_zooms = vec![0.5, 2.0];
                hydrus_store::settings::set(ctx.conn(), &viewer)
            })
            .unwrap();
        let options = client.open_options();
        show_page(&options, "media playback");
        let (i, _) = row(&options, "Media zooms:");
        options.invoke_text_edited(i, text.into());
        options.invoke_apply();
        options.hide().unwrap();
        let saved: hydrus_core::media_viewer::MediaViewerSettings = client.setting();
        let theirs: Vec<f64> = serde_json::from_value(parse["saved"].clone()).unwrap();
        assert_eq!(saved.media_zooms, theirs, "{text:?}");
    }
}
