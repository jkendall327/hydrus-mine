//! The content parser editor's per-kind controls (urls, file hash, watcher
//! title, http headers, temporary variable, veto), starting from the parsers
//! the reference client recorded in `parser_editors.json`: each kind shows
//! its reference controls with the recorded values, edits reach the parser
//! handed back on Apply (invalid priorities are refused and nothing is
//! handed back), and that parser parses the recorded document as the
//! reference did.
use hydrus_gui::{
    ParserEditWindow, headless,
    parser_editors_window::{self as windows, Slots},
};
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_legacy::{objects::parsers, serialisable::SerialisableObject};
use hydrus_parse::content::{ContentKind, ContentParser};
use hydrus_parse::formula::ParsingContext;
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{cell::RefCell, rc::Rc};

struct Case {
    parser: ContentParser,
    text: String,
    results: serde_json::Value,
    error: serde_json::Value,
}

fn recorded(kind: &str) -> Case {
    let all = hydrus_testkit::fixture_json("parser_editors.json");
    let case = all
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "content" && c["kind"] == kind)
        .unwrap();
    let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
    Case {
        parser: parsers::content_parser(&object).unwrap(),
        text: case["text"].as_str().unwrap().into(),
        results: case["results"].clone(),
        error: case["error"].clone(),
    }
}

/// What one editor session handed back, and the fields as shown.
struct Session {
    _dir: tempfile::TempDir,
    slots: Slots,
    window: ParserEditWindow,
    applied: Rc<RefCell<Vec<ContentParser>>>,
}

fn open(parser: &ContentParser) -> Session {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let slots = Slots::default();
    let applied = Rc::new(RefCell::new(Vec::new()));
    let window = windows::open_content(
        &store,
        parser,
        FormulaTestData::default(),
        &slots,
        &[0, 1, 2, 3, 4, 5, 6, 7, 8],
        Rc::new({
            let applied = applied.clone();
            move |p| {
                applied.borrow_mut().push(p);
                Ok(())
            }
        }),
    )
    .unwrap();
    Session {
        _dir: dir,
        slots,
        window,
        applied,
    }
}

impl Session {
    fn field(&self, label: &str) -> hydrus_gui::DefinitionField {
        self.window
            .get_fields()
            .iter()
            .find(|f| f.label == label)
            .unwrap_or_else(|| panic!("no field {label:?}"))
    }
    fn labels(&self) -> Vec<String> {
        self.window
            .get_fields()
            .iter()
            .map(|f| f.label.to_string())
            .collect()
    }
    fn apply(&self) -> Option<ContentParser> {
        let before = self.applied.borrow().len();
        self.window.invoke_action("apply".into());
        self.applied.borrow().get(before).cloned()
    }
}

/// The saved parser against the recording: the same results, or the same
/// refusal.
fn parses_as_recorded(saved: &ContentParser, case: &Case) {
    let mut context = ParsingContext::new();
    context.insert("url".into(), "https://example.com/post/1".into());
    let parsed = saved.parse(&context, &format!("<p>{}</p>", case.text));
    if case.error.is_null() {
        let results: Vec<_> = parsed
            .unwrap()
            .contents
            .into_iter()
            .map(|c| c.text)
            .collect();
        assert_eq!(serde_json::to_value(results).unwrap(), case.results);
    } else {
        assert!(parsed.is_err());
    }
}

// leaf: content-urls
#[test]
fn url_content_has_four_roles_and_a_bounded_priority() {
    let _windows = headless::init();
    let case = recorded("urls");
    let s = open(&case.parser);
    let roles = s.field("URL type");
    assert_eq!(
        roles
            .options
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>(),
        [
            "download/pursue (file/post)",
            "associate (source)",
            "next gallery page",
            "sub-gallery page"
        ]
    );
    assert_eq!(roles.chosen, 0, "the recorded parser's role");
    assert_eq!(s.field("priority (0–100)").text, "50");
    // priorities outside 0 to 100, or not numbers, are refused: nothing is
    // handed back and the editor says why
    for bad in ["101", "-1", "fifty", ""] {
        s.window.invoke_text_edited(3, bad.into());
        assert!(s.apply().is_none(), "{bad:?}");
        assert!(!s.window.get_error().is_empty(), "{bad:?}");
    }
    s.window.invoke_text_edited(3, "100".into());
    s.window.invoke_choice_edited(2, 2);
    let saved = s.apply().expect("a valid priority applies");
    assert_eq!(
        saved.kind,
        ContentKind::Url {
            url_type: 6,
            priority: 100
        }
    );
    parses_as_recorded(&saved, &case);
}

