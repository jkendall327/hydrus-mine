//! Native parser dialogs use real-store staged writes and protected child ownership.
use hydrus_core::url::{UrlClass, UrlClassSettings, UrlType};
use hydrus_gui::{
    ParserEditWindow, headless,
    parser_editors_window::{self as windows, Slots},
};
use hydrus_gui_model::parser_editors::{new_content, new_page};
use hydrus_parse::{
    Downloaders,
    content::{ContentKind, SubsidiaryPageParser},
    formula::{FormulaKind, ParsingContext},
};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, Model as _};
pub(super) fn until_fetch(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(8),
            "test fetch did not finish"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
/// Loopback-only HTTP boundary; /hold retains the socket until owner cancellation.
#[derive(Debug)]
pub(super) struct TestDocuments {
    pub base: String,
    pub requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl TestDocuments {
    pub fn start() -> Self {
        Self::start_with_image(Vec::new())
    }
    pub(super) fn start_with_image(image: Vec<u8>) -> Self {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let thread = std::thread::spawn({
            let stop = stop.clone();
            let requests = requests.clone();
            move || {
                let mut held = Vec::new();
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let Ok((mut stream, _)) = listener.accept() else {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    };
                    // Windows accepted sockets inherit the listener's nonblocking
                    // mode. Wait for complete headers instead of closing a socket
                    // whose client has not written its request yet.
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    let mut bytes = [0; 8192];
                    let mut count = 0;
                    while count < bytes.len()
                        && !bytes[..count].windows(4).any(|v| v == b"\r\n\r\n")
                    {
                        match stream.read(&mut bytes[count..]) {
                            Ok(0) | Err(_) => break,
                            Ok(read) => count += read,
                        }
                    }
                    if count == 0 {
                        continue;
                    }
                    let request = String::from_utf8_lossy(&bytes[..count]).into_owned();
                    let hold = request.starts_with("GET /hold ");
                    let error = request.starts_with("GET /error ");
                    let image_request = request.starts_with("GET /png ");
                    requests.lock().unwrap().push(request);
                    if hold {
                        held.push(stream);
                        continue;
                    }
                    let (status, body): (&str, &[u8]) = if image_request {
                        ("200 OK", &image)
                    } else if error {
                        ("404 Not Found", b"missing")
                    } else {
                        ("200 OK", b"<p>fetched caf\xe9</p>")
                    };
                    let content_type = if image_request {
                        "image/png"
                    } else {
                        "text/html; charset=iso-8859-1"
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nSet-Cookie: test-document=saved; Path=/\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.write_all(body);
                }
            }
        });
        Self {
            base,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for TestDocuments {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn content_test_panel_fetches_raw_data_and_hands_it_to_its_formula() {
    let server = TestDocuments::start();
    let (_dir, store, slots) = setup();
    headless::init();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    content.set_fetch_url(format!(" {}/document ", server.base).into());
    content.set_post_index("7".into());
    content.set_variables("token=preserved".into());
    content.invoke_fetch_from_url();
    until_fetch(|| !content.get_fetching());
    assert_eq!(content.get_document(), "<p>fetched café</p>");
    assert_eq!(content.get_post_index(), "0");
    assert_eq!(content.get_variables(), "token=preserved");
    content.invoke_action("formula".into());
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(formula.get_document(), content.get_document());
    assert!(
        formula
            .get_context()
            .contains(&format!("url={}/document", server.base))
    );
    assert_eq!(formula.get_examples().row_count(), 2);
    formula.invoke_cancel();
    content.set_fetch_url(format!("{}/error", server.base).into());
    content.invoke_fetch_from_url();
    until_fetch(|| !content.get_fetching());
    assert_eq!(content.get_document(), "fetch failed:\n\n404: missing");
    assert!(
        !server.requests.lock().unwrap()[0]
            .to_lowercase()
            .contains("referer:")
    );
    page.invoke_force_close();
    list.invoke_action("cancel".into());
    assert_eq!(definitions(&store), original);
}
#[test]
fn fetched_page_examples_use_actual_network_headers_cookies_context_and_child_converter() {
    use hydrus_core::{
        network::NetworkContext,
        url::strings::{Conversion, StringConverter},
    };
    use hydrus_store::network::{self, Approval};
    let server = TestDocuments::start();
    let (_dir, store, slots) = setup();
    let rendered = headless::init();
    let mut saved = definitions(&store);
    saved.parsers[0].example_urls = vec![format!("{}/a b", server.base)];
    saved.parsers[0].converter = StringConverter {
        conversions: vec![Conversion::Append("<!-- converted -->".into())],
        ..StringConverter::default()
    };
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &saved)?;
            network::set_header(
                ctx.conn(),
                &NetworkContext::global(),
                "X-Test-Document",
                Some("approved"),
                Some(Approval::Approved),
                Some("synthetic test document"),
            )
        })
        .unwrap();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.set_test_url("".into());
    page.invoke_fetch();
    assert_eq!(page.get_test_url(), format!("{}/a b", server.base));
    assert!(!page.get_fetching());
    assert!(server.requests.lock().unwrap().is_empty());
    page.set_document("previous paste".into());
    page.set_post_index("12".into());
    page.set_variables("token=preserved".into());
    page.set_referral_url("https://referral.example/from".into());
    page.invoke_fetch();
    assert!(page.get_fetching() && page.get_child_open());
    page.invoke_action("apply".into());
    assert!(slots.page.borrow().is_some());
    until_fetch(|| !page.get_fetching());
    assert_eq!(page.get_document(), "<p>fetched café</p>");
    assert_eq!(page.get_test_url(), format!("{}/a%20b", server.base));
    assert_eq!(page.get_post_index(), "0");
    assert_eq!(page.get_variables(), "token=preserved");
    assert_eq!(page.get_examples().row_count(), 2);
    screenshot(&rendered, 1, "parser-fetch.png", &page);
    let request = server.requests.lock().unwrap()[0].to_lowercase();
    assert!(request.starts_with("get /a%20b "));
    assert!(request.contains("referer: https://referral.example/from"));
    assert!(request.contains("x-test-document: approved"));
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    assert_eq!(
        content.get_document(),
        "<p>fetched café</p><!-- converted -->"
    );
    assert_eq!(content.get_test_url(), page.get_test_url());
    assert_eq!(content.get_examples().row_count(), 2);
    content.invoke_action("cancel".into());
    page.set_example(0);
    page.invoke_example_chosen();
    assert_eq!(page.get_document(), "previous paste");
    page.set_example(1);
    page.invoke_example_chosen();
    assert_eq!(page.get_document(), "<p>fetched café</p>");
    page.set_test_url(format!("{}/error", server.base).into());
    page.invoke_fetch();
    until_fetch(|| !page.get_fetching());
    assert_eq!(page.get_document(), "fetch failed: 404: missing\n\nmissing");
    assert!(server.requests.lock().unwrap()[1].contains("test-document=saved"));
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let usage = store
        .read(|conn| hydrus_store::bandwidth::usage(conn, now))
        .unwrap()
        .into_iter()
        .find(|(c, _)| *c == NetworkContext::global())
        .unwrap()
        .1;
    assert_eq!(
        usage.all_usage(hydrus_core::bandwidth::BandwidthType::Requests),
        2
    );
    page.set_test_url("invalid".into());
    page.invoke_fetch();
    assert!(!page.get_error().is_empty());
    assert!(!page.get_fetching());
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    page.set_test_url(format!("{}/hold", server.base).into());
    page.invoke_fetch();
    until_fetch(|| server.requests.lock().unwrap().len() == 3);
    page.invoke_cancel_fetch();
    until_fetch(|| !page.get_fetching());
    assert_eq!(page.get_document(), "fetch cancelled");
    page.set_test_url(format!("{}/hold", server.base).into());
    page.invoke_fetch();
    until_fetch(|| server.requests.lock().unwrap().len() == 4);
    page.invoke_force_close();
    list.invoke_action("edit".into());
    let reopened = child(&slots.page);
    slint::platform::update_timers_and_animations();
    assert_eq!(reopened.get_document(), "");
    assert!(!reopened.get_fetching());
    assert_eq!(definitions(&store), original);
    reopened.invoke_force_close();
    list.invoke_action("cancel".into());
}
#[test]
fn page_fetch_error_menu_retains_completed_failure_and_clears_at_next_request() {
    use std::{cell::RefCell, rc::Rc};
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/parser_fetch_errors.json"
    ))
    .unwrap();
    assert_eq!(
        reference["menus"][0],
        serde_json::json!(["show error", "copy error"])
    );
    assert_eq!(reference["states"][2]["has_job"], false);
    assert_eq!(reference["states"][2]["has_error"], true);
    assert_eq!(reference["states"][3]["has_error"], false);
    assert_eq!(reference["show_copy_identical"], true);
    let server = TestDocuments::start();
    let (_dir, store, slots) = setup();
    let rendered = headless::init();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    let copies = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| copies.borrow_mut().push(clip.clone())
    });
    page.set_test_url(format!("{}/error", server.base).into());
    page.invoke_fetch();
    assert!(!page.get_fetch_has_error());
    until_fetch(|| !page.get_fetching());
    assert!(page.get_fetch_has_error());
    page.invoke_fetch_error_action(8);
    let error = hydrus_gui::network_job_control::last_error().unwrap();
    assert_eq!(error.get_error_text(), "404: missing\n\nmissing");
    page.invoke_fetch_error_action(9);
    assert_eq!(
        copies.borrow().last().unwrap(),
        &hydrus_gui::Clip::Text(error.get_error_text().to_string())
    );
    let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 660, 370);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    headless::save_png(
        &std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("parser-fetch-error.png"),
        &pixels,
        660,
        370,
    )
    .unwrap();
    error.invoke_close_clicked();
    page.set_test_url("".into());
    page.invoke_fetch();
    assert!(page.get_fetch_has_error());
    page.set_test_url("invalid".into());
    page.invoke_fetch();
    assert!(page.get_fetch_has_error());
    // A raw test-panel fetch has no NetworkJobControl owner in the reference.
    page.set_fetch_url(format!("{}/document", server.base).into());
    page.invoke_fetch_from_url();
    until_fetch(|| !page.get_fetching());
    assert!(page.get_fetch_has_error());
    page.set_test_url(format!("{}/document", server.base).into());
    page.invoke_fetch();
    assert!(!page.get_fetch_has_error());
    until_fetch(|| !page.get_fetching());
    assert!(!page.get_fetch_has_error());
    let count = copies.borrow().len();
    page.invoke_fetch_error_action(9);
    assert_eq!(copies.borrow().len(), count);
    page.set_test_url(format!("{}/error", server.base).into());
    page.invoke_fetch();
    until_fetch(|| !page.get_fetching());
    page.invoke_fetch_error_action(8);
    let retained = hydrus_gui::network_job_control::last_error().unwrap();
    page.invoke_force_close();
    assert!(!retained.window().is_visible());
    assert!(slots.page.borrow().is_none());
    page.invoke_fetch_error_action(8);
    page.invoke_fetch_error_action(9);
    assert!(!retained.window().is_visible());
    assert_eq!(copies.borrow().len(), count);
    list.invoke_action("cancel".into());
    assert_eq!(definitions(&store), original);
}
fn child(slot: &std::rc::Rc<std::cell::RefCell<Option<ParserEditWindow>>>) -> ParserEditWindow {
    slot.borrow().as_ref().unwrap().clone_strong()
}
fn setup() -> (tempfile::TempDir, std::sync::Arc<Store>, Slots) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut page = new_page();
    page.key = "page".into();
    page.name = "test page".into();
    page.example_urls = vec!["https://example.com/post/1".into()];
    let mut content = new_content();
    content.formula.kind = FormulaKind::Static {
        text: "first\n\nsecond".into(),
        count: 1,
    };
    page.content_parsers.push(content.clone());
    let mut subsidiary = new_page();
    subsidiary.name = "preserved subsidiary".into();
    page.subsidiary.push(SubsidiaryPageParser {
        formula: content.formula,
        sort_by_source_time: true,
        parser: subsidiary,
    });
    let definitions = Downloaders {
        parsers: vec![page],
        ..Downloaders::default()
    };
    let classes = UrlClassSettings {
        url_classes: vec![UrlClass {
            name: "test post".into(),
            key: vec![1],
            url_type: UrlType::Post,
            ..UrlClass::default()
        }],
        parser_keys: vec!["page".into()],
        ..UrlClassSettings::default()
    };
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &definitions)?;
            settings::set(ctx.conn(), &classes)
        })
        .unwrap();
    (dir, store, Slots::default())
}
fn definitions(store: &Store) -> Downloaders {
    store.read(settings::get).unwrap()
}
#[test]
fn reusable_content_kinds_remap_restricted_choices_and_preserve_login_context() {
    use hydrus_gui_model::formula_editors::FormulaTestData;
    use std::{cell::RefCell, rc::Rc};
    let (_dir, store, slots) = setup();
    headless::init();
    let mut parser = new_content();
    parser.kind = ContentKind::Variable {
        name: "token".into(),
    };
    parser.formula.kind = FormulaKind::ContextVariable {
        variable: "csrf".into(),
    };
    let test = FormulaTestData {
        context: [("csrf".into(), "synthetic login token".into())].into(),
        ..FormulaTestData::default()
    };
    let accepted = Rc::new(RefCell::new(None));
    let open = || {
        windows::open_content(
            &store,
            &parser,
            test.clone(),
            &slots,
            &[7, 8],
            Rc::new({
                let accepted = accepted.clone();
                move |value| {
                    *accepted.borrow_mut() = Some(value);
                    Ok(())
                }
            }),
        )
        .unwrap()
    };
    let editor = open();
    let options = editor.get_fields().row_data(1).unwrap().options;
    assert_eq!(
        (0..options.row_count())
            .map(|i| options.row_data(i).unwrap().to_string())
            .collect::<Vec<_>>(),
        ["temporary variable", "veto"]
    );
    editor.invoke_action("test".into());
    assert!(editor.get_preview().contains("synthetic login token"));
    editor.invoke_choice_edited(1, 1);
    editor.invoke_action("apply".into());
    assert!(matches!(
        accepted.borrow().as_ref().unwrap().kind,
        ContentKind::Veto { .. }
    ));
    let editor = open();
    editor.invoke_text_edited(0, "discarded".into());
    editor.invoke_action("cancel".into());
    editor.invoke_answered(true);
    assert!(slots.content.borrow().is_none());
    assert_eq!(parser.name, "new content parser");
    assert!(matches!(
        accepted.borrow().as_ref().unwrap().kind,
        ContentKind::Veto { .. }
    ));
}
fn screenshot(rendered: &headless::Windows, index: usize, name: &str, w: &ParserEditWindow) {
    let window = rendered.get(index).unwrap();
    let pixels = headless::render(&window, 1000, 780);
    assert!(w.get_footer_y() >= 0. && w.get_footer_y() < 780.);
    assert!(pixels.chunks_exact(4).any(|pixel| pixel != &pixels[..4]));
    if let Ok(dir) = std::env::var("HYDRUS_PARSER_SCREENSHOTS") {
        headless::save_png(&std::path::Path::new(&dir).join(name), &pixels, 1000, 780).unwrap();
    }
}
#[test]
fn native_page_content_apply_roundtrip_preserves_subsidiary_and_formula() {
    let (dir, store, slots) = setup();
    let rendered = headless::init();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_text_edited(0, "edited page".into());
    page.invoke_action("edit-content".into());
    assert!(slots.content.borrow().is_none());
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    assert!(page.get_child_open());
    list.invoke_action("delete".into());
    page.invoke_action("delete-content".into());
    page.invoke_action("apply".into());
    content.invoke_choice_edited(1, 2);
    content.invoke_text_edited(5, "".into());
    content.invoke_action("test".into());
    assert!(content.get_preview().contains("first\n\nsecond"));
    content.invoke_text_edited(0, "note parser".into());
    screenshot(&rendered, 2, "content-note.png", &content);
    content.invoke_action("apply".into());
    assert!(slots.content.borrow().is_none());
    assert!(!page.get_child_open());
    assert_eq!(definitions(&store), original);
    screenshot(&rendered, 1, "page-parser.png", &page);
    page.invoke_action("apply".into());
    assert!(slots.page.borrow().is_none());
    assert!(!list.get_child_open());
    list.invoke_action("apply".into());
    let saved = definitions(&store);
    assert_eq!(saved.parsers[0].name, "edited page");
    assert_eq!(
        saved.parsers[0].content_parsers[0].kind,
        ContentKind::Note {
            name: "note".into()
        }
    );
    assert_eq!(saved.parsers[0].subsidiary, original.parsers[0].subsidiary);
    assert_eq!(
        saved.parsers[0].content_parsers[0].formula,
        original.parsers[0].content_parsers[0].formula
    );
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(definitions(&reopened), saved);
}
#[test]
fn canceled_stale_nested_windows_and_invalid_fields_never_overwrite() {
    let (_dir, store, slots) = setup();
    headless::init();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    content.invoke_text_edited(3, "invalid".into());
    content.invoke_text_edited(0, "name cannot clear invalid priority".into());
    content.invoke_action("apply".into());
    assert!(slots.content.borrow().is_some());
    assert!(!content.get_error().is_empty());
    content.invoke_text_edited(3, "75".into());
    content.invoke_action("formula".into());
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    content.invoke_choice_edited(1, 2);
    content.invoke_action("apply".into());
    assert!(slots.content.borrow().is_some());
    page.invoke_force_close();
    assert!(slots.page.borrow().is_none());
    assert!(slots.content.borrow().is_none());
    assert!(slots.formula.formula.borrow().is_none());
    formula.invoke_apply();
    content.invoke_action("apply".into());
    page.invoke_action("apply".into());
    assert_eq!(definitions(&store), original);
    list.invoke_action("cancel".into());
    list.invoke_action("apply".into());
    assert_eq!(definitions(&store), original);
}
#[test]
fn url_class_links_are_staged_cancel_safe_and_refresh_capability() {
    let (_dir, store, slots) = setup();
    headless::init();
    let links = windows::open(&store, &slots, true).unwrap();
    assert_eq!(links.get_rows().row_count(), 1);
    links.invoke_row_clicked(0, false, false);
    links.set_chosen_parser(0);
    links.invoke_action("link".into());
    assert!(
        store
            .snapshot()
            .url_classes
            .settings()
            .parser_links
            .is_empty()
    );
    links.invoke_action("cancel".into());
    assert!(
        store
            .snapshot()
            .url_classes
            .settings()
            .parser_links
            .is_empty()
    );
    let links = windows::open(&store, &slots, true).unwrap();
    links.invoke_row_clicked(0, false, false);
    links.set_chosen_parser(0);
    links.invoke_action("link".into());
    links.invoke_action("apply".into());
    assert_eq!(
        store.snapshot().url_classes.settings().parser_links,
        vec![("01".into(), Some("page".into()))]
    );
    let links = windows::open(&store, &slots, true).unwrap();
    links.invoke_row_clicked(0, false, false);
    links.invoke_action("clear".into());
    links.invoke_answered(true);
    links.invoke_action("apply".into());
    assert_eq!(
        store.snapshot().url_classes.settings().parser_links,
        vec![("01".into(), None)]
    );
}

