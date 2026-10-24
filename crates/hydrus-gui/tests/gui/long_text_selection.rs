//! Text inputs that take pasted lines refuse to grow one too wide for the
//! renderer.
//!
//! Slint 1.18.1's software renderer draws a text input's selection as one
//! rectangle the width of the selected text and casts it to 16-bit device
//! pixels (`fill_rectangle` in i-slint-renderer-software), panicking past
//! 32767: a ~4800-character URL pasted into the search box and selected
//! crashed the client. The search and tag inputs (`WriteTagLineEdit`) and the
//! URL and query inputs (`CappedLineEdit`) refuse an edit that would leave
//! their line wider than `TextCap` (26000 device pixels) and longer than it
//! was, saying so, and let a line over it be shortened (DIFFERENCES.md, "Text
//! inputs that take pasted lines"). The tag inputs also draw no selection
//! for a line over the cap, so one grown past it by a higher scale factor
//! draws selected. Plain `LineEdit`s elsewhere are not capped and remain
//! exposed upstream, as does a `CappedLineEdit` holding a line over the cap.

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

/// A URL of `n` characters.
fn url(n: usize) -> String {
    let start = "https://example.com/";
    format!(
        "{start}{}",
        "abcdefgh/".repeat(n).split_at(n - start.len()).0
    )
}

fn scale(native: &MinimalSoftwareWindow, scale_factor: f32) {
    native
        .window()
        .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor });
    headless::render(native, 1100, 700);
}

/// Selects all of the focused input and draws the window at each scale.
fn select_all_and_draw(window: &slint::Window, native: &MinimalSoftwareWindow, scales: &[f32]) {
    widgets::ctrl_key(window, "a");
    for &scale_factor in scales {
        scale(native, scale_factor);
    }
    scale(native, 1.0);
}

