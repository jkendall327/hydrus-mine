//! Recorded real Python/native cross-roundtrips and bounded hostile transport.
use hydrus_downloader_exchange::{
    Definition, MAX_BYTES, Native, decode_png, decode_text, encode_png, encode_text,
};
use serde_json::{Value, json};
use std::io::Write;
fn reference() -> Value {
    hydrus_testkit::fixture_json("downloader_interchange.json")
}
#[test]
fn reference_json_and_real_png_roundtrip_without_losing_subsidiary_or_editor_data() {
    let fixture = reference();
    let text = fixture["reference"].to_string();
    let definitions = decode_text(&text).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&encode_text(&definitions).unwrap()).unwrap(),
        fixture["reference"]
    );
    let png =
        std::fs::read(hydrus_testkit::fixtures_dir().join("downloader_interchange.png")).unwrap();
    assert_eq!(decode_png(&png).unwrap(), definitions);
    assert_eq!(
        decode_png(&encode_png(&definitions).unwrap()).unwrap(),
        definitions
    );
    assert_eq!(
        fixture["native_exports_loaded_in_reference"],
        json!(["text", "png"])
    );
}
#[test]
fn older_formats_upgrade_or_reject_without_partial_packages() {
    for vector in reference()["old_versions"].as_array().unwrap() {
        let definitions = decode_text(&vector["source"].to_string()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&encode_text(&definitions).unwrap()).unwrap(),
            vector["upgraded"]
        );
    }
    let current = json!([136, 1, ["static", 2, "", [84, 1, [26, 3, []]]]]);
    let old = json!([133, 1, [current, current, [84, 1, [26, 3, []]]]]);
    let parsed = decode_text(&old.to_string()).unwrap();
    assert_eq!(parsed[0].tuple().unwrap()[1], 2);
    assert!(decode_text(&json!([26, 3, [[2, current], [2, [133, 999, []]]]]).to_string()).is_err());
    assert!(
        decode_text(&json!([27, 1, []]).to_string())
            .unwrap_err()
            .to_string()
            .contains("Update it in the reference client")
    );
    // The native legacy decoder does not retain unknown step data. Reject it
    // before it can enter a settings draft rather than dropping its payload.
    assert!(
        decode_text(
            &json!([136,1,["static",2,"",[84,1,[26,3,[[2,[999,1,{"essential":"keep"}]]]]]]])
                .to_string()
        )
        .is_err()
    );
}
fn png_pixels(pixels: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, width, height);
        e.set_color(png::ColorType::Grayscale);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(pixels).unwrap();
    }
    out
}
#[test]
fn malformed_png_and_compression_never_allocate_unbounded_data() {
    assert!(decode_png(b"not a PNG").is_err());
    assert!(decode_png(&png_pixels(&[0, 255, 0, 0], 2, 2)).is_err());
    let mut p = vec![0, 1, 0, 0, 0, 200, 0, 0];
    assert!(decode_png(&png_pixels(&p, 2, 4)).is_err());
    p[2..6].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(decode_png(&png_pixels(&p, 2, 4)).is_err());
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(&vec![b' '; MAX_BYTES + 1]).unwrap();
    let data = z.finish().unwrap();
    let mut pixels = vec![0, 1];
    pixels.extend_from_slice(&(data.len() as u32).to_be_bytes());
    pixels.extend_from_slice(&data);
    let rows = pixels.len().div_ceil(2);
    pixels.resize(rows * 2, 0);
    assert!(decode_png(&png_pixels(&pixels, 2, rows as u32)).is_err());
}
proptest::proptest! {
    #[test]
    fn authored_static_formulas_roundtrip(text in ".{0,100}",count in 0usize..10){
        let f=hydrus_parse::formula::Formula{
        reference_auxiliary: None,name:"authored".into(),kind:hydrus_parse::formula::FormulaKind::Static{text,count},processor:hydrus_core::url::StringProcessor::default()};
        let input=vec![Definition::new(Native::Formula(f.clone()))];let parsed=decode_text(&encode_text(&input).unwrap()).unwrap();let Native::Formula(parsed) = &parsed[0].native else { unreachable!() }; proptest::prop_assert_eq!(&parsed.kind,&f.kind); proptest::prop_assert_eq!(&parsed.processor,&f.processor); proptest::prop_assert!(parsed.reference_auxiliary.is_some());
    }
}

