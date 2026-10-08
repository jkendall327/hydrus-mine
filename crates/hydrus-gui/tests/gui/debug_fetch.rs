//! Real local HTTP/Store/toaster boundary, captured bytes and retired GUI owners.
use hydrus_gui::{MainWindow, Pages, bind, headless};
use hydrus_store::{Store, network, popups, settings};
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn jobs(store: &Store) -> Vec<popups::Job> {
    store
        .read(|conn| popups::all(conn, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap()
}
fn select(ui: &MainWindow, pane: i32, label: &str) {
    let rows = ui.get_menu_panes().row_data(pane as usize).unwrap().lines;
    let row = rows.iter().position(|row| row.label == label).unwrap();
    ui.invoke_menu_line_clicked(pane, row as i32, 0.0, 0.0, 0.0);
}
fn launch(ui: &MainWindow) {
    let help = ui
        .get_menu_titles()
        .iter()
        .position(|row| row.label == "help")
        .unwrap();
    ui.invoke_menu_title_pressed(help as i32, 20.0, 22.0);
    select(ui, 0, "debug");
    select(ui, 1, "network actions");
    let rows = ui.get_menu_panes().row_data(2).unwrap().lines;
    assert_eq!(
        rows.row_data(0).unwrap().label,
        "review current network jobs"
    );
    select(ui, 2, "fetch a url");
}
fn submit(control: &hydrus_gui::debug_fetch::Control, url: &str) -> hydrus_gui::DebugFetchWindow {
    control.open();
    let input = control
        .windows()
        .into_iter()
        .find(hydrus_gui::DebugFetchWindow::get_input_mode)
        .unwrap();
    assert_eq!(input.get_message(), "Enter the URL.");
    input.set_url(url.into());
    input.invoke_accepted();
    input
}
fn until(mut condition: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "owned HTTP boundary timed out"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn ready(control: &hydrus_gui::debug_fetch::Control) -> hydrus_gui::DebugFetchWindow {
    until(|| {
        control.tick();
        control.running() == 0
    });
    control
        .windows()
        .into_iter()
        .find(|w| !w.get_input_mode())
        .unwrap()
}
fn unpause(store: &Store) {
    store
        .write(|ctx| {
            let mut pauses: settings::Pauses = settings::get(ctx.conn())?;
            pauses.network_traffic = false;
            settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
}

// leaf: audit-options-help-debug-action-fetch-a-url
#[test]
fn actual_menu_binary_save_decoded_clipboard_forget_cookie_headers_and_dismiss_deadline() {
    let fixture = hydrus_testkit::fixture_json("debug_fetch_url.json");
    let bytes = hex::decode(fixture["body_hex"].as_str().unwrap()).unwrap();
    let server = super::parser_editors::TestDocuments::start_with_response(
        bytes.clone(),
        "text/plain; charset=iso-8859-1",
    );
    let (_dirs, store) = super::subscriptions::store();
    unpause(&store);
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &network::NetworkContext::global(),
                "X-Debug-Review",
                Some("native-local"),
                Some(network::Approval::Approved),
                Some("test"),
            )
        })
        .unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    launch(&ui);
    let cancelled = bound.debug_fetch.windows()[0].clone_strong();
    cancelled.invoke_cancelled();
    cancelled.invoke_accepted();
    assert_eq!(bound.debug_fetch.running(), 0);
    assert!(server.requests.lock().unwrap().is_empty());
    let destination = tempfile::tempdir().unwrap();
    let path = destination.path().join("output.txt");
    hydrus_gui::set_picker({
        let path = path.clone();
        move |_, title| {
            assert_eq!(title, "select where to save content");
            vec![path.clone()]
        }
    });
    submit(&bound.debug_fetch, &format!("{}/png", server.base));
    let result = ready(&bound.debug_fetch);
    assert_eq!(
        result.get_message(),
        fixture["questions"][0]["message"].as_str().unwrap()
    );
    assert_eq!(
        jobs(&store)[0].status_title.as_deref(),
        Some("debug network job")
    );
    let completed = jobs(&store)[0].clone();
    assert!(completed.done && !completed.cancellable && !completed.dismissed);
    let deadline = completed.dismiss_at.unwrap();
    assert!(
        store
            .read(|conn| popups::get(conn, &completed.key, deadline))
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .read(|conn| popups::get(conn, &completed.key, deadline + 1))
            .unwrap()
            .is_none()
    );
    // Deadline visibility is independent of the result's captured response bytes.
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("debug-fetch-result.png"),
        &headless::render(&windows.get(windows.count() - 1).unwrap(), 600, 180),
        600,
        180,
    )
    .unwrap();
    result.invoke_choice(0);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::write(&path, b"unchanged").unwrap();
    result.show().unwrap();
    result.invoke_choice(0);
    assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let hydrus_gui::Clip::Text(text) = clip {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    submit(&bound.debug_fetch, &format!("{}/png", server.base));
    ui.hide().unwrap();
    bound.debug_fetch.open();
    assert_eq!(
        bound.debug_fetch.running(),
        1,
        "hidden launch refused without losing admitted work"
    );
    let text = ready(&bound.debug_fetch);
    assert!(
        text.window().is_visible(),
        "reference existing hidden request still delivers its response question"
    );
    text.invoke_choice(1);
    assert_eq!(
        &*copied.borrow(),
        &[fixture["clipboard"][0][1].as_str().unwrap().to_owned()]
    );
    ui.show().unwrap();
    submit(&bound.debug_fetch, &format!("{}/png", server.base));
    let forgotten = ready(&bound.debug_fetch);
    forgotten.invoke_cancelled();
    forgotten.show().unwrap();
    forgotten.invoke_choice(1);
    assert_eq!(copied.borrow().len(), 1);
    submit(&bound.debug_fetch, &format!("{}/png", server.base));
    let stale_save = ready(&bound.debug_fetch);
    hydrus_gui::set_picker({
        let control = bound.debug_fetch.clone();
        let path = path.clone();
        move |_, _| {
            control.retire();
            vec![path.clone()]
        }
    });
    stale_save.invoke_choice(0);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"unchanged",
        "owner retirement during file picker prevents a late write"
    );
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.contains("X-Debug-Review: native-local")
                || r.contains("x-debug-review: native-local"))
    );
    assert!(server.requests.lock().unwrap()[1].contains("test-document=saved"));
    let usage = store
        .read(|conn| hydrus_store::bandwidth::usage(conn, hydrus_core::TimestampMs::now().0 / 1000))
        .unwrap();
    let global = usage
        .into_iter()
        .find(|(context, _)| *context == network::NetworkContext::global())
        .unwrap()
        .1;
    assert_eq!(
        global.all_usage(hydrus_core::bandwidth::BandwidthType::Requests),
        4
    );
    assert_eq!(
        global.all_usage(hydrus_core::bandwidth::BandwidthType::Data),
        4 * bytes.len() as u64
    );
    hydrus_gui::set_picker(|_, _| Vec::new());
}

