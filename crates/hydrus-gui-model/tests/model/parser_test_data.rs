//! Real Qt raw preview strings, Unicode clipping and byte-based media detection.
use hydrus_gui_model::parser_test_data as model;
#[test]
fn recorded_raw_data_previews_match_without_changing_the_parser_document() {
    let reference = hydrus_testkit::fixture_json("parser_raw_preview.json");
    let png = hex::decode(reference["png_hex"].as_str().unwrap()).unwrap();
    for case in reference["states"].as_array().unwrap() {
        if case["input"].is_null() || case["case"] == "paste_error" {
            continue;
        }
        let input = &case["input"];
        let text = if let Some(repeat) = input["repeat"].as_str() {
            repeat.repeat(usize::try_from(input["count"].as_u64().unwrap()).unwrap())
        } else {
            input["text"].as_str().unwrap().to_owned()
        };
        let mime = if input["mime"] == "png" {
            model::detect_mime(&text, &png)
        } else {
            None
        };
        let preview = model::preview(&text, mime);
        assert_eq!(preview.description, case["description"], "{}", case["case"]);
        assert_eq!(
            preview.parse_enabled,
            case["parse_enabled"].as_bool().unwrap()
        );
        assert_eq!(
            preview.text.chars().count(),
            usize::try_from(case["preview_length"].as_u64().unwrap()).unwrap()
        );
        if let Some(text) = case["preview"].as_str() {
            assert_eq!(preview.text, text);
        }
        assert_eq!(
            preview.text.chars().take(100).collect::<String>(),
            case["preview_start"]
        );
    }
    assert!(model::detect_mime("{}", &png).is_none());
    assert!(model::detect_mime("<html>document</html>", &png).is_none());
}
#[test]
fn fetched_media_detection_tracks_example_selection_and_clears_on_text_changes() {
    let mut mimes = model::ExampleMimes::default();
    mimes.remember(2, "binary", Some(hydrus_core::Mime::ImagePng));
    assert_eq!(mimes.get(2, "binary"), Some(hydrus_core::Mime::ImagePng));
    assert!(mimes.get(0, "binary").is_none());
    assert!(mimes.get(2, "edited").is_none());
    mimes.remove(0);
    assert_eq!(mimes.get(1, "binary"), Some(hydrus_core::Mime::ImagePng));
    mimes.remember(1, "new text", None);
    assert!(mimes.get(1, "new text").is_none());
}
