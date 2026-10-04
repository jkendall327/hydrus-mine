//! Login credential/dialog consumers, exchange staging and stale handle safety.
use hydrus_gui::{
    headless, login_credential_window as credential,
    login_workflows_window::{self as windows, Slots},
};
use hydrus_legacy::{objects::logins as legacy, serialisable::SerialisableObject};
use hydrus_parse::login::LoginManager;
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io::{Read as _, Write as _},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
fn store() -> (tempfile::TempDir, Arc<Store>, LoginManager) {
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let manager = legacy::manager(
        &SerialisableObject::from_tuple_str(&fixture["manager"].to_string()).unwrap(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let initial = manager.clone();
    store
        .write_and_refresh(move |ctx| hydrus_store::logins::save(ctx.conn(), &initial))
        .unwrap();
    (dir, store, manager)
}
#[test]
fn credential_entry_masks_password_and_replays_reference_invalid_confirmation() {
    let (_dir, _store, manager) = store();
    let rendered = headless::init();
    let slot = credential::CredentialsSlot::default();
    let accepted = Rc::new(RefCell::new(None));
    let applied: credential::CredentialsApplied = Rc::new({
        let accepted = accepted.clone();
        move |value| {
            *accepted.borrow_mut() = Some(value);
            Ok(())
        }
    });
    let window = credential::open_credentials(
        &manager.scripts[0].credentials,
        &BTreeMap::new(),
        &slot,
        applied.clone(),
    )
    .unwrap();
    assert_eq!(window.get_rows().row_data(0).unwrap().name, "username");
    assert!(!window.get_rows().row_data(0).unwrap().hidden);
    assert!(window.get_rows().row_data(1).unwrap().hidden);
    window.invoke_edited(0, "1?!".into());
    window.invoke_edited(1, "x".into());
    window.invoke_action("apply".into());
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    assert_eq!(
        window.get_question(),
        fixture["credentials"][2]["questions"][0].as_str().unwrap()
    );
    window.invoke_action("back".into());
    assert!(window.get_question().is_empty());
    assert!(accepted.borrow().is_none());
    let pixels = headless::render(&rendered.get(0).unwrap(), 760, 360);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("login_credentials.png"),
        &pixels,
        760,
        360,
    )
    .unwrap();
    window.invoke_edited(0, "alice".into());
    window.invoke_edited(1, "dummy-pass".into());
    window.invoke_action("apply".into());
    assert_eq!(accepted.borrow().as_ref().unwrap()["username"], "alice");
    assert!(slot.borrow().is_none());
    *accepted.borrow_mut() = None;
    let fresh = credential::open_credentials(
        &manager.scripts[0].credentials,
        &BTreeMap::new(),
        &slot,
        applied,
    )
    .unwrap();
    fresh.invoke_action("apply".into());
    fresh.invoke_action("confirm".into());
    assert_eq!(accepted.borrow().as_ref().unwrap()["password"], "");
    *accepted.borrow_mut() = None;
    window.invoke_action("confirm".into());
    assert!(accepted.borrow().is_none());
}
#[test]
fn script_definition_matcher_child_stages_until_parent_apply_and_cancel_discards() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_credential_clicked(1, false, false);
    script.invoke_action("edit-credential".into());
    let child = slots.definition.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(child.get_name(), "username");
    child.invoke_action("matcher".into());
    let matcher = slots.strings.step.borrow().as_ref().unwrap().clone_strong();
    assert!(child.get_child_open());
    matcher.set_match_type(1);
    matcher.set_fixed("token".into());
    matcher.invoke_changed();
    matcher.invoke_apply();
    assert!(!child.get_child_open());
    child.set_name("account".into());
    child.set_kind(1);
    child.invoke_action("apply".into());
    assert!(slots.definition.borrow().is_none());
    assert_eq!(
        script
            .get_credentials()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "account"
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    script.invoke_action("apply".into());
    assert!(!script.get_question().is_empty());
    script.invoke_action("back".into());
    assert!(slots.script.borrow().is_some());
    script.invoke_action("apply".into());
    script.invoke_action("confirm".into());
    list.invoke_action("cancel".into());
    child.invoke_action("apply".into());
    script.invoke_action("confirm".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.set_name("renamed login".into());
    script.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[0].name, "renamed login");
    assert_eq!(saved.scripts[0].key, original.scripts[0].key);
    assert_eq!(saved.domains, original.domains);
    let list = windows::open_scripts(&store, &Slots::default()).unwrap();
    assert_eq!(
        list.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "renamed login"
    );
    list.invoke_action("cancel".into());
}
#[test]
fn login_exchange_reviews_without_mutating_then_parent_apply_persists_unique_scripts() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_action("import".into());
    let exchange = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    exchange.set_text(fixture["script"].to_string().into());
    exchange.invoke_action("review".into());
    assert!(exchange.get_ready());
    assert_eq!(list.get_rows().row_count(), 1);
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    exchange.invoke_action("apply".into());
    assert_eq!(list.get_rows().row_count(), 2);
    assert!(!list.get_child_open());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[1].name, "synthetic login (1)");
    assert_ne!(saved.scripts[1].key, original.scripts[0].key);
    assert_eq!(saved.domains, original.domains);
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_action("import".into());
    let child = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    list.invoke_action("cancel".into());
    child.set_text(fixture["script"].to_string().into());
    child.invoke_action("review".into());
    child.invoke_action("apply".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}

