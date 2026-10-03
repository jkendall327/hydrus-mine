//! Tag summary generators against `oracle/fixtures/tag_summaries.json`
//! (made by `oracle/dump_tag_summaries.py`): the reference's defaults and
//! random generators read as the reference reads them, and each making the
//! reference's summary of each set of tags (shown as the reference's default
//! tag presentation shows them, and with underscores shown as spaces).

use serde_json::{Value as Json, json};

use hydrus_core::tag_presentation::TagPresentation;
use hydrus_core::tag_summary::TagSummaryGenerator;
use hydrus_legacy::objects::tag_summary::tag_summary_generator;

mod common;
use common::seeds::object;

fn facts(g: &TagSummaryGenerator) -> Json {
    json!({
        "background": g.background,
        "text": g.text,
        "namespace_info": g
            .namespace_info
            .iter()
            .map(|i| json!([i.namespace, i.prefix, i.separator]))
            .collect::<Vec<_>>(),
        "separator": g.separator,
        "example_tags": g.example_tags,
        "show": g.show,
    })
}

#[test]
fn generators_read_and_summarise_as_the_reference_s() {
    let recorded = hydrus_testkit::fixture_json("tag_summaries.json");
    let presentation = TagPresentation::default();
    let render = |subtag: &str| presentation.render(subtag);
    let spaced = TagPresentation {
        replace_underscores: true,
        ..TagPresentation::default()
    };
    let render_spaced = |subtag: &str| spaced.render(subtag);
    let tag_sets: Vec<Vec<String>> = recorded["tag_sets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|set| {
            set.as_array()
                .unwrap()
                .iter()
                .map(|t| t.as_str().unwrap().to_owned())
                .collect()
        })
        .collect();
    let mut made_something = false;
    for case in recorded["generators"].as_array().unwrap() {
        let name = &case["name"];
        let g = tag_summary_generator(&object(&case["stored"])).unwrap();
        assert_eq!(facts(&g), case["facts"], "{name}");
        let ours: Vec<String> = tag_sets
            .iter()
            .map(|tags| g.summary(tags.iter().map(String::as_str), render))
            .collect();
        assert_eq!(json!(ours), case["summaries"], "{name}");
        let ours: Vec<String> = tag_sets
            .iter()
            .map(|tags| g.summary(tags.iter().map(String::as_str), render_spaced))
            .collect();
        assert_eq!(
            json!(ours),
            case["summaries_underscores_replaced"],
            "{name}"
        );
        made_something |= ours.iter().any(|s| !s.is_empty());
    }
    assert!(made_something);
    // the reference's defaults are ours
    for (name, ours) in [
        ("thumbnail_top", TagSummaryGenerator::thumbnail_top()),
        (
            "thumbnail_bottom_right",
            TagSummaryGenerator::thumbnail_bottom_right(),
        ),
        ("media_viewer_top", TagSummaryGenerator::media_viewer_top()),
    ] {
        let case = recorded["generators"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let theirs = tag_summary_generator(&object(&case["stored"])).unwrap();
        // (the example tags are stored in no particular order)
        let sorted = |g: &TagSummaryGenerator| {
            let mut g = g.clone();
            g.example_tags.sort();
            g
        };
        assert_eq!(sorted(&ours), sorted(&theirs), "{name}");
    }
}