#[test]
fn sorted_parser_rows_and_sparse_link_rows_select_the_visible_definition() {
    let (_dir, store, slots) = setup();
    headless::init();
    store
        .write_and_refresh(|ctx| {
            let conn = ctx.conn();
            let mut values: Downloaders = settings::get(conn)?;
            values.parsers[0].name = "z last".into();
            let mut first = new_page();
            first.name = "a first".into();
            first.key = "first".into();
            values.parsers.push(first);
            let mut classes: UrlClassSettings = settings::get(conn)?;
            classes.url_classes.insert(
                0,
                UrlClass {
                    name: "file without parser".into(),
                    key: vec![2],
                    url_type: UrlType::File,
                    ..UrlClass::default()
                },
            );
            classes.parser_keys.push("first".into());
            settings::set(conn, &values)?;
            settings::set(conn, &classes)
        })
        .unwrap();
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    assert_eq!(page.get_fields().row_data(0).unwrap().text, "a first");
    page.invoke_action("cancel".into());
    list.invoke_sort(0, false);
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    assert_eq!(page.get_fields().row_data(0).unwrap().text, "z last");
    page.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    let links = windows::open(&store, &slots, true).unwrap();
    assert_eq!(links.get_rows().row_count(), 1);
    links.invoke_row_clicked(0, false, false);
    links.set_chosen_parser(1);
    links.invoke_action("link".into());
    links.invoke_action("apply".into());
    assert_eq!(
        store.snapshot().url_classes.settings().parser_links,
        vec![("01".into(), Some("first".into()))]
    );
}

