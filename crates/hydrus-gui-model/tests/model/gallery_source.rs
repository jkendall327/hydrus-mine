//! Replay actual Qt captions, choices and functionality from installed definitions.
use hydrus_core::url::{GugOptions, Gugs, UrlClassSettings};
use hydrus_gui_model::gallery_source::{self, Selector};
use hydrus_legacy::{
    objects::{domain, parsers},
    serialisable::SerialisableObject,
};
use hydrus_parse::Downloaders;
use serde_json::{Value, json};

fn object(value: &Value) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

fn definitions(f: &Value) -> (Downloaders, UrlClassSettings) {
    let parser = parsers::page_parser(&object(&f["parser"])).unwrap();
    let classes = UrlClassSettings {
        url_classes: f["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| domain::url_class(&object(c)).unwrap())
            .collect(),
        parser_links: vec![
            ("51".repeat(32), Some(parser.key.clone())),
            ("53".repeat(32), Some("63".repeat(32))),
        ],
        // Deliberately include a phantom key: only installed parsers count.
        parser_keys: vec![parser.key.clone(), "63".repeat(32)],
        ..Default::default()
    };
    (
        Downloaders {
            gugs: Gugs {
                gugs: f["gugs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|g| parsers::gug(&object(g)).unwrap())
                    .collect(),
                keys_to_display: serde_json::from_value(f["display"].clone()).unwrap(),
            },
            parsers: vec![parser],
            ..Default::default()
        },
        classes,
    )
}

#[test]
fn captions_and_installed_parser_functionality_match_real_reference() {
    let f = hydrus_testkit::fixture_json("gallery_source.json");
    let (downloaders, classes) = definitions(&f);
    for case in f["labels"].as_array().unwrap() {
        let current = gallery_source::resolve(
            &downloaders.gugs,
            serde_json::from_value(case["given"].clone()).unwrap(),
        );
        assert_eq!(json!(current), case["value"]);
        assert_eq!(
            gallery_source::caption(&downloaders.gugs, current.as_ref()),
            case["caption"]
        );
    }
    for (gug, case) in downloaders
        .gugs
        .gugs
        .iter()
        .zip(f["functional"].as_array().unwrap())
    {
        assert_eq!(gug.name(), case["name"]);
        assert_eq!(
            json!(
                gallery_source::check_functional(
                    gug,
                    &downloaders,
                    &classes,
                    GugOptions::default()
                )
                .err()
            ),
            case["error"],
            "{}",
            gug.name()
        );
    }
}

#[test]
fn two_stage_choices_selection_cancellation_and_empty_warning_match_reference() {
    let f = hydrus_testkit::fixture_json("gallery_source.json");
    let (downloaders, classes) = definitions(&f);
    for case in f["cases"].as_array().unwrap() {
        let empty = case["case"].as_str().unwrap().starts_with("empty");
        let definitions = if empty {
            Downloaders::default()
        } else {
            downloaders.clone()
        };
        let mut selector = Selector::new(
            &definitions,
            &classes,
            GugOptions::default(),
            Some(("4d".repeat(32), "beta hidden".into())),
            case["case"] == "empty subscription",
        );
        if empty {
            assert_eq!(json!([selector.warning.unwrap()]), case["warnings"]);
            continue;
        }
        let mut accepted = None;
        for dialog in case["dialogs"].as_array().unwrap() {
            assert_eq!(
                json!(
                    selector
                        .entries()
                        .iter()
                        .map(|e| &e.label)
                        .collect::<Vec<_>>()
                ),
                dialog["choices"]
            );
            assert_eq!(
                json!(
                    selector
                        .selected
                        .map(|i| vec![selector.entries()[i].label.clone()])
                        .unwrap_or_default()
                ),
                dialog["selected"]
            );
            if let Some(index) = dialog["answer"].as_u64() {
                selector.selected = Some(index as usize);
                accepted = selector.accept();
            }
        }
        assert_eq!(json!(accepted), case["value"]);
    }
}

#[test]
fn key_wins_over_conflicting_name_and_cycles_are_bounded() {
    let f = hydrus_testkit::fixture_json("gallery_source.json");
    let (mut downloaders, classes) = definitions(&f);
    assert_eq!(
        gallery_source::resolve(
            &downloaders.gugs,
            Some(("0c".repeat(32), "beta hidden".into()))
        ),
        Some(("0c".repeat(32), "Alpha visible".into()))
    );
    let hydrus_core::url::AnyGug::Nested(nested) = &mut downloaders.gugs.gugs[9] else {
        panic!("nested")
    };
    nested.gugs = vec![(nested.key.clone(), nested.name.clone())];
    assert_eq!(
        gallery_source::check_functional(
            &downloaders.gugs.gugs[9],
            &downloaders,
            &classes,
            GugOptions::default()
        ),
        Err("Unusual error: cyclic nested gallery".into())
    );
}
