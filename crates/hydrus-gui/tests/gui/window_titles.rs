//! Secondary windows end their titles with the application display name, as
//! Qt shows every window of the reference on Linux ("title — name version",
//! `oracle/record_window_titles.py`): a window open when the name changes is
//! retitled, one opened after is made with it, for each name the recording
//! has (the reference's version replaced by hydrus-rs's).

use hydrus_gui::network_sessions_window::{self as windows, Slots};
use hydrus_store::Store;

/// The suffix the recording shows after `title` for the recorded `window`.
fn suffix(name: &serde_json::Value, window: &str, version: &str) -> String {
    let shown = name[window]["shown"].as_str().unwrap();
    let title = name[window]["title"].as_str().unwrap();
    let suffix = shown.strip_prefix(title).unwrap();
    format!(
        "{}{}",
        suffix.strip_suffix(version).unwrap(),
        env!("CARGO_PKG_VERSION")
    )
}

// leaf: audit-options-gui-main-window-application-display-name
#[test]
fn secondary_windows_compose_their_titles_with_the_display_name() {
    let recorded = hydrus_testkit::fixture_json("window_titles.json");
    assert_eq!(recorded["platform"], "xcb");
    let version = recorded["version"].to_string();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let _windows = hydrus_gui::headless::init();
    hydrus_gui::app_title::set_display_name("synthetic name");

    // a list window, and the editor above it, open before the name changes
    let slots = Slots::default();
    let browser = windows::open(&store, &slots, false).unwrap();
    browser.invoke_add_clicked();
    let edit = windows::last_edit_opened().unwrap();
    assert!(!edit.get_window_title().is_empty());
    for name in recorded["names"].as_array().unwrap() {
        let what = name["saved"].to_string();
        hydrus_gui::app_title::set_display_name(name["saved"].as_str().unwrap());
        // the ones open are retitled
        let retitled = suffix(name, "opened_before", &version);
        for window in [browser.get_window_title(), edit.get_window_title()] {
            assert!(!window.is_empty());
        }
        assert_eq!(
            browser.get_composed_title(),
            format!("{}{retitled}", browser.get_window_title()),
            "{what}"
        );
        assert_eq!(
            edit.get_composed_title(),
            format!("{}{retitled}", edit.get_window_title()),
            "{what}"
        );
        // and the next is made with it
        let again = windows::open(&store, &Slots::default(), true).unwrap();
        assert_eq!(
            again.get_composed_title(),
            format!(
                "{}{}",
                again.get_window_title(),
                suffix(name, "opened_after", &version)
            ),
            "{what}"
        );
    }
}
