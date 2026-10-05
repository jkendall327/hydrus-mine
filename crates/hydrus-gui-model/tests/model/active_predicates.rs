//! Replay actual selected active-list search commands and populated edit values.
use hydrus_gui_model::{
    active_predicates::{self, Command},
    predicate_editors::{Blank, Context, Editor, defaults::CustomDefaults},
};
use hydrus_search::{Predicate, TextContext};
use serde_json::Value;
use std::collections::HashSet;
fn decode(value: &Value) -> Predicate {
    let object =
        hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(&value.to_string())
            .unwrap();
    hydrus_legacy::objects::predicates::predicate(&object).unwrap()
}
fn predicates(value: &Value) -> Vec<Predicate> {
    value.as_array().unwrap().iter().map(decode).collect()
}
fn search(value: &Value) -> Vec<Predicate> {
    predicates(&value["predicates"])
}
fn set(values: Vec<Predicate>) -> HashSet<Predicate> {
    values.into_iter().collect()
}
fn menu_labels(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| {
            let mut labels = vec![row["label"].as_str().unwrap().to_owned()];
            labels.extend(menu_labels(&row["children"]));
            labels
        })
        .collect()
}
fn context() -> Context {
    Context {
        file_services: Vec::new(),
        tag_services: Vec::new(),
        url_classes: Vec::new(),
        rating_services: Vec::new(),
        today: hydrus_search::Clock::system().today(),
    }
}
#[test]
fn actual_qt_commands_preserve_inverse_add_vs_ctrl_toggle_and_exact_result_sets() {
    let recording = hydrus_testkit::fixture_json("active_predicate_edit.json");
    assert_eq!(recording["events"].as_array().unwrap().len(), 13);
    for event in recording["events"].as_array().unwrap() {
        let mut current = search(&event["before"]);
        let selected = predicates(&event["selected"]);
        let text = TextContext::default();
        let editable = selected.len() == 1 && Editor::existing(&selected[0], &context()).is_some();
        let actual_menu = menu_labels(&event["menu"]);
        for (_, label) in active_predicates::menu(
            &selected,
            &current,
            &text,
            editable.then_some(selected.as_slice()),
        ) {
            assert!(actual_menu.contains(&label), "{}: {label}", event["name"]);
        }
        let command = match event["command"].as_str().unwrap() {
            "remove_predicates" | "activate" => Command::Remove,
            "add_inverse_predicates" => Command::Invert,
            "ctrl_activate" => Command::InvertToggle,
            "replace_or_predicate" => Command::ReplaceOr,
            "dissolve_or_predicate" => Command::DissolveOr,
            "add_namespace_predicate" => Command::Namespace,
            "add_inverse_namespace_predicate" => Command::ExcludeNamespace,
            "shift_activate" => Command::Edit,
            unknown => panic!("unrecorded command {unknown}"),
        };
        if command == Command::Edit {
            active_predicates::replace(
                &mut current,
                &selected,
                &active_predicates::inverses(&selected, &text),
                &text,
            );
        } else {
            active_predicates::apply(&mut current, &selected, command, &text);
        }
        assert_eq!(
            set(current),
            set(search(&event["after"])),
            "{}",
            event["name"]
        );
    }
}
#[test]
fn supplied_size_and_limit_reopen_exact_values_despite_saved_creation_defaults() {
    let recording = hydrus_testkit::fixture_json("active_predicate_edit.json");
    let context = context();
    let defaults = CustomDefaults {
        predicates: hydrus_search::parse_api_search(&serde_json::json!(["system:filesize > 99MB"]))
            .unwrap(),
    };
    for edit in recording["edits"].as_array().unwrap() {
        let name = edit["name"].as_str().unwrap();
        let original = predicates(&edit["initial"]);
        if !matches!(name, "size" | "limit") {
            assert!(Editor::existing(&original[0], &context).is_none());
            continue;
        }
        let mut editor = Editor::existing(&original[0], &context).unwrap();
        editor.apply_defaults(&defaults, &context);
        assert_eq!(editor.pages.len(), 1);
        assert!(editor.pages[0].buttons.is_empty());
        assert!(editor.pages[0].recent_types.is_empty());
        let panel = &mut editor.pages[0].panels[0];
        assert_eq!(panel.predicates(&context).unwrap(), original);
        if name == "size" {
            panel.choose(1, 4);
            panel.set_number(2, 11);
        }
        assert_eq!(
            set(panel.predicates(&context).unwrap()),
            set(predicates(&edit["value"]))
        );
        let mut current = search(&edit["before"]);
        let before = current.clone();
        if edit["accepted"].as_bool().unwrap() {
            active_predicates::replace(
                &mut current,
                &original,
                &panel.predicates(&context).unwrap(),
                &TextContext::default(),
            );
        }
        assert_eq!(set(current.clone()), set(search(&edit["after"])));
        if !edit["accepted"].as_bool().unwrap() {
            assert_eq!(current, before);
        }
    }
    let mut creation = Editor::new(Blank::Filesize, &context);
    creation.apply_defaults(&defaults, &context);
    assert_eq!(
        creation.pages[0].panels[0].predicates(&context).unwrap(),
        defaults.predicates
    );
}

