//! Text inputs that take pasted lines refuse one too wide for the renderer.
//!
//! Slint 1.18.1's software renderer draws a text input's selection as one
//! rectangle the width of the selected text and casts it to 16-bit device
//! pixels (`fill_rectangle` in i-slint-renderer-software), panicking past
//! 32767: a ~4800-character URL pasted into the search box and selected
//! crashed the client. The search and tag inputs (`WriteTagLineEdit`) and the
//! URL and query inputs (`CappedLineEdit`) refuse an edit that would make
//! their line wider than `TextCap` (8000px), saying so; the widest line they
//! take, selected, draws at a scale factor of 3 (DIFFERENCES.md, "Text
//! inputs that take pasted lines"). Plain `LineEdit`s elsewhere are not capped and
//! remain exposed upstream.

use std::sync::Arc;

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy, queues};
use slint::ComponentHandle as _;
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{Key, WindowAdapter as _};

use crate::common::widgets;

const NOTICE: &str = "too long to enter here";

fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// Selects all of the focused input and draws the window at each scale.
fn select_all_and_draw(ui: &MainWindow, native: &MinimalSoftwareWindow) {
    widgets::ctrl_key(ui.window(), "a");
    for scale_factor in [1.0, 2.0, 3.0] {
        native
            .window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor });
        headless::render(native, 1100, 700);
    }
    native
        .window()
        .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
    headless::render(native, 1100, 700);
}

/// Types `chunk` into the focused input until an edit is refused; returns
/// what the input kept.
fn type_until_refused(ui: &MainWindow, chunk: &str, text: impl Fn() -> String) -> String {
    for _ in 0..2_000 {
        let before = text();
        widgets::key(ui.window(), chunk);
        if text() == before {
            assert!(widgets::shows(ui.window(), NOTICE));
            return before;
        }
        assert!(!widgets::shows(ui.window(), NOTICE));
    }
    panic!("never refused");
}

#[test]
fn the_search_box_refuses_a_line_too_wide_and_draws_the_widest_it_takes() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store)));
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 700);
    ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
    widgets::settle();
    assert!(ui.get_search_focused());

    // a pasted URL of ~4800 characters (~40000 pixels): refused, as typed
    // and as pasted, and the box says so
    let long = format!("https://example.com/{}", "abcdefgh".repeat(600));
    widgets::key(ui.window(), long.as_str());
    assert_eq!(ui.get_search_text(), "");
    assert!(widgets::shows(ui.window(), NOTICE));
    select_all_and_draw(&ui, &native);
    headless::set_clipboard_text(&long);
    widgets::ctrl_key(ui.window(), "v");
    assert_eq!(ui.get_search_text(), "");
    // what fits goes in, and the notice goes
    widgets::key(ui.window(), "blue eyes");
    assert_eq!(ui.get_search_text(), "blue eyes");
    assert!(!widgets::shows(ui.window(), NOTICE));
    // pasted into the middle of what is there: refused, the text kept
    widgets::ctrl_key(ui.window(), "v");
    assert_eq!(ui.get_search_text(), "blue eyes");
    assert!(widgets::shows(ui.window(), NOTICE));

    // the widest line the box takes, of URL characters (a realistic long
    // URL fits) and of the widest glyph found in common fonts (Inter's
    // arrows, ~2.7em), selected, draws at a scale factor of 3
    widgets::ctrl_key(ui.window(), "a");
    widgets::key(ui.window(), Key::Backspace);
    let kept = type_until_refused(&ui, "abcdefgh/", || ui.get_search_text().to_string());
    assert!(kept.chars().count() > 800, "{}", kept.chars().count());
    select_all_and_draw(&ui, &native);
    widgets::key(ui.window(), Key::Backspace);
    let kept = type_until_refused(&ui, "\u{1f852}\u{1f850}", || {
        ui.get_search_text().to_string()
    });
    assert!(kept.chars().count() > 100, "{}", kept.chars().count());
    select_all_and_draw(&ui, &native);
}

#[test]
fn the_url_import_box_refuses_a_url_too_long_and_sends_only_what_fits() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::open(store.clone()).unwrap());
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 700);
    // download, then urls
    ui.invoke_new_page();
    ui.invoke_chooser_pressed(4);
    ui.invoke_chooser_pressed(8);
    assert_eq!(bound.pages.borrow().shown().name, "url import");
    let queue = bound
        .current
        .borrow()
        .borrow()
        .importer()
        .map(|i| i.queue)
        .unwrap();
    headless::render(&native, 1100, 700);
    ui.set_page_focus_requests(ui.get_page_focus_requests() + 1);
    widgets::settle();
    assert!(ui.get_page_input_focused());

    // too long: refused, not cut and sent; Enter sends nothing
    let long = format!("https://example.com/{}", "abcdefgh".repeat(600));
    widgets::key(ui.window(), long.as_str());
    assert!(widgets::shows(ui.window(), NOTICE));
    select_all_and_draw(&ui, &native);
    widgets::key(ui.window(), Key::Return);
    let sent = store
        .write(move |ctx| queues::take_url_requests(ctx.conn(), queue))
        .unwrap();
    assert!(sent.is_empty(), "{sent:?}");
    // one that fits goes in and is sent whole
    let fits = format!("https://example.com/{}", "abcdefgh".repeat(60));
    widgets::key(ui.window(), fits.as_str());
    assert!(!widgets::shows(ui.window(), NOTICE));
    widgets::key(ui.window(), Key::Return);
    let sent = store
        .write(move |ctx| queues::take_url_requests(ctx.conn(), queue))
        .unwrap();
    assert_eq!(sent, [fits]);
}