// leaf: content-hashes
#[test]
fn hash_content_chooses_type_and_encoding() {
    let _windows = headless::init();
    let case = recorded("file hash");
    let s = open(&case.parser);
    let hash_type = s.field("hash type");
    assert_eq!(
        hash_type
            .options
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>(),
        ["md5", "sha1", "sha256", "sha512"]
    );
    assert_eq!(hash_type.chosen, 2, "the recorded sha256");
    let encoding = s.field("encoding");
    assert_eq!(
        encoding
            .options
            .iter()
            .map(|o| o.to_string())
            .collect::<Vec<_>>(),
        ["hex", "base64"]
    );
    assert_eq!(encoding.chosen, 0);
    let unchanged = open(&case.parser).apply().unwrap();
    assert_eq!(unchanged, case.parser);
    parses_as_recorded(&unchanged, &case);
    s.window.invoke_choice_edited(6, 3);
    s.window.invoke_choice_edited(7, 1);
    let saved = s.apply().unwrap();
    assert_eq!(
        saved.kind,
        ContentKind::Hash {
            hash_type: "sha512".into(),
            encoding: "base64".into()
        }
    );
    assert_eq!(saved.formula, case.parser.formula);
}

// leaf: content-title
#[test]
fn title_content_has_a_bounded_priority() {
    let _windows = headless::init();
    let case = recorded("watcher title");
    let s = open(&case.parser);
    assert_eq!(s.field("priority (0–100)").text, "75");
    assert!(!s.labels().contains(&"URL type".into()));
    s.window.invoke_text_edited(3, "101".into());
    assert!(s.apply().is_none());
    assert!(!s.window.get_error().is_empty());
    s.window.invoke_text_edited(3, "0".into());
    let saved = s.apply().unwrap();
    assert_eq!(saved.kind, ContentKind::Title { priority: 0 });
    parses_as_recorded(&saved, &case);
}

// leaf: content-headers
#[test]
fn header_content_keeps_its_header_name() {
    let _windows = headless::init();
    let case = recorded("http headers");
    let s = open(&case.parser);
    assert_eq!(s.field("header name").text, "Authorization");
    s.window.invoke_text_edited(5, "X-Token".into());
    let saved = s.apply().unwrap();
    assert_eq!(
        saved.kind,
        ContentKind::HttpHeader {
            name: "X-Token".into()
        }
    );
    parses_as_recorded(&saved, &case);
    // the parsed header carries the name for the child jobs
    let mut context = ParsingContext::new();
    context.insert("url".into(), "https://example.com/post/1".into());
    let post = saved
        .parse(&context, &format!("<p>{}</p>", case.text))
        .unwrap();
    assert_eq!(post.contents[0].kind, saved.kind);
}

// leaf: content-variable
#[test]
fn variable_content_keeps_its_variable_name() {
    let _windows = headless::init();
    let case = recorded("temporary variable");
    let s = open(&case.parser);
    assert_eq!(s.field("variable name").text, "token");
    s.window.invoke_text_edited(5, "session".into());
    let saved = s.apply().unwrap();
    assert_eq!(
        saved.kind,
        ContentKind::Variable {
            name: "session".into()
        }
    );
    parses_as_recorded(&saved, &case);
    let mut context = ParsingContext::new();
    context.insert("url".into(), "https://example.com/post/1".into());
    let post = saved
        .parse(&context, &format!("<p>{}</p>", case.text))
        .unwrap();
    assert_eq!(post.contents[0].kind, saved.kind);
}

#[test]
fn veto_content_toggles_its_polarity_and_edits_its_matcher() {
    let _windows = headless::init();
    let case = recorded("veto");
    let s = open(&case.parser);
    assert!(s.field("veto if match found").checked);
    assert!(!s.field("string match").enabled, "edited by its own window");
    let unchanged = open(&case.parser).apply().unwrap();
    assert_eq!(unchanged, case.parser);
    // recorded: a match found vetoes the page
    parses_as_recorded(&unchanged, &case);
    // the matcher's own window changes the condition: only "other" matches
    s.window.invoke_action("match".into());
    assert_eq!(s.window.get_error().as_str(), "");
    let matcher = s
        .slots
        .formula
        .strings
        .step
        .borrow()
        .as_ref()
        .expect("the matcher opens")
        .clone_strong();
    matcher.set_match_type(1);
    matcher.set_fixed("other".into());
    matcher.invoke_changed();
    matcher.invoke_apply();
    assert!(s.field("string match").text.contains("other"));
    let narrowed = s.apply().unwrap();
    let mut context = ParsingContext::new();
    context.insert("url".into(), "https://example.com/post/1".into());
    assert!(
        narrowed.parse(&context, "<p>blocked</p>").is_ok(),
        "no longer matches, so no veto"
    );
    assert!(narrowed.parse(&context, "<p>other</p>").is_err());
    // the polarity flips it: now not finding a match vetoes
    let s = open(&case.parser);
    s.window.invoke_toggled(9, false);
    assert!(!s.field("veto if match found").checked);
    let flipped = s.apply().unwrap();
    assert!(matches!(
        flipped.kind,
        ContentKind::Veto {
            if_matches_found: false,
            ..
        }
    ));
    // (the recorded matcher matches anything)
    assert!(flipped.parse(&context, "<p>blocked</p>").is_ok());
}

