use hydrus_gui_model::viewer_cursor::CursorWait;

#[test]
fn actual_cursor_checks_match_reference_clock_focus_menu_and_motion() {
    let fixture = hydrus_testkit::fixture_json("viewer_cursor_options.json");
    for event in fixture["events"].as_array().unwrap() {
        let delay = event["value"]
            .as_u64()
            .map(|value| u32::try_from(value).unwrap());
        let mut wait = CursorWait::new(0);
        for step in event["trace"].as_array().unwrap() {
            let phase = step["phase"].as_str().unwrap();
            wait.check(
                step["now_ms"].as_u64().unwrap(),
                delay,
                phase != "inactive",
                phase != "ineligible-widget",
                false,
            );
            assert_eq!(wait.blank, step["blank"].as_bool().unwrap(), "{step:?}");
            assert_eq!(
                wait.touched_ms,
                step["touch_ms"].as_u64().unwrap(),
                "{step:?}"
            );
            assert_eq!(
                wait.next_check_ms.map(u64::from),
                step["interval_ms"].as_u64()
            );
        }
    }
    let mut wait = CursorWait::new(0);
    for step in fixture["menu"].as_array().unwrap() {
        wait.check(
            step["now_ms"].as_u64().unwrap(),
            Some(700),
            true,
            true,
            step["phase"] == "inside-real-menu",
        );
        assert_eq!(wait.blank, step["blank"].as_bool().unwrap());
        assert_eq!(wait.touched_ms, step["touch_ms"].as_u64().unwrap());
    }
    for step in fixture["movement"].as_array().unwrap() {
        wait.motion(
            step["now_ms"].as_u64().unwrap(),
            step["phase"] == "hidden-drag",
        );
        assert_eq!(wait.blank, step["blank"].as_bool().unwrap());
        assert_eq!(
            wait.next_check_ms.is_some(),
            step["timer_active"].as_bool().unwrap()
        );
        assert_eq!(wait.touched_ms, step["touch_ms"].as_u64().unwrap());
    }
}