#[test]
fn network_login_scripts_menu_reaches_persisted_editor() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let ui = hydrus_gui::MainWindow::new().unwrap();
    let bound = hydrus_gui::bind(&ui, hydrus_gui::Pages::open(store.clone()).unwrap());
    let titles = ui.get_menu_titles();
    let network = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "network")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(network).unwrap(), 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let logins = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "logins")
        .unwrap();
    ui.invoke_menu_line_hovered(0, i32::try_from(logins).unwrap(), 300.0, 100.0, 10.0);
    let lines = ui.get_menu_panes().row_data(1).unwrap().lines;
    let scripts = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "login scripts…")
        .unwrap();
    assert!(lines.row_data(scripts).unwrap().usable);
    ui.invoke_menu_line_clicked(1, i32::try_from(scripts).unwrap(), 0.0, 0.0, 0.0);
    let window = bound
        .login_workflows
        .scripts
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(window.get_rows().row_count(), original.scripts.len());
    window.invoke_action("cancel".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
}

#[test]
fn script_step_content_preview_apply_and_parent_cancel_are_owned_and_restricted() {
    let (_dir, store, original) = store();
    let rendered = headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_step_clicked(0);
    script.invoke_action("edit-step".into());
    let step = slots.step.step.borrow().as_ref().unwrap().clone_strong();
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 880, 680);
    assert!(step.get_footer_y() >= 0.0 && step.get_footer_y() + step.get_footer_height() <= 680.0);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("login_step.png"),
        &pixels,
        880,
        680,
    )
    .unwrap();
    let fixture = hydrus_testkit::fixture_json("login_editors.json");
    let cells = step.get_content().row_data(0).unwrap().cells;
    assert_eq!(
        cells.row_data(1).unwrap(),
        fixture["step_states"][0]["state"]["content_rows"][0][1]
            .as_str()
            .unwrap()
    );
    step.invoke_content_clicked(0, false, false);
    step.invoke_action("edit-content".into());
    let content = slots
        .step
        .parsers
        .content
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let choices = content.get_fields().row_data(1).unwrap().options;
    assert_eq!(
        serde_json::to_value(
            (0..choices.row_count())
                .map(|i| choices.row_data(i).unwrap().to_string())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        fixture["permitted_content_types"][0]
    );
    assert!(content.get_document().is_empty());
    content.invoke_action("test".into());
    assert!(content.get_preview().contains("dummy-csrf"));
    content.invoke_text_edited(0, "renamed response".into());
    content.invoke_text_edited(5, "token".into());
    content.invoke_action("apply".into());
    assert_eq!(
        step.get_content()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "renamed response"
    );
    step.set_name("edited request".into());
    step.set_path("signin".into());
    step.set_has_subdomain(true);
    step.set_subdomain("".into());
    step.invoke_action("apply".into());
    assert_eq!(
        script
            .get_steps()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        "edited request"
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    script.invoke_action("apply".into());
    assert!(!script.get_question().is_empty());
    script.invoke_action("confirm".into());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[0].steps[0].path, "/signin");
    assert!(saved.scripts[0].steps[0].subdomain.is_none());
    assert_eq!(
        saved.scripts[0].steps[0].static_args,
        original.scripts[0].steps[0].static_args
    );
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_step_clicked(0);
    script.invoke_action("edit-step".into());
    let step = slots.step.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_content_clicked(0, false, false);
    step.invoke_action("edit-content".into());
    let content = slots
        .step
        .parsers
        .content
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    content.invoke_action("formula".into());
    let formula = slots
        .step
        .parsers
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 900, 650);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    list.invoke_action("cancel".into());
    assert!(slots.step.step.borrow().is_none());
    assert!(slots.step.parsers.content.borrow().is_none());
    assert!(slots.step.parsers.formula.formula.borrow().is_none());
    formula.invoke_apply();
    content.invoke_action("apply".into());
    step.invoke_action("apply".into());
    script.invoke_action("apply".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}

