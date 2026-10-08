//! Real widgets with a native store: detached edits, persistence and nested cancellation.
use hydrus_gui::{
    headless,
    network_sessions_window::{self as windows, Slots},
};
use hydrus_store::{
    Store,
    network::{self, Approval, NetworkContext},
};
use slint::{ComponentHandle, Model};
fn screenshot(rendered: &headless::Windows, index: usize, name: &str, width: u32, height: u32) {
    let pixels = headless::render(&rendered.get(index).unwrap(), width, height);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name),
        &pixels,
        width,
        height,
    )
    .unwrap();
}
// leaf: audit-network-cookies-identity
#[test]
fn browse_create_edit_cancel_apply_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let rendered = headless::init();
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    assert_eq!(browser.get_rows().row_count(), 0);
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_domain("example.com".into());
    edit.invoke_apply_clicked();
    assert!(browser.get_show_empty());
    assert_eq!(browser.get_rows().row_count(), 1);
    browser.set_show_empty(false);
    browser.invoke_filter_changed();
    assert_eq!(browser.get_rows().row_count(), 0);
    browser.set_show_empty(true);
    browser.set_filter("unmatched".into());
    browser.invoke_filter_changed();
    assert_eq!(browser.get_rows().row_count(), 0);
    browser.set_filter("example.com".into());
    browser.invoke_filter_changed();
    assert_eq!(browser.get_rows().row_count(), 1);
    browser.set_filter("".into());
    browser.invoke_filter_changed();
    screenshot(&rendered, 0, "network-session-browser.png", 1100, 620);
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_name("sid".into());
    edit.set_value("token".into());
    edit.set_session_cookie(true);
    edit.set_secure(true);
    screenshot(
        &rendered,
        rendered.count() - 1,
        "network-cookie-editor.png",
        680,
        560,
    );
    edit.invoke_apply_clicked();
    assert_eq!(cookies.get_rows().row_count(), 1);
    assert!(
        store
            .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
            .unwrap()
            .is_empty()
    );
    cookies.invoke_cancel_clicked();
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(cookies.get_rows().row_count(), 0);
    cookies.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_name("sid".into());
    edit.set_value("token".into());
    edit.set_session_cookie(true);
    edit.set_secure(true);
    edit.invoke_apply_clicked();
    cookies.invoke_apply_clicked();
    let stored = store
        .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(stored[0].expires, None);
    assert!(stored[0].secure);
    browser.invoke_refresh_clicked();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_row_clicked(0, false, false);
    cookies.invoke_edit_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_name("renamed".into());
    edit.set_path("/private".into());
    edit.set_expires("4102444800".into());
    edit.set_session_cookie(false);
    edit.invoke_apply_clicked();
    cookies.invoke_apply_clicked();
    let stored = store
        .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "renamed");
    assert_eq!(stored[0].path, "/private");
    assert_eq!(stored[0].expires, Some(4_102_444_800));
    browser.invoke_refresh_clicked();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_delete_clicked();
    assert_eq!(
        browser.get_question(),
        hydrus_gui_model::network_sessions::CLEAR_QUESTION
    );
    browser.invoke_answered(false);
    assert_eq!(store.read(network::sessions).unwrap().len(), 1);
    browser.invoke_delete_clicked();
    browser.invoke_answered(true);
    assert!(store.read(network::sessions).unwrap().is_empty());
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    browser.invoke_cancel_clicked();
    assert!(!edit.window().is_visible());
    edit.set_domain("stale.example".into());
    edit.invoke_apply_clicked();
    assert!(store.read(network::sessions).unwrap().is_empty());
}
// leaf: audit-network-headers-fields
#[test]
fn headers_validate_duplicate_delete_cancel_reopen_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let rendered = headless::init();
    let slots = Slots::default();
    let headers = windows::open(&store, &slots, true).unwrap();
    assert_eq!(headers.get_rows().row_count(), 3);
    headers.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_domain("example.com".into());
    edit.set_name("X Token".into());
    edit.invoke_apply_clicked();
    assert!(!edit.get_error().is_empty());
    assert!(edit.window().is_visible());
    edit.set_name("X-Token".into());
    edit.set_value("secret".into());
    edit.set_reason("login".into());
    edit.set_approval(1);
    screenshot(
        &rendered,
        rendered.count() - 1,
        "network-header-editor.png",
        680,
        560,
    );
    edit.invoke_apply_clicked();
    assert_eq!(headers.get_rows().row_count(), 4);
    assert!(
        store
            .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
            .unwrap()
            .is_empty()
    );
    headers.invoke_apply_clicked();
    let stored = store
        .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert_eq!(stored[0].approval, Approval::Denied);
    assert_eq!(stored[0].reason, "login");
    let headers = windows::open(&store, &slots, true).unwrap();
    let index = headers
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap().contains("example.com"))
        .unwrap();
    headers.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    headers.invoke_duplicate_clicked();
    assert_eq!(headers.get_rows().row_count(), 5);
    headers.invoke_cancel_clicked();
    let headers = windows::open(&store, &slots, true).unwrap();
    assert_eq!(headers.get_rows().row_count(), 4);
    let index = headers
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap().contains("example.com"))
        .unwrap();
    headers.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    headers.invoke_delete_clicked();
    headers.invoke_answered(true);
    headers.invoke_apply_clicked();
    assert!(
        store
            .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
            .unwrap()
            .is_empty()
    );
}
#[test]
fn applied_widgets_change_existing_engines_outgoing_requests() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        for _ in 0..3 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let body = String::from_utf8(request).unwrap();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _rendered = headless::init();
    let slots = Slots::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let engine = {
        let _guard = runtime.enter();
        hydrus_net::NetEngine::new(
            store.clone(),
            hydrus_net::NetOptions {
                obey_bandwidth: false,
                ..hydrus_net::NetOptions::default()
            },
        )
        .unwrap()
    };
    let browser = windows::open(&store, &slots, false).unwrap();
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_domain(address.clone().into());
    edit.invoke_apply_clicked();
    assert!(edit.get_error().is_empty(), "{}", edit.get_error());
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    let cookie_path = dir.path().join("cookies.txt");
    std::fs::write(&cookie_path, "# Netscape HTTP Cookie File\n#HttpOnly_127.0.0.1\tFALSE\t/\tFALSE\t0\tsid\tfrom-gui\n127.0.0.1\tFALSE\t/private\tFALSE\t0\tprivate\thidden\n127.0.0.1\tFALSE\t/\tFALSE\t1\texpired\thidden\n").unwrap();
    hydrus_gui::set_picker(move |_, _| vec![cookie_path.clone()]);
    cookies.invoke_import_file_clicked();
    assert!(cookies.get_error().is_empty(), "{}", cookies.get_error());
    cookies.invoke_apply_clicked();
    let headers = windows::open(&store, &slots, true).unwrap();
    headers.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_domain(address.into());
    edit.set_name("X-Gui".into());
    edit.set_value("approved".into());
    edit.invoke_apply_clicked();
    headers.invoke_apply_clicked();
    let request = hydrus_net::Request::get(format!("http://{}/echo", listener_address(&store)));
    let text = runtime
        .block_on(engine.fetch(&request, &hydrus_net::Job::new()))
        .unwrap()
        .text()
        .to_lowercase();
    assert!(text.contains("cookie: sid=from-gui"), "{text}");
    assert!(
        !text.contains("hidden"),
        "expired and wrong-path imports must not be sent: {text}"
    );
    assert!(text.contains("x-gui: approved"), "{text}");
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    let index = cookies
        .get_rows()
        .iter()
        .position(|row| row.cells.row_data(0).unwrap() == "sid")
        .unwrap();
    cookies.invoke_row_clicked(i32::try_from(index).unwrap(), false, false);
    cookies.invoke_edit_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_secure(true);
    edit.invoke_apply_clicked();
    cookies.invoke_apply_clicked();
    let text = runtime
        .block_on(engine.fetch(&request, &hydrus_net::Job::new()))
        .unwrap()
        .text()
        .to_lowercase();
    assert!(
        !text.contains("cookie:"),
        "secure cookies must not be sent on HTTP: {text}"
    );
    let headers = windows::open(&store, &slots, true).unwrap();
    let i = headers
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(1).unwrap().starts_with("X-Gui:"))
        .unwrap();
    headers.invoke_row_clicked(i32::try_from(i).unwrap(), false, false);
    headers.invoke_edit_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_approval(1);
    edit.invoke_apply_clicked();
    headers.invoke_apply_clicked();
    let text = runtime
        .block_on(engine.fetch(&request, &hydrus_net::Job::new()))
        .unwrap()
        .text()
        .to_lowercase();
    assert!(!text.contains("x-gui:"), "{text}");
    server.join().unwrap();
    browser.invoke_cancel_clicked();
}
fn listener_address(store: &Store) -> String {
    store
        .read(network::header_contexts)
        .unwrap()
        .into_iter()
        .find(|c| c.data.starts_with("127.0.0.1:"))
        .unwrap()
        .data
}
#[test]
fn dropping_final_owner_cancels_open_drafts_and_invalidates_child_callbacks() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _rendered = headless::init();
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    browser.invoke_add_clicked();
    let child = windows::last_edit_opened().unwrap();
    let clone = slots.clone();
    drop(slots);
    assert!(browser.window().is_visible());
    assert!(child.window().is_visible());
    drop(clone);
    assert!(!browser.window().is_visible());
    assert!(!child.window().is_visible());
    child.set_domain("stale.example".into());
    child.invoke_apply_clicked();
    browser.invoke_add_clicked();
    assert!(store.read(network::sessions).unwrap().is_empty());
    let slots = Slots::default();
    let headers = windows::open(&store, &slots, true).unwrap();
    headers.invoke_add_clicked();
    let child = windows::last_edit_opened().unwrap();
    child.set_global(true);
    child.set_name("X-Stale".into());
    child.set_value("discarded".into());
    drop(slots);
    assert!(!headers.window().is_visible());
    assert!(!child.window().is_visible());
    child.invoke_apply_clicked();
    headers.invoke_apply_clicked();
    assert!(
        store
            .read(|c| network::headers(c, &NetworkContext::global()))
            .unwrap()
            .iter()
            .all(|h| h.name != "X-Stale")
    );
    let session = NetworkContext::domain("example.com");
    store
        .write(move |ctx| network::create_session(ctx.conn(), &session))
        .unwrap();
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    browser.set_show_empty(true);
    browser.invoke_filter_changed();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_add_clicked();
    let child = windows::last_edit_opened().unwrap();
    drop(slots);
    assert!(!browser.window().is_visible());
    assert!(!cookies.window().is_visible());
    assert!(!child.window().is_visible());
    child.invoke_apply_clicked();
    cookies.invoke_apply_clicked();
    assert!(
        store
            .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cookie_exchange_widgets_confirm_filter_cancel_import_export_and_report_errors() {
    use std::{cell::RefCell, rc::Rc};
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/cookie_exchange.json"
    ))
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let rendered = headless::init();
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    let text = Rc::new(RefCell::new(
        f["clipboard_text"].as_str().unwrap().to_owned(),
    ));
    hydrus_gui::set_paster({
        let text = text.clone();
        move || text.borrow().clone()
    });
    browser.invoke_import_clipboard_clicked();
    let import = windows::last_import_opened().unwrap();
    assert!(!import.get_choosing());
    import.invoke_action("cancel".into());
    assert!(store.read(network::sessions).unwrap().is_empty());
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_domain("example.com".into());
    edit.invoke_apply_clicked();
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_import_clipboard_clicked();
    let import = windows::last_import_opened().unwrap();
    assert!(import.get_choosing());
    assert_eq!(
        import.get_message(),
        f["questions"][0]["text"].as_str().unwrap()
    );
    assert_eq!(
        import.get_matching_label(),
        f["questions"][0]["yes"].as_str().unwrap()
    );
    import.invoke_action("matching".into());
    assert_eq!(
        import.get_message(),
        f["questions"][1]["text"].as_str().unwrap()
    );
    screenshot(
        &rendered,
        rendered.count() - 1,
        "cookie-import-confirmation.png",
        700,
        350,
    );
    import.invoke_action("cancel".into());
    assert_eq!(cookies.get_rows().row_count(), 0);
    cookies.invoke_import_clipboard_clicked();
    let import = windows::last_import_opened().unwrap();
    import.invoke_action("matching".into());
    import.invoke_action("import".into());
    assert_eq!(cookies.get_rows().row_count(), 1);
    assert_eq!(cookies.get_message(), "Added 1 cookies!");
    assert!(
        store
            .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
            .unwrap()
            .is_empty()
    );
    cookies.invoke_import_clipboard_clicked();
    let import = windows::last_import_opened().unwrap();
    import.invoke_action("all".into());
    assert_eq!(
        import.get_message(),
        f["questions"][3]["text"].as_str().unwrap()
    );
    import.invoke_action("import".into());
    assert_eq!(cookies.get_rows().row_count(), 2);
    let copied = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    cookies.invoke_row_clicked(0, false, false);
    cookies.invoke_export_clicked();
    let hydrus_gui::Clip::Text(export) = copied.borrow()[0].clone() else {
        panic!("cookie export must be text");
    };
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&export)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    cookies.invoke_cancel_clicked();
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(cookies.get_rows().row_count(), 0);
    let path = dir.path().join("cookies.txt");
    std::fs::write(&path, f["netscape_text"].as_str().unwrap()).unwrap();
    hydrus_gui::set_picker({
        let path = path.clone();
        move |kind, title| {
            assert_eq!(kind, hydrus_gui::Pick::Files);
            assert_eq!(title, "select cookies.txt");
            vec![path.clone()]
        }
    });
    cookies.invoke_import_file_clicked();
    assert!(cookies.get_error().is_empty(), "{}", cookies.get_error());
    assert_eq!(cookies.get_rows().row_count(), 4);
    assert_eq!(cookies.get_message(), f["messages"][0].as_str().unwrap());
    std::fs::write(&path, "# Netscape HTTP Cookie File\nmalformed").unwrap();
    cookies.invoke_import_file_clicked();
    assert!(cookies.get_error().contains("line 2"));
    assert_eq!(cookies.get_rows().row_count(), 4);
    *text.borrow_mut() = "not json".into();
    cookies.invoke_import_clipboard_clicked();
    assert!(cookies.get_error().contains("Did not understand"));
    *text.borrow_mut() = "[]".into();
    cookies.invoke_import_clipboard_clicked();
    assert_eq!(cookies.get_message(), f["messages"][3].as_str().unwrap());
    hydrus_gui::set_picker(|_, _| Vec::new());
    cookies.invoke_import_file_clicked();
    assert_eq!(cookies.get_rows().row_count(), 4);
    cookies.invoke_apply_clicked();
    let stored = store
        .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
        .unwrap();
    assert!(
        stored
            .iter()
            .any(|c| c.secure && c.rest == vec![("HTTPOnly".into(), Some(String::new()))])
    );
    // Browser clipboard imports commit immediately after their separate confirmation.
    *text.borrow_mut() = "[[\"new\",\"token\",\".sub.example.com\",\"/\",0]]".into();
    browser.invoke_import_clipboard_clicked();
    let import = windows::last_import_opened().unwrap();
    import.invoke_action("import".into());
    assert!(
        store
            .read(|c| network::cookies(c, &NetworkContext::domain("example.com")))
            .unwrap()
            .iter()
            .any(|c| c.name == "new")
    );
    browser.invoke_row_clicked(0, false, false);
    browser.invoke_export_clicked();
    assert_eq!(copied.borrow().len(), 2);
    // Dropping the browser while a confirmation is open invalidates stale child actions.
    browser.invoke_import_clipboard_clicked();
    let import = windows::last_import_opened().unwrap();
    browser.invoke_cancel_clicked();
    assert!(!import.window().is_visible());
    import.invoke_action("import".into());
}

