//! Stale geometry fitted to the screens as a window opens by the ordinary
//! dialogs' frame (`regular_dialog`): `SetInitialTLWSizeAndPosition`'s
//! wanted place put through `GetSafePosition` and, for a default top-left
//! place, `SlideOffScreenTLWUpAndLeft`, with the message the user is shown,
//! replayed from `oracle/record_regular_dialog.py`'s openings.
use hydrus_core::windows::FrameLocation;
use hydrus_gui_model::frame_placement::{
    Parent, Rect, Surroundings, fitted, initial_size, opening, rescue_message,
};
use hydrus_gui_model::window_rescue::{self, Screen};
use hydrus_store::settings::WindowRescueSettings;
use serde_json::Value;

fn pair(value: &Value) -> Option<(i32, i32)> {
    let value = value.as_array()?;
    Some((
        i32::try_from(value[0].as_i64()?).ok()?,
        i32::try_from(value[1].as_i64()?).ok()?,
    ))
}

/// A frame location as `GetFrameLocation` returns it.
fn frame(row: &Value) -> FrameLocation {
    FrameLocation {
        remember_size: row[0].as_bool().unwrap(),
        remember_position: row[1].as_bool().unwrap(),
        last_size: pair(&row[2]),
        last_position: pair(&row[3]),
        default_gravity: pair(&row[4]).unwrap(),
        default_position: row[5].as_str().unwrap().into(),
        maximised: row[6].as_bool().unwrap(),
        fullscreen: row[7].as_bool().unwrap(),
    }
}

fn screens(recorded: &Value) -> Vec<Screen> {
    recorded["screens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|screen| {
            let g = &screen["geometry"];
            let a = &screen["available"];
            Screen {
                geometry: window_rescue::Rect {
                    x: g[0].as_i64().unwrap(),
                    y: g[1].as_i64().unwrap(),
                    width: g[2].as_i64().unwrap(),
                    height: g[3].as_i64().unwrap(),
                },
                available_top_left: (a[0].as_i64().unwrap(), a[1].as_i64().unwrap()),
            }
        })
        .collect()
}

/// The reference's message with its `QPoint` reprs written as pairs.
fn native_message(message: &str) -> String {
    message.replace("PySide6.QtCore.QPoint", "")
}

#[test]
fn stale_and_off_screen_places_are_fitted_to_the_screens_as_the_reference_fits_them() {
    let recorded = hydrus_testkit::fixture_json("regular_dialog.json");
    let screens = screens(&recorded);
    let rescue: WindowRescueSettings = serde_json::from_value(recorded["rescue"].clone()).unwrap();
    assert_eq!(rescue, WindowRescueSettings::default(), "the defaults");
    let openings = recorded["opening"].as_array().unwrap();
    assert_eq!(openings.len(), 8);
    for case in openings {
        let name = case["name"].as_str().unwrap();
        let frame = frame(&case["frame"]);
        let main = &case["main"];
        let around = Surroundings {
            parent: Some(Parent {
                frame: Rect {
                    x: i32::try_from(main[0].as_i64().unwrap()).unwrap(),
                    y: i32::try_from(main[1].as_i64().unwrap()).unwrap(),
                    width: i32::try_from(main[2].as_i64().unwrap()).unwrap(),
                    height: i32::try_from(main[3].as_i64().unwrap()).unwrap(),
                },
                fullscreen: false,
            }),
            display: Some(Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 800,
            }),
            mouse: None,
            frame_padding: (0, 0),
        };
        // (the window's own size where nothing is remembered: what the
        // reference's panel asked for)
        let hint = pair(&case["placed"]["size"]).unwrap();
        let size = initial_size(&frame, hint, &around);
        assert_eq!(Some(size), pair(&case["shown"]["size"]), "{name}: size");
        let wanted = opening(&frame, size, &around).expect("a place");
        let (position, message) =
            fitted(wanted, size, &rescue, &screens, "regular_dialog").expect("a screen takes it");
        assert_eq!(
            Some(position),
            pair(&case["shown"]["position"]),
            "{name}: place"
        );
        let messages: Vec<String> = case["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| native_message(m.as_str().unwrap()))
            .collect();
        assert_eq!(
            message.into_iter().collect::<Vec<_>>(),
            messages,
            "{name}: told"
        );
    }
}

// (worked from `SetInitialTLWSizeAndPosition` and `GetSafePosition`: the
// recording has no mouse places)
#[test]
fn a_place_by_the_mouse_or_with_no_main_window_is_fitted_as_the_reference_s_code_has_it() {
    let screens = [Screen {
        geometry: window_rescue::Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 800,
        },
        available_top_left: (0, 0),
    }];
    let frame = |position: &str| FrameLocation {
        remember_size: false,
        remember_position: false,
        last_size: None,
        last_position: None,
        default_gravity: (-1, -1),
        default_position: position.into(),
        maximised: false,
        fullscreen: false,
    };
    let rescue = WindowRescueSettings::default();
    // by the mouse, on a screen but the window off it: rescued, said
    // nothing of
    let around = Surroundings {
        mouse: Some((790, 790)),
        ..Surroundings::default()
    };
    let wanted = opening(&frame("mouse"), (2000, 2000), &around).unwrap();
    assert_eq!(wanted.desired, (790 - 999, 790 - 999));
    assert!(!wanted.report);
    let (_, told) = fitted(wanted, (2000, 2000), &rescue, &screens, "regular_dialog").unwrap();
    assert_eq!(told, None);
    // the mouse off every screen (`GetMouseScreen` none): the padding place
    let around = Surroundings {
        mouse: Some((5000, 5000)),
        ..Surroundings::default()
    };
    let wanted = opening(&frame("mouse"), (200, 100), &around).unwrap();
    assert_eq!(
        fitted(wanted, (200, 100), &rescue, &screens, "regular_dialog"),
        Some(((24, 24), None))
    );
    // no main window: the display's top-left, said nothing of
    let around = Surroundings {
        display: Some(Rect {
            x: 0,
            y: 0,
            width: 800,
            height: 800,
        }),
        ..Surroundings::default()
    };
    let wanted = opening(&frame("topleft"), (200, 100), &around).unwrap();
    assert_eq!(
        (wanted.desired, wanted.slide, wanted.report),
        ((24, 24), true, false)
    );
    // a rescue switched off keeps any place
    let off = WindowRescueSettings {
        disabled: true,
        ..WindowRescueSettings::default()
    };
    let far = opening(
        &FrameLocation {
            remember_position: true,
            last_position: Some((9000, 9000)),
            ..frame("topleft")
        },
        (200, 100),
        &around,
    )
    .unwrap();
    assert_eq!(
        fitted(far, (200, 100), &off, &screens, "regular_dialog"),
        Some(((9000, 9000), None))
    );
    assert_eq!(
        rescue_message("regular_dialog", (1, 2), (3, 4)),
        "A window with frame key \"regular_dialog\" that wanted to display at \"(1, 2)\" was rescued from apparent off-screen to the new location at \"(3, 4)\"."
    );
}
