//! Reference containers retain all eight kinds through clipboard and PNG exchange.
use hydrus_downloader_exchange::import_options;
use serde_json::{Value, json};

#[test]
fn recorded_containers_round_trip_all_native_fields_and_pngs() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    for original in fixture["tuples"].as_array().unwrap() {
        let slice = import_options::decode_text(&original.to_string()).unwrap();
        assert_eq!(import_options::tuple(&slice).unwrap(), *original);
        assert_eq!(
            serde_json::from_str::<Value>(&import_options::encode_text(&slice).unwrap()).unwrap(),
            *original
        );
        assert_eq!(
            import_options::decode_png(&import_options::encode_png(&slice).unwrap()).unwrap(),
            slice
        );
    }
}

#[test]
fn wrong_objects_future_versions_and_unrepresentable_locations_are_rejected() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_options.json");
    assert!(import_options::decode_text("[26,3,[]]").is_err());
    assert!(import_options::decode_text("not json").is_err());
    let mut future = fixture["tuples"][0].clone();
    future[1] = json!(999);
    assert!(import_options::decode_text(&future.to_string()).is_err());
    let mut deleted = fixture["tuples"][0].clone();
    deleted[2][2][3][1][1][2][0][2][1] = json!(["aa"]);
    assert!(import_options::decode_text(&deleted.to_string()).is_err());
}
