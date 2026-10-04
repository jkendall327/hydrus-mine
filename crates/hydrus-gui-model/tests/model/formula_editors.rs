//! Real formula/rule control recordings replayed through the typed editors.
use hydrus_core::url::strings::{
    Conversion, ProcessingStep, StringConverter, StringMatch, StringProcessor,
};
use hydrus_gui_model::formula_editors::{
    FormulaEditor, FormulaTestData, Rule, RuleEditor, new_formula,
};
use hydrus_parse::formula::{FormulaKind, HtmlContent, HtmlWalk, JsonContent, JsonRule, TagSearch};
use serde_json::{Value, json};
fn fixture() -> Vec<Value> {
    serde_json::from_value(hydrus_testkit::fixture_json("formula_editors.json")).unwrap()
}
#[test]
fn formula_editor_controls_and_parses_match_reference() {
    for case in fixture() {
        let mut e = FormulaEditor::new(
            &new_formula(case["case"] == "json_formula"),
            FormulaTestData {
                text: case["text"].as_str().unwrap_or_default().to_owned(),
                ..FormulaTestData::default()
            },
        );
        match case["case"].as_str().unwrap() {
            "html_rule" => {
                let mut rule = RuleEditor::new(&e.new_rule());
                rule.tag = "a".into();
                rule.put_attribute("class".into(), "image".into());
                rule.index = Some(-1);
                rule.depth = 2;
                rule.kind = match case["type"].as_str().unwrap() {
                    "ancestors" => 1,
                    "previous" => 2,
                    "next" => 3,
                    _ => 0,
                };
                assert_eq!(rule.value().description(), case["description"]);
                assert_eq!(rule.kind != 1, case["search_enabled"].as_bool().unwrap());
                assert_eq!(rule.kind == 1, case["depth_enabled"].as_bool().unwrap());
            }
            "html_formula" => {
                e.formula.name = "links".into();
                let FormulaKind::Html { content, .. } = &mut e.formula.kind else {
                    unreachable!()
                };
                *content = match case["content"].as_str().unwrap() {
                    "string" => HtmlContent::Text,
                    "html" => HtmlContent::Html,
                    _ => HtmlContent::Attribute("href".into()),
                };
                assert_eq!(e.formula.name, case["name"]);
                assert_eq!(
                    json!(e.rules().iter().map(Rule::description).collect::<Vec<_>>()),
                    case["rules"]
                );
                assert_eq!(json!(e.results().unwrap()), case["results"]);
            }
            "json_newlines" => {
                e.formula.kind = FormulaKind::Json {
                    rules: vec![JsonRule::AllItems],
                    content: JsonContent::Strings,
                };
                e.test.collapse_newlines = false;
                assert_eq!(json!(e.results().unwrap()), case["results"]);
            }
            "attribute_veto" => {
                let FormulaKind::Html { content, .. } = &mut e.formula.kind else {
                    unreachable!()
                };
                *content = HtmlContent::Attribute(String::new());
                assert_eq!(e.value().unwrap_err(), case["error"]);
            }
            "json_formula" => {
                let r = match case["type"].as_str().unwrap() {
                    "all" => JsonRule::AllItems,
                    "index" => JsonRule::Index(-1),
                    "filter" => JsonRule::TestStringItems(StringMatch::fixed("42")),
                    "ascend" => JsonRule::Ascend(1),
                    "deminify" => JsonRule::Deminify(0),
                    _ => JsonRule::DictKey(StringMatch::fixed("posts")),
                };
                assert_eq!(Rule::Json(r.clone()).description(), case["description"]);
                let mut rule = RuleEditor::new(&Rule::Json(r.clone()));
                assert_eq!(rule.value(), Rule::Json(r.clone()));
                rule.kind = 1;
                assert_eq!(rule.value(), Rule::Json(JsonRule::AllItems));
                let FormulaKind::Json { rules, content } = &mut e.formula.kind else {
                    unreachable!()
                };
                *rules = if case["type"] == "ascend" {
                    vec![JsonRule::DictKey(StringMatch::fixed("posts")), r]
                } else {
                    vec![r]
                };
                *content = match case["content"].as_str().unwrap() {
                    "json" => JsonContent::Json,
                    "dictionary keys" => JsonContent::DictKeys,
                    _ => JsonContent::Strings,
                };
                assert_eq!(json!(e.results().unwrap()), case["results"]);
            }
            "processor_test" => {
                let FormulaKind::Html { content, .. } = &mut e.formula.kind else {
                    unreachable!()
                };
                *content = HtmlContent::Text;
                e.test.collapse_newlines = false;
                e.formula.processor = StringProcessor {
                    steps: vec![ProcessingStep::Convert(StringConverter {
                        conversions: vec![Conversion::Append("!".into())],
                        example: String::new(),
                    })],
                };
                assert_eq!(json!(e.processor_texts()), case["before"]);
                assert_eq!(json!(e.results().unwrap()), case["after"]);
            }
            "queue" | "change_type" | "named_list" | "named_multi" => {}
            other => panic!("unreplayed {other}"),
        }
    }
}
#[test]
fn formula_editor_rule_queue_matches_reference_actions() {
    let mut e = FormulaEditor::new(&new_formula(false), FormulaTestData::default());
    for case in fixture().into_iter().filter(|c| c["case"] == "queue") {
        match case["action"].as_str().unwrap() {
            "add" => {
                let mut r = RuleEditor::new(&e.new_rule());
                r.tag = "span".into();
                e.put(None, r.value());
            }
            "edit" => {
                e.click(1, false, false);
                let mut r = RuleEditor::new(&e.rules()[1]);
                r.tag = "div".into();
                e.put(Some(1), r.value());
            }
            "up" => e.shift(false),
            "down" => e.shift(true),
            "delete" => e.delete(),
            "cancel_add" => {}
            other => panic!("{other}"),
        }
        assert_eq!(
            json!(e.rules().iter().map(Rule::description).collect::<Vec<_>>()),
            case["rules"]
        );
        assert_eq!(json!(e.selected()), case["selected"]);
    }
}
#[test]
fn formula_editors_preserve_other_kinds_and_inactive_rule_settings() {
    let mut f = new_formula(false);
    f.kind = FormulaKind::Static {
        text: "kept".into(),
        count: 2,
    };
    f.name = "original".into();
    let mut e = FormulaEditor::new(&f, FormulaTestData::default());
    assert!(e.supported());
    e.put(None, e.new_rule());
    e.delete();
    e.shift(false);
    assert_eq!(e.value().unwrap(), f);
    let mut r = RuleEditor::new(&Rule::Html(hydrus_parse::formula::HtmlRule {
        walk: HtmlWalk::Descendants(TagSearch::default()),
        tag_name: None,
        text_match: None,
    }));
    r.put_attribute("class".into(), "old".into());
    r.put_attribute("class".into(), "new".into());
    r.index = Some(-2);
    r.set_match(StringMatch::fixed("hello"));
    r.match_on = true;
    r.kind = 1;
    let Rule::Html(a) = r.value() else {
        unreachable!()
    };
    assert_eq!(a.walk, HtmlWalk::Ancestor { depth: 1 });
    assert_eq!(a.tag_name, None);
    assert_eq!(a.text_match, Some(StringMatch::fixed("hello")));
    r.kind = 3;
    let Rule::Html(a) = r.value() else {
        unreachable!()
    };
    assert_eq!(
        a.walk,
        HtmlWalk::NextSiblings(hydrus_parse::formula::TagSearch {
            attrs: vec![("class".into(), "new".into())],
            index: Some(-2)
        })
    );
    r.match_on = false;
    let Rule::Html(a) = r.value() else {
        unreachable!()
    };
    assert_eq!(a.text_match, None);
    e.test.text = "irrelevant".into();
    assert_eq!(e.results().unwrap(), ["kept", "kept"]);
    e.formula = new_formula(true);
    e.test.text = "<html>oops".into();
    assert!(e.results().unwrap_err().contains("HTML instead of JSON"));
    assert_eq!(e.processor_texts(), [""]);
}

