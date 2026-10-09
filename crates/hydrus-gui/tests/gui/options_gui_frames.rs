//! The remembered/default frame editor of the gui page against the
//! reference's `EditFrameLocationPanel`: what its remember switches,
//! optional sizes and places, and start-maximised/fullscreen switches do to a
//! window as it next opens.

use slint::{ComponentHandle as _, Model as _};

use hydrus_core::windows::WindowSettings;
use hydrus_gui::FrameLocationWindow;

use crate::options_gui_support::{Client, show_page};

fn index(window: &hydrus_gui::OptionsWindow, name: &str) -> i32 {
    let rows = window.get_frame_rows();
    (0..rows.row_count())
        .position(|i| rows.row_data(i).unwrap().cells.row_data(0).unwrap() == name)
        .unwrap_or_else(|| panic!("frame {name:?}")) as i32
}

/// The media viewer's frame, edited by `change` in the editor child and
/// applied through the options window.
fn edit_media_viewer(client: &Client, change: &dyn Fn(&FrameLocationWindow)) {
    let options = client.open_options();
    show_page(&options, "gui");
    options.invoke_frame_clicked(index(&options, "media_viewer"), false, false);
    options.invoke_frame_action("edit".into());
    let child = client
        .bound
        .options_frame_child
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    assert_eq!(
        child.get_message(),
        "Setting frame location info for media_viewer."
    );
    change(&child);
    child.invoke_apply();
    options.invoke_apply();
}

/// A viewer opened now, and its window as it opened.
fn opened_viewer(client: &Client) -> ((f32, f32), bool, bool) {
    client.ui.invoke_thumbnail_activated(0);
    let viewer = client
        .bound
        .viewer
        .borrow()
        .as_ref()
        .expect("the viewer opens")
        .clone_strong();
    let window = viewer.window();
    let size = window.size().to_logical(window.scale_factor());
    let opened = (
        (size.width, size.height),
        window.is_maximized(),
        window.is_fullscreen(),
    );
    viewer.invoke_close_requested();
    opened
}

// leaf: audit-options-nested-frame-location-remember
// leaf: audit-options-gui-frame-locations-flip-remember-size
#[test]
fn the_frame_editor_s_switches_decide_how_the_media_viewer_next_opens() {
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    // the reference's defaults for the media viewer: remembered 640x480,
    // opening maximised
    let defaults = client.setting::<WindowSettings>().media_viewer;
    assert!(defaults.remember_size && defaults.maximised);

    // remember the size, and open neither maximised nor fullscreen
    edit_media_viewer(&client, &|child| {
        child.set_remember_size(true);
        child.set_size_none(false);
        child.set_last_width(700);
        child.set_last_height(500);
        child.set_maximised(false);
        child.set_fullscreen(false);
    });
    let saved = client.setting::<WindowSettings>().media_viewer;
    assert_eq!(
        (
            saved.remember_size,
            saved.last_size,
            saved.maximised,
            saved.fullscreen
        ),
        (true, Some((700, 500)), false, false)
    );
    assert_eq!(opened_viewer(&client), ((700.0, 500.0), false, false));

    // another size is the one opened
    edit_media_viewer(&client, &|child| {
        child.set_last_width(820);
        child.set_last_height(610);
    });
    assert_eq!(opened_viewer(&client).0, (820.0, 610.0));

    // not remembering the size: the saved one is kept but not used
    edit_media_viewer(&client, &|child| child.set_remember_size(false));
    let saved = client.setting::<WindowSettings>().media_viewer;
    assert_eq!(
        (saved.remember_size, saved.last_size),
        (false, Some((820, 610)))
    );
    assert_ne!(opened_viewer(&client).0, (820.0, 610.0));

    // start maximised; then fullscreen
    edit_media_viewer(&client, &|child| child.set_maximised(true));
    assert!(client.setting::<WindowSettings>().media_viewer.maximised);
    let (_, maximised, fullscreen) = opened_viewer(&client);
    assert!(maximised && !fullscreen);
    edit_media_viewer(&client, &|child| {
        child.set_maximised(false);
        child.set_fullscreen(true);
    });
    let (_, maximised, fullscreen) = opened_viewer(&client);
    assert!(!maximised && fullscreen);
}