#[test]
fn namespace_control_is_disabled_without_losing_its_saved_text() {
    let (_dir, store, slots) = setup();
    headless::init();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    content.invoke_choice_edited(1, 1);
    content.invoke_text_edited(5, "artist".into());
    content.invoke_toggled(4, true);
    let field = content.get_fields().row_data(3).unwrap();
    assert_eq!(field.id, 5);
    assert!(!field.enabled);
    assert_eq!(field.text, "artist");
    content.invoke_text_edited(5, "ignored disabled edit".into());
    content.invoke_toggled(4, false);
    let field = content.get_fields().row_data(3).unwrap();
    assert!(field.enabled);
    assert_eq!(field.text, "artist");
    content.invoke_action("cancel".into());
    assert!(!content.get_question().is_empty());
    content.invoke_answered(true);
    assert!(slots.content.borrow().is_none());
    page.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    assert_eq!(definitions(&store), original);
}

#[test]
fn deletion_confirmation_is_modal_and_removes_only_its_snapshotted_keys() {
    let (_dir, store, slots) = setup();
    headless::init();
    store
        .write(|ctx| {
            let mut values: Downloaders = settings::get(ctx.conn())?;
            let mut other = new_page();
            other.key = "other".into();
            other.name = "z other".into();
            values.parsers.push(other);
            settings::set(ctx.conn(), &values)
        })
        .unwrap();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("delete".into());
    assert!(!list.get_question().is_empty());
    list.invoke_row_clicked(1, false, false);
    list.invoke_sort(0, false);
    list.invoke_action("edit".into());
    list.invoke_action("duplicate".into());
    list.invoke_action("apply".into());
    assert!(slots.page.borrow().is_none());
    assert_eq!(definitions(&store), original);
    assert!(list.get_rows().row_data(0).unwrap().selected);
    assert!(!list.get_rows().row_data(1).unwrap().selected);
    list.invoke_answered(false);
    assert!(list.get_question().is_empty());
    assert_eq!(list.get_rows().row_count(), 2);
    list.invoke_action("delete".into());
    list.invoke_row_clicked(1, false, false);
    list.invoke_answered(true);
    list.invoke_answered(true);
    assert_eq!(list.get_rows().row_count(), 1);
    list.invoke_action("apply".into());
    let saved = definitions(&store);
    assert_eq!(saved.parsers.len(), 1);
    assert_eq!(saved.parsers[0].key, "other");
}

#[test]
fn link_apply_rejects_a_class_changed_to_a_file_in_another_editor() {
    let (_dir, store, slots) = setup();
    headless::init();
    let links = windows::open(&store, &slots, true).unwrap();
    links.invoke_row_clicked(0, false, false);
    links.set_chosen_parser(0);
    links.invoke_action("link".into());
    store
        .write_and_refresh(|ctx| {
            let mut classes: UrlClassSettings = settings::get(ctx.conn())?;
            classes.url_classes[0].url_type = UrlType::File;
            settings::set(ctx.conn(), &classes)
        })
        .unwrap();
    links.invoke_action("apply".into());
    assert!(!links.get_error().is_empty());
    assert!(slots.links.borrow().is_some());
    let classes: UrlClassSettings = store.read(settings::get).unwrap();
    assert_eq!(classes.url_classes[0].url_type, UrlType::File);
    assert!(classes.parser_links.is_empty());
    links.invoke_action("cancel".into());
}

