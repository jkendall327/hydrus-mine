use hydrus_downloader_exchange::external_calls as exchange;
use serde_json::{Value, json};

#[test]
fn actual_qt_callable_exports_and_manager_round_trip_without_losing_identity_or_process_options() {
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    let text = reference["export"].as_str().unwrap();
    let calls = exchange::decode_text(text).unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(&exchange::encode_text(&calls).unwrap()).unwrap(),
        serde_json::from_str::<Value>(text).unwrap()
    );
    let manager = exchange::decode_manager(&reference["saved_manager"].to_string()).unwrap();
    assert_eq!(
        manager
            .calls
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>(),
        reference["reopened_names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    for default in reference["defaults"].as_array().unwrap() {
        let calls = exchange::decode_text(&default.to_string()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&exchange::encode_text(&calls).unwrap()).unwrap(),
            *default
        );
    }
}
#[test]
fn import_rejects_wrong_classes_and_mixed_invalid_packages_before_staging_any_call() {
    let reference = hydrus_testkit::fixture_json("external_calls.json");
    let single = reference["defaults"][0].clone();
    assert!(
        exchange::decode_text(
            &json!([26, 3, [[2, single.clone()], [2, [157, 1, []]]]]).to_string()
        )
        .is_err()
    );
    let mut newer = single.clone();
    newer[2] = json!(2);
    assert!(exchange::decode_text(&newer.to_string()).is_err());
    let mut key = single.clone();
    key[3][0] = json!("bad");
    assert!(exchange::decode_text(&key.to_string()).is_err());
    let mut pipeline = single;
    pipeline[3][1] = json!(999);
    assert!(exchange::decode_text(&pipeline.to_string()).is_err());
}
