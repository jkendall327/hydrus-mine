//! Recorded standalone wrapper packages retain recursive runtime and editor data.
use hydrus_downloader_exchange::subsidiaries as exchange;
use hydrus_parse::{content::PageParser, formula::ParsingContext};
use serde_json::{Value, json};
#[test]
fn real_reference_subsidiary_clipboard_and_png_preserve_recursive_wrappers() {
    let reference = hydrus_testkit::fixture_json("subsidiary_exchange.json");
    let parsers = exchange::decode_text(&reference["bundle"].to_string()).unwrap();
    assert_eq!(parsers.len(), 2);
    assert!(parsers[0].sort_by_source_time);
    assert_eq!(parsers[0].parser.subsidiary.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&exchange::encode_text(&parsers).unwrap()).unwrap(),
        reference["bundle"]
    );
    assert_eq!(exchange::tuple(&parsers[0]).unwrap(), reference["single"]);
    let png =
        std::fs::read(hydrus_testkit::fixtures_dir().join("subsidiary_exchange.png")).unwrap();
    assert_eq!(exchange::decode_png(&png).unwrap(), parsers);
    assert_eq!(
        exchange::decode_png(&exchange::encode_png(&parsers).unwrap()).unwrap(),
        parsers
    );
    assert_eq!(reference["png_loaded"], reference["bundle"]);
    let mut old = reference["single"].clone();
    old[1] = json!(1);
    old[2].as_array_mut().unwrap().remove(1);
    let upgraded = exchange::decode_text(&old.to_string()).unwrap();
    assert!(!upgraded[0].sort_by_source_time);
    let page = PageParser {
        reference_auxiliary: None,
        name: "runtime".into(),
        key: "44".repeat(32),
        converter: hydrus_core::url::strings::StringConverter::default(),
        subsidiary: parsers,
        content_parsers: Vec::new(),
        example_urls: Vec::new(),
    };
    let parsed = page
        .parse(&mut ParsingContext::new(), "parent document")
        .unwrap();
    let texts = parsed
        .iter()
        .map(|post| {
            post.contents
                .iter()
                .map(|content| content.text.clone())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(json!(texts), reference["parsed"]);
}
#[test]
fn incompatible_or_unsupported_subsidiary_packages_are_rejected_before_staging() {
    let valid = hydrus_testkit::fixture_json("subsidiary_exchange.json")["single"].clone();
    let wrong = json!([136, 1, ["static", 1, "", [84, 1, [26, 3, []]]]]);
    assert!(exchange::decode_text(&json!([26, 3, [[2, valid], [2, wrong]]]).to_string()).is_err());
    let mut unsafe_parser = valid.clone();
    unsafe_parser[2][0][2][3] = json!([84,1,[26,3,[[2,[999,1,{"essential":"preserve"}]]]]]);
    assert!(exchange::decode_text(&unsafe_parser.to_string()).is_err());
    assert!(exchange::decode_text(&" ".repeat(hydrus_downloader_exchange::MAX_BYTES + 1)).is_err());
    assert!(exchange::decode_png(b"not a PNG").is_err());
}