#[test]
fn overlapping_held_jobs_popup_stop_error_and_cancelled_save_have_real_consumers() {
    let server = super::parser_editors::TestDocuments::start();
    let (_dirs, store) = super::subscriptions::store();
    unpause(&store);
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    submit(&bound.debug_fetch, &format!("{}/hold", server.base));
    until(|| !server.requests.lock().unwrap().is_empty());
    submit(&bound.debug_fetch, &format!("{}/document", server.base));
    until(|| {
        bound.debug_fetch.tick();
        bound.debug_fetch.running() == 1 && !bound.debug_fetch.windows().is_empty()
    });
    let quick = bound.debug_fetch.windows()[0].clone_strong();
    assert!(!quick.get_input_mode());
    hydrus_gui::set_picker(|_, _| Vec::new());
    quick.invoke_choice(0);
    assert!(bound.debug_fetch.windows().is_empty());
    assert_eq!(
        jobs(&store)[0].status_title.as_deref(),
        Some("debug network job")
    );
    ui.invoke_popup_cancel(0);
    until(|| {
        bound.debug_fetch.tick();
        bound.debug_fetch.running() == 0
    });
    assert!(
        bound.debug_fetch.windows().is_empty(),
        "cancelled HTTP never offers partial response bytes"
    );
    let errors = jobs(&store)
        .iter()
        .filter(|job| job.status_title.as_deref() == Some("Exception"))
        .count();
    assert!(errors >= 1);
    submit(&bound.debug_fetch, &format!("{}/error", server.base));
    until(|| {
        bound.debug_fetch.tick();
        bound.debug_fetch.running() == 0
    });
    assert!(bound.debug_fetch.windows().is_empty());
    assert!(jobs(&store).iter().any(|job| {
        job.traceback
            .as_ref()
            .is_some_and(|error| error.contains("404") && error.contains("missing"))
    }));
}

