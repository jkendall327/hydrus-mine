//! The page parser editor's "fetch example data" against the reference's own
//! (oracle/fixtures/parser_fetch_controls.json, from
//! oracle/record_parser_fetch_controls.py, the same kind of local server):
//! a blank url box, the network job control while a request holds, cancel,
//! one connection attempt, an invalid url, and closing the editor mid-fetch.
use super::parser_editors::{TestDocuments, child, definitions, setup, until_fetch};
use hydrus_gui::{ParserEditWindow, headless, parser_editors_window as windows};
use hydrus_store::settings;
use serde_json::Value;
use std::io::Read as _;

/// The control as the recorder writes it.
fn control(page: &ParserEditWindow) -> Value {
    serde_json::json!({
        "left": page.get_fetch_status().to_string(),
        "right": page.get_fetch_right().to_string(),
        "gauge": [page.get_fetch_gauge_value(), page.get_fetch_gauge_range()],
        "can_cancel": page.get_fetching(),
    })
}

fn text(v: &Value, base: &str) -> String {
    v.as_str().unwrap().replace("<base>", base)
}

// leaf: parser-fetch
#[test]
fn the_page_editor_s_fetch_controls_act_as_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("parser_fetch_controls.json");
    let server = TestDocuments::start();
    let base = server.base.clone();
    let (_dir, store, slots) = setup();
    let _windows = headless::init();
    let mut saved = definitions(&store);
    saved.parsers[0].example_urls = vec![format!("{base}/a b")];
    store
        .write_and_refresh(move |ctx| settings::set(ctx.conn(), &saved))
        .unwrap();
    let list = windows::open(&store, &slots, false).unwrap();
    list.invoke_row_clicked(0, false, false);
    list.invoke_action("edit".into());
    let page = child(&slots.page);
    page.set_document("previous".into());
    let asked = || server.requests.lock().unwrap().len();

    // a blank url box takes the first example url, and nothing is asked
    let blank = &recorded["blank"];
    page.set_test_url("".into());
    page.invoke_fetch();
    assert_eq!(
        page.get_test_url().to_string(),
        text(&blank["test_url"], &base)
    );
    assert_eq!(asked(), 0);
    assert_eq!(blank["asked"], 0);
    assert_eq!(page.get_document(), text(&blank["text"], &base));
    assert!(!page.get_fetching());

    // the control while a request holds, then cancelled from it
    for (name, path) in [("progress", "/slow"), ("cancel", "/hold")] {
        let case = &recorded[name];
        let before = asked();
        page.set_test_url(format!("{base}{path}").into());
        page.invoke_fetch();
        until_fetch(|| asked() > before && control(&page) == case["holding"]);
        page.invoke_cancel_fetch();
        until_fetch(|| !page.get_fetching());
        assert_eq!(page.get_document(), text(&case["text"], &base), "{name}");
        assert_eq!(
            page.get_test_url().to_string(),
            text(&case["test_url"], &base)
        );
        assert_eq!(
            page.get_post_index(),
            case["context"]["post_index"].as_str().unwrap()
        );
        assert_eq!(control(&page), case["after"], "{name}");
    }

    // one connection attempt: a dropped connection is not tried again
    let once = &recorded["once"];
    let before = asked();
    page.set_test_url(format!("{base}/drop").into());
    page.invoke_fetch();
    until_fetch(|| !page.get_fetching());
    assert_eq!(asked() - before, once["asked"].as_u64().unwrap() as usize);
    let document = page.get_document().to_string();
    assert_eq!(
        document.lines().next().unwrap_or_default(),
        once["text"].as_str().unwrap()
    );
    assert_eq!(
        page.get_fetch_has_error(),
        once["error_shown"].as_bool().unwrap()
    );
    assert_eq!(control(&page), once["after"]);

    // an invalid url: the reference's message, nothing asked, the document kept
    let invalid = &recorded["invalid"];
    let before = asked();
    page.set_test_url("invalid".into());
    page.invoke_fetch();
    assert_eq!(page.get_error(), invalid["message"].as_str().unwrap());
    assert_eq!(
        asked() - before,
        invalid["asked"].as_u64().unwrap() as usize
    );
    assert_eq!(page.get_document().to_string(), document);
    assert!(!page.get_fetching());

    // closing the editor mid-fetch leaves the request to finish, as the
    // reference leaves its job in the engine (not cancelled, not done)
    let close = &recorded["close"];
    assert_eq!(
        (close["cancelled"].as_bool(), close["done"].as_bool()),
        (Some(false), Some(false))
    );
    let held = server.held.lock().unwrap().len();
    page.set_test_url(format!("{base}/hold").into());
    page.invoke_fetch();
    until_fetch(|| server.held.lock().unwrap().len() > held);
    page.invoke_force_close();
    std::thread::sleep(std::time::Duration::from_secs(2));
    slint::platform::update_timers_and_animations();
    let socket = server
        .held
        .lock()
        .unwrap()
        .last()
        .unwrap()
        .try_clone()
        .unwrap();
    socket.set_nonblocking(true).unwrap();
    let mut byte = [0; 1];
    let still_open = matches!(
        (&socket).read(&mut byte),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
    );
    assert_eq!(still_open, close["in_engine"].as_bool().unwrap());
    list.invoke_action("cancel".into());
}
