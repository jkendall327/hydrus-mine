//! The tag filter editor's live test box with real siblings in the store,
//! replayed from `oracle/record_tag_filter_sibling_testing.py`.
use std::rc::Rc;

use crate::options_gui_support::Client;
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::RelationKind;

// leaf: audit-shared-tag-testing
#[test]
fn blacklist_tests_check_every_sibling_on_every_tag_service_as_the_reference_does() {
    let recording = hydrus_testkit::fixture_json("tag_filter_sibling_testing.json");
    let client = Client::basic();
    for pair in recording["siblings"].as_array().unwrap() {
        let service = client
            .store
            .snapshot()
            .services
            .by_name(pair[0].as_str().unwrap())
            .unwrap()
            .id;
        tag_relations::apply(
            &client.store,
            RelationKind::Siblings,
            vec![RelationUpdate {
                service,
                left: hydrus_core::Tag::new(pair[1].as_str().unwrap()).unwrap(),
                right: hydrus_core::Tag::new(pair[2].as_str().unwrap()).unwrap(),
                action: RelationAction::Add,
            }],
        )
        .unwrap();
    }
    let slot = hydrus_gui::tag_filter_window::Slot::default();
    for case in recording["cases"].as_array().unwrap() {
        let mut filter = TagFilter::new();
        for rule in case["blacklist"].as_array().unwrap() {
            filter.set_rule(rule.as_str().unwrap(), FilterRule::Blacklist);
        }
        let blacklist_only = case["blacklist_only"].as_bool().unwrap();
        let w = hydrus_gui::tag_filter_window::open(
            &client.store,
            &filter,
            blacklist_only,
            "edit tag filter",
            "",
            &slot,
            Rc::new(|_| {}),
        )
        .unwrap();
        for test in case["tests"].as_array().unwrap() {
            w.set_test_input(test["text"].as_str().unwrap().into());
            w.invoke_test_edited();
            assert_eq!(
                w.get_test_result(),
                test["result"].as_str().unwrap(),
                "{case:?} {test}"
            );
            let good = match test["style"].as_str().unwrap() {
                "HydrusValid" => 1,
                "HydrusInvalid" => -1,
                _ => 0,
            };
            assert_eq!(w.get_test_good(), good, "{test}");
        }
        w.set_test_input("".into());
        w.invoke_test_edited();
        assert_eq!(w.get_test_result(), case["empty"].as_str().unwrap());
        assert_eq!(w.get_test_good(), 0);
        w.invoke_cancel();
    }
}