#[test]
fn recursive_formula_edits_reach_saved_page_parser_and_consumer() {
    let (dir, store, slots) = setup();
    let _windows = headless::init();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    content.invoke_choice_edited(1, 1);
    content.invoke_text_edited(5, "series".into());
    content.invoke_action("formula".into());
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    formula.set_kind(2);
    formula.invoke_type_chosen();
    formula.invoke_edit_child(false);
    let first = {
        let children = slots.formula.child.borrow();
        let formula = children.as_ref().unwrap().formula.borrow();
        formula.as_ref().unwrap().clone_strong()
    };
    first.set_kind(5);
    first.invoke_type_chosen();
    first.set_static_text("{\"posts\":[\"edited tag\"]}".into());
    first.invoke_apply();
    formula.invoke_edit_child(true);
    let second = {
        let children = slots.formula.child.borrow();
        let formula = children.as_ref().unwrap().formula.borrow();
        formula.as_ref().unwrap().clone_strong()
    };
    assert_eq!(second.get_document(), "{\"posts\":[\"edited tag\"]}");
    second.invoke_add();
    let rule = {
        let children = slots.formula.child.borrow();
        let rule = children.as_ref().unwrap().rule.borrow();
        rule.as_ref().unwrap().clone_strong()
    };
    rule.set_rule_type(1);
    rule.invoke_changed();
    rule.invoke_apply();
    second.invoke_apply();
    formula.invoke_apply();
    content.invoke_action("test".into());
    assert!(
        content.get_preview().contains("tags: edited tag"),
        "{}",
        content.get_preview()
    );
    assert_eq!(definitions(&store), original);
    content.invoke_action("apply".into());
    page.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = definitions(&store);
    assert!(matches!(
        saved.parsers[0].content_parsers[0].formula.kind,
        FormulaKind::Nested { .. }
    ));
    assert_eq!(saved.parsers[0].subsidiary, original.parsers[0].subsidiary);
    let consumer = &saved.parsers[0].content_parsers[0];
    let context = ParsingContext::default();
    let parsed = consumer.parse(&context, "unused").unwrap();
    assert_eq!(
        parsed.tags().into_iter().collect::<Vec<_>>(),
        ["series:edited tag"]
    );
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(definitions(&reopened), saved);
}

