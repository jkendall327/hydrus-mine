//! Secondary windows end their titles with the application display name, as
//! Qt shows every window ("title - name version"): the composed title of
//! several windows of different files, among them one whose title once
//! lacked the suffix.

use hydrus_gui::network_sessions_window::{self as windows, Slots};
use hydrus_store::Store;

#[test]
fn secondary_windows_compose_their_titles_with_the_display_name() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = hydrus_gui::headless::init();
    hydrus_gui::app_title::set_display_name("synthetic name");
    let suffix = format!(" - synthetic name {}", env!("CARGO_PKG_VERSION"));

    // a list window, and the editor above it
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    assert_eq!(
        browser.get_composed_title(),
        format!("{}{suffix}", browser.get_window_title())
    );
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    assert!(!edit.get_window_title().is_empty());
    assert_eq!(
        edit.get_composed_title(),
        format!("{}{suffix}", edit.get_window_title())
    );
    // changing the name retitles the windows already open, and the next
    hydrus_gui::app_title::set_display_name("other");
    let renamed = format!(" - other {}", env!("CARGO_PKG_VERSION"));
    assert_eq!(
        browser.get_composed_title(),
        format!("{}{renamed}", browser.get_window_title())
    );
    assert_eq!(
        edit.get_composed_title(),
        format!("{}{renamed}", edit.get_window_title())
    );
    let again = windows::open(&store, &Slots::default(), true).unwrap();
    assert_eq!(
        again.get_composed_title(),
        format!("{}{renamed}", again.get_window_title())
    );
}
