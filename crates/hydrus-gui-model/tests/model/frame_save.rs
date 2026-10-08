//! `SaveTLWSizeAndPosition` against the reference's, run on a stand-in window
//! over a two-display topology (`oracle/fixtures/save_geometry.json`, from
//! `oracle/record_save_geometry.py`): every case's frame after saving.

use hydrus_core::windows::{FrameLocation, WindowState};
use hydrus_gui_model::frame_save::{Surroundings, saved};
use hydrus_gui_model::window_rescue::{Rect, Screen};
use hydrus_store::settings::WindowRescueSettings;
use serde_json::Value;

fn pair(v: &Value) -> Option<(i32, i32)> {
    v.as_array()
        .map(|a| (a[0].as_i64().unwrap() as i32, a[1].as_i64().unwrap() as i32))
}

fn frame(row: &Value) -> FrameLocation {
    let row = row.as_array().unwrap();
    FrameLocation {
        remember_size: row[0].as_bool().unwrap(),
        remember_position: row[1].as_bool().unwrap(),
        last_size: pair(&row[2]),
        last_position: pair(&row[3]),
        default_gravity: pair(&row[4]).unwrap(),
        default_position: row[5].as_str().unwrap().to_owned(),
        maximised: row[6].as_bool().unwrap(),
        fullscreen: row[7].as_bool().unwrap(),
    }
}

fn screens(recorded: &Value) -> Vec<Screen> {
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

#[test]
fn a_closing_window_is_saved_as_the_references_saves_it() {
    let recorded = hydrus_testkit::fixture_json("save_geometry.json");
    let screens = screens(&recorded);
    let cases = recorded["cases"].as_array().unwrap();
    assert!(cases.len() >= 25);
    for case in cases {
        let label = case["label"].as_str().unwrap();
        let window = &case["window"];
        let rescue = WindowRescueSettings {
            disabled: case["disabled_position_test"].as_bool().unwrap(),
            add_padding: recorded["fuzzy_relocate"].as_bool().unwrap(),
            padding: recorded["padding"].as_u64().unwrap() as u8,
        };
        let state = WindowState {
            size: pair(&window["size"]).unwrap(),
            position: pair(&window["position"]).unwrap(),
            maximised: window["maximised"].as_bool().unwrap(),
            fullscreen: window["fullscreen"].as_bool().unwrap(),
        };
        let around = Surroundings {
            minimised: window["minimised"].as_bool().unwrap(),
            visible: window["visible"].as_bool().unwrap(),
            screens: &screens,
            window_screen: window["screen"].as_u64().map(|n| n as usize),
            rescue: &rescue,
        };
        let got = saved(&frame(&case["before"]), state, &around);
        assert_eq!(got, frame(&case["after"]), "{label}");
    }
}
