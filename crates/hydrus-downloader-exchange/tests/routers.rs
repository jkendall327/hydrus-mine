//! Recorded metadata migration queues preserve every source and destination.
use hydrus_downloader_exchange::routers as exchange;
use serde_json::{Value, json};

#[test]
fn recorded_router_queues_preserve_editor_fields_and_reference_png() {
    let reference = hydrus_testkit::fixture_json("router_exchange.json");
    for queue in reference["queues"].as_array().unwrap() {
        let routers = exchange::decode_text(&queue["text"].to_string()).unwrap();
        assert_eq!(
            routers.len(),
            usize::try_from(queue["count"].as_u64().unwrap()).unwrap()
        );
        assert_eq!(
            serde_json::from_str::<Value>(&exchange::encode_text(&routers).unwrap()).unwrap(),
            queue["text"]
        );
    }
    let routers = exchange::decode_text(&reference["png_loaded"].to_string()).unwrap();
    assert_eq!(routers.len(), 6);
    let png = std::fs::read(hydrus_testkit::fixtures_dir().join("router_exchange.png")).unwrap();
    assert_eq!(exchange::decode_png(&png).unwrap(), routers);
    assert_eq!(
        exchange::decode_png(&exchange::encode_png(&routers).unwrap()).unwrap(),
        routers
    );
    let upgraded = exchange::decode_text(&reference["old"]["source"].to_string()).unwrap();
    assert_eq!(
        exchange::tuple(&upgraded[0]).unwrap(),
        reference["old"]["upgraded"]
    );
}

#[test]
fn router_exchange_rejects_incompatible_or_lossy_packages_atomically() {
    let reference = hydrus_testkit::fixture_json("router_exchange.json");
    let valid = reference["exports"][1].clone();
    assert!(
        exchange::decode_text(
            &json!([
                26,
                3,
                [
                    [2, valid],
                    [2, [136, 1, ["x", 1, "", [84, 1, [26, 3, []]]]]]
                ]
            ])
            .to_string()
        )
        .is_err()
    );
    let mut unknown = valid.clone();
    unknown[2][1] = json!([84,1,[26,3,[[2,[999,1,{"essential":"preserve"}]]]]]);
    assert!(exchange::decode_text(&unknown.to_string()).is_err());
    let mut timestamp = reference["imports"][3].clone();
    timestamp[2][2][2][2][2] = json!(123_456_789);
    assert!(exchange::decode_text(&timestamp.to_string()).is_err());
    let mut destination = reference["imports"][0].clone();
    destination[2][2][2] = json!(["unknown editor field"]);
    assert!(exchange::decode_text(&destination.to_string()).is_err());
    assert!(exchange::decode_text(&" ".repeat(hydrus_downloader_exchange::MAX_BYTES + 1)).is_err());
    assert!(exchange::encode_text(&[]).is_err());
    assert!(exchange::decode_png(b"not a PNG").is_err());
}

#[test]
fn inspected_packages_preserve_routers_with_recognised_other_objects() {
    let reference = hydrus_testkit::fixture_json("router_import.json");
    for case in reference["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| !case["text"].is_null())
    {
        let report = exchange::inspect_text(&case["text"].to_string()).unwrap();
        let added = case["added"].as_array().unwrap();
        for expected in added {
            assert!(
                report
                    .routers
                    .iter()
                    .any(|router| exchange::tuple(router).unwrap() == *expected)
            );
        }
        if case["name"] == "mixed" || case["name"] == "wrong_only" {
            assert_eq!(
                report.other_types.into_iter().collect::<Vec<_>>(),
                ["StringMatch"]
            );
            // The strict codec still protects every existing atomic exchange caller.
            assert!(exchange::decode_text(&case["text"].to_string()).is_err());
        }
    }
    let first =
        std::fs::read(hydrus_testkit::fixtures_dir().join("router_import_first.png")).unwrap();
    let report = exchange::inspect_png(&first).unwrap();
    assert_eq!(report.routers.len(), 1);
    let broken =
        std::fs::read(hydrus_testkit::fixtures_dir().join("router_import_broken.png")).unwrap();
    assert!(exchange::inspect_png(&broken).is_err());
    let unknown = json!([
        26,
        3,
        [[2, [999, 1, []]], [2, reference["cases"][0]["added"][0]]]
    ]);
    assert!(exchange::inspect_text(&unknown.to_string()).is_err());
    assert!(
        exchange::inspect_text(&" ".repeat(hydrus_downloader_exchange::MAX_BYTES + 1)).is_err()
    );
}