fn wait_for_header(mut predicate: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !predicate() {
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn automatic_header_dialog_approval_denial_later_and_stale_callbacks() {
    use hydrus_gui::network_header_approval::{Monitor, last_question};
    use hydrus_store::{
        network_runtime::{NetworkJob, Snapshot, WaitReason},
        settings,
    };
    let fixture = hydrus_testkit::fixture_json("header_approval.json");
    let rendered = headless::init();
    let parent = hydrus_gui::MainWindow::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = jiff::Timestamp::now().as_second();
    store
        .write(move |ctx| {
            for (context, name, value, reason) in [
                (
                    NetworkContext::global(),
                    "X-Test",
                    "global-value",
                    "global reason",
                ),
                (
                    NetworkContext::domain("example.com"),
                    "Authorization",
                    "token",
                    "login reason",
                ),
            ] {
                network::set_header(
                    ctx.conn(),
                    &context,
                    name,
                    Some(value),
                    Some(Approval::Pending),
                    Some(reason),
                )?;
            }
            settings::set(
                ctx.conn(),
                &Snapshot {
                    epoch: "question test".into(),
                    at: now,
                    usage: Vec::new(),
                    jobs: vec![NetworkJob {
                        id: 1,
                        url: "https://example.com/file".into(),
                        status: "waiting".into(),
                        wait: WaitReason::Headers,
                        bytes_read: 0,
                        bytes_total: None,
                        speed: 0,
                        contexts: vec![
                            NetworkContext::global(),
                            NetworkContext::domain("example.com"),
                        ],
                        obeys_bandwidth: true,
                    }],
                    ..Snapshot::default()
                },
            )
        })
        .unwrap();
    let monitor = Monitor::bind(&parent, store.clone());
    wait_for_header(|| last_question().is_some());
    let first = last_question().unwrap();
    assert_eq!(
        first.get_question(),
        fixture["questions"][0].as_str().unwrap()
    );
    screenshot(
        &rendered,
        rendered.count() - 1,
        "pending-header-question.png",
        700,
        360,
    );
    first.invoke_answer(true);
    wait_for_header(|| last_question().is_some_and(|w| w.get_question().contains("Authorization")));
    let second = last_question().unwrap();
    assert_eq!(
        second.get_question(),
        fixture["questions"][1].as_str().unwrap()
    );
    second.invoke_later();
    assert!(!second.window().is_visible());
    assert_eq!(
        store
            .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
            .unwrap()[0]
            .approval,
        Approval::Pending
    );
    // Later suppresses this exact job/question; changing the payload asks anew.
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "Authorization",
                Some("new token"),
                None,
                None,
            )
        })
        .unwrap();
    wait_for_header(|| last_question().is_some_and(|w| w.get_question().contains("new token")));
    second.invoke_answer(true);
    let third = last_question().unwrap();
    third.invoke_answer(false);
    wait_for_header(|| last_question().is_none());
    assert_eq!(
        store
            .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
            .unwrap()[0]
            .approval,
        Approval::Denied
    );
    assert_eq!(
        store
            .read(|c| network::headers(c, &NetworkContext::global()))
            .unwrap()
            .iter()
            .find(|h| h.name == "X-Test")
            .unwrap()
            .approval,
        Approval::Approved
    );
    store
        .write(|ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain("example.com"),
                "Authorization",
                None,
                Some(Approval::Pending),
                None,
            )?;
            let mut snapshot = settings::get::<Snapshot>(ctx.conn())?;
            snapshot.at = jiff::Timestamp::now().as_second();
            settings::set(ctx.conn(), &snapshot)
        })
        .unwrap();
    wait_for_header(|| last_question().is_some());
    let abandoned = last_question().unwrap();
    drop(monitor);
    assert!(!abandoned.window().is_visible());
    abandoned.invoke_answer(true);
    assert_eq!(
        store
            .read(|c| network::headers(c, &NetworkContext::domain("example.com")))
            .unwrap()[0]
            .approval,
        Approval::Pending
    );
}

