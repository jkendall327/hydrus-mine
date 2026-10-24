//! A text input holding a line far wider than the renderer can address,
//! all of it selected: the window should still draw.

use hydrus_gui::{MainWindow, Pages, SearchPage, bind, headless};
use hydrus_store::{Store, import::import_legacy};
use slint::ComponentHandle as _;
use slint::platform::WindowAdapter as _;

use crate::common::widgets;

/// Slint 1.18.1's software renderer draws a text input's selection as one
/// rectangle the width of the selected text and casts it to 16-bit device
/// pixels (`fill_rectangle` in i-slint-renderer-software), panicking when it
/// is wider than 32767: pasting a ~4800-character URL into the search box
/// and selecting it all crashes the client (DIFFERENCES.md, "Very long
/// lines in text inputs"). The upstream report is drafted in PR #280; run
/// with `--ignored` to see it.
#[test]
#[ignore = "crashes in Slint's software renderer (upstream, i-slint-renderer-software fill_rectangle)"]
fn a_very_long_line_typed_and_selected_is_drawn() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let _bound = bind(&ui, Pages::single(SearchPage::new(store)));
    ui.show().unwrap();
    let native = windows.get(0).unwrap();
    headless::render(&native, 1100, 700);
    ui.set_search_focus_requests(ui.get_search_focus_requests() + 1);
    widgets::settle();
    assert!(ui.get_search_focused());
    // (a pasted URL of ~4800 characters: ~40000 pixels)
    let long = format!("https://example.com/{}", "abcdefgh".repeat(600));
    widgets::key(ui.window(), long.as_str());
    widgets::ctrl_key(ui.window(), "a");
    for scale_factor in [1.0, 2.0] {
        native
            .window()
            .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor });
        headless::render(&native, 1100, 700);
    }
}