#[test]
fn formula_editor_type_changes_use_fresh_defaults_and_preserve_separated_content() {
    let mut e = FormulaEditor::new(&new_formula(false), FormulaTestData::default());
    e.formula.name = "old name".into();
    e.formula
        .processor
        .steps
        .push(ProcessingStep::Filter(StringMatch::fixed("x")));
    e.click(0, false, false);
    e.change_type(false);
    assert_eq!(e.formula.name, "old name");
    assert_eq!(e.selected(), [0]);
    e.change_type(true);
    assert_eq!(e.formula, new_formula(true));
    assert!(e.selected().is_empty());
    let FormulaKind::Json { content, .. } = &mut e.formula.kind else {
        panic!()
    };
    *content = JsonContent::Json;
    e.change_type(false);
    let FormulaKind::Html { content, .. } = &e.formula.kind else {
        panic!()
    };
    assert_eq!(*content, HtmlContent::Html);
    e.change_type(true);
    let FormulaKind::Json { content, .. } = &e.formula.kind else {
        panic!()
    };
    assert_eq!(*content, JsonContent::Json);
    let mut r = RuleEditor::new(&Rule::Json(JsonRule::DictKey(StringMatch::fixed("posts"))));
    r.set_match(StringMatch::fixed("urls"));
    r.kind = 3;
    r.set_match(StringMatch::fixed("123"));
    assert_eq!(
        r.value(),
        Rule::Json(JsonRule::TestStringItems(StringMatch::fixed("123")))
    );
    r.kind = 0;
    assert_eq!(r.string_match(), &StringMatch::fixed("urls"));
    assert_eq!(
        r.value(),
        Rule::Json(JsonRule::DictKey(StringMatch::fixed("urls")))
    );
}