#[test]
fn formula_metadata_survives_native_serialization_and_edits_take_precedence() {
    let raw = json!([
        27,
        8,
        [
            [26, 3, []],
            1,
            "retained unused attribute",
            "original",
            [84, 1, [26, 3, []]]
        ]
    ]);
    let mut definition = decode_text(&raw.to_string()).unwrap().remove(0);
    let Native::Formula(formula) = &mut definition.native else {
        unreachable!()
    };
    formula.name = "edited".into();
    let stored = serde_json::to_string(formula).unwrap();
    let formula = serde_json::from_str::<hydrus_parse::formula::Formula>(&stored).unwrap();
    let mut definition = Definition::new(Native::Formula(formula));
    assert_eq!(
        definition.tuple().unwrap()[2][2],
        "retained unused attribute"
    );
    assert_eq!(definition.tuple().unwrap()[2][3], "edited");
    let Native::Formula(formula) = &mut definition.native else {
        unreachable!()
    };
    formula.kind = hydrus_parse::formula::FormulaKind::Html {
        rules: Vec::new(),
        content: hydrus_parse::formula::HtmlContent::Attribute("new native attribute".into()),
    };
    assert_eq!(definition.tuple().unwrap()[2][2], "new native attribute");
}

#[test]
fn old_plain_json_png_is_still_importable() {
    let text = br#"[136, 1, ["old", 1, "", [84, 1, [26, 3, []]]]]"#;
    let mut pixels = vec![0, 1];
    pixels.extend_from_slice(&(text.len() as u32).to_be_bytes());
    pixels.extend_from_slice(text);
    let rows = pixels.len().div_ceil(2);
    pixels.resize(rows * 2, 0);
    let imported = decode_png(&png_pixels(&pixels, 2, rows as u32)).unwrap();
    assert!(matches!(imported[0].native, Native::Formula(_)));
}

#[test]
fn malformed_old_versions_return_errors_before_any_package_can_be_staged() {
    for (kind, version, expected) in [
        (27, 7, 4),
        (31, 3, 3),
        (59, 2, 3),
        (60, 2, 2),
        (62, 1, 5),
        (133, 1, 3),
        (135, 1, 2),
    ] {
        for length in 0..expected {
            let malformed = json!([kind, version, vec![Value::Null; length]]);
            let result = std::panic::catch_unwind(|| decode_text(&malformed.to_string()));
            assert!(result.is_ok(), "panicked on {malformed}");
            assert!(result.unwrap().is_err(), "accepted {malformed}");
            let package = json!([
                26,
                3,
                [
                    [2, [136, 1, ["valid", 1, "", [84, 1, [26, 3, []]]]]],
                    [2, malformed]
                ]
            ]);
            assert!(decode_text(&package.to_string()).is_err());
        }
    }
}
#[test]
fn subsidiary_contexts_follow_stable_page_keys_when_children_are_sorted_and_renamed() {
    let fixture = reference();
    let mut raw = fixture["reference"][2]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v[1].clone())
        .find(|v| v[0] == json!(58) && v[1] == json!("exchange page"))
        .unwrap();
    let first = raw[3][3][2][0].clone();
    let mut second = first.clone();
    raw[3][3][2][0][1][2][2][1] = json!("z last");
    raw[3][3][2][0][1][2][2][3][0] = json!("z last");
    raw[3][3][2][0][1][2][2][3][6] = json!({"token":"context z"});
    second[1][2][2][1] = json!("a first");
    second[1][2][2][3][0] = json!("a first");
    second[1][2][2][3][1] = json!("99".repeat(32));
    second[1][2][2][3][6] = json!({"token":"context a"});
    raw[3][3][2].as_array_mut().unwrap().push(second);
    let mut definitions = decode_text(&raw.to_string()).unwrap();
    let Native::Page(page) = &mut definitions[0].native else {
        unreachable!()
    };
    page.subsidiary[0].parser.name = "renamed z".into();
    let exported = definitions[0].tuple().unwrap();
    let children = exported[3][3][2].as_array().unwrap();
    assert_eq!(children[0][1][2][2][1], "a first");
    assert_eq!(children[0][1][2][2][3][6]["token"], "context a");
    assert_eq!(children[1][1][2][2][1], "renamed z");
    assert_eq!(children[1][1][2][2][3][6]["token"], "context z");
}

#[test]
fn maximum_documented_definition_count_roundtrips_and_the_next_one_is_rejected() {
    let definition = decode_text(r#"[136, 1, ["limit", 1, "", [84, 1, [26, 3, []]]]]"#)
        .unwrap()
        .remove(0);
    let definitions = vec![definition; hydrus_downloader_exchange::MAX_OBJECTS];
    let text = encode_text(&definitions).unwrap();
    assert_eq!(
        decode_text(&text).unwrap().len(),
        hydrus_downloader_exchange::MAX_OBJECTS
    );
    let mut oversized: Value = serde_json::from_str(&text).unwrap();
    let extra = oversized[2][0].clone();
    oversized[2].as_array_mut().unwrap().push(extra);
    assert!(decode_text(&oversized.to_string()).is_err());
}
