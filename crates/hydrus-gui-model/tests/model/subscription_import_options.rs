//! Actual subscription menu bindings and clear decisions replayed from Qt.
use hydrus_core::subscriptions::SubscriptionSettings;
use hydrus_downloader_exchange::import_options;
use hydrus_gui_model::subscriptions_dialog::{ImportOptionsPaste, Subscriptions};
use serde_json::json;

#[test]
fn subscription_paste_modes_and_clear_match_recorded_reference_outputs() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    let existing = import_options::decode_text(&fixture["existing"].to_string()).unwrap();
    let incoming = import_options::decode_text(&fixture["incoming"].to_string()).unwrap();
    let mut dialog = Subscriptions::new(
        ["alpha", "beta"]
            .map(|name| {
                (
                    None,
                    name.into(),
                    SubscriptionSettings {
                        import_options: existing.clone(),
                        ..SubscriptionSettings::default()
                    },
                    Vec::new(),
                )
            })
            .to_vec(),
    );
    let keys = dialog.order(0);
    dialog.selection.select_many(&keys);
    assert_eq!(
        json!(dialog.clear_import_options_question(0).unwrap()),
        fixture["questions"][0]
    );
    for (index, step) in fixture["steps"]
        .as_array()
        .unwrap()
        .iter()
        .take(3)
        .enumerate()
    {
        for subscription in &mut dialog.subscriptions {
            subscription.settings.import_options = existing.clone();
        }
        dialog.paste_import_options(
            &keys,
            ImportOptionsPaste::from_menu_index(index).unwrap(),
            &incoming,
        );
        for (subscription, row) in dialog
            .subscriptions
            .iter()
            .zip(step["rows"].as_array().unwrap())
        {
            assert_eq!(subscription.name, row["name"]);
            assert_eq!(
                import_options::tuple(&subscription.settings.import_options).unwrap(),
                row["options"]
            );
        }
    }
    dialog.clear_import_options(&keys[..1]);
    assert!(dialog.subscriptions[0].settings.import_options.is_empty());
    assert!(!dialog.subscriptions[1].settings.import_options.is_empty());
    // The prompt snapshots keys: changing selection must not change its target.
    dialog.selection.select_many(&keys[1..]);
    dialog.clear_import_options(&keys);
    assert!(
        dialog
            .subscriptions
            .iter()
            .all(|s| s.settings.import_options.is_empty())
    );
    assert!(ImportOptionsPaste::from_menu_index(9).is_none());
}
