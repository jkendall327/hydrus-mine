//! Actual Qt close notifications, including retained/missing/unowned owners.
use hydrus_gui_model::viewer_closing::{Action, actions};
use hydrus_store::settings::ViewerClosingSettings;

#[test]
fn closing_actions_match_real_reference_notification_order() {
    let fixture = hydrus_testkit::fixture_json("viewer_closing_options.json");
    let defaults = ViewerClosingSettings::default();
    assert_eq!(
        serde_json::json!([
            defaults.reselect_page,
            defaults.select_exit_media,
            defaults.activate_focusing,
            defaults.activate_always
        ]),
        fixture["initial"]
    );
    for event in fixture["events"].as_array().unwrap() {
        let values: Vec<_> = event["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_bool().unwrap())
            .collect();
        let settings = ViewerClosingSettings {
            reselect_page: values[0],
            select_exit_media: values[1],
            activate_focusing: values[2],
            activate_always: values[3],
        };
        let owned = event["scenario"] != "unowned";
        let requested = actions(&settings, owned);
        let activation: Vec<_> = requested
            .iter()
            .filter_map(|action| match action {
                Action::ActivateFocusing => Some("focusing-panel"),
                Action::ActivateDebug => Some("debug-main"),
                Action::ReselectPage | Action::SelectExitMedia => None,
            })
            .collect();
        assert_eq!(
            serde_json::json!(activation),
            event["after"]["activation"],
            "{event:?}"
        );
        assert_eq!(
            requested.contains(&Action::ReselectPage),
            owned && values[0]
        );
        assert_eq!(
            requested.contains(&Action::SelectExitMedia),
            owned && values[1]
        );
    }
    let all = ViewerClosingSettings {
        reselect_page: true,
        select_exit_media: true,
        activate_focusing: true,
        activate_always: true,
    };
    assert_eq!(
        actions(&all, false),
        [Action::ActivateDebug],
        "no current media or destroyed owner has no regular exit receiver"
    );
}
