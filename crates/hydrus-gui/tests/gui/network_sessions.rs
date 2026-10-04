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
    edit.set_domain("stale.com".into());
    edit.invoke_apply_clicked();
    assert!(store.read(network::sessions).unwrap().is_empty());
}
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
    cookies.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    edit.set_name("sid".into());
    edit.set_value("from-gui".into());
    edit.set_domain("127.0.0.1".into());
    edit.set_session_cookie(true);
    edit.set_secure(false);
    edit.invoke_apply_clicked();
    assert!(edit.get_error().is_empty());
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
    assert!(text.contains("x-gui: approved"), "{text}");
    browser.invoke_edit_clicked();
    let cookies = slots.cookies.borrow().as_ref().unwrap().clone_strong();
    cookies.invoke_row_clicked(0, false, false);
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