/// The options dialog's own frame, edited by `change` in the editor child
/// (through the open dialog) and applied.
fn edit_options_frame(client: &Client, change: &dyn Fn(&FrameLocationWindow)) {
    let options = client.open_options();
    show_page(&options, "gui");
    options.invoke_frame_clicked(index(&options, "manage_options_dialog"), false, false);
    options.invoke_frame_action("edit".into());
    let child = client
        .bound
        .options_frame_child
        .borrow()
        .as_ref()
        .expect("the editor opens")
        .clone_strong();
    change(&child);
    child.invoke_apply();
    options.invoke_apply();
}

// leaf: audit-options-nested-frame-location-gravity
#[test]
fn default_gravity_and_position_decide_where_a_child_window_next_opens() {
    let client = Client::basic();
    // a main window of 1000x800 to open relative to
    let native = client.native();
    hydrus_gui::headless::render(&native, 1000, 800);
    let logical = |window: &hydrus_gui::OptionsWindow| {
        let size = window
            .window()
            .size()
            .to_logical(window.window().scale_factor());
        (size.width, size.height)
    };

    // the reference's default: as large as the dialog needs, at the main
    // window's top-left (less the child padding)
    let natural = {
        let options = client.open_options();
        let size = logical(&options);
        options.invoke_cancel();
        size
    };
    let frame = |client: &Client| {
        client
            .setting::<WindowSettings>()
            .frame("manage_options_dialog")
            .cloned()
            .unwrap()
    };
    let placement = |client: &Client| {
        let options = client.open_options();
        let placed = hydrus_gui::windows::placement(options.window(), &frame(client), false);
        options.invoke_cancel();
        placed
    };
    assert_eq!(frame(&client).default_gravity, (-1, -1));
    assert_eq!(
        placement(&client).size,
        (natural.0 as i32, natural.1 as i32)
    );
    assert_eq!(placement(&client).position, Some((24, 24)));

    // expand to the width and the height of the parent
    edit_options_frame(&client, &|child| {
        child.set_gravity_x(0);
        child.set_gravity_y(0);
    });
    assert_eq!(frame(&client).default_gravity, (1, 1));
    let options = client.open_options();
    assert_eq!(logical(&options), (952.0, 752.0), "1000x800 less 24 a side");
    options.invoke_cancel();

    // expand the width only: the dialog's own height is what it needs (a
    // headless window has none until it is drawn, so the placement is
    // worked out for one that was)
    edit_options_frame(&client, &|child| child.set_gravity_y(1));
    assert_eq!(frame(&client).default_gravity, (1, -1));
    let options = client.open_options();
    let last = client.windows().count() - 1;
    hydrus_gui::headless::render(&client.windows().get(last).unwrap(), 640, 480);
    let placed = hydrus_gui::windows::placement(options.window(), &frame(&client), false);
    assert_eq!(placed.size, (952, 480));
    options.invoke_cancel();

    // centred on the parent: its centre less the dialog's
    edit_options_frame(&client, &|child| child.set_default_position(1));
    assert_eq!(frame(&client).default_position, "center");
    let placed = placement(&client);
    let (width, height) = placed.size;
    assert_eq!(
        placed.position,
        Some((499 - (width - 1) / 2, 399 - (height - 1) / 2))
    );
    // remembered, its own size and place win
    edit_options_frame(&client, &|child| {
        child.set_remember_size(true);
        child.set_size_none(false);
        child.set_last_width(640);
        child.set_last_height(480);
        child.set_remember_position(true);
        child.set_position_none(false);
        child.set_last_x(33);
        child.set_last_y(44);
    });
    let placed = placement(&client);
    assert_eq!((placed.size, placed.position), ((640, 480), Some((33, 44))));
}

