//! Actual list permitted-type filtering stays separate from strict exchange decoding.
use hydrus_downloader_exchange::{Error, subscription_import as import, subscriptions as exchange};
use serde_json::json;

#[test]
fn recorded_nested_packages_warn_after_preserving_complete_permitted_objects() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_flow.json");
    let now = fixture["now"].as_i64().unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let mixed = cases[0]["sources"]["mixed"].as_str().unwrap();
    let package = import::decode_text_at(mixed, now).unwrap();
    assert_eq!(
        package
            .subscriptions
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["Artist", "Second"]
    );
    assert_eq!(
        package
            .other_types
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["CheckerOptions", "str"]
    );
    let warning = package.warning().unwrap();
    let reference_warning = cases[0]["messages"][0]["text"].as_str().unwrap();
    let mut actual_types = warning
        .split("\n\n")
        .nth(1)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    let mut expected_types = reference_warning
        .split("\n\n")
        .nth(1)
        .unwrap()
        .lines()
        .collect::<Vec<_>>();
    actual_types.sort_unstable();
    expected_types.sort_unstable();
    assert_eq!(actual_types, expected_types);
    assert_eq!(
        warning.split("\n\n").skip(2).collect::<Vec<_>>(),
        reference_warning.split("\n\n").skip(2).collect::<Vec<_>>()
    );
    let expected = exchange::decode_text_at(
        &json!([
            26,
            3,
            cases[0]["exported"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| json!([2, s]))
                .collect::<Vec<_>>()
        ])
        .to_string(),
        now,
    )
    .unwrap();
    let mut actual = package.subscriptions.clone();
    for (actual, expected) in actual.iter_mut().zip(&expected) {
        for (actual, expected) in actual.queries.iter_mut().zip(&expected.queries) {
            exchange::rename_history(actual, expected.log_name.clone());
        }
    }
    assert_eq!(actual, expected);
    // Filtering belongs to the list control. The generic exchange API stays strict.
    assert!(exchange::decode_text_at(mixed, now).is_err());
    let png = hydrus_downloader_exchange::text_png::encode(mixed, 512, &vec![255; 512]).unwrap();
    assert_eq!(import::decode_png_at(&png, now).unwrap(), package);
    let wrong =
        import::decode_text_at(cases[1]["sources"]["wrong"].as_str().unwrap(), now).unwrap();
    assert!(wrong.subscriptions.is_empty());
    assert_eq!(
        wrong.other_types.into_iter().collect::<Vec<_>>(),
        ["CheckerOptions"]
    );
}

#[test]
fn future_nested_objects_and_limits_fail_before_any_list_objects_are_returned() {
    let fixture = hydrus_testkit::fixture_json("subscription_import_flow.json");
    let cases = fixture["cases"].as_array().unwrap();
    let source = &cases[4]["sources"];
    let good: serde_json::Value = serde_json::from_str(source["a"].as_str().unwrap()).unwrap();
    let future: serde_json::Value =
        serde_json::from_str(source["future"].as_str().unwrap()).unwrap();
    let nested = json!([26, 3, [[2, good], [2, [26, 3, [[2, future]]]]]]);
    assert!(matches!(
        import::decode_text_at(&nested.to_string(), 1_700_000_000),
        Err(Error::Unsupported(_))
    ));
    assert!(import::decode_text_at("[99999, 1, []]", 0).is_err());
    assert!(import::decode_text_at("not json", 0).is_err());
    assert!(matches!(
        import::decode_text_at(&" ".repeat(hydrus_downloader_exchange::MAX_BYTES + 1), 0),
        Err(Error::Limit)
    ));
    let too_many = json!([
        26,
        3,
        vec![json!([0, "wrong type"]); hydrus_downloader_exchange::MAX_OBJECTS]
    ]);
    assert!(matches!(
        import::decode_text_at(&too_many.to_string(), 0),
        Err(Error::Limit)
    ));
}