#[test]
fn subsidiary_separator_uses_converted_data_preserves_newlines_and_saves() {
    use hydrus_core::url::strings::{Conversion, StringConverter};
    use hydrus_gui_model::formula_editors::new_formula;
    use hydrus_parse::formula::{HtmlContent, HtmlWalk, TagSearch};
    let (dir, store, slots) = setup();
    let rendered = headless::init();
    let cases: Vec<serde_json::Value> = serde_json::from_value(hydrus_testkit::fixture_json(
        "recursive_formula_editors.json",
    ))
    .unwrap();
    let before = cases
        .iter()
        .find(|c| c["case"] == "subsidiary_separator")
        .unwrap();
    let after = cases
        .iter()
        .find(|c| c["case"] == "subsidiary_separator_edit")
        .unwrap();
    let mut initial = definitions(&store);
    let page = &mut initial.parsers[0];
    page.content_parsers.clear();
    page.converter = StringConverter {
        conversions: vec![Conversion::Append("<!-- converted -->".into())],
        example: String::new(),
    };
    let subsidiary = &mut page.subsidiary[0];
    subsidiary.parser.name = "preserved child".into();
    let mut note = new_content();
    note.name = "note content".into();
    note.kind = ContentKind::Note {
        name: "note".into(),
    };
    note.formula = new_formula(false);
    let FormulaKind::Html { rules, content } = &mut note.formula.kind else {
        panic!()
    };
    rules[0].tag_name = Some("p".into());
    *content = HtmlContent::Text;
    subsidiary.parser.content_parsers = vec![note];
    subsidiary.formula = new_formula(false);
    let FormulaKind::Html { rules, content } = &mut subsidiary.formula.kind else {
        panic!()
    };
    rules[0].tag_name = Some("div".into());
    rules[0].walk = HtmlWalk::Descendants(TagSearch {
        attrs: vec![("class".into(), "post".into())],
        index: None,
    });
    *content = HtmlContent::Html;
    let original = initial.clone();
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &initial))
        .unwrap();
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.set_document(before["raw"].as_str().unwrap().into());
    assert_eq!(page.get_subsidiaries().row_count(), 1);
    page.invoke_subsidiary_clicked(0, false, false);
    assert!(page.get_subsidiary_sorted());
    page.invoke_action("separator".into());
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        formula.get_document(),
        before["converted"].as_str().unwrap()
    );
    assert!(formula.get_newline_note().contains("not collapsed"));
    assert_eq!(
        serde_json::json!(formula_results(&formula)),
        before["before"]
    );
    formula.set_kind(5);
    formula.invoke_type_chosen();
    formula.set_static_text("discarded".into());
    formula.invoke_cancel();
    assert_eq!(definitions(&store), original);
    page.invoke_action("separator".into());
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(formula.get_kind(), 0);
    formula.set_kind(5);
    formula.invoke_type_chosen();
    formula.set_static_text(after["text"].as_str().unwrap().into());
    formula.invoke_changed();
    assert_eq!(
        serde_json::json!(formula_results(&formula)),
        after["results"]
    );
    page.invoke_subsidiary_sort_changed(false);
    page.invoke_action("apply".into());
    assert!(slots.page.borrow().is_some());
    assert_eq!(definitions(&store), original);
    formula.invoke_apply();
    page.invoke_subsidiary_sort_changed(after["sort"].as_bool().unwrap());
    assert!(!page.get_subsidiary_sorted());
    screenshot(&rendered, 1, "subsidiary-separator-owner.png", &page);
    page.invoke_action("test".into());
    assert!(
        page.get_preview().contains("edited\n\nnote"),
        "{}",
        page.get_preview()
    );
    page.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = definitions(&store);
    assert_eq!(
        saved.parsers[0].subsidiary[0].parser,
        original.parsers[0].subsidiary[0].parser
    );
    assert!(!saved.parsers[0].subsidiary[0].sort_by_source_time);
    let mut context = ParsingContext::default();
    let posts = saved.parsers[0]
        .parse(&mut context, before["raw"].as_str().unwrap())
        .unwrap();
    let texts = posts
        .iter()
        .map(|post| {
            post.contents
                .iter()
                .map(|content| content.text.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(serde_json::json!(texts), after["post_texts"]);
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(definitions(&reopened), saved);
}
fn formula_results(formula: &hydrus_gui::FormulaWindow) -> Vec<String> {
    (0..formula.get_results().row_count())
        .map(|i| {
            formula
                .get_results()
                .row_data(i)
                .unwrap()
                .cells
                .row_data(0)
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn multiple_examples_restore_sources_and_propagate_converted_selected_child_data() {
    use hydrus_core::url::strings::{Conversion, StringConverter};
    use hydrus_gui_model::formula_editors::new_formula;
    use hydrus_parse::formula::HtmlContent;
    let (_dir, store, slots) = setup();
    let _rendered = headless::init();
    let cases: Vec<serde_json::Value> =
        serde_json::from_value(hydrus_testkit::fixture_json("parser_test_data.json")).unwrap();
    let first = cases
        .iter()
        .find(|c| c["case"] == "converted_example" && c["sequence"] == 0)
        .unwrap();
    let second = cases
        .iter()
        .find(|c| c["case"] == "converted_example" && c["sequence"] == 1)
        .unwrap();
    let mut initial = definitions(&store);
    initial.parsers[0].converter = StringConverter {
        conversions: vec![Conversion::Append("<!-- converted -->".into())],
        example: String::new(),
    };
    let content = &mut initial.parsers[0].content_parsers[0];
    content.kind = ContentKind::Note {
        name: "note".into(),
    };
    content.formula = new_formula(false);
    let FormulaKind::Html { rules, content } = &mut content.formula.kind else {
        panic!()
    };
    rules[0].tag_name = Some("p".into());
    *content = HtmlContent::Text;
    let original = initial.clone();
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &initial))
        .unwrap();
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.set_document(first["raw"].as_str().unwrap().into());
    page.set_test_url(first["context"]["url"].as_str().unwrap().into());
    page.set_variables("token=preserved".into());
    page.invoke_action("add-example".into());
    assert_eq!(page.get_examples().row_count(), 2);
    assert_eq!(page.get_document(), "");
    page.set_document(second["raw"].as_str().unwrap().into());
    page.set_test_url("https://test-docs.example/second".into());
    page.set_example(0);
    page.invoke_example_chosen();
    assert_eq!(page.get_document(), first["raw"].as_str().unwrap());
    assert_eq!(
        page.get_test_url(),
        first["context"]["url"].as_str().unwrap()
    );
    page.set_example(1);
    page.invoke_example_chosen();
    assert_eq!(page.get_document(), second["raw"].as_str().unwrap());
    assert_eq!(page.get_test_url(), "https://test-docs.example/second");
    page.invoke_row_clicked(0, false, false);
    page.invoke_action("edit-content".into());
    let content = child(&slots.content);
    assert_eq!(
        content.get_document(),
        second["child_texts"][0].as_str().unwrap()
    );
    assert_eq!(content.get_examples().row_count(), 2);
    assert_eq!(content.get_test_url(), "https://test-docs.example/second");
    content.invoke_action("formula".into());
    let formula = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    assert_eq!(
        formula.get_document(),
        second["child_texts"][0].as_str().unwrap()
    );
    assert_eq!(formula_results(&formula), ["second"]);
    assert_eq!(formula.get_examples().row_count(), 2);
    assert!(formula.get_context().contains("token=preserved"));
    formula.set_example(1);
    formula.invoke_example_chosen();
    assert_eq!(
        formula.get_document(),
        first["child_texts"][0].as_str().unwrap()
    );
    assert_eq!(formula_results(&formula), ["first"]);
    assert!(
        formula
            .get_context()
            .contains(first["context"]["url"].as_str().unwrap())
    );
    page.set_example(0);
    page.invoke_example_chosen();
    assert_eq!(page.get_example(), 1);
    formula.invoke_cancel();
    content.invoke_action("cancel".into());
    page.invoke_action("remove-example".into());
    assert_eq!(page.get_examples().row_count(), 1);
    assert_eq!(page.get_document(), first["raw"].as_str().unwrap());
    page.invoke_action("cancel".into());
    list.invoke_action("cancel".into());
    assert_eq!(definitions(&store), original);
}

fn recursive_child(slots: &Slots) -> (Slots, ParserEditWindow) {
    let owned = (**slots.child.borrow().as_ref().unwrap()).clone();
    let window = child(&owned.page);
    (owned, window)
}

#[test]
fn recursive_subsidiary_creation_is_staged_and_owner_cancels_descendants() {
    let rendered = headless::init();
    let (_dir, store, slots) = setup();
    let before = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    let raw = "<div class=\"thumb\"><p>first\n\nnote</p></div><div class=\"thumb\"><p>second note</p></div>";
    page.set_document(raw.into());
    page.set_test_url("https://children.example/post".into());
    page.set_variables("token=preserved".into());
    page.invoke_action("add-subsidiary".into());
    let (owned, subsidiary) = recursive_child(&slots);
    assert!(subsidiary.get_subsidiary());
    assert!(!subsidiary.get_own_sorted());
    assert_eq!(subsidiary.get_document(), raw);
    screenshot(&rendered, 2, "recursive-subsidiary.png", &subsidiary);
    subsidiary.invoke_text_edited(0, "new child".into());
    subsidiary.invoke_action("add-content".into());
    let content = child(&owned.content);
    assert_eq!(content.get_examples().row_count(), 2);
    assert_eq!(
        content.get_document(),
        "<div class=\"thumb\"><p>first\n\nnote</p></div>"
    );
    assert_eq!(content.get_variables(), "token=preserved");
    content.invoke_choice_edited(1, 2);
    content.invoke_action("formula".into());
    let formula = owned
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    formula.set_kind(5);
    formula.invoke_type_chosen();
    formula.set_static_text("child\n\nnote".into());
    formula.invoke_changed();
    formula.invoke_apply();
    content.invoke_action("apply".into());
    subsidiary.invoke_action("apply".into());
    assert_eq!(page.get_subsidiaries().row_count(), 2);
    assert_eq!(definitions(&store), before);
    page.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = definitions(&store);
    let added = saved.parsers[0]
        .subsidiary
        .iter()
        .find(|s| s.parser.name == "new child")
        .unwrap();
    assert_eq!(added.parser.content_parsers.len(), 1);
    assert!(matches!(
        added.parser.content_parsers[0].kind,
        ContentKind::Note { .. }
    ));
    let parsed = saved.parsers[0]
        .parse(&mut ParsingContext::new(), raw)
        .unwrap();
    assert_eq!(parsed.len(), 2);
    assert!(
        parsed
            .iter()
            .all(|p| p.contents.iter().any(|c| c.text == "child\n\nnote"))
    );
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.set_document(raw.into());
    page.invoke_action("add-subsidiary".into());
    let (owned, subsidiary) = recursive_child(&slots);
    subsidiary.invoke_action("add-subsidiary".into());
    let (grand_slots, grandchild) = recursive_child(&owned);
    assert!(page.get_child_open());
    page.invoke_action("delete-subsidiary".into());
    assert_eq!(page.get_subsidiaries().row_count(), 2);
    page.invoke_force_close();
    assert!(slots.child.borrow().is_none());
    assert!(owned.page.borrow().is_none());
    assert!(grand_slots.page.borrow().is_none());
    grandchild.invoke_action("apply".into());
    subsidiary.invoke_action("apply".into());
    list.invoke_action("cancel".into());
    assert_eq!(definitions(&store), saved);
}

#[test]
fn subsidiary_edits_preserve_nested_page_identity_and_cancel_metadata_without_prompt() {
    headless::init();
    let (_dir, store, slots) = setup();
    let cases = hydrus_testkit::fixture_json("parser_children.json");
    let recorded = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["action"] == "add_nested")
        .unwrap();
    let object = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
        &recorded["tuple"].to_string(),
    )
    .unwrap();
    let imported = hydrus_legacy::objects::parsers::page_parser(&object).unwrap();
    store
        .write_and_refresh(move |ctx| {
            let mut definitions: Downloaders = settings::get(ctx.conn())?;
            definitions.parsers[0] = imported;
            settings::set(ctx.conn(), &definitions)
        })
        .unwrap();
    let before = definitions(&store);
    assert!(
        before.parsers[0].subsidiary[0]
            .parser
            .reference_auxiliary
            .is_some()
    );
    assert_eq!(before.parsers[0].subsidiary[0].parser.subsidiary.len(), 1);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_subsidiary_clicked(0, false, false);
    page.invoke_action("edit-subsidiary".into());
    let (_, subsidiary) = recursive_child(&slots);
    subsidiary.invoke_own_sort_changed(true);
    subsidiary.invoke_action("cancel".into());
    assert!(slots.child.borrow().is_none());
    assert_eq!(subsidiary.get_question(), "");
    page.invoke_action("edit-subsidiary".into());
    let (_, subsidiary) = recursive_child(&slots);
    subsidiary.invoke_text_edited(0, "edited subsidiary".into());
    subsidiary.invoke_own_sort_changed(false);
    subsidiary.invoke_action("apply".into());
    page.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let saved = definitions(&store);
    let old = &before.parsers[0].subsidiary[0];
    let new = &saved.parsers[0].subsidiary[0];
    assert_eq!(new.parser.key, old.parser.key);
    assert_eq!(
        new.parser.reference_auxiliary,
        old.parser.reference_auxiliary
    );
    assert_eq!(new.formula, old.formula);
    assert_eq!(new.parser.subsidiary, old.parser.subsidiary);
    assert_eq!(new.parser.name, "edited subsidiary");
    assert!(!new.sort_by_source_time);
}

#[test]
fn subsidiary_queue_exchange_is_staged_preserves_wrappers_and_reaches_saved_parser() {
    use hydrus_downloader_exchange::subsidiaries as exchange;
    use std::{cell::RefCell, rc::Rc};
    let reference: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../oracle/fixtures/subsidiary_exchange.json"
    ))
    .unwrap();
    let (dir, store, slots) = setup();
    let rendered = headless::init();
    let mut saved = definitions(&store);
    saved.parsers[0].content_parsers.clear();
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &saved))
        .unwrap();
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_action("import-subsidiary".into());
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text(reference["bundle"].to_string().into());
    import.invoke_action("review".into());
    assert!(import.get_ready());
    page.invoke_action("apply".into());
    assert!(slots.page.borrow().is_some());
    import.invoke_action("accept".into());
    assert_eq!(page.get_subsidiaries().row_count(), 2);
    assert!(page.get_subsidiaries().iter().all(|row| row.selected));
    assert!(!page.get_subsidiary_selected());
    assert_eq!(definitions(&store), original);
    page.invoke_action("export-subsidiary".into());
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(export.get_text().as_str()).unwrap(),
        reference["bundle"]
    );
    let copies = Rc::new(RefCell::new(Vec::new()));
    hydrus_gui::set_clipper({
        let copies = copies.clone();
        move |clip| copies.borrow_mut().push(clip.clone())
    });
    export.invoke_action("copy".into());
    assert_eq!(
        copies.borrow()[0],
        hydrus_gui::Clip::Text(export.get_text().to_string())
    );
    let path = dir.path().join("subsidiaries.png");
    export.set_path(path.to_string_lossy().as_ref().into());
    export.invoke_action("save".into());
    assert_eq!(
        exchange::decode_png(&std::fs::read(path).unwrap()).unwrap(),
        exchange::decode_text(&reference["bundle"].to_string()).unwrap()
    );
    export.invoke_action("cancel".into());
    page.invoke_action("duplicate-subsidiary".into());
    assert_eq!(page.get_subsidiaries().row_count(), 4);
    page.invoke_action("delete-subsidiary".into());
    assert_eq!(
        page.get_question(),
        reference["deletes"][0]["question"].as_str().unwrap()
    );
    page.invoke_answered(false);
    assert_eq!(page.get_subsidiaries().row_count(), 4);
    page.invoke_action("delete-subsidiary".into());
    page.invoke_answered(true);
    assert_eq!(page.get_subsidiaries().row_count(), 2);
    screenshot(&rendered, 1, "subsidiary-exchange.png", &page);
    page.invoke_subsidiary_clicked(0, false, false);
    page.invoke_subsidiary_clicked(1, true, false);
    page.invoke_action("export-subsidiary".into());
    let export = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(export.get_text().as_str()).unwrap(),
        reference["bundle"]
    );
    export.invoke_action("cancel".into());
    page.invoke_action("apply".into());
    list.invoke_action("apply".into());
    let persisted = definitions(&store);
    let texts = persisted.parsers[0]
        .parse(&mut ParsingContext::new(), "parent document")
        .unwrap()
        .iter()
        .map(|post| {
            post.contents
                .iter()
                .map(|c| c.text.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(serde_json::json!(texts), reference["parsed"]);
    assert_eq!(
        exchange::tuple(&persisted.parsers[0].subsidiary[0]).unwrap(),
        reference["single"]
    );
    // Wrong-type packages and stale accepted imports cannot change an owner draft.
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_action("import-subsidiary".into());
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    import.set_text(r#"[136,1,["wrong",1,"",[84,1,[26,3,[]]]]]"#.into());
    import.invoke_action("review".into());
    assert!(!import.get_ready());
    assert!(!import.get_error().is_empty());
    assert_eq!(page.get_subsidiaries().row_count(), 2);
    import.set_text(reference["single"].to_string().into());
    import.invoke_action("review".into());
    page.invoke_force_close();
    assert!(!slots.exchange.has_open());
    import.invoke_action("accept".into());
    list.invoke_action("cancel".into());
    assert_eq!(definitions(&store), persisted);
}

#[test]
fn raw_content_preview_preserves_clipboard_context_and_detects_fetched_png_bytes() {
    use hydrus_gui::{Clip, formula_window};
    use hydrus_gui_model::formula_editors::FormulaTestData;
    use std::{cell::RefCell, rc::Rc};
    let reference = hydrus_testkit::fixture_json("parser_raw_preview.json");
    let server = TestDocuments::start_with_image(
        hex::decode(reference["png_hex"].as_str().unwrap()).unwrap(),
    );
    let (_dir, store, slots) = setup();
    let rendered = headless::init();
    let original = definitions(&store);
    let mut test = FormulaTestData {
        text: "original".into(),
        ..FormulaTestData::default()
    };
    test.context
        .insert("url".into(), "https://raw-preview.example/original".into());
    test.context.insert("post_index".into(), "7".into());
    test.context.insert("custom".into(), "kept".into());
    let mut parser = new_content();
    parser.formula = hydrus_parse::formula::Formula {
        reference_auxiliary: None,
        name: String::new(),
        kind: FormulaKind::ContextVariable {
            variable: "url".into(),
        },
        processor: hydrus_core::url::strings::StringProcessor::default(),
    };
    let content = windows::open_content(
        &store,
        &parser,
        test.clone(),
        &slots,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8],
        Rc::new(|_| Ok(())),
    )
    .unwrap();
    *slots.content.borrow_mut() = Some(content.clone_strong());
    let pasted = reference["states"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["case"] == "paste")
        .unwrap();
    let raw = pasted["input"]["text"].as_str().unwrap().to_owned();
    hydrus_gui::set_clipboard_reader({
        let raw = raw.clone();
        move || Ok(Some(raw.clone()))
    });
    content.invoke_raw_action("paste".into());
    assert_eq!(content.get_document(), raw);
    assert_eq!(
        content.get_raw_description(),
        pasted["description"].as_str().unwrap()
    );
    assert_eq!(
        content.get_raw_preview(),
        pasted["preview"].as_str().unwrap()
    );
    assert!(content.get_parse_enabled());
    assert_eq!(content.get_post_index(), "7");
    assert_eq!(content.get_variables(), "custom=kept");
    assert_eq!(
        content.get_test_url(),
        "https://raw-preview.example/original"
    );
    let copied = Rc::new(RefCell::new(String::new()));
    hydrus_gui::set_clipper({
        let copied = copied.clone();
        move |clip| {
            if let Clip::Text(text) = clip {
                *copied.borrow_mut() = text.clone();
            }
        }
    });
    content.invoke_raw_action("copy".into());
    assert_eq!(*copied.borrow(), raw);
    assert_ne!(*copied.borrow(), content.get_raw_preview().as_str());
    hydrus_gui::set_clipboard_reader(|| Err("synthetic clipboard error".into()));
    content.invoke_raw_action("paste".into());
    assert!(content.get_error().contains("Problem loading!"));
    assert_eq!(content.get_document(), raw);
    content.set_fetch_url(format!("{}/png", server.base).into());
    content.invoke_fetch_from_url();
    until_fetch(|| !content.get_fetching());
    assert_eq!(content.get_raw_description(), "That looked like a png!");
    assert_eq!(content.get_raw_preview(), "no preview");
    assert!(!content.get_parse_enabled());
    assert_eq!(content.get_test_url(), format!("{}/png", server.base));
    assert_eq!(content.get_post_index(), "0");
    assert_eq!(content.get_variables(), "custom=kept");
    let before = content.get_preview();
    content.invoke_action("test".into());
    assert_eq!(content.get_preview(), before);
    content.invoke_action("formula".into());
    let child = slots
        .formula
        .formula
        .borrow()
        .as_ref()
        .unwrap()
        .clone_strong();
    // Child inheritance is text-only in Qt; its own raw MIME inspection does not leak.
    assert!(child.get_parse_enabled());
    assert_eq!(child.get_document(), content.get_document());
    child.invoke_cancel();
    let pixels = headless::render(&rendered.get(0).unwrap(), 920, 700);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("parser_raw_preview.png"),
        &pixels,
        920,
        700,
    )
    .unwrap();
    let old = content.clone_strong();
    slots.cancel();
    hydrus_gui::set_clipboard_reader(|| Ok(Some("stale replacement".into())));
    old.invoke_raw_action("paste".into());
    assert_ne!(old.get_document(), "stale replacement");
    assert_eq!(definitions(&store), original);
    // The standalone formula owner detects bytes independently and restores examples.
    let formula_slots = formula_window::Slots::default();
    let formula = formula_window::open(
        &store,
        &parser.formula,
        test,
        &formula_slots,
        Rc::new(|_| {}),
    )
    .unwrap();
    *formula_slots.formula.borrow_mut() = Some(formula.clone_strong());
    formula.set_fetch_url(format!("{}/png", server.base).into());
    formula.invoke_fetch();
    until_fetch(|| !formula.get_fetching());
    assert_eq!(formula.get_raw_description(), "That looked like a png!");
    assert!(!formula.get_parse_enabled());
    assert_eq!(formula.get_results().row_count(), 0);
    formula.set_example(0);
    formula.invoke_example_chosen();
    assert!(formula.get_parse_enabled());
    assert_eq!(formula.get_document(), "original");
    formula.set_example(1);
    formula.invoke_example_chosen();
    assert!(!formula.get_parse_enabled());
    formula.invoke_test();
    assert_eq!(formula.get_results().row_count(), 0);
    hydrus_gui::set_clipboard_reader({
        let raw = raw.clone();
        move || Ok(Some(raw.clone()))
    });
    formula.invoke_raw_action("paste".into());
    assert_eq!(formula.get_document(), raw);
    assert_eq!(
        formula.get_raw_preview(),
        pasted["preview"].as_str().unwrap()
    );
    assert!(formula.get_parse_enabled());
    assert_eq!(
        formula
            .get_results()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        format!("{}/png", server.base)
    );
    formula.invoke_raw_action("copy".into());
    assert_eq!(*copied.borrow(), raw);
    formula_slots.cancel();
    hydrus_gui::set_clipboard_reader(|| Ok(Some("stale replacement".into())));
    formula.invoke_raw_action("paste".into());
    assert_eq!(formula.get_document(), raw);
}

#[test]
fn subsidiary_export_uses_owned_reference_png_parameters_and_discards_stale_export() {
    let (_dir, store, slots) = setup();
    headless::init();
    let reference = hydrus_testkit::fixture_json("parser_png_export.json");
    let original = definitions(&store);
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.invoke_action("import-subsidiary".into());
    let import = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    let case = reference
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "subsidiary_queue")
        .unwrap();
    import.set_text(case["payload"].to_string().into());
    import.invoke_action("review".into());
    import.invoke_action("accept".into());
    page.invoke_action("export-subsidiary".into());
    let exchange = slots.exchange.0.borrow().as_ref().unwrap().clone_strong();
    assert!(exchange.get_png_enabled());
    exchange.invoke_export_png();
    let png = slots.exchange.1.window().unwrap();
    assert_eq!(png.get_png_title(), case["default_title"].as_str().unwrap());
    assert_eq!(
        png.get_payload_description(),
        case["summary"].as_str().unwrap()
    );
    assert_eq!(png.get_png_width(), 512);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("subsidiaries.png");
    png.set_path(path.to_string_lossy().into_owned().into());
    png.set_png_width(300);
    png.set_png_title("recorded queue 日本".into());
    png.set_description("synthetic typed export".into());
    png.invoke_action("export".into());
    assert!(png.get_done(), "{}", png.get_error());
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &hydrus_downloader_exchange::text_png::decode(&bytes).unwrap()
        )
        .unwrap(),
        case["loaded"]
    );
    let stale = dir.path().join("stale.png");
    png.set_path(stale.to_string_lossy().into_owned().into());
    slots.cancel();
    assert!(slots.exchange.1.window().is_none());
    assert!(!exchange.get_active());
    png.invoke_action("export".into());
    exchange.invoke_export_png();
    assert!(!stale.exists());
    assert_eq!(definitions(&store), original);
}