/// The media viewer's frame opening maximised or fullscreen, and what is kept
/// of it when it closes, in every combination the reference's real window
/// was put through (oracle/record_frame_state.py).
// leaf: audit-options-nested-frame-location-state
#[test]
#[allow(clippy::cast_possible_truncation)]
fn the_start_maximised_and_fullscreen_switches_open_and_keep_the_window_as_the_reference_does() {
    let recorded: serde_json::Value = hydrus_testkit::fixture_json("frame_state.json");
    let client = Client::basic();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    // (the viewer keeps its window as it closes, as the reference does if asked to)
    client
        .store
        .write(|ctx| {
            let mut windows: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
            windows.save_media_viewer_on_close = true;
            hydrus_store::settings::set(ctx.conn(), &windows)
        })
        .unwrap();
    let mut compared = 0;
    for case in recorded["cases"].as_array().unwrap() {
        let frame = &case["frame"];
        let (maximised, fullscreen) = (
            frame["maximised"].as_bool().unwrap(),
            frame["fullscreen"].as_bool().unwrap(),
        );
        let remember_size = frame["remember_size"].as_bool().unwrap();
        let position = frame["last_position"]
            .as_array()
            .map(|p| (p[0].as_i64().unwrap() as i32, p[1].as_i64().unwrap() as i32));
        let user = case["user"].as_str().unwrap();
        let what = format!(
            "remember size {remember_size}, maximised {maximised}, fullscreen {fullscreen}, position {position:?}, then {user}"
        );

        // the frame, set in the editor
        edit_media_viewer(&client, &|child| {
            child.set_remember_size(remember_size);
            child.set_size_none(false);
            child.set_last_width(700);
            child.set_last_height(500);
            child.set_remember_position(position.is_some());
            child.set_position_none(position.is_none());
            if let Some((x, y)) = position {
                child.set_last_x(x);
                child.set_last_y(y);
            }
            child.set_maximised(maximised);
            child.set_fullscreen(fullscreen);
        });

        // opened
        client.ui.invoke_thumbnail_activated(0);
        let viewer = client
            .bound
            .viewer
            .borrow()
            .as_ref()
            .expect("the viewer opens")
            .clone_strong();
        let window = viewer.window();
        let opened = &case["opened"];
        assert_eq!(
            (window.is_maximized(), window.is_fullscreen()),
            (
                opened["maximised"].as_bool().unwrap(),
                opened["fullscreen"].as_bool().unwrap()
            ),
            "{what}: opened"
        );
        if !maximised && !fullscreen && remember_size {
            let size = window.size().to_logical(window.scale_factor());
            assert_eq!(
                (size.width, size.height),
                (
                    opened["size"][0].as_f64().unwrap() as f32,
                    opened["size"][1].as_f64().unwrap() as f32
                ),
                "{what}: its size"
            );
        }

        // the user changes it, and closes it
        match user {
            "restored" => {
                window.set_fullscreen(false);
                window.set_maximized(false);
            }
            "maximised by hand" => {
                window.set_fullscreen(false);
                window.set_maximized(true);
            }
            "fullscreened by hand" => window.set_fullscreen(true),
            _ => {}
        }
        let after = &case["after_user"];
        assert_eq!(
            (
                window.is_maximized() && !window.is_fullscreen(),
                window.is_fullscreen()
            ),
            (
                after["maximised"].as_bool().unwrap(),
                after["fullscreen"].as_bool().unwrap()
            ),
            "{what}: after the user"
        );
        viewer.invoke_close_requested();

        // what is kept
        let kept = client.setting::<WindowSettings>().media_viewer;
        let saved = &case["saved"];
        assert_eq!(
            kept.remember_size,
            saved["remember_size"].as_bool().unwrap(),
            "{what}"
        );
        assert_eq!(
            (kept.maximised, kept.fullscreen),
            (
                saved["maximised"].as_bool().unwrap(),
                saved["fullscreen"].as_bool().unwrap()
            ),
            "{what}: the switches kept"
        );
        if saved["last_size"] == serde_json::json!([700, 500]) {
            assert_eq!(kept.last_size, Some((700, 500)), "{what}: the size kept");
        }
        compared += 1;
    }
    assert_eq!(compared, 40);
}
