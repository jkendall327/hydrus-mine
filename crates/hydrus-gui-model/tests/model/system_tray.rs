//! The tray's rules against every case the reference was recorded in
//! (`oracle/fixtures/system_tray.json`, from `oracle/record_system_tray.py`).

use hydrus_gui_model::system_tray::{self as tray, Click, Close};

fn recorded() -> serde_json::Value {
    hydrus_testkit::fixture_json("system_tray.json")
}

fn flag(case: &serde_json::Value, key: &str) -> bool {
    case[key].as_bool().unwrap_or_else(|| panic!("{key}"))
}

#[test]
fn the_icon_exists_in_the_cases_the_reference_makes_it() {
    let cases = recorded()["icon"].as_array().unwrap().clone();
    assert_eq!(cases.len(), 8);
    for case in cases {
        assert_eq!(
            tray::needs_icon(
                flag(&case, "available"),
                flag(&case, "always_show"),
                flag(&case, "hidden")
            ),
            flag(&case, "icon"),
            "{case}"
        );
    }
}

#[test]
fn closing_hides_or_asks_to_exit_as_the_reference_does() {
    for case in recorded()["close"].as_array().unwrap() {
        let hides =
            tray::close(flag(case, "available"), flag(case, "close_to_tray")) == Close::HideToTray;
        assert_eq!(hides, flag(case, "hidden"), "{case}");
        assert_eq!(hides, !flag(case, "exit_asked"), "{case}");
    }
}

#[test]
fn minimising_hides_in_the_cases_the_reference_hides() {
    for case in recorded()["minimise"].as_array().unwrap() {
        assert_eq!(
            tray::minimise_hides(flag(case, "available"), flag(case, "minimise_to_tray")),
            flag(case, "hidden"),
            "{case}"
        );
    }
}

#[test]
fn a_click_on_the_icon_does_what_the_reference_does() {
    for case in recorded()["activation"].as_array().unwrap() {
        let click = tray::click(
            flag(case, "was_hidden"),
            flag(case, "active"),
            flag(case, "minimise_to_tray"),
        );
        let hidden = matches!(click, Click::HideToTray);
        let minimised = matches!(click, Click::Minimise);
        // (a hidden client shows; an active one hides or minimises; else
        // nothing changes but the focus)
        let was_hidden = flag(case, "was_hidden");
        assert_eq!(flag(case, "hidden"), hidden, "{case}");
        assert_eq!(flag(case, "minimised"), minimised, "{case}");
        assert_eq!(click == Click::ShowAndRaise, was_hidden, "{case}");
    }
}

#[test]
fn the_file_entry_is_shown_in_the_cases_the_reference_shows_it() {
    let recorded = recorded();
    for case in recorded["file_menu"].as_array().unwrap() {
        let visible = tray::file_entry_visible(
            flag(case, "available"),
            flag(case, "windows"),
            flag(case, "advanced_mode"),
        );
        assert_eq!(visible, flag(case, "menu_item_visible"), "{case}");
        assert_eq!(visible, flag(case, "hid_by_action"), "{case}");
        assert_eq!(case["menu_item_text"], tray::FILE_ENTRY);
        assert_eq!(case["menu_item_tooltip"], tray::FILE_ENTRY_TIP);
    }
}

#[test]
fn the_tooltip_and_the_first_entry_say_what_the_reference_says() {
    let recorded = recorded();
    let menu = &recorded["menu"];
    for case in menu["tooltips"].as_array().unwrap() {
        assert_eq!(
            tray::tooltip(
                "my hydrus",
                flag(case, "network_paused"),
                flag(case, "subscriptions_paused")
            ),
            case["tooltip"].as_str().unwrap()
        );
    }
    assert_eq!(menu["shown"][0]["text"], tray::show_hide_label(true));
    assert_eq!(menu["hidden"][0]["text"], tray::show_hide_label(false));
}

#[test]
fn a_boot_starts_hidden_in_the_cases_the_reference_does() {
    for case in recorded()["start"].as_array().unwrap() {
        let hidden = tray::starts_hidden(flag(case, "available"), flag(case, "start_option"));
        assert_eq!(hidden, flag(case, "hidden"), "{case}");
        assert_eq!(hidden, !flag(case, "window_visible"), "{case}");
        assert_eq!(hidden, flag(case, "icon"), "{case}");
    }
}