/// Types `chunk` into the focused input until an edit is refused; returns
/// what the input kept.
fn type_until_refused(window: &slint::Window, chunk: &str, text: impl Fn() -> String) -> String {
    for _ in 0..5_000 {
        let before = text();
        widgets::key(window, chunk);
        if text() == before {
            assert!(
                widgets::shows(window, NOTICE),
                "{} characters: refused with no notice",
                before.chars().count()
            );
            return before;
        }
        assert!(!widgets::shows(window, NOTICE));
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
    let text = || ui.get_search_text().to_string();

    // a pasted URL of 10000 characters (~65000 pixels): refused, as typed
    // and as pasted, and the box says so
    let long = url(10_000);
    widgets::key(ui.window(), long.as_str());
    assert_eq!(text(), "");
    assert!(widgets::shows(ui.window(), NOTICE));
    select_all_and_draw(ui.window(), &native, &[1.0, 2.0, 3.0]);
    headless::set_clipboard_text(&long);
    widgets::ctrl_key(ui.window(), "v");
    assert_eq!(text(), "");
    // what fits goes in, and the notice goes
    widgets::key(ui.window(), "blue eyes");
    assert_eq!(text(), "blue eyes");
    assert!(!widgets::shows(ui.window(), NOTICE));
    // pasted into the middle of what is there: refused, the text kept
    widgets::ctrl_key(ui.window(), "v");
    assert_eq!(text(), "blue eyes");
    assert!(widgets::shows(ui.window(), NOTICE));
    // the notice goes when the program changes the text
    ui.set_search_text("".into());
    assert!(!widgets::shows(ui.window(), NOTICE));

    // the longest URL the box takes at each scale factor: about 4000
    // characters at 1, half that at 2, a third at 3
    let mut longest = Vec::new();
    for scale_factor in [1.0, 2.0, 3.0] {
        scale(&native, scale_factor);
        widgets::ctrl_key(ui.window(), "a");
        widgets::key(ui.window(), Key::Backspace);
        let kept = type_until_refused(ui.window(), "abcdefgh/", text);
        longest.push(kept.chars().count());
        assert!(ui.get_search_refused());
        // (the line doesn't widen the box: the row's buttons still show)
        assert!(widgets::shows(ui.window(), "lock"));
        // selected, it draws at its own scale factor
        select_all_and_draw(ui.window(), &native, &[scale_factor]);
    }
    eprintln!("the longest URLs taken at 1, 2 and 3: {longest:?}");
    assert!(longest[0] > 3_500, "{longest:?}");
    assert!(longest[1].abs_diff(longest[0] / 2) < 100, "{longest:?}");
    assert!(longest[2].abs_diff(longest[0] / 3) < 100, "{longest:?}");

    // the longest taken at 1, at 3: over the cap, which the box says, and
    // selected it draws (with no selection rectangle)
    scale(&native, 1.0);
    widgets::ctrl_key(ui.window(), "a");
    widgets::key(ui.window(), Key::Backspace);
    type_until_refused(ui.window(), "abcdefgh/", text);
    widgets::key(ui.window(), "a");
    scale(&native, 3.0);
    assert!(widgets::shows(ui.window(), NOTICE));
    select_all_and_draw(ui.window(), &native, &[2.0, 3.0]);

    // of the widest glyph found in common fonts (Inter's arrows, ~2.7em)
    widgets::ctrl_key(ui.window(), "a");
    widgets::key(ui.window(), Key::Backspace);
    let kept = type_until_refused(ui.window(), "\u{1f852}\u{1f850}", text);
    assert!(kept.chars().count() > 500, "{}", kept.chars().count());
    select_all_and_draw(ui.window(), &native, &[1.0, 2.0, 3.0]);

    // a line over the cap that the program put there can be shortened, not
    // lengthened
    ui.set_search_text(long.clone().into());
    assert!(widgets::shows(ui.window(), NOTICE));
    widgets::key(ui.window(), Key::End);
    widgets::key(ui.window(), Key::Backspace);
    assert_eq!(text(), long[..long.len() - 1]);
    widgets::key(ui.window(), "x");
    assert_eq!(text(), long[..long.len() - 1]);
    select_all_and_draw(ui.window(), &native, &[1.0, 3.0]);
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
    let sent = || {
        store
            .write(move |ctx| queues::take_url_requests(ctx.conn(), queue))
            .unwrap()
    };

    // too long, typed or pasted: refused, not cut and sent; Enter sends
    // nothing
    let long = url(10_000);
    widgets::key(ui.window(), long.as_str());
    assert!(widgets::shows(ui.window(), NOTICE));
    select_all_and_draw(ui.window(), &native, &[1.0, 2.0, 3.0]);
    // (a long line doesn't widen the box: the row's paste button still
    // shows)
    widgets::key(ui.window(), url(3_000).as_str());
    assert!(widgets::shows(ui.window(), "paste"));
    widgets::ctrl_key(ui.window(), "a");
    widgets::key(ui.window(), Key::Backspace);
    headless::set_clipboard_text(&long);
    widgets::ctrl_key(ui.window(), "v");
    assert!(widgets::shows(ui.window(), NOTICE));
    widgets::key(ui.window(), Key::Return);
    assert!(sent().is_empty());
    // a long URL that fits, pasted: goes in, is sent whole, and the notice
    // goes with the text
    let fits = url(2_000);
    headless::set_clipboard_text(&fits);
    widgets::ctrl_key(ui.window(), "v");
    assert!(!widgets::shows(ui.window(), NOTICE));
    widgets::key(ui.window(), Key::Return);
    assert_eq!(sent(), std::slice::from_ref(&fits));
    widgets::key(ui.window(), long.as_str());
    assert!(widgets::shows(ui.window(), NOTICE));
    widgets::key(ui.window(), Key::Return);
    assert!(!widgets::shows(ui.window(), NOTICE));
    assert!(sent().is_empty());
}

#[test]
fn a_url_box_holding_a_line_over_the_cap_shortens_it_and_refuses_to_lengthen_it() {
    let (_dirs, store) = store();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store)));
    ui.show().unwrap();
    bound.debug_fetch.open();
    let input = bound
        .debug_fetch
        .windows()
        .into_iter()
        .find(hydrus_gui::DebugFetchWindow::get_input_mode)
        .unwrap();
    input.show().unwrap();
    let native = windows.get(windows.count() - 1).unwrap();
    headless::render(&native, 600, 200);
    // (the program puts a URL over the cap there)
    let long = url(10_000);
    input.set_url(long.clone().into());
    widgets::settle();
    assert!(widgets::shows(input.window(), NOTICE));
    widgets::key(input.window(), Key::End);
    widgets::key(input.window(), Key::Backspace);
    assert_eq!(input.get_url(), long[..long.len() - 1]);
    widgets::key(input.window(), "x");
    assert_eq!(input.get_url(), long[..long.len() - 1]);
    // emptied, the notice goes
    widgets::ctrl_key(input.window(), "a");
    widgets::key(input.window(), Key::Backspace);
    assert_eq!(input.get_url(), "");
    assert!(!widgets::shows(input.window(), NOTICE));
}