// leaf: content-veto
#[test]
fn veto_content_vetoes_as_the_reference_does_under_either_polarity_and_matcher() {
    let _windows = headless::init();
    let all = hydrus_testkit::fixture_json("parser_editors.json");
    let variants = all
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "veto_variants")
        .expect("recorded")["variants"]
        .as_array()
        .unwrap()
        .clone();
    let case = recorded("veto");
    let mut context = ParsingContext::new();
    context.insert("url".into(), "https://example.com/post/1".into());
    let mut vetoes = 0;
    for variant in &variants {
        let name = variant.to_string();
        // the editor: the polarity, and a matcher narrowed to a fixed string
        let s = open(&case.parser);
        let polarity = variant["if_match_found"].as_bool().unwrap();
        if s.field("veto if match found").checked != polarity {
            s.window.invoke_toggled(9, polarity);
        }
        if let Some(fixed) = variant["match"].as_str() {
            s.window.invoke_action("match".into());
            let matcher = s
                .slots
                .formula
                .strings
                .step
                .borrow()
                .as_ref()
                .expect("the matcher opens")
                .clone_strong();
            matcher.set_match_type(1);
            matcher.set_fixed(fixed.into());
            matcher.invoke_changed();
            matcher.invoke_apply();
        }
        let saved = s.apply().unwrap();
        let parsed = saved.parse(
            &context,
            &format!("<p>{}</p>", variant["text"].as_str().unwrap()),
        );
        match variant["error"].as_str() {
            Some(error) => {
                vetoes += 1;
                assert_eq!(parsed.unwrap_err().to_string(), error, "{name}");
            }
            None => {
                assert!(parsed.unwrap().contents.is_empty(), "{name}");
            }
        }
    }
    assert!(
        vetoes >= 4 && vetoes < variants.len(),
        "both outcomes were replayed"
    );
}

// leaf: content-tags
#[test]
fn tag_content_keeps_its_namespace_as_the_reference_does_and_previews_the_cleaned_tags() {
    let _windows = headless::init();
    let all = hydrus_testkit::fixture_json("parser_editors.json");
    let variants = all
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["case"] == "tag_variants")
        .expect("recorded")["variants"]
        .as_array()
        .unwrap()
        .clone();
    let case = recorded("tags");
    for variant in &variants {
        let name = format!("{} {}", variant["namespace"], variant["text"]);
        let s = open(&case.parser);
        // (the document the preview parses)
        s.window
            .set_document(format!("<p>{}</p>", variant["text"].as_str().unwrap()).into());
        let labels = s.labels();
        let any = labels
            .iter()
            .position(|l| l.contains("any namespace"))
            .unwrap_or_else(|| panic!("{labels:?}"));
        let any_field = s.window.get_fields().row_data(any).unwrap();
        match variant["namespace"].as_str() {
            Some(namespace) => {
                if any_field.checked {
                    s.window.invoke_toggled(any_field.id, false);
                }
                // (the namespace text is the field after the "any namespace" box)
                let text_field = s.window.get_fields().row_data(any + 1).unwrap();
                s.window.invoke_text_edited(text_field.id, namespace.into());
            }
            None => {
                if !any_field.checked {
                    s.window.invoke_toggled(any_field.id, true);
                }
            }
        }
        s.window.invoke_action("test".into());
        assert_eq!(
            s.window.get_preview().as_str(),
            variant["preview"].as_str().unwrap(),
            "{name}"
        );
        // and the parser it hands back holds the namespace as recorded
        let saved = s.apply().unwrap();
        match (&saved.kind, variant["additional_info"].as_str()) {
            (ContentKind::Tag { namespace }, theirs) => {
                assert_eq!(namespace.as_deref(), theirs, "{name}");
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn the_content_test_panel_previews_every_kind_as_the_reference_does() {
    let _windows = headless::init();
    let all = hydrus_testkit::fixture_json("parser_editors.json");
    let mut seen = 0;
    for kind in [
        "urls",
        "tags",
        "notes",
        "file hash",
        "timestamp",
        "watcher title",
        "http headers",
        "temporary variable",
        "veto",
    ] {
        let theirs = all
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["case"] == "content" && c["kind"] == kind)
            .unwrap();
        let case = recorded(kind);
        let s = open(&case.parser);
        s.window
            .set_document(format!("<p>{}</p>", case.text).into());
        s.window.set_test_url("https://example.com/post/1".into());
        s.window.invoke_action("test".into());
        assert_eq!(
            s.window.get_preview().as_str(),
            theirs["preview"].as_str().unwrap(),
            "{kind}"
        );
        seen += 1;
    }
    assert_eq!(seen, 9);
}
