//! Help > debug > report modes, clicked in the real menu and compared with the
//! reference's own reports for the same situations (the recordings
//! `oracle/fixtures/debug_*.json`, made by `oracle/record_debug_*.py`).
use super::debug_menu_actions::{ONE_AT_A_TIME, capture, start};
use hydrus_store::settings::{self, GuiIdleSettings};
use serde_json::Value;

// leaf: audit-options-help-debug-action-idle-report-mode
#[test]
fn idle_report_mode_says_what_the_reference_says_in_each_situation() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let d = start();
    let seen = capture();
    let recording = hydrus_testkit::fixture_json("debug_idle_report.json");
    assert_eq!(
        recording["menu_path"],
        serde_json::json!(["help", "debug", "report modes", "idle report mode"])
    );
    let now: i64 = recording["now_ms"].as_i64().unwrap();
    let monitor = d.bound.session_autosave.clone();
    // off, it says nothing
    assert!(!d.report_mode_checked("idle report mode"));
    monitor.idle_at(now);
    assert!(seen.lock().unwrap().is_empty());
    d.click(&["report modes", "idle report mode"]);
    assert!(d.report_mode_checked("idle report mode"));
    let seconds = |sc: &Value, key: &str, default: Option<u64>| -> Option<u64> {
        match sc.get(key) {
            Some(Value::Null) => None,
            Some(v) => v.as_u64(),
            None => default,
        }
    };
    let mut forced_on = false;
    let mut shutting_down = None;
    for sc in recording["scenarios"].as_array().unwrap() {
        let name = sc["name"].as_str().unwrap();
        if sc["shutting_down"] == true {
            shutting_down = Some(sc.clone());
            continue;
        }
        let forced = sc["force"] == true;
        if forced != forced_on {
            d.click(&["debug modes", "force idle mode"]);
            forced_on = forced;
        }
        let config = GuiIdleSettings {
            enabled: sc.get("normal").is_none_or(|n| n == true),
            user_seconds: seconds(sc, "user_period", Some(1800)),
            mouse_seconds: seconds(sc, "mouse_period", Some(600)),
            api_seconds: seconds(sc, "api_timeout", None),
            ..GuiIdleSettings::default()
        };
        d.store
            .write(move |ctx| settings::set(ctx.conn(), &config))
            .unwrap();
        let ago = |key: &str| now - sc[key].as_i64().unwrap() * 1000;
        monitor.boot_at(ago("boot"));
        monitor.user_at(ago("user"));
        monitor.mouse_at(ago("mouse"));
        monitor.api_at(ago("api"));
        monitor.set_previously_idle(sc["was_idle"] == true);
        seen.lock().unwrap().clear();
        let answer = monitor.idle_at(now);
        let said: Vec<String> = std::mem::take(&mut *seen.lock().unwrap());
        let recorded: Vec<String> = sc["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(said, recorded, "{name}");
        assert_eq!(answer, sc["result"] == true, "{name}");
    }
    if forced_on {
        d.click(&["debug modes", "force idle mode"]);
    }
    // closing the program: never idle (the binding is retired)
    if let Some(sc) = shutting_down {
        drop(d.bound);
        seen.lock().unwrap().clear();
        let answer = monitor.idle_at(now);
        assert_eq!(
            *seen.lock().unwrap(),
            [sc["messages"][0].as_str().unwrap().to_owned()]
        );
        assert!(!answer);
    }
}
