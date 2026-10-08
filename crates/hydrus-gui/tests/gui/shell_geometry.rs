//! A closing window saves its geometry as `SaveTLWSizeAndPosition` does,
//! over the displays: through the real main window and store, with the
//! reference's recorded two-display topology (`save_geometry.json`).

use slint::ComponentHandle as _;

use hydrus_core::windows::{FrameLocation, WindowSettings};
use hydrus_gui::windows::{save_named_on, state};
use hydrus_gui_model::window_rescue::{Rect, Screen};

use crate::options_gui_support::Client;

fn topology() -> Vec<Screen> {
    let recorded = hydrus_testkit::fixture_json("save_geometry.json");
    recorded["topology"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let g = &s["geometry"];
            let a = &s["available"];
            Screen {
                geometry: Rect {
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

fn keep(client: &Client, frame: FrameLocation) {
    client
        .store
        .write(move |ctx| {
            let mut settings: WindowSettings = hydrus_store::settings::get(ctx.conn())?;
            settings.main_gui = frame;
            hydrus_store::settings::set(ctx.conn(), &settings)
        })
        .unwrap();
}

fn main_gui(client: &Client) -> FrameLocation {
    client.setting::<WindowSettings>().main_gui
}

// leaf: audit-options-geometry
#[test]
fn the_main_window_saves_what_the_reference_saves_and_not_while_minimised_or_hidden() {
    let client = Client::basic();
    let screens = topology();
    let window = client.ui.window();
    window.set_size(slint::LogicalSize::new(912.0, 678.0));
    let before = FrameLocation {
        last_size: Some((300, 200)),
        last_position: Some((100, 150)),
        maximised: true,
        fullscreen: false,
        ..FrameLocation::main_gui()
    };
    keep(&client, before.clone());
    let now = state(window);
    assert_eq!(now.size, (912, 678));

    // minimised: nothing is saved
    window.set_minimized(true);
    save_named_on(window, &client.store, "main_gui", &screens, Some(0));
    assert_eq!(main_gui(&client), before, "minimised");
    window.set_minimized(false);

    // hidden: nothing is saved
    client.ui.hide().unwrap();
    save_named_on(window, &client.store, "main_gui", &screens, Some(0));
    assert_eq!(main_gui(&client), before, "hidden");
    client.ui.show().unwrap();

    // maximised, carried to the second display from the first it was saved
    // on: its size stays, its place goes to the same spot on the new display
    window.set_maximized(true);
    save_named_on(window, &client.store, "main_gui", &screens, Some(1));
    let saved = main_gui(&client);
    assert!(saved.maximised && !saved.fullscreen);
    assert_eq!(
        saved.last_size,
        Some((300, 200)),
        "a maximised size isn't kept"
    );
    assert_eq!(saved.last_position, Some((-900, 250)));

    // maximised on a display it can't be placed on: just off its corner
    keep(
        &client,
        FrameLocation {
            last_position: Some((5000, 5000)),
            ..saved.clone()
        },
    );
    save_named_on(window, &client.store, "main_gui", &screens, Some(0));
    assert_eq!(main_gui(&client).last_position, Some((20, 20)));

    // restored to a window on the first display: its size and place now
    window.set_maximized(false);
    save_named_on(window, &client.store, "main_gui", &screens, Some(0));
    let saved = main_gui(&client);
    assert!(!saved.maximised && !saved.fullscreen);
    assert_eq!(saved.last_size, Some((912, 678)));
    assert_eq!(saved.last_position, Some(state(window).position));
}

// leaf: audit-options-geometry
#[test]
fn the_main_window_keeps_its_geometry_a_moment_after_it_changes_not_only_on_close() {
    let client = Client::basic();
    keep(
        &client,
        FrameLocation {
            maximised: false,
            ..FrameLocation::main_gui()
        },
    );
    let window = client.ui.window();
    window.set_size(slint::LogicalSize::new(777.0, 555.0));
    let end = std::time::Instant::now() + std::time::Duration::from_millis(1200);
    while std::time::Instant::now() < end && main_gui(&client).last_size != Some((777, 555)) {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(main_gui(&client).last_size, Some((777, 555)));
    assert_eq!(
        main_gui(&client).last_position,
        Some(state(window).position)
    );
}
