//! The tag filter editor, step by step as the reference's
//! (`oracle/record_tag_filter_editor.py`): its tabs, what each shows of the
//! filter, what it says of entries already covered, the current filter and
//! the test box.

use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::tag_filter_editor::{SimpleView, TagFilterEditor};
use serde_json::Value;

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn bools(value: &Value) -> Vec<bool> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_bool().unwrap())
        .collect()
}

fn rules(value: &Value) -> TagFilter {
    let mut filter = TagFilter::new();
    for rule in value.as_array().unwrap() {
        let rule = strings(rule);
        filter.set_rule(
            rule[0].clone(),
            if rule[1] == "black" {
                FilterRule::Blacklist
            } else {
                FilterRule::Whitelist
            },
        );
    }
    filter
}

fn check_simple(ours: &SimpleView, theirs: &Value, at: &str) {
    assert_eq!(ours.enabled, theirs["enabled"].as_bool().unwrap(), "{at}");
    assert_eq!(ours.error, theirs["error"].as_str().unwrap(), "{at}");
    assert_eq!(ours.list, strings(&theirs["list"]), "{at}");
    assert_eq!(ours.global.to_vec(), bools(&theirs["global"]), "{at}");
    assert_eq!(ours.namespaces, bools(&theirs["namespaces"]), "{at}");
    assert_eq!(
        ours.namespaces_enabled,
        theirs["namespaces_enabled"].as_bool().unwrap(),
        "{at}"
    );
}

#[test]
fn the_editor_shows_and_edits_a_filter_as_the_reference_does() {
    let recorded = hydrus_testkit::fixture_json("tag_filter_editor.json");
    let namespaces = strings(&recorded["namespaces"]);
    // (no siblings in the recording's client)
    let alone = |tags: &[String]| tags.iter().map(|t| vec![t.clone()]).collect();
    for (n, case) in recorded["cases"].as_array().unwrap().iter().enumerate() {
        let mut editor = TagFilterEditor::new(
            &rules(&case["rules"]),
            case["blacklist_only"].as_bool().unwrap(),
            &namespaces,
        );
        let mut typed = String::new();
        for state in case["states"].as_array().unwrap() {
            let step = &state["step"];
            let at = format!("case {n}, {step}");
            let mut redundant = None;
            if let Some(step) = step.as_array() {
                let kind = step[0].as_str().unwrap();
                let index = || usize::try_from(step[1].as_u64().unwrap()).unwrap();
                let slices = || strings(&step[1]);
                match kind {
                    "white_global" => editor.whitelist_global(index()),
                    "white_ns" => editor.whitelist_namespace(index()),
                    "black_global" => editor.blacklist_global(index()),
                    "black_ns" => editor.blacklist_namespace(index()),
                    "white_add" => editor.add_simple_whitelist(&slices()),
                    "black_add" => editor.add_simple_blacklist(&slices()),
                    "adv_black_add" => editor.add_advanced_blacklist(&slices()),
                    "adv_white_add" => editor.add_advanced_whitelist(&slices()),
                    "white_remove" => editor.remove_simple_whitelist(&slices()),
                    "black_remove" => editor.remove_simple_blacklist(&slices()),
                    "adv_black_delete" => editor.delete_advanced_blacklist(&slices()),
                    "adv_white_delete" => editor.delete_advanced_whitelist(&slices()),
                    "block_everything" => editor.block_everything(),
                    "test" => step[1].as_str().unwrap().clone_into(&mut typed),
                    other => panic!("{other}"),
                }
                redundant = editor.take_redundant();
            }
            let theirs = &state["state"];
            let tabs: Vec<&str> = editor.tabs().iter().map(|t| t.label()).collect();
            assert_eq!(tabs, strings(&theirs["tabs"]), "{at}");
            // (the tab chosen as it opens, where the user stays)
            if step.is_null() {
                assert_eq!(editor.start_tab().label(), theirs["tab"], "{at}");
            }
            let view = editor.view();
            check_simple(&view.whitelist, &theirs["whitelist"], &at);
            check_simple(&view.blacklist, &theirs["blacklist"], &at);
            assert_eq!(
                view.advanced_blacklist,
                strings(&theirs["advanced_blacklist"]),
                "{at}"
            );
            assert_eq!(
                view.advanced_whitelist,
                strings(&theirs["advanced_whitelist"]),
                "{at}"
            );
            assert_eq!(
                view.except_input_enabled,
                theirs["except_input_enabled"].as_bool().unwrap(),
                "{at}"
            );
            assert_eq!(
                redundant.unwrap_or_default(),
                theirs["redundant"].as_str().unwrap(),
                "{at}"
            );
            assert_eq!(view.current, theirs["current"].as_str().unwrap(), "{at}");
            let (test, good) = editor.test(&typed, &alone);
            assert_eq!(test, theirs["test"].as_str().unwrap(), "{at}");
            let colour = match good {
                None => "",
                Some(true) => "HydrusValid",
                Some(false) => "HydrusInvalid",
            };
            assert_eq!(colour, theirs["test_colour"].as_str().unwrap(), "{at}");
            let value = editor.value();
            let mut ours: Vec<Vec<String>> = value
                .rules()
                .map(|(s, r)| {
                    vec![
                        s.to_owned(),
                        if r == FilterRule::Blacklist {
                            "black"
                        } else {
                            "white"
                        }
                        .to_owned(),
                    ]
                })
                .collect();
            ours.sort();
            let theirs_rules: Vec<Vec<String>> = theirs["rules"]
                .as_array()
                .unwrap()
                .iter()
                .map(strings)
                .collect();
            assert_eq!(ours, theirs_rules, "{at}");
            assert_eq!(value.to_permitted_string(), theirs["permitted"], "{at}");
            assert_eq!(value.to_filter_string(), theirs["filter_string"], "{at}");
            assert_eq!(
                value.to_blacklist_string(),
                theirs["blacklist_string"],
                "{at}"
            );
        }
    }
}
