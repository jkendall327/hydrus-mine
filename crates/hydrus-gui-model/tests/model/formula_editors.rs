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

fn recursive_cases() -> Vec<Value> {
    serde_json::from_value(hydrus_testkit::fixture_json(
        "recursive_formula_editors.json",
    ))
    .unwrap()
}
fn embedded_pair() -> hydrus_parse::formula::Formula {
    let mut formula = hydrus_gui_model::formula_editors::new_formula_kind(2);
    let FormulaKind::Nested { main, sub } = &mut formula.kind else {
        panic!()
    };
    let FormulaKind::Html { rules, content } = &mut main.kind else {
        panic!()
    };
    rules[0].tag_name = Some("script".into());
    *content = HtmlContent::Text;
    let FormulaKind::Json { rules, .. } = &mut sub.kind else {
        panic!()
    };
    rules.push(JsonRule::AllItems);
    formula.processor = StringProcessor {
        steps: vec![ProcessingStep::Convert(StringConverter {
            conversions: vec![Conversion::Append("!".into())],
            example: String::new(),
        })],
    };
    formula
}
#[test]
fn recursive_formula_fields_and_transformed_child_data_match_reference() {
    use hydrus_gui_model::formula_editors::{FormulaChild, new_formula_kind};
    let cases = recursive_cases();
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_formula")
        .unwrap();
    let original = embedded_pair();
    let mut editor = FormulaEditor::new(
        &original,
        FormulaTestData {
            context: serde_json::from_value(case["context"].clone()).unwrap(),
            text: case["text"].as_str().unwrap().into(),
            ..FormulaTestData::default()
        },
    );
    editor.formula.name = case["name"].as_str().unwrap().into();
    assert_eq!(json!(editor.results().unwrap()), case["results"]);
    assert_eq!(
        json!(editor.child_test_data(FormulaChild::Sub).examples),
        case["sub_texts"]
    );
    assert_eq!(editor.child_test_data(FormulaChild::Main), editor.test);
    let mut main = new_formula_kind(5);
    main.kind = FormulaKind::Static {
        text: "{\"posts\":[\"changed\"]}".into(),
        count: 1,
    };
    let staged = editor.child(FormulaChild::Main).unwrap();
    assert_ne!(staged, main);
    assert_eq!(editor.results().unwrap(), ["alpha!", "beta!"]);
    editor.put_child(FormulaChild::Main, main);
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_main_edit")
        .unwrap();
    assert_eq!(json!(editor.results().unwrap()), case["results"]);
    assert_eq!(
        json!(editor.child_test_data(FormulaChild::Sub).examples),
        case["sub_texts"]
    );
    editor.put_child(FormulaChild::Sub, new_formula_kind(4));
    let case = cases
        .iter()
        .find(|c| c["case"] == "nested_sub_edit")
        .unwrap();
    assert_eq!(json!(editor.results().unwrap()), case["results"]);
    assert_eq!(original, embedded_pair());
    for case in cases.iter().filter(|c| c["case"] == "nested_sub_test") {
        let mut formula = embedded_pair();
        if case["type"] == "error_fallback" {
            let FormulaKind::Nested { main, .. } = &mut formula.kind else {
                panic!()
            };
            **main = new_formula(true);
        }
        let editor = FormulaEditor::new(
            &formula,
            FormulaTestData {
                context: serde_json::from_value(case["context"].clone()).unwrap(),
                examples: serde_json::from_value(case["texts"].clone()).unwrap(),
                ..FormulaTestData::default()
            },
        );
        let child_data = editor.child_test_data(FormulaChild::Sub);
        assert_eq!(json!(child_data.examples), case["sub_texts"]);
        assert_eq!(json!(child_data.context), case["context"]);
        assert_eq!(
            child_data.text,
            child_data.examples.first().cloned().unwrap_or_default()
        );
    }
}
#[test]
fn zipper_member_queue_and_substitution_match_reference() {
    use hydrus_gui_model::formula_editors::{FormulaChild, new_formula_kind};
    let mut formula = new_formula_kind(3);
    let mut constant = new_formula_kind(5);
    constant.kind = FormulaKind::Static {
        text: "first".into(),
        count: 1,
    };
    let FormulaKind::Zipper { formulae, .. } = &mut formula.kind else {
        panic!()
    };
    *formulae = vec![constant];
    let mut editor = FormulaEditor::new(&formula, FormulaTestData::default());
    for case in recursive_cases()
        .into_iter()
        .filter(|c| c["case"] == "zipper_queue")
    {
        match case["action"].as_str().unwrap() {
            "add" => {
                let mut child = new_formula_kind(5);
                child.kind = FormulaKind::Static {
                    text: "second".into(),
                    count: 1,
                };
                editor.put_child(FormulaChild::Member(None), child);
            }
            "edit" => {
                editor.click(1, false, false);
                let mut child = editor.child(FormulaChild::Member(Some(1))).unwrap();
                child.kind = FormulaKind::Static {
                    text: "replacement".into(),
                    count: 1,
                };
                editor.put_child(FormulaChild::Member(Some(1)), child);
            }
            "up" => editor.shift(false),
            "down" => editor.shift(true),
            "delete" => editor.delete(),
            "cancel_add" | "cancel_edit" => {}
            other => panic!("{other}"),
        }
        let FormulaKind::Zipper { formulae, .. } = &editor.formula.kind else {
            panic!()
        };
        let texts = formulae
            .iter()
            .map(|f| {
                let FormulaKind::Static { text, .. } = &f.kind else {
                    panic!()
                };
                text.clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(json!(texts), case["texts"]);
        assert_eq!(json!(editor.selected()), case["selected"]);
    }
    let original = editor.formula.clone();
    editor.put_child(FormulaChild::Member(Some(usize::MAX)), new_formula_kind(4));
    assert_eq!(editor.formula, original);
    let case = recursive_cases()
        .into_iter()
        .find(|c| c["case"] == "zipper_formula")
        .unwrap();
    let mut left = new_formula_kind(5);
    left.kind = FormulaKind::Static {
        text: "left".into(),
        count: 2,
    };
    editor.formula.kind = FormulaKind::Zipper {
        formulae: vec![left, new_formula_kind(4)],
        phrase: case["phrase"].as_str().unwrap().into(),
    };
    editor.formula.processor = embedded_pair().processor;
    editor.test.context = serde_json::from_value(case["context"].clone()).unwrap();
    assert_eq!(json!(editor.results().unwrap()), case["results"]);
}
#[test]
fn six_kind_chooser_defaults_match_reference() {
    use hydrus_gui_model::formula_editors::new_formula_kind;
    for case in recursive_cases()
        .into_iter()
        .filter(|c| c["case"] == "change_recursive_type")
    {
        let mut original = new_formula(false);
        original.name = "reset".into();
        original.processor = embedded_pair().processor;
        let mut editor = FormulaEditor::new(&original, FormulaTestData::default());
        let kind = match case["type"].as_str().unwrap() {
            "nested" => 2,
            "zipper" => 3,
            "context" => 4,
            "static" => 5,
            _ => panic!(),
        };
        editor.change_kind(kind);
        assert_eq!(editor.formula, new_formula_kind(kind));
        assert_eq!(editor.formula.name, case["name"]);
        assert_eq!(
            json!(editor.formula.processor.processing_strings()),
            case["processing"]
        );
        match &editor.formula.kind {
            FormulaKind::Nested { main, sub } => {
                assert_eq!(**main, new_formula(false));
                assert_eq!(**sub, new_formula(true));
                assert_eq!(case["main_type"], "ParseFormulaHTML");
                assert_eq!(case["sub_type"], "ParseFormulaJSON");
            }
            FormulaKind::Zipper { formulae, phrase } => {
                assert_eq!(phrase, case["phrase"].as_str().unwrap());
                assert_eq!(json!(formulae.len()), case["children"]);
            }
            FormulaKind::ContextVariable { variable } => {
                assert_eq!(variable, case["variable"].as_str().unwrap())
            }
            FormulaKind::Static { text, count } => {
                assert_eq!(text, case["text"].as_str().unwrap());
                assert_eq!(json!(count), case["count"]);
            }
            _ => panic!(),
        }
    }
}

#[test]
fn multiple_test_documents_retain_edits_sources_and_selected_child_order() {
    let cases: Vec<Value> =
        serde_json::from_value(hydrus_testkit::fixture_json("parser_test_data.json")).unwrap();
    let first = cases
        .iter()
        .find(|c| c["case"] == "converted_example" && c["sequence"] == 0)
        .unwrap();
    let second = cases
        .iter()
        .find(|c| c["case"] == "converted_example" && c["sequence"] == 1)
        .unwrap();
    let mut test = FormulaTestData {
        context: serde_json::from_value(first["context"].clone()).unwrap(),
        text: first["raw"].as_str().unwrap().into(),
        ..FormulaTestData::default()
    };
    test.prepare_examples();
    let original_url = test.context["url"].clone();
    let index = test.add_example(
        second["raw"].as_str().unwrap().into(),
        Some("https://test-docs.example/second".into()),
    );
    assert_eq!(index, 1);
    assert_eq!(test.context["url"], "https://test-docs.example/second");
    assert_eq!(test.context["token"], "preserved");
    test.remember_example(1, "<p>edited second</p>".into());
    assert!(test.choose_example(0));
    assert_eq!(test.text, first["raw"].as_str().unwrap());
    assert_eq!(test.context["url"], original_url);
    assert!(test.choose_example(1));
    assert_eq!(test.text, "<p>edited second</p>");
    let child = test.selected_first(1);
    assert_eq!(child.examples, ["<p>edited second</p>", "<p>first</p>"]);
    assert_eq!(
        child.source_urls,
        [
            Some("https://test-docs.example/second".into()),
            Some(original_url.clone())
        ]
    );
    assert_eq!(child.context, test.context);
    assert!(!test.choose_example(usize::MAX));
    assert_eq!(test.text, "<p>edited second</p>");
    assert_eq!(test.remove_example(1), 0);
    assert_eq!(test.text, first["raw"].as_str().unwrap());
    assert_eq!(test.context["url"], original_url);
    assert_eq!(test.remove_example(0), 0);
    assert_eq!(test.examples.len(), 1);
}