#[test]
fn formula_editor_type_and_named_list_actions_match_reference() {
    let mut e = FormulaEditor::new(&new_formula(false), FormulaTestData::default());
    let FormulaKind::Html { content, .. } = &mut e.formula.kind else {
        panic!()
    };
    *content = HtmlContent::Html;
    e.formula.name = "old name".into();
    for case in fixture().into_iter().filter(|c| c["case"] == "change_type") {
        e.change_type(case["type"] == "json");
        assert_eq!(e.formula.name, case["name"]);
        assert_eq!(
            json!(e.formula.processor.processing_strings()),
            case["processor"]
        );
        match &e.formula.kind {
            FormulaKind::Html { content, .. } => assert_eq!(*content, HtmlContent::Html),
            FormulaKind::Json { content, .. } => assert_eq!(*content, JsonContent::Json),
            _ => panic!(),
        }
    }
    let mut values = Vec::new();
    for case in fixture().into_iter().filter(|c| c["case"] == "named_list") {
        match case["action"].as_str().unwrap() {
            "add" => hydrus_gui_model::formula_editors::put_simple_formula(
                &mut values,
                None,
                hydrus_parse::simple::SimpleFormula {
                    name: "links".into(),
                    formula: new_formula(false),
                },
            ),
            "edit" => {
                let f = values[0].clone();
                hydrus_gui_model::formula_editors::put_simple_formula(&mut values, Some(0), f);
            }
            "delete" => {
                values.remove(0);
            }
            _ => panic!(),
        }
        assert_eq!(
            json!(values.iter().map(|f| f.name.clone()).collect::<Vec<_>>()),
            case["names"]
        );
    }
}

#[test]
fn formula_editor_bulk_name_conflicts_match_reference() {
    for case in fixture().into_iter().filter(|c| c["case"] == "named_multi") {
        let mut values = case["before"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| hydrus_parse::simple::SimpleFormula {
                name: name.as_str().unwrap().to_owned(),
                formula: new_formula(false),
            })
            .collect::<Vec<_>>();
        for i in 0..values.len() {
            let f = values[i].clone();
            hydrus_gui_model::formula_editors::put_simple_formula(&mut values, Some(i), f);
        }
        assert_eq!(
            json!(values.iter().map(|f| f.name.clone()).collect::<Vec<_>>()),
            case["after"]
        );
    }
    // A stale or out-of-range replacement position appends without losing
    // an existing draft; in particular, len is already outside the queue.
    let mut values = vec![hydrus_parse::simple::SimpleFormula {
        name: "kept".into(),
        formula: new_formula(false),
    }];
    for (name, at) in [("at end", 1), ("past end", usize::MAX)] {
        hydrus_gui_model::formula_editors::put_simple_formula(
            &mut values,
            Some(at),
            hydrus_parse::simple::SimpleFormula {
                name: name.into(),
                formula: new_formula(false),
            },
        );
    }
    assert_eq!(
        values.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["kept", "at end", "past end"]
    );
}

#[test]
fn scalar_formula_controls_match_reference() {
    let cases: Vec<Value> = serde_json::from_value(hydrus_testkit::fixture_json(
        "recursive_formula_editors.json",
    ))
    .unwrap();
    for case in cases {
        let kind = match case["case"].as_str().unwrap() {
            "context_formula" => 4,
            "static_formula" => 5,
            _ => continue,
        };
        let mut e = FormulaEditor::new(
            &hydrus_gui_model::formula_editors::new_formula_kind(kind),
            FormulaTestData {
                context: serde_json::from_value(case["context"].clone()).unwrap_or_default(),
                collapse_newlines: case["collapse"].as_bool().unwrap(),
                ..FormulaTestData::default()
            },
        );
        e.formula.name = case["name"].as_str().unwrap().into();
        e.formula.processor = StringProcessor {
            steps: vec![ProcessingStep::Convert(StringConverter {
                conversions: vec![Conversion::Append("!".into())],
                example: String::new(),
            })],
        };
        match &mut e.formula.kind {
            FormulaKind::ContextVariable { variable } => {
                *variable = case["variable"].as_str().unwrap().into()
            }
            FormulaKind::Static { text, count } => {
                *text = case["text"].as_str().unwrap().into();
                *count = case["count"].as_u64().unwrap().try_into().unwrap();
                assert_eq!(case["minimum"], 1);
                assert_eq!(case["maximum"], 65535);
            }
            _ => panic!(),
        }
        assert_eq!(e.kind_index(), kind);
        assert_eq!(json!(e.results().unwrap()), case["results"]);
        if kind == 4 {
            assert_eq!(json!(e.processor_texts()), case["before"]);
            e.formula.kind = FormulaKind::ContextVariable {
                variable: "absent".into(),
            };
            assert!(e.results().unwrap().is_empty());
        }
    }
}
