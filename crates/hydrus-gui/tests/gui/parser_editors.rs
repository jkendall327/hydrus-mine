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
    formula::FormulaKind,
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
                    requests.lock().unwrap().push(request);
                    if hold {
                        held.push(stream);
                        continue;
                    }
                    let (status, body): (&str, &[u8]) = if error {
                        ("404 Not Found", b"missing")
                    } else {
                        ("200 OK", b"<p>fetched caf\xe9</p>")
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=iso-8859-1\r\nContent-Length: {}\r\nSet-Cookie: test-document=saved; Path=/\r\nConnection: close\r\n\r\n",
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
        let w = children
            .as_ref()
            .unwrap()
            .formula
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        w
    };
    first.set_kind(5);
    first.invoke_type_chosen();
    first.set_static_text("{\"posts\":[\"edited tag\"]}".into());
    first.invoke_apply();
    formula.invoke_edit_child(true);
    let second = {
        let children = slots.formula.child.borrow();
        let w = children
            .as_ref()
            .unwrap()
            .formula
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        w
    };
    assert_eq!(second.get_document(), "{\"posts\":[\"edited tag\"]}");
    second.invoke_add();
    let rule = {
        let children = slots.formula.child.borrow();
        let w = children
            .as_ref()
            .unwrap()
            .rule
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        w
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
    let context = Default::default();
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
    page.invoke_subsidiary_clicked(0);
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
    let mut context = Default::default();
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