#[test]
fn links_auto_fill_and_api_review_reproduce_reference_and_preserve_installed_consumers() {
    use hydrus_legacy::{
        objects::{domain, parsers},
        serialisable::SerialisableObject,
    };
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("parser_auto_links.json");
    let classes = fixture["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            domain::url_class(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    let parsers = fixture["parsers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            parsers::page_parser(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    fn rows(model: &slint::ModelRc<hydrus_gui::TableRow>) -> serde_json::Value {
        serde_json::json!(
            (0..model.row_count())
                .map(|i| {
                    let cells = model.row_data(i).unwrap().cells;
                    (0..cells.row_count())
                        .map(|j| cells.row_data(j).unwrap().to_string())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        )
    }
    for case in fixture["cases"].as_array().unwrap() {
        let settings = UrlClassSettings {
            url_classes: classes.clone(),
            parser_keys: parsers.iter().map(|p| p.key.clone()).collect(),
            parser_links: case["existing"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        hex::encode(
                            &classes
                                .iter()
                                .find(|class| class.name == pair[0].as_str().unwrap())
                                .unwrap()
                                .key,
                        ),
                        Some(
                            parsers
                                .iter()
                                .find(|parser| parser.name == pair[1].as_str().unwrap())
                                .unwrap()
                                .key
                                .clone(),
                        ),
                    )
                })
                .collect(),
            ..UrlClassSettings::default()
        };
        let definitions = Downloaders {
            parsers: parsers.clone(),
            ..Downloaders::default()
        };
        let saved = settings.clone();
        store
            .write_and_refresh(move |ctx| {
                settings::set(ctx.conn(), &definitions)?;
                settings::set(ctx.conn(), &saved)
            })
            .unwrap();
        let slots = Slots::default();
        let window = windows::open(&store, &slots, true).unwrap();
        assert_eq!(
            window.get_gaps_exist(),
            case["steps"][0]["enabled"].as_bool().unwrap()
        );
        assert_eq!(rows(&window.get_rows()), case["steps"][0]["rows"]);
        window.invoke_action("auto-link".into());
        assert_eq!(rows(&window.get_rows()), case["steps"][1]["rows"]);
        assert_eq!(
            store.read(settings::get::<UrlClassSettings>).unwrap(),
            settings
        );
        window.set_links_tab(1);
        assert_eq!(rows(&window.get_api_rows()), case["api_pairs"]);
        assert_eq!(
            serde_json::json!(
                (0..window.get_api_columns().row_count())
                    .map(|i| window
                        .get_api_columns()
                        .row_data(i)
                        .unwrap()
                        .title
                        .to_string())
                    .collect::<Vec<_>>()
            ),
            case["api_columns"]
        );
        window.invoke_api_sort(1, false);
        assert_eq!(rows(&window.get_api_rows()), case["api_pairs"]);
        window.invoke_action("clear".into());
        assert_eq!(
            rows(&window.get_rows()),
            case["steps"][1]["rows"],
            "API tab has no parser mutation controls"
        );
        window.invoke_action("cancel".into());
        window.invoke_action("auto-link".into());
        assert_eq!(
            store.read(settings::get::<UrlClassSettings>).unwrap(),
            settings
        );
        let reopened = windows::open(&store, &slots, true).unwrap();
        assert_eq!(rows(&reopened.get_api_rows()), case["api_pairs"]);
        reopened.invoke_action("apply".into());
        assert_eq!(
            store.read(settings::get::<UrlClassSettings>).unwrap(),
            settings
        );
        if case["name"] == "installed" {
            let target = parsers.iter().find(|p| p.name == "API parser").unwrap();
            let (url, parser) = store
                .snapshot()
                .url_classes
                .url_to_fetch_and_parser("https://links.example/redirect")
                .unwrap();
            assert_eq!(url, "https://links.example/api");
            assert_eq!(parser, target.key);
        }
    }
}

#[test]
fn owned_parser_picker_and_clear_questions_replay_reference_and_reach_live_resolver() {
    use hydrus_legacy::{
        objects::{domain, parsers},
        serialisable::SerialisableObject,
    };
    let _windows = headless::init();
    let fixture = hydrus_testkit::fixture_json("parser_link_picker.json");
    let class = domain::url_class(
        &SerialisableObject::from_tuple_str(&fixture["classes"][0].to_string()).unwrap(),
    )
    .unwrap();
    let parsers = fixture["parsers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            parsers::page_parser(&SerialisableObject::from_tuple_str(&value.to_string()).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    let class_key = hex::encode(&class.key);
    let initial = UrlClassSettings {
        url_classes: vec![class],
        parser_keys: parsers.iter().map(|p| p.key.clone()).collect(),
        parser_links: vec![(class_key.clone(), Some(parsers[0].key.clone()))],
        ..UrlClassSettings::default()
    };
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let definitions = Downloaders {
        parsers: parsers.clone(),
        ..Downloaders::default()
    };
    let saved = initial.clone();
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &definitions)?;
            settings::set(ctx.conn(), &saved)
        })
        .unwrap();
    let slots = Slots::default();
    let owner = windows::open(&store, &slots, true).unwrap();
    owner.invoke_row_clicked(0, false, false);
    assert!(owner.get_single_selected());
    assert!(owner.get_clearable());
    for (step, index) in fixture["steps"]
        .as_array()
        .unwrap()
        .iter()
        .zip([0, 2, 0, 3])
    {
        owner.invoke_action("pick-parser".into());
        let child = slots.picker.borrow().as_ref().unwrap().clone_strong();
        assert!(owner.get_child_open());
        assert_eq!(child.get_window_title(), step["title"].as_str().unwrap());
        assert_eq!(
            child.get_selected_index(),
            i32::try_from(step["initial"][0].as_i64().unwrap()).unwrap()
        );
        assert_eq!(
            serde_json::json!(
                (0..child.get_rows().row_count())
                    .map(|i| child
                        .get_rows()
                        .row_data(i)
                        .unwrap()
                        .cells
                        .row_data(0)
                        .unwrap()
                        .to_string())
                    .collect::<Vec<_>>()
            ),
            step["choices"]
        );
        child.invoke_selected(-1);
        assert_eq!(
            child.get_selected_index(),
            i32::try_from(step["initial"][0].as_i64().unwrap()).unwrap()
        );
        owner.invoke_action("apply".into());
        assert!(slots.links.borrow().is_some(), "owner is blocked by picker");
        child.invoke_selected(index);
        child.invoke_answered(step["accepted"].as_bool().unwrap());
        assert!(slots.picker.borrow().is_none());
        assert!(!owner.get_child_open());
        assert_eq!(
            owner
                .get_rows()
                .row_data(0)
                .unwrap()
                .cells
                .row_data(2)
                .unwrap(),
            step["value"].as_str().unwrap()
        );
        child.invoke_selected(0);
        child.invoke_answered(true);
        assert_eq!(
            owner
                .get_rows()
                .row_data(0)
                .unwrap()
                .cells
                .row_data(2)
                .unwrap(),
            step["value"].as_str().unwrap(),
            "retired child cannot rewrite link"
        );
        assert_eq!(
            store.read(settings::get::<UrlClassSettings>).unwrap(),
            initial
        );
    }
    owner.invoke_action("clear".into());
    assert_eq!(
        owner.get_question(),
        fixture["clear"][0]["question"].as_str().unwrap()
    );
    owner.invoke_answered(false);
    assert!(owner.get_clearable());
    owner.invoke_action("apply".into());
    let other = parsers.iter().find(|p| p.name == "a other").unwrap();
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .url_to_fetch_and_parser("https://links.example/direct")
            .unwrap()
            .1,
        other.key
    );
    let reopened = windows::open(&store, &slots, true).unwrap();
    reopened.invoke_row_clicked(0, false, false);
    reopened.invoke_action("pick-parser".into());
    let stale = slots.picker.borrow().as_ref().unwrap().clone_strong();
    assert_eq!(stale.get_selected_index(), 3);
    slots.cancel();
    assert!(slots.links.borrow().is_none());
    assert!(slots.picker.borrow().is_none());
    stale.invoke_selected(0);
    stale.invoke_answered(true);
    reopened.invoke_action("apply".into());
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .url_to_fetch_and_parser("https://links.example/direct")
            .unwrap()
            .1,
        other.key
    );
    let reopened = windows::open(&store, &slots, true).unwrap();
    reopened.invoke_row_clicked(0, false, false);
    reopened.invoke_action("clear".into());
    reopened.invoke_answered(true);
    assert!(!reopened.get_clearable());
    assert_eq!(
        reopened
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(2)
            .unwrap(),
        ""
    );
    reopened.invoke_action("cancel".into());
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .url_to_fetch_and_parser("https://links.example/direct")
            .unwrap()
            .1,
        other.key
    );
    let reopened = windows::open(&store, &slots, true).unwrap();
    reopened.invoke_row_clicked(0, false, false);
    reopened.invoke_action("clear".into());
    reopened.invoke_answered(true);
    reopened.invoke_action("apply".into());
    assert_eq!(
        store.snapshot().url_classes.settings().parser_links,
        vec![(class_key, None)]
    );
    assert!(
        store
            .snapshot()
            .url_classes
            .url_to_fetch_and_parser("https://links.example/direct")
            .is_err()
    );
    store
        .write_and_refresh(|ctx| settings::set(ctx.conn(), &Downloaders::default()))
        .unwrap();
    let empty = windows::open(&store, &slots, true).unwrap();
    empty.invoke_row_clicked(0, false, false);
    empty.invoke_action("pick-parser".into());
    assert_eq!(empty.get_error(), fixture["warning"].as_str().unwrap());
    assert!(slots.picker.borrow().is_none());
    slots.cancel();
}