#[test]
fn automatic_header_answers_reach_an_existing_engine_request() {
    use hydrus_gui::network_header_approval::{Monitor, last_question};
    use std::io::{Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let body = String::from_utf8(request).unwrap();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let _rendered = headless::init();
    let parent = hydrus_gui::MainWindow::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let engine = {
        let _guard = runtime.enter();
        std::sync::Arc::new(
            hydrus_net::NetEngine::new(
                store.clone(),
                hydrus_net::NetOptions {
                    obey_bandwidth: false,
                    ..hydrus_net::NetOptions::default()
                },
            )
            .unwrap(),
        )
    };
    store
        .write({
            let address = address.clone();
            move |ctx| {
                network::set_header(
                    ctx.conn(),
                    &NetworkContext::global(),
                    "X-Test",
                    Some("global-value"),
                    Some(Approval::Pending),
                    Some("global reason"),
                )?;
                network::set_header(
                    ctx.conn(),
                    &NetworkContext::domain(address),
                    "Authorization",
                    Some("token"),
                    Some(Approval::Pending),
                    Some("login reason"),
                )
            }
        })
        .unwrap();
    let task = runtime.spawn({
        let engine = engine.clone();
        async move {
            engine
                .fetch(
                    &hydrus_net::Request::get(format!("http://{address}/echo")),
                    &hydrus_net::Job::new(),
                )
                .await
        }
    });
    runtime.block_on(async { tokio::time::sleep(std::time::Duration::from_millis(30)).await });
    assert_eq!(
        engine.runtime_snapshot().jobs[0].wait,
        hydrus_store::network_runtime::WaitReason::Headers
    );
    engine.publish_runtime().unwrap();
    let monitor = Monitor::bind(&parent, store.clone());
    wait_for_header(|| last_question().is_some());
    let first = last_question().unwrap();
    assert!(first.get_question().contains("X-Test: global-value"));
    first.invoke_answer(true);
    wait_for_header(|| {
        last_question().is_some_and(|q| q.get_question().contains("Authorization: token"))
    });
    last_question().unwrap().invoke_answer(false);
    wait_for_header(|| last_question().is_none());
    let response = runtime.block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(8), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    });
    let sent = response.text().to_ascii_lowercase();
    assert!(sent.contains("x-test: global-value"), "{sent}");
    assert!(!sent.contains("authorization:"), "{sent}");
    assert_eq!(
        store
            .read(|c| network::headers(c, &NetworkContext::global()))
            .unwrap()
            .iter()
            .find(|h| h.name == "X-Test")
            .unwrap()
            .reason,
        "global reason"
    );
    monitor.close();
    server.join().unwrap();
}
// leaf: session-create
#[test]
fn a_new_session_can_be_a_web_domain_or_a_hydrus_service_as_the_reference_s_editor_offers() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let key = hydrus_core::ServiceKey::new(vec![62; 16]);
    let service_key = key.clone();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &service_key,
                "session repository",
                &hydrus_store::services::ServiceKind::TagRepository(
                    hydrus_store::services::RepositoryConfig::default(),
                ),
            )?;
            Ok(())
        })
        .unwrap();
    let _rendered = headless::init();
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    // web domain first, with the reference's note; the service choice lists
    // the repositories
    assert_eq!(edit.get_window_title(), "enter new network context");
    assert_eq!(edit.get_context_type(), 0);
    assert_eq!(
        edit.get_type_info(),
        "Network traffic going to or from a web domain (or a subdomain)."
    );
    assert_eq!(edit.get_service_names().row_count(), 1);
    assert_eq!(
        edit.get_service_names().row_data(0).unwrap(),
        "session repository"
    );
    // a hydrus service: the note changes, and the domain is not read
    edit.set_context_type(1);
    edit.invoke_type_changed();
    assert_eq!(
        edit.get_type_info(),
        "Network traffic going to or from a hydrus service."
    );
    edit.set_domain("not a domain!!".into());
    edit.invoke_apply_clicked();
    assert_eq!(edit.get_error(), "");
    let sessions = store.read(network::sessions).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].kind, hydrus_core::network::CONTEXT_HYDRUS);
    assert_eq!(sessions[0].data, key.to_hex());
    assert!(browser.get_show_empty());
    assert_eq!(browser.get_rows().row_count(), 1);
    // a domain still has to be one
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_domain("not a domain!!".into());
    edit.invoke_apply_clicked();
    assert_ne!(edit.get_error(), "");
    assert_eq!(store.read(network::sessions).unwrap().len(), 1);
    edit.set_domain("Example.com".into());
    edit.invoke_apply_clicked();
    assert_eq!(store.read(network::sessions).unwrap().len(), 2);
}