#[test]
fn domain_menu_credentials_stage_activation_and_persist_only_at_parent_apply() {
    let (_dir, store, original) = store();
    let _rendered = headless::init();
    let ui = hydrus_gui::MainWindow::new().unwrap();
    let bound = hydrus_gui::bind(&ui, hydrus_gui::Pages::open(store.clone()).unwrap());
    let titles = ui.get_menu_titles();
    let at = (0..titles.row_count())
        .position(|i| titles.row_data(i).unwrap().label == "network")
        .unwrap();
    ui.invoke_menu_title_pressed(i32::try_from(at).unwrap(), 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "logins")
        .unwrap();
    ui.invoke_menu_line_hovered(0, i32::try_from(at).unwrap(), 300.0, 100.0, 10.0);
    let lines = ui.get_menu_panes().row_data(1).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "logins…")
        .unwrap();
    ui.invoke_menu_line_clicked(1, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let slots = &bound.login_workflows.domains;
    let domains = slots.domains.borrow().as_ref().unwrap().clone_strong();
    domains.invoke_row_clicked(0, false, false);
    assert!(domains.get_can_edit());
    domains.invoke_action("credentials".into());
    let child = slots.credentials.borrow().as_ref().unwrap().clone_strong();
    assert!(child.get_rows().row_data(1).unwrap().hidden);
    child.invoke_edited(0, "1?!".into());
    child.invoke_edited(1, "x".into());
    child.invoke_action("apply".into());
    child.invoke_action("confirm".into());
    assert_eq!(
        domains
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(3)
            .unwrap(),
        "no"
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    domains.invoke_action("credentials".into());
    let valid = slots.credentials.borrow().as_ref().unwrap().clone_strong();
    valid.invoke_edited(0, "bob".into());
    valid.invoke_edited(1, "dummy-pass".into());
    valid.invoke_action("apply".into());
    assert_eq!(
        domains.get_question(),
        "Activate this login script for this domain?"
    );
    domains.invoke_action("leave-inactive".into());
    domains.invoke_action("cancel".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    let domains = hydrus_gui::login_domains_window::open(&store, slots).unwrap();
    domains.invoke_row_clicked(0, false, false);
    domains.invoke_action("flip-active".into());
    domains.invoke_action("credentials".into());
    let child = slots.credentials.borrow().as_ref().unwrap().clone_strong();
    child.invoke_edited(0, "bob".into());
    child.invoke_action("apply".into());
    domains.invoke_action("activate".into());
    domains.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(
        saved.domains["login.example"].credentials["username"],
        "bob"
    );
    assert!(saved.domains["login.example"].active);
    assert_eq!(saved.scripts, original.scripts);
    let domains = hydrus_gui::login_domains_window::open(&store, slots).unwrap();
    domains.invoke_row_clicked(0, false, false);
    domains.invoke_action("credentials".into());
    let child = slots.credentials.borrow().as_ref().unwrap().clone_strong();
    domains.invoke_action("cancel".into());
    assert!(slots.credentials.borrow().is_none());
    child.invoke_edited(0, "cancelled".into());
    child.invoke_action("apply".into());
    domains.invoke_action("apply".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}

fn recorded_result(row: &serde_json::Value) -> hydrus_net::login::TestResult {
    hydrus_net::login::TestResult {
        name: row[0].as_str().unwrap().to_owned(),
        url: row[1].as_str().unwrap().to_owned(),
        body: row[2].as_str().map(str::to_owned),
        data: row[3].as_str().unwrap().to_owned(),
        new_variables: serde_json::from_value(row[4].clone()).unwrap(),
        new_cookies: serde_json::from_value(row[5].clone()).unwrap(),
        result: row[6].as_str().unwrap().to_owned(),
    }
}
fn assert_review(window: &hydrus_gui::LoginTestResultWindow, state: &serde_json::Value) {
    assert_eq!(window.get_name(), state["name"].as_str().unwrap());
    assert_eq!(window.get_url(), state["url"].as_str().unwrap());
    assert_eq!(window.get_body(), state["body"].as_str().unwrap());
    assert_eq!(window.get_data(), state["data"].as_str().unwrap());
    assert_eq!(window.get_variables(), state["variables"].as_str().unwrap());
    assert_eq!(window.get_cookies(), state["cookies"].as_str().unwrap());
    assert_eq!(window.get_result(), state["result"].as_str().unwrap());
}
#[test]
fn result_review_replays_real_qt_fields_unicode_preview_full_copy_and_stale_close() {
    let rendered = headless::init();
    let fixture = hydrus_testkit::fixture_json("login_execution.json");
    let slot = hydrus_gui::login_test_window::ResultSlot::default();
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |value| {
            if let hydrus_gui::Clip::Text(text) = value {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    for case in fixture.as_array().unwrap() {
        for (row, state) in case["results"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["reviews"].as_array().unwrap())
        {
            let result = recorded_result(row);
            let window = hydrus_gui::login_test_window::open_result(&result, &slot).unwrap();
            assert_review(&window, state);
            window.invoke_action("copy".into());
            assert_eq!(
                copied.borrow().last().unwrap(),
                state["copied"][0][1].as_str().unwrap()
            );
            assert!(window.get_copied());
            window.invoke_action("close".into());
            assert!(slot.borrow().is_none());
            let count = copied.borrow().len();
            window.invoke_action("copy".into());
            assert_eq!(copied.borrow().len(), count);
        }
    }
    let long = &fixture[0]["long_review"];
    let result = recorded_result(&long["input"]);
    let window = hydrus_gui::login_test_window::open_result(&result, &slot).unwrap();
    assert_review(&window, &long["state"]);
    assert_eq!(window.get_data().chars().count(), 1024);
    window.invoke_action("copy".into());
    assert_eq!(copied.borrow().last().unwrap(), &result.data);
    assert_eq!(result.data.chars().count(), 1400);
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 900, 650);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("login_test_result.png"),
        &pixels,
        900,
        650,
    )
    .unwrap();
    hydrus_gui::login_test_window::cancel_result(&slot);
    assert!(!window.window().is_visible());
    hydrus_gui::set_clipper(|_| {});
}
struct LoginSite {
    domain: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl LoginSite {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let domain = listener.local_addr().unwrap().to_string();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread = std::thread::spawn({
            let stop = stop.clone();
            let requests = requests.clone();
            move || {
                while !stop.load(Ordering::Relaxed) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    };
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = vec![0; 8192];
                    let mut count = 0;
                    let mut expected = None;
                    while count < bytes.len() {
                        match stream.read(&mut bytes[count..]) {
                            Ok(0) | Err(_) => break,
                            Ok(read) => count += read,
                        }
                        if expected.is_none()
                            && let Some(end) =
                                bytes[..count].windows(4).position(|v| v == b"\r\n\r\n")
                        {
                            let headers = String::from_utf8_lossy(&bytes[..end]);
                            let length = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length: ")
                                        .map(|n| n.parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            expected = Some(end + 4 + length);
                        }
                        if expected.is_some_and(|expected| count >= expected) {
                            break;
                        }
                    }
                    if count == 0 {
                        continue;
                    }
                    let request = String::from_utf8_lossy(&bytes[..count]).into_owned();
                    let post = request.starts_with("POST ");
                    requests.lock().unwrap().push(request);
                    let (body, cookie) = if post {
                        ("login response", "session=ok; Path=/")
                    } else {
                        (
                            "<input name=\"csrf\" value=\"loop-token\"><p>start</p>",
                            "preflight=ready; Path=/",
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nSet-Cookie: {cookie}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
        });
        Self {
            domain,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for LoginSite {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn until_login(mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(
            started.elapsed() < Duration::from_secs(16),
            "login did not finish"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn script_editor_runs_real_http_with_fresh_cookies_and_reviews_without_saving() {
    let site = LoginSite::start();
    let fixture = hydrus_testkit::fixture_json("login_execution.json");
    let value = legacy::login_script(
        &SerialisableObject::from_tuple_str(&fixture[0]["script"].to_string()).unwrap(),
    )
    .unwrap();
    let (_dir, store, mut original) = store();
    original.scripts = vec![value];
    let manager = original.clone();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::logins::save(ctx.conn(), &manager)?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::network::NetworkSettings {
                    detect_sleep: false,
                    network_timeout: 2,
                    max_connection_attempts: 1,
                    max_get_attempts: 1,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let window = slots.script.borrow().as_ref().unwrap().clone_strong();
    window.set_test_domain(site.domain.clone().into());
    window.invoke_action("run-test".into());
    assert!(window.get_child_open());
    let credentials = slots.credentials.borrow().as_ref().unwrap().clone_strong();
    for i in 0..credentials.get_rows().row_count() {
        let row = credentials.get_rows().row_data(i).unwrap();
        credentials.invoke_edited(
            i32::try_from(i).unwrap(),
            fixture[0]["credentials"][row.name.as_str()]
                .as_str()
                .unwrap()
                .into(),
        );
    }
    credentials.invoke_action("apply".into());
    assert!(window.get_running());
    window.invoke_action("apply".into());
    assert_eq!(
        window.get_error(),
        "Currently testing! Please cancel it first!"
    );
    until_login(|| !window.get_running());
    assert_eq!(
        window.get_final_result(),
        fixture[0]["outcome"].as_str().unwrap()
    );
    assert_eq!(window.get_results().row_count(), 2);
    let requests = site.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[1].contains("token=loop-token"));
    assert!(requests[1].contains("pass=dummy+%2B+pass%26%3D"));
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("cookie: preflight=ready")
    );
    drop(requests);
    assert!(
        store
            .read(hydrus_store::network::sessions)
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    window.invoke_result_clicked(1);
    window.invoke_action("review-result".into());
    let review = slots.result.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        review.get_data(),
        fixture[0]["reviews"][1]["data"].as_str().unwrap()
    );
    review.invoke_action("close".into());
    window.invoke_action("run-test".into());
    let credentials = slots.credentials.borrow().as_ref().unwrap().clone_strong();
    credentials.invoke_action("apply".into());
    until_login(|| site.requests.lock().unwrap().len() == 3);
    assert!(slots.run.busy());
    window.invoke_action("cancel".into());
    assert!(!slots.run.busy());
    assert!(slots.result.borrow().is_none());
    assert!(!review.window().is_visible());
    window.invoke_action("run-test".into());
    review.invoke_action("copy".into());
    assert!(slots.credentials.borrow().is_none());
    for _ in 0..20 {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(site.requests.lock().unwrap().len(), 3);
    list.invoke_action("cancel".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
}

#[test]
fn domain_login_confirmation_saves_then_real_http_persists_session_and_outcome() {
    let site = LoginSite::start();
    let second = LoginSite::start();
    let (dir, store, mut original) = store();
    let fixture = hydrus_testkit::fixture_json("login_execution.json");
    let script = legacy::login_script(
        &SerialisableObject::from_tuple_str(&fixture[0]["script"].to_string()).unwrap(),
    )
    .unwrap();
    let mut domain = original.domains.values().next().unwrap().clone();
    domain.script_key.clone_from(&script.key);
    domain.script_name.clone_from(&script.name);
    domain.active = true;
    domain.validity = hydrus_parse::login::Validity::Untested;
    domain.credentials = serde_json::from_value(fixture[0]["credentials"].clone()).unwrap();
    original.scripts = vec![script];
    original.domains.clear();
    original.domains.insert(site.domain.clone(), domain.clone());
    original.domains.insert(second.domain.clone(), domain);
    let manager = original.clone();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::logins::save(ctx.conn(), &manager)?;
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::network::NetworkSettings {
                    detect_sleep: false,
                    network_timeout: 2,
                    max_connection_attempts: 1,
                    max_get_attempts: 1,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    headless::init();
    let slots = hydrus_gui::login_domains_window::Slots::default();
    let window = hydrus_gui::login_domains_window::open(&store, &slots).unwrap();
    window.invoke_row_clicked(0, false, false);
    window.invoke_row_clicked(1, true, false);
    window.invoke_action("do-login".into());
    let reference = hydrus_testkit::fixture_json("login_editors.json");
    let mut names = [site.domain.as_str(), second.domain.as_str()];
    names.sort_unstable();
    assert_eq!(
        window.get_question(),
        reference["domain_login_actions"][0]["questions"][0]
            .as_str()
            .unwrap()
            .replace("login.example", &names.join("\n"))
    );
    window.invoke_action("back-login".into());
    assert!(!slots.run.busy());
    assert!(site.requests.lock().unwrap().is_empty());
    window.invoke_action("do-login".into());
    window.invoke_action("confirm-login".into());
    assert!(slots.domains.borrow().is_none());
    assert!(slots.run.busy());
    assert!(!window.window().is_visible());
    window.invoke_action("flip-active".into());
    until_login(|| !slots.run.busy());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(
        saved.domains[&site.domain].validity,
        hydrus_parse::login::Validity::Valid
    );
    assert!(saved.domains[&site.domain].active);
    assert_eq!(
        saved.domains[&second.domain].validity,
        hydrus_parse::login::Validity::Valid
    );
    assert_eq!(second.requests.lock().unwrap().len(), 2);
    assert_eq!(saved.scripts, original.scripts);
    assert_eq!(site.requests.lock().unwrap().len(), 2);
    assert!(hydrus_net::login::logged_in(&store, &saved.scripts[0], &site.domain).unwrap());
    let reopened = Store::open(dir.path()).unwrap();
    assert!(hydrus_net::login::logged_in(&reopened, &saved.scripts[0], &site.domain).unwrap());
    let window = hydrus_gui::login_domains_window::open(&store, &slots).unwrap();
    window.invoke_row_clicked(0, false, false);
    window.invoke_action("do-login".into());
    assert_eq!(
        window.get_error(),
        reference["domain_login_actions"][2]["warnings"][0]
            .as_str()
            .unwrap()
    );
    assert!(!slots.run.busy());
    window.invoke_action("cancel".into());
    slots.cancel();
}

#[test]
fn step_argument_draft_rejects_duplicates_stages_cancel_and_reaches_actual_http() {
    let fixture = hydrus_testkit::fixture_json("login_execution.json");
    let mut script = legacy::login_script(
        &SerialisableObject::from_tuple_str(&fixture[0]["script"].to_string()).unwrap(),
    )
    .unwrap();
    let (_dir, store, _manager) = store();
    store
        .write_and_refresh(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::network::NetworkSettings {
                    detect_sleep: false,
                    network_timeout: 2,
                    max_connection_attempts: 1,
                    max_get_attempts: 1,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    headless::init();
    let slots = hydrus_gui::login_step_window::Slots::default();
    let accepted = Rc::new(RefCell::new(None));
    let callback: hydrus_gui::login_step_window::Applied = Rc::new({
        let accepted = accepted.clone();
        move |step| {
            *accepted.borrow_mut() = Some(step);
            Ok(())
        }
    });
    let window =
        hydrus_gui::login_step_window::open(&store, &script.steps[0], &slots, callback.clone())
            .unwrap();
    window.invoke_action("add-variable".into());
    window.set_variable_kind(1);
    window.set_variable_key("lang".into());
    window.set_variable_value("bad".into());
    window.invoke_action("save-variable".into());
    assert!(window.get_variable_editing());
    assert_eq!(window.get_error(), "That parameter name already exists!");
    window.invoke_action("cancel-variable".into());
    window.invoke_action("add-variable".into());
    window.set_variable_key("discarded".into());
    window.set_variable_value("".into());
    window.invoke_action("save-variable".into());
    window.invoke_action("cancel".into());
    assert!(accepted.borrow().is_none());
    let window =
        hydrus_gui::login_step_window::open(&store, &script.steps[0], &slots, callback).unwrap();
    assert_eq!(window.get_variables().row_count(), 1);
    window.invoke_variable_clicked(0);
    assert!(window.get_variables().row_data(0).unwrap().selected);
    window.invoke_action("edit-variable".into());
    window.set_variable_key("probe".into());
    window.set_variable_value("changed".into());
    window.invoke_action("save-variable".into());
    window.invoke_variable_clicked(0);
    window.invoke_action("delete-variable".into());
    assert!(window.get_deleting());
    window.invoke_action("back".into());
    assert_eq!(window.get_variables().row_count(), 1);
    window.invoke_action("apply".into());
    script.steps[0] = accepted.borrow_mut().take().unwrap();
    assert_eq!(script.steps[0].static_args.len(), 1);
    assert_eq!(script.steps[0].static_args["probe"], "changed");
    let site = LoginSite::start();
    let run = hydrus_gui::login_test_window::RunSlot::default();
    let completed = Rc::new(RefCell::new(None));
    run.start(
        hydrus_gui::login_test_window::Input {
            source: store,
            script,
            domain: site.domain.clone(),
            credentials: serde_json::from_value(fixture[0]["credentials"].clone()).unwrap(),
            test: true,
        },
        Rc::new(|_| {}),
        Rc::new({
            let completed = completed.clone();
            move |execution| *completed.borrow_mut() = Some(execution)
        }),
    );
    until_login(|| !run.busy());
    assert_eq!(
        completed.borrow().as_ref().unwrap().outcome,
        hydrus_net::login::Outcome::Success
    );
    assert!(site.requests.lock().unwrap()[0].starts_with("GET /start?probe=changed "));
    window.invoke_action("apply".into());
    assert!(accepted.borrow().is_none());
}

fn fixed_cookie_matcher(slots: &hydrus_gui::login_cookies_window::Slots, text: &str) {
    let child = slots.strings.step.borrow().as_ref().unwrap().clone_strong();
    child.set_match_type(1);
    child.set_fixed(text.into());
    child.invoke_changed();
    child.invoke_apply();
}
#[test]
fn shared_cookie_list_owns_matchers_and_stages_both_script_and_step_consumers() {
    let (_dir, store, original) = store();
    headless::init();
    let slots = Slots::default();
    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_action("cookies".into());
    let cookies = slots
        .cookies
        .window
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert!(script.get_child_open());
    cookies.invoke_action("add".into());
    cookies.invoke_action("name-match".into());
    assert!(cookies.get_child_open());
    fixed_cookie_matcher(&slots.cookies, "probe");
    cookies.invoke_action("value-match".into());
    fixed_cookie_matcher(&slots.cookies, "ready");
    cookies.invoke_action("save-row".into());
    assert_eq!(cookies.get_rows().row_count(), 2);
    cookies.invoke_action("apply".into());
    assert!(!script.get_child_open());
    assert_eq!(script.get_cookies().row_count(), 2);
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    script.invoke_step_clicked(1);
    script.invoke_action("edit-step".into());
    let step = slots.step.step.borrow().as_ref().unwrap().clone_strong();
    step.invoke_action("cookies".into());
    let child = slots
        .step
        .cookies
        .window
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    child.invoke_row_clicked(0, false, false);
    child.invoke_action("edit".into());
    child.invoke_action("value-match".into());
    fixed_cookie_matcher(&slots.step.cookies, "updated");
    child.invoke_action("save-row".into());
    child.invoke_action("apply".into());
    step.invoke_action("apply".into());
    script.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(saved.scripts[0].required_cookies.len(), 2);
    assert!(
        saved.scripts[0]
            .required_cookies
            .iter()
            .any(|row| row.name == hydrus_core::url::strings::StringMatch::fixed("probe"))
    );
    assert_eq!(
        saved.scripts[0].steps[1].required_cookies[0].value,
        hydrus_core::url::strings::StringMatch::fixed("updated")
    );
    let site = LoginSite::start();
    let mut executing = saved.scripts[0].clone();
    executing.steps[0].subdomain = None;
    store
        .write_and_refresh(|ctx| {
            hydrus_store::settings::set(
                ctx.conn(),
                &hydrus_store::network::NetworkSettings {
                    detect_sleep: false,
                    max_connection_attempts: 1,
                    max_get_attempts: 1,
                    network_timeout: 2,
                    ..Default::default()
                },
            )
        })
        .unwrap();
    let run = hydrus_gui::login_test_window::RunSlot::default();
    let finished = Rc::new(RefCell::new(None));
    run.start(
        hydrus_gui::login_test_window::Input {
            source: store.clone(),
            script: executing,
            domain: site.domain.clone(),
            credentials: original
                .domains
                .values()
                .next()
                .unwrap()
                .credentials
                .clone(),
            test: true,
        },
        Rc::new(|_| {}),
        Rc::new({
            let finished = finished.clone();
            move |result| *finished.borrow_mut() = Some(result)
        }),
    );
    until_login(|| !run.busy());
    assert!(
        matches!(&finished.borrow().as_ref().unwrap().outcome,hydrus_net::login::Outcome::Verification(error) if error.contains("updated"))
    );
    assert_eq!(site.requests.lock().unwrap().len(), 2);

    let list = windows::open_scripts(&store, &slots).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let script = slots.script.borrow().as_ref().unwrap().clone_strong();
    script.invoke_action("cookies".into());
    let cookies = slots
        .cookies
        .window
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    cookies.invoke_row_clicked(0, false, false);
    cookies.invoke_row_clicked(1, true, false);
    cookies.invoke_action("delete".into());
    assert!(cookies.get_deleting());
    cookies.invoke_action("back".into());
    assert_eq!(cookies.get_rows().row_count(), 2);
    cookies.invoke_action("delete".into());
    cookies.invoke_action("confirm-delete".into());
    assert_eq!(cookies.get_rows().row_count(), 0);
    cookies.invoke_action("add".into());
    cookies.invoke_action("name-match".into());
    let matcher = slots
        .cookies
        .strings
        .step
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    script.invoke_action("cancel".into());
    assert!(slots.cookies.window.borrow().is_none());
    assert!(!matcher.window().is_visible());
    matcher.invoke_apply();
    cookies.invoke_action("apply".into());
    list.invoke_action("cancel".into());
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), saved);
}