#[test]
fn mixed_controls_replay_real_qt_values_row_order_cancel_and_parser_vetoes() {
    use hydrus_gui_model::predicate_editors::batch::simple_predicate;
    let recording = hydrus_testkit::fixture_json("active_predicate_mixed.json");
    let context = context();
    let text = TextContext::default();
    let defaults = CustomDefaults {
        predicates: hydrus_search::parse_api_search(&serde_json::json!(["system:filesize > 99MB"]))
            .unwrap(),
    };
    assert_eq!(recording["simple_cases"].as_array().unwrap().len(), 14);
    for case in recording["simple_cases"].as_array().unwrap() {
        let result = simple_predicate(case["text"].as_str().unwrap());
        if let Some(error) = case["error"].as_str() {
            assert_eq!(result.unwrap_err(), error);
        } else {
            assert_eq!(result.unwrap(), decode(&case["predicate"]));
        }
    }
    assert_eq!(recording["mixed"].as_array().unwrap().len(), 8);
    for case in recording["mixed"].as_array().unwrap() {
        let selected = predicates(&case["selected"]);
        let mut editor = Editor::mixed(&selected, &context, &text).unwrap();
        editor.apply_defaults(&defaults, &context);
        assert_eq!(
            set(editor.mixed_predicates(&context).unwrap()),
            set(predicates(&case["initial"]))
        );
        let batch = editor.batch.as_mut().unwrap();
        for (draft, changed) in batch
            .simple
            .iter_mut()
            .zip(case["simple"].as_array().unwrap())
        {
            *draft = changed.as_str().unwrap().to_owned();
        }
        if case["flip"] == true {
            for p in &mut batch.invertible {
                *p = p.inverse(&|_| false).unwrap();
            }
        }
        for (panel, size) in editor.pages[0]
            .panels
            .iter_mut()
            .zip(case["sizes"].as_array().unwrap())
        {
            panel.choose(1, 4);
            panel.set_number(2, size.as_i64().unwrap());
        }
        let made = editor.mixed_predicates(&context).unwrap();
        assert_eq!(set(made.clone()), set(predicates(&case["value"])));
        let mut current = search(&case["before"]);
        if case["accepted"] == true {
            active_predicates::replace(&mut current, &selected, &made, &text);
        }
        assert_eq!(set(current.clone()), set(search(&case["after"])));
        let batch = editor.batch.as_mut().unwrap();
        if !batch.simple.is_empty() {
            batch.simple[0].clear();
            assert!(
                editor.mixed_predicates(&context).is_err(),
                "one invalid simple field vetoes all staged values"
            );
        }
        if case["name"] == "mixed" {
            assert_eq!(editor.batch.as_ref().unwrap().order, [-1, 0]);
        }
    }
}

#[test]
fn actual_qt_inherited_copy_payloads_and_page_routes_match_all_recorded_selections() {
    use hydrus_gui_model::active_predicates::routes::{self, Route};
    let recording = hydrus_testkit::fixture_json("active_predicate_routes.json");
    assert_eq!(recording["cases"].as_array().unwrap().len(), 14);
    for case in recording["cases"].as_array().unwrap() {
        let current = predicates(&case["current"]);
        let raw_selected = predicates(&case["selected"]);
        let selected = current
            .iter()
            .filter(|p| raw_selected.contains(p))
            .cloned()
            .collect::<Vec<_>>();
        let menu = routes::menu(&selected, &current, &TextContext::default());
        let recorded = case["actions"].as_array().unwrap();
        assert_eq!(menu.len(), recorded.len(), "{}", case["name"]);
        for ((route, label), actual) in menu.iter().zip(recorded) {
            assert_eq!(label, actual["label"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(
                if route.group() == 0 { "copy" } else { "open" },
                actual["group"].as_str().unwrap()
            );
            match *route {
                Route::Copy(copy) => {
                    let payload =
                        routes::copy_text(&selected, &current, copy, &TextContext::default());
                    assert_eq!(
                        payload,
                        actual["publications"][0]["text"].as_str().unwrap(),
                        "{} {label}",
                        case["name"]
                    );
                }
                Route::Open(open) => {
                    let batches = routes::searches(&selected, open);
                    let actual_batches = actual["publications"].as_array().unwrap();
                    assert_eq!(batches.len(), actual_batches.len());
                    // Qt obtains open batches from its selected-term set. Match
                    // each real page by its values, without assuming set order.
                    for batch in batches {
                        let actual = actual_batches
                            .iter()
                            .find(|a| set(predicates(&a["predicates"])) == set(batch.clone()))
                            .unwrap();
                        assert_eq!(
                            routes::page_name(
                                &batch,
                                &TextContext::default(),
                                open == routes::Open::Duplicates
                            ),
                            actual["name"].as_str().unwrap()
                        );
                        assert_eq!(
                            actual["topic"].as_str().unwrap(),
                            if open == routes::Open::Duplicates {
                                "new_page_duplicates"
                            } else {
                                "new_page_query"
                            }
                        );
                    }
                }
            }
        }
        assert_eq!(
            case["select_files_publications"],
            serde_json::json!([]),
            "the actual active-list inherited handler is a no-op"
        );
    }
    let raised = recording["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "two_tags_raise")
        .unwrap();
    let each = raised["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "open new search pages for each in selection")
        .unwrap();
    assert_eq!(each["publications"][0]["activate"], true);
    assert_eq!(each["publications"][1]["activate"], false);
}
