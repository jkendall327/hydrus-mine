//! Script controls own notices and launch real local help; request controls use an isolated engine.
use hydrus_gui::{
    headless,
    login_workflows_window::{self as windows, Slots},
};
use hydrus_parse::login::{LoginScript, LoginStep};
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::RefCell,
    io::{Read as _, Write as _},
    rc::Rc,
    sync::mpsc,
};

#[test]
fn script_information_error_help_and_retired_children_replay_actual_qt() {
    let fixture = hydrus_testkit::fixture_json("login_script_controls.json");
    let rendered = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let slots = Slots::default();
    let script = LoginScript::default();
    let window = windows::open_script(&store, &script, &slots, Rc::new(|_| Ok(()))).unwrap();
    let controls = slots.test_control.borrow().as_ref().unwrap().clone();
    window.invoke_test_control_menu();
    assert!(window.get_test_cog().url.is_empty());
    assert!(!window.get_test_cog().has_error);
    window.invoke_test_control_action(7);
    assert!(window.get_test_cog().auto_override);
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    controls.set_error(fixture["errors"][0]["message"].as_str().unwrap().into());
    assert!(window.get_test_cog().has_error);
    window.invoke_test_control_action(9);
    assert_eq!(
        copied.borrow()[0],
        fixture["errors"][0]["message"].as_str().unwrap()
    );
    window.invoke_test_control_action(8);
    let error = controls.error_window().unwrap();
    assert_eq!(
        error.get_window_title(),
        fixture["errors"][0]["title"].as_str().unwrap()
    );
    assert_eq!(error.get_message(), copied.borrow()[0]);
    assert!(window.get_child_open());
    error.invoke_cancelled();
    assert!(!window.get_child_open());
    assert!(
        window.get_test_cog().has_error,
        "closing the popup retains the owner error"
    );
    controls.clear_error();
    assert!(!window.get_test_cog().has_error);
    let launched = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_launcher({
        let launched = launched.clone();
        move |path| launched.borrow_mut().push(path.to_owned())
    });
    window.invoke_action("help".into());
    assert_eq!(launched.borrow().len(), 1);
    assert!(std::path::Path::new(&launched.borrow()[0]).is_file());
    assert!(launched.borrow()[0].ends_with("/docs/downloader_login.md"));
    for (index, case) in fixture["completion"].as_array().unwrap().iter().enumerate() {
        let acknowledged = Rc::new(RefCell::new(0));
        controls
            .information(
                case["input"].as_str().unwrap(),
                Rc::new({
                    let acknowledged = acknowledged.clone();
                    move || *acknowledged.borrow_mut() += 1
                }),
            )
            .unwrap();
        let information = controls.information_window().unwrap();
        assert_eq!(
            information.get_window_title(),
            case["visible"]["title"].as_str().unwrap()
        );
        assert_eq!(
            information.get_message(),
            case["visible"]["message"].as_str().unwrap()
        );
        assert_eq!(
            information.get_notice_ok_label(),
            case["visible"]["buttons"][0].as_str().unwrap()
        );
        assert!(information.get_notice_only());
        assert!(window.get_child_open());
        window.invoke_action("apply".into());
        assert!(slots.script.borrow().is_some());
        window.hide().unwrap();
        information.invoke_cancelled();
        assert_eq!(*acknowledged.borrow(), 0);
        assert!(controls.information_window().is_some());
        window.show().unwrap();
        let native = rendered.get(rendered.count() - 1).unwrap();
        let pixels = headless::render(&native, 520, 200);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("login-script-information.png"),
            &pixels,
            520,
            200,
        )
        .unwrap();
        let text = if index == 1 {
            slint::platform::Key::Escape
        } else {
            slint::platform::Key::Return
        };
        native.dispatch_event(slint::platform::WindowEvent::WindowActiveChanged(true));
        native.dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
        native.dispatch_event(slint::platform::WindowEvent::KeyReleased { text: text.into() });
        assert_eq!(*acknowledged.borrow(), 1);
        information.invoke_cancelled();
        information.invoke_force_close();
        assert_eq!(*acknowledged.borrow(), 1);
        assert!(!window.get_child_open());
    }
    controls
        .information(
            "retired completion",
            Rc::new(|| panic!("retired popup acknowledged")),
        )
        .unwrap();
    let retired = controls.information_window().unwrap();
    window.invoke_action("cancel".into());
    assert!(!retired.window().is_visible());
    let successor = windows::open_script(&store, &script, &slots, Rc::new(|_| Ok(()))).unwrap();
    let successor_controls = slots.test_control.borrow().as_ref().unwrap().clone();
    successor_controls
        .information("successor completion", Rc::new(|| {}))
        .unwrap();
    retired.invoke_cancelled();
    window.invoke_action("help".into());
    window.invoke_test_control_action(9);
    assert!(successor_controls.information_window().is_some());
    assert!(successor.get_child_open());
    assert_eq!(launched.borrow().len(), 1);
    assert_eq!(copied.borrow().len(), 1);
    successor.invoke_action("cancel".into());
    hydrus_gui::set_clipper(|_| {});
    hydrus_gui::set_launcher(|_| {});
}