#[test]
fn hidden_children_close_cancel_rebind_final_bound_drop_and_main_drop_retire_owned_work() {
    let server = super::parser_editors::TestDocuments::start();
    let (_dirs, store) = super::subscriptions::store();
    unpause(&store);
    store
        .write(|ctx| {
            let mut gui: settings::GuiSettings = settings::get(ctx.conn())?;
            gui.confirm_exit = true;
            // Isolate confirmed owner retirement from shutdown maintenance.
            let mut shutdown: hydrus_store::settings::ShutdownWork =
                hydrus_store::settings::get(ctx.conn())?;
            shutdown.action = 0;
            hydrus_store::settings::set(ctx.conn(), &shutdown)?;
            settings::set(ctx.conn(), &gui)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    bound.debug_fetch.open();
    let hidden = bound.debug_fetch.windows()[0].clone_strong();
    hidden.set_url(format!("{}/document", server.base).into());
    hidden.hide().unwrap();
    hidden.invoke_accepted();
    assert_eq!(bound.debug_fetch.running(), 0);
    hidden.invoke_cancelled();
    submit(&bound.debug_fetch, &format!("{}/hold", server.base));
    until(|| !server.requests.lock().unwrap().is_empty());
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(false);
    assert_eq!(bound.debug_fetch.running(), 1);
    let old = bound.debug_fetch.clone();
    let successor = bind(&ui, Pages::open(store.clone()).unwrap());
    assert_eq!(old.running(), 0);
    old.open();
    old.tick();
    assert!(old.windows().is_empty());
    submit(&successor.debug_fetch, &format!("{}/document", server.base));
    let result = ready(&successor.debug_fetch);
    ui.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);
    ui.invoke_answer(true);
    assert!(!ui.window().is_visible());
    assert!(!result.window().is_visible());
    ui.show().unwrap();
    result.show().unwrap();
    result.invoke_choice(1);
    assert!(successor.debug_fetch.windows().is_empty());
    successor.debug_fetch.open();
    assert!(successor.debug_fetch.windows().is_empty());
    let fresh = bind(&ui, Pages::open(store.clone()).unwrap());
    let retained = fresh.debug_fetch.clone();
    let clone = fresh.clone();
    drop(fresh);
    submit(&retained, &format!("{}/hold", server.base));
    assert_eq!(retained.running(), 1);
    drop(clone);
    assert_eq!(retained.running(), 0);
    retained.open();
    assert!(retained.windows().is_empty());
    let final_owner = bind(&ui, Pages::open(store.clone()).unwrap());
    let retained = final_owner.debug_fetch.clone();
    submit(&retained, &format!("{}/hold", server.base));
    ui.hide().unwrap();
    drop(ui);
    retained.tick();
    assert_eq!(retained.running(), 0);
    retained.open();
    assert!(retained.windows().is_empty());
}

#[test]
fn ordinary_get_waits_for_saved_bandwidth_rules_and_popup_stop_cancels_before_any_http() {
    use hydrus_core::bandwidth::{BandwidthType, Rule, Rules};
    use hydrus_store::bandwidth::BandwidthSettings;
    let server = super::parser_editors::TestDocuments::start();
    let (_dirs, store) = super::subscriptions::store();
    unpause(&store);
    store
        .write(|ctx| {
            let mut policy: BandwidthSettings = settings::get(ctx.conn())?;
            policy
                .rules
                .retain(|(context, _)| *context != network::NetworkContext::global());
            policy.rules.push((
                network::NetworkContext::global(),
                Rules::new([Rule {
                    kind: BandwidthType::Requests,
                    time_delta: Some(3600),
                    max_allowed: 0,
                }]),
            ));
            settings::set(ctx.conn(), &policy)
        })
        .unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    submit(&bound.debug_fetch, &format!("{}/document", server.base));
    until(|| {
        jobs(&store).iter().any(|job| {
            job.network_job
                .as_ref()
                .is_some_and(|network| network.status.starts_with("bandwidth free"))
        })
    });
    assert!(server.requests.lock().unwrap().is_empty());
    assert_eq!(bound.debug_fetch.running(), 1);
    ui.invoke_popup_cancel(0);
    until(|| {
        bound.debug_fetch.tick();
        bound.debug_fetch.running() == 0
    });
    assert!(
        server.requests.lock().unwrap().is_empty(),
        "cancellation releases a genuine bandwidth waiter without bypassing the saved rules"
    );
    assert!(bound.debug_fetch.windows().is_empty());
}
