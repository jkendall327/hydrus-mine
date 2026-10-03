//! A tag filter step's example test, as the reference's
//! (`oracle/fixtures/string_tag_filter_tests.json`, from
//! `oracle/dump_string_tag_filter_tests.py`): texts in any case and
//! spacing, namespaced or not, and invalid, through a few filters.

use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::url::strings::TagFilterStep;
use hydrus_gui_model::string_editors::{TagFilterStepEditor, tag_filter_test};
use serde_json::Value;

fn rules(value: &Value) -> TagFilter {
    let mut filter = TagFilter::new();
    for rule in value.as_array().unwrap() {
        filter.set_rule(
            rule[0].as_str().unwrap().to_owned(),
            if rule[1] == "black" {
                FilterRule::Blacklist
            } else {
                FilterRule::Whitelist
            },
        );
    }
    filter
}

#[test]
fn a_tag_filter_step_tests_its_example_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("string_tag_filter_tests.json");
    for case in recorded["cases"].as_array().unwrap() {
        let filter = rules(&case["rules"]);
        for result in case["results"].as_array().unwrap() {
            let text = result[0].as_str().unwrap();
            let expected = result[1].as_str();
            assert_eq!(
                tag_filter_test(&filter, text).err().as_deref(),
                expected,
                "{text} through {}",
                case["rules"]
            );
            // (the editor says so, and "ok" refuses a failing example)
            let editor = TagFilterStepEditor::new(
                &TagFilterStep::new(filter.clone()),
                Some(text.to_owned()),
            );
            let (said, ok) = editor.test_result();
            match expected {
                None => assert_eq!((said.as_str(), ok), ("Example matches ok!", true)),
                Some(why) => {
                    assert_eq!(said, format!("Example does not match - {why}"));
                    assert!(!ok);
                    assert_eq!(
                        editor.value().unwrap_err(),
                        "Please enter an example text that matches the given rules!"
                    );
                }
            }
        }
    }
}