fn controls_poll(slots: &Slots) {
    slots.test_control.borrow().as_ref().unwrap().poll();
}

#[test]
fn actual_login_cog_targets_isolated_http_request_and_drops_stale_command() {
    let _rendered = headless::init();
    let (_dirs, store) = super::subscriptions::store();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let domain = listener.local_addr().unwrap().to_string();
    let (release, released) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        let read = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..read]).starts_with("GET /login "));
        released
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap();
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    let script = LoginScript {
        steps: vec![LoginStep {
            scheme: "http".into(),
            path: "/login".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let slots = Slots::default();
    let window = windows::open_script(&store, &script, &slots, Rc::new(|_| Ok(()))).unwrap();
    window.set_test_domain(domain.clone().into());
    window.invoke_action("run-test".into());
    let prompt = slots.test_domain.borrow().as_ref().unwrap().clone_strong();
    prompt.invoke_name_entered(domain.into());
    let started = std::time::Instant::now();
    loop {
        slint::platform::update_timers_and_animations();
        if slots.run.review().is_some_and(|review| {
            review
                .runtime
                .jobs
                .iter()
                .any(|job| job.wait == hydrus_store::network_runtime::WaitReason::Downloading)
        }) {
            break;
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    window.invoke_test_control_menu();
    let first = slots.run.review().unwrap();
    assert!(
        !first.runtime.jobs[0].obeys_bandwidth,
        "Qt login jobs bypass startup bandwidth immediately"
    );
    assert_eq!(window.get_test_cog().actions.row_count(), 0);
    let tracker = first
        .usage
        .iter()
        .find(|(context, _)| *context == hydrus_core::network::NetworkContext::global())
        .unwrap()
        .1
        .clone();
    assert_eq!(
        tracker.all_usage(hydrus_core::bandwidth::BandwidthType::Requests),
        1,
        "login bandwidth bypass still accounts the request"
    );
    assert!(window.get_test_download().can_cancel);
    window.invoke_test_control_action(7);
    assert!(slots.run.review().unwrap().runtime.controls[0].auto_override);
    let saved_rules = store
        .read(hydrus_store::settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
        .unwrap();
    for accepted in [false, true] {
        window.invoke_test_control_menu();
        let rule = window.get_test_cog().rules.row_data(0).unwrap();
        window.invoke_test_control_action(rule.id);
        while hydrus_gui::network_data_window::last_rules().is_none() {
            slint::platform::update_timers_and_animations();
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let rules = hydrus_gui::network_data_window::last_rules().unwrap();
        controls_poll(&slots);
        assert!(window.get_child_open());
        window.invoke_action("apply".into());
        assert!(slots.script.borrow().is_some());
        rules.set_requests(true);
        rules.set_amount("23".into());
        rules.set_seconds("60".into());
        rules.invoke_add_rule();
        if accepted {
            rules.invoke_apply_clicked();
        } else {
            rules.invoke_cancel_clicked();
            assert_eq!(
                store
                    .read(hydrus_store::settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
                    .unwrap(),
                saved_rules
            );
        }
        while hydrus_gui::network_data_window::last_rules().is_some() {
            slint::platform::update_timers_and_animations();
            assert!(started.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        controls_poll(&slots);
        assert!(!window.get_child_open());
        if accepted {
            let saved = store
                .read(hydrus_store::settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
                .unwrap();
            assert!(saved.rules.iter().any(|(context, rules)| {
                context == &hydrus_core::network::NetworkContext::global()
                    && rules.rules().contains(&hydrus_core::bandwidth::Rule::new(
                        hydrus_core::bandwidth::BandwidthType::Requests,
                        Some(60),
                        23,
                    ))
            }));
            assert_eq!(
                slots.run.review().unwrap().settings,
                first.settings,
                "Qt edits the global manager without replacing its isolated test manager"
            );
        }
    }
    let foreign = hydrus_store::network_runtime::Command {
        epoch: "another-engine".into(),
        job: first.runtime.jobs[0].id,
        action: hydrus_store::network_runtime::JobAction::Cancel,
    };
    assert!(!slots.run.command(&foreign));
    release.send(()).unwrap();
    server.join().unwrap();
    let controls = slots.test_control.borrow().as_ref().unwrap().clone();
    while controls.information_window().is_none() {
        slint::platform::update_timers_and_animations();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(window.get_running());
    assert!(window.get_final_result().is_empty());
    assert!(!window.get_test_download().can_cancel);
    assert!(!window.get_test_cog().has_error);
    window.invoke_test_control_action(2);
    assert!(!slots.run.busy());
    controls.information_window().unwrap().invoke_cancelled();
    assert_eq!(window.get_final_result(), "Login OK!");
    assert!(!window.get_running());
    assert!(
        store
            .read(hydrus_store::network::sessions)
            .unwrap()
            .is_empty()
    );
    // A fresh isolated Store restarts its persisted engine epoch and request ID.
    // The GUI generation must still reject the old menu and resend retained policy.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let domain = listener.local_addr().unwrap().to_string();
    let (release, released) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let read = stream.read(&mut request).unwrap();
        assert!(read > 0, "the restarted login request reaches its server");
        released
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    window.invoke_action("run-test".into());
    let prompt = slots.test_domain.borrow().as_ref().unwrap().clone_strong();
    prompt.invoke_name_entered(domain.into());
    let second_started = std::time::Instant::now();
    loop {
        slint::platform::update_timers_and_animations();
        controls.poll();
        if slots.run.review().is_some_and(|review| {
            review
                .runtime
                .controls
                .first()
                .is_some_and(|meta| meta.auto_override)
        }) {
            break;
        }
        assert!(second_started.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let second = slots.run.review().unwrap();
    assert_eq!(first.runtime.jobs[0].id, second.runtime.jobs[0].id);
    assert_ne!(first.runtime.epoch, second.runtime.epoch);
    let stale = hydrus_store::network_runtime::Command {
        epoch: first.runtime.epoch.clone(),
        job: first.runtime.jobs[0].id,
        action: hydrus_store::network_runtime::JobAction::Cancel,
    };
    assert!(!slots.run.command(&stale));
    assert!(window.get_running());
    assert_eq!(window.get_final_result(), "Login OK!");
    release.send(()).unwrap();
    server.join().unwrap();
    while controls.information_window().is_none() {
        slint::platform::update_timers_and_animations();
        assert!(second_started.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(window.get_final_result(), "Login OK!");
    controls.information_window().unwrap().invoke_cancelled();
    window.invoke_action("cancel".into());
}
