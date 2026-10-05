//! Actual command controls, including independent captures and wheel accumulation.
use hydrus_core::shortcuts::{Binding, Gesture, Settings};
use hydrus_gui_model::shortcut_capture::{Capture, Wheel};
fn bits(fixture: &serde_json::Value, flags: u64) -> u8 {
    ["Control", "Alt", "Shift", "Keypad", "GroupSwitch", "Meta"]
        .iter()
        .enumerate()
        .fold(0, |out, (i, key)| {
            out | if flags & fixture["modifier_masks"][*key].as_u64().unwrap() != 0 {
                1 << i
            } else {
                0
            }
        })
}
#[test]
fn actual_qt_keyboard_and_mouse_controls_replay_exact_values_text_and_modes() {
    let f = hydrus_testkit::fixture_json("shortcut_capture.json");
    for case in f["cases"].as_array().unwrap() {
        let mut capture = Capture::new(
            Gesture::default(),
            case["merge_numpad"].as_bool().unwrap_or(true),
        );
        let mut wheel = Wheel::default();
        for step in case["steps"].as_array().unwrap() {
            if case["kind"] == "keyboard" {
                let qt = step["qt_key"].as_u64().unwrap();
                let key = f["qt_special_keys"][qt.to_string()].as_u64();
                capture.keyboard(
                    if key.is_some() { 2 } else { 0 },
                    key.unwrap_or(qt) as u32,
                    bits(&f, step["effective_modifiers"].as_u64().unwrap()),
                );
            } else {
                let flags = bits(&f, step["modifiers"].as_u64().unwrap());
                match step["kind"].as_str().unwrap() {
                    "choice" => capture.choose_release(step["delta"] == 1),
                    "wheel" => {
                        capture.wheel(step["delta"].as_i64().unwrap() as i32, flags, &mut wheel);
                    }
                    "horizontal" => {
                        capture.wheel(0, flags, &mut wheel);
                    }
                    kind => {
                        let button = step["button"].as_u64().unwrap();
                        let key = match button {
                            1 => 0,
                            2 => 1,
                            4 => 2,
                            8 => 7,
                            16 => 8,
                            32 => 9,
                            _ => panic!("recorded button"),
                        };
                        capture.mouse(
                            key,
                            match kind {
                                "press" => 0,
                                "release" => 1,
                                "double" => 2,
                                _ => unreachable!(),
                            },
                            flags,
                        );
                    }
                }
                assert_eq!(
                    capture.mouse.appropriate_for_release(),
                    step["press_release_enabled"]
                );
                assert_eq!(capture.release, step["choice"] == 1);
                assert_eq!(capture.mouse_selected, step["mouse_selected"]);
            }
            let value = capture.value();
            assert_eq!(
                serde_json::json!([value.kind, value.key, value.press, value.modifiers]),
                step["value"][2],
                "{}",
                step["label"]
            );
            if !cfg!(target_os = "macos") {
                assert_eq!(
                    value.text(case["primary_labels"].as_bool().unwrap_or(false)),
                    step["text"]
                );
            }
        }
    }
    let mut wheel = Wheel::default();
    assert_eq!(wheel.key(i32::MIN), Some(4));
}
#[test]
fn saved_keypad_policy_preserves_digits_and_resolves_non_number_alternates() {
    let plain = Gesture::new(2, 11, 0, 4);
    let pad = Gesture::new(2, 11, 0, 12);
    let number = Gesture::new(0, 55, 0, 8);
    let mut settings = Settings::default();
    settings.sets.insert(
        "main_gui".into(),
        vec![
            Binding {
                gesture: pad.clone(),
                action: 78,
                text: None,
            },
            Binding {
                gesture: number.clone(),
                action: 7,
                text: None,
            },
        ],
    );
    assert_eq!(settings.command("main_gui", &plain), Some(78));
    settings.merge_numpad = false;
    assert_eq!(settings.command("main_gui", &plain), None);
    assert_eq!(settings.command("main_gui", &pad), Some(78));
    assert_eq!(settings.command("main_gui", &number), Some(7));
    assert_eq!(
        settings.command("main_gui", &Gesture::new(0, 55, 0, 0)),
        None
    );
}