#[test]
fn timestamp_content_has_only_source_choice_and_persists_real_parsed_metadata() {
    use hydrus_gui_model::formula_editors::FormulaTestData;
    use hydrus_legacy::{objects::parsers, serialisable::SerialisableObject};
    use std::rc::Rc;
    let (_dir, store, slots) = setup();
    let rendered = headless::init();
    let reference = hydrus_testkit::fixture_json("content_time.json");
    for case in reference["cases"].as_array().unwrap() {
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let mut parser = parsers::content_parser(&object).unwrap();
        if case["input_type"] != "datestring" {
            parser.kind = ContentKind::Timestamp {
                timestamp_type: case["input_type"].as_i64(),
            };
        }
        let original = definitions(&store);
        let content = windows::open_content(
            &store,
            &parser,
            FormulaTestData::default(),
            &slots,
            &[4],
            Rc::new({
                let store = store.clone();
                move |parser| {
                    store
                        .write(move |ctx| {
                            let mut saved: Downloaders = settings::get(ctx.conn())?;
                            saved.parsers[0].content_parsers = vec![parser];
                            settings::set(ctx.conn(), &saved)
                        })
                        .map_err(|e| e.to_string())
                }
            }),
        )
        .unwrap();
        let field = content
            .get_fields()
            .iter()
            .find(|field| field.id == 8)
            .unwrap();
        assert_eq!(field.kind, 1);
        assert_eq!(field.label, "timestamp type");
        assert_eq!(field.chosen, 0);
        assert_eq!(
            field
                .options
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            vec!["source time"]
        );
        // Retained text callbacks and invalid choices cannot author timestamp
        // metadata that is not in the original one-choice control.
        content.invoke_text_edited(8, "7".into());
        content.invoke_choice_edited(8, 99);
        content.invoke_choice_edited(8, 0);
        content.set_test_url("https://source-time.example/post/1".into());
        content.set_document(case["cases"][0]["document"].as_str().unwrap().into());
        content.invoke_action("test".into());
        assert!(content.get_preview().contains("timestamp:"));
        assert_eq!(definitions(&store), original);
        let pixels = headless::render(&rendered.get(rendered.count() - 1).unwrap(), 1000, 780);
        assert_eq!(pixels.len(), 1000 * 780 * 4);
        headless::save_png(
            &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("content_time.png"),
            &pixels,
            1000,
            780,
        )
        .unwrap();
        content.invoke_action("apply".into());
        assert!(slots.content.borrow().is_none());
        let saved = definitions(&store).parsers[0].content_parsers[0].clone();
        assert_eq!(
            saved.kind,
            ContentKind::Timestamp {
                timestamp_type: Some(0)
            }
        );
        assert_eq!(saved.formula, parser.formula);
        let mut context = ParsingContext::new();
        context.insert("url".into(), "https://source-time.example/post/1".into());
        for parsed in case["cases"].as_array().unwrap() {
            let post = saved
                .parse(&context, parsed["document"].as_str().unwrap())
                .unwrap();
            assert_eq!(
                serde_json::json!(
                    post.contents
                        .iter()
                        .map(|c| c.text.as_str())
                        .collect::<Vec<_>>()
                ),
                parsed["texts"]
            );
            assert_eq!(
                serde_json::json!(post.timestamp(0, reference["now"].as_i64().unwrap())),
                parsed["source_time"]
            );
        }
        content.invoke_choice_edited(8, 0);
        content.invoke_action("apply".into());
        assert_eq!(definitions(&store).parsers[0].content_parsers[0], saved);
    }
}
