//! Full subscription exchange is proved against the actual Qt list exports.
use hydrus_downloader_exchange::subscriptions as exchange;
use serde_json::{Value, json};

#[test]
fn actual_subscription_clipboard_and_png_retain_settings_headers_and_both_histories() {
    let reference = hydrus_testkit::fixture_json("subscription_exchange.json");
    let subscriptions = exchange::decode_text(&reference["bundle"].to_string()).unwrap();
    assert_eq!(subscriptions.len(), 2);
    assert_eq!(subscriptions[0].name, "Artist");
    assert_eq!(subscriptions[1].name, "Artist (1)");
    assert_ne!(
        subscriptions[0].queries[0].log_name,
        subscriptions[1].queries[0].log_name
    );
    let query = &subscriptions[0].queries[0];
    let log = query.log.as_ref().unwrap();
    assert_eq!(log.file_seeds[0].note, "ignored\nrecorded reason");
    assert_eq!(
        log.file_seeds[0].notes,
        [("note".into(), "first\n\nsecond".into())]
    );
    assert_eq!(
        log.file_seeds[0].hashes,
        [("sha256".into(), "33".repeat(32))]
    );
    assert_eq!(log.gallery_seeds[0].note, "gallery failure");
    assert_eq!(query.state.display_name.as_deref(), Some("blue artist"));
    assert_eq!(
        exchange::tuple(&subscriptions[0]).unwrap(),
        reference["single"]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&exchange::encode_text(&subscriptions).unwrap()).unwrap(),
        reference["bundle"]
    );
    let png =
        std::fs::read(hydrus_testkit::fixtures_dir().join("subscription_exchange.png")).unwrap();
    assert_eq!(exchange::decode_png(&png).unwrap(), subscriptions);
    assert_eq!(
        exchange::decode_png(&exchange::encode_png(&subscriptions).unwrap()).unwrap(),
        subscriptions
    );
    assert_eq!(reference["png_loaded"], reference["bundle"]);
    // Editing native state updates its actual reference fields without replacing caches/history.
    let mut edited = subscriptions[0].clone();
    edited.settings.publish_label_override = Some("edited label".into());
    edited.queries[0].state.query_text = "new search".into();
    let encoded = exchange::tuple(&edited).unwrap();
    assert_eq!(encoded[2][0][3][13], "edited label");
    assert_eq!(encoded[2][0][3][1][0][2][1], "new search");
    assert_eq!(encoded[2][1], reference["single"][2][1]);
    assert_eq!(
        encoded[2][0][3][1][0][2][15],
        reference["single"][2][0][3][1][0][2][15]
    );
}

#[test]
fn subscription_exchange_reports_missing_logs_and_rejects_invalid_packages_atomically() {
    let valid = hydrus_testkit::fixture_json("subscription_exchange.json")["single"].clone();
    let mut missing = valid.clone();
    missing[2][1] = json!([26, 3, []]);
    let decoded = exchange::decode_text(&missing.to_string()).unwrap();
    assert!(decoded[0].queries[0].log.is_none());
    let mut future = valid.clone();
    future[1] = json!(99);
    assert!(exchange::decode_text(&future.to_string()).is_err());
    future = valid.clone();
    future[2][0][2] = json!(99);
    assert!(exchange::decode_text(&future.to_string()).is_err());
    let wrong = valid[2][0].clone();
    assert!(exchange::decode_text(&wrong.to_string()).is_err());
    assert!(exchange::decode_text(&json!([26, 3, [[2, valid], [2, wrong]]]).to_string()).is_err());
    assert!(exchange::decode_text("not JSON").is_err());
    assert!(exchange::decode_text(&" ".repeat(hydrus_downloader_exchange::MAX_BYTES + 1)).is_err());
    assert!(exchange::encode_text(&[]).is_err());
    assert!(exchange::decode_png(b"not PNG").is_err());
}
