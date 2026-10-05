//! Actual Qt cache upgrades survive direct legacy and modern-log exchange.
use hydrus_downloader_exchange::subscriptions as exchange;
use serde_json::{Value, json};

#[test]
fn historical_cache_versions_replay_exact_qt_history_headers_and_list_exports() {
    let fixture = hydrus_testkit::fixture_json("legacy_seed_caches.json");
    let now = fixture["now"].as_i64().unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut imported = exchange::decode_text_at(&case["source"].to_string(), now).unwrap();
        let mut actual = exchange::tuple(&imported[0]).unwrap();
        let expected = &case["direct"];
        // The only nondeterministic fields are the generated log identities.
        actual[2][0][3][1][0][2][0] = expected[2][0][3][1][0][2][0].clone();
        actual[2][1][2][0][1][1] = expected[2][1][2][0][1][1].clone();
        assert_eq!(actual, *expected, "cache version {}", case["version"]);
        assert_eq!(actual[2][1][2][0][1][3][1], case["upgraded_cache"]);
        let exported = &case["exported"];
        exchange::rename_history(
            &mut imported[0].queries[0],
            exported[2][0][3][1][0][2][0].as_str().unwrap().into(),
        );
        assert_eq!(exchange::tuple(&imported[0]).unwrap(), *exported);
        // Historical caches can also occur inside a current container/log.
        let mut modern = exported.clone();
        modern[2][1][2][0][1][3][1] = case["source"][3][1][0][2][8].clone();
        let modern = exchange::decode_text_at(&modern.to_string(), now).unwrap();
        assert_eq!(exchange::tuple(&modern[0]).unwrap(), *exported);
        let log = exchange::decode_log(&actual[2][1][2][0][1]).unwrap();
        let png = exchange::encode_png(&imported).unwrap();
        assert_eq!(exchange::decode_png(&png).unwrap(), imported);
        assert_eq!(log.file_seeds.len(), 4);
        assert_eq!(
            log.file_seeds[0].note,
            if case["version"] == 1 { "42" } else { "first" }
        );
        assert_eq!(
            log.file_seeds[1].data,
            "https://legacy.example/pending%20space"
        );
        assert_eq!(log.file_seeds[2].seed_type, 0);
        assert_eq!(log.file_seeds[2].note, "local veto");
    }
    for case in fixture["rewrites"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["notes"].as_array().unwrap())
    {
        let log = json!([86, "history", 1, [[67, 1, [26, 3, []]], case["source"]]]);
        let decoded = exchange::decode_log(&log);
        if case["safe"] == false {
            assert!(
                decoded
                    .unwrap_err()
                    .to_string()
                    .contains("Python string conversion")
            );
        } else {
            let upgraded = exchange::log_tuple(&decoded.unwrap());
            assert_eq!(upgraded[3][1], case["upgraded"]);
        }
    }
}

#[test]
fn unsafe_or_malformed_history_fails_explicitly_without_truncating_a_package() {
    let fixture = hydrus_testkit::fixture_json("legacy_seed_caches.json");
    for case in fixture["cases"].as_array().unwrap() {
        if case["version"].as_u64().unwrap() >= 5 {
            let mut source = case["source"].clone();
            source[3][1][0][2][8] = case["duplicate_cache"].clone();
            let error = exchange::decode_text(&source.to_string())
                .unwrap_err()
                .to_string();
            assert!(error.contains("duplicate identities"), "{error}");
            assert_eq!(
                case["duplicate_upgraded"][2][2].as_array().unwrap().len(),
                5
            );
        }
    }
    for failure in fixture["failures"].as_array().unwrap() {
        let mut source = fixture["cases"][0]["source"].clone();
        source[3][1][0][2][8] = failure["source"].clone();
        assert!(!failure["error"].as_str().unwrap().is_empty());
        // An earlier valid object does not make a later broken history disappear.
        let package = json!([26, 1, [fixture["cases"][0]["source"], source]]);
        assert!(exchange::decode_text(&package.to_string()).is_err());
    }
    for note in [json!(1.5), json!(["complex"]), json!({"complex": true})] {
        let mut source = fixture["cases"][0]["source"].clone();
        source[3][1][0][2][8][2][0][1]["note"] = note;
        assert!(
            exchange::decode_text(&source.to_string())
                .unwrap_err()
                .to_string()
                .contains("Python string conversion")
        );
    }
    let mut source = fixture["cases"][0]["source"].clone();
    source[3][1][0][2][8] = Value::Null;
    assert!(exchange::decode_text(&source.to_string()).is_err());
}
