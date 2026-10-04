//! Autosave dispatch/cadence from the running reference, plus idle boundaries.
use hydrus_gui_model::session_lifecycle::{Action, Autosave, Idle};
use hydrus_store::settings::{GuiIdleSettings, GuiSessionSettings};

#[test]
fn autosave_replays_idle_retries_period_changes_and_stopping_for_other_startup() {
    let fixture = hydrus_testkit::fixture_json("session_autosave.json");
    let mut config = GuiSessionSettings::default();
    assert_eq!(
        u64::from(config.autosave_minutes),
        fixture["default_minutes"].as_u64().unwrap()
    );
    assert_eq!(
        config.only_during_idle,
        fixture["default_only_idle"].as_bool().unwrap()
    );
    let mut saving = Autosave::new(0, &config);
    assert_eq!(saving.poll(299999, true, &config), Action::Wait);
    for step in fixture["steps"].as_array().unwrap() {
        config.startup = Some(step["startup"].as_str().unwrap().into());
        config.only_during_idle = step["only_idle"].as_bool().unwrap();
        config.autosave_minutes = u16::try_from(step["minutes"].as_u64().unwrap()).unwrap();
        let now = saving.next().unwrap();
        let action = saving.poll(now, step["idle"].as_bool().unwrap(), &config);
        if let Some(call) = step["calls"].as_array().unwrap().first() {
            assert_eq!(
                saving.next().unwrap() - now,
                call["delay"].as_i64().unwrap() * 1000
            );
            assert_eq!(
                action,
                if step["saves"].as_array().unwrap().is_empty() {
                    Action::Wait
                } else {
                    Action::Save
                }
            );
        } else {
            assert_eq!(action, Action::Stopped);
            assert_eq!(saving.next(), None);
        }
    }
    config.startup = Some(hydrus_store::sessions::LAST_SESSION.into());
    assert_eq!(saving.poll(10000000, true, &config), Action::Stopped);
    config.autosave_minutes = 0;
    assert_eq!(Autosave::new(0, &config).next(), Some(60000));
    config.autosave_minutes = u16::MAX;
    assert_eq!(Autosave::new(0, &config).next(), Some(86400000));
}

#[test]
fn idle_requires_boot_user_mouse_and_api_timeouts_and_enabled_normal_work() {
    let mut settings = GuiIdleSettings {
        enabled: true,
        user_seconds: Some(10),
        mouse_seconds: Some(5),
        api_seconds: None,
    };
    let mut idle = Idle::new(0);
    assert!(!idle.eligible(120000, &settings));
    assert!(idle.eligible(120001, &settings));
    idle.user(120001);
    assert!(!idle.eligible(130001, &settings));
    assert!(idle.eligible(130002, &settings));
    idle.mouse(130002);
    assert!(!idle.eligible(135002, &settings));
    assert!(idle.eligible(135003, &settings));
    settings.api_seconds = Some(2);
    idle.api(135003);
    assert!(!idle.eligible(137003, &settings));
    assert!(idle.eligible(137004, &settings));
    settings.enabled = false;
    assert!(!idle.eligible(i64::MAX, &settings));

    let fixture = hydrus_testkit::fixture_json("session_autosave.json");
    let mut idle = Idle::new(0);
    for step in fixture["idle_steps"].as_array().unwrap() {
        let times = &step["times"];
        idle.user(times["last_user_action"].as_i64().unwrap());
        idle.mouse(times["last_mouse_action"].as_i64().unwrap());
        idle.api(times["last_client_api_action"].as_i64().unwrap());
        let config = GuiIdleSettings {
            enabled: step["enabled"].as_bool().unwrap(),
            user_seconds: Some(10),
            mouse_seconds: Some(5),
            api_seconds: step["api_seconds"].as_u64(),
        };
        assert_eq!(
            idle.eligible(step["now"].as_i64().unwrap(), &config),
            step["eligible"].as_bool().unwrap()
        );
    }
}
