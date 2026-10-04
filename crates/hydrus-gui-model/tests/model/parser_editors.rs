//! Typed parser edits replay the running reference's real Qt controls.
use hydrus_core::url::UrlClassSettings;
use hydrus_gui_model::formula_editors::FormulaTestData;
use hydrus_gui_model::parser_editors::{self as editors, ContentEditor, Draft, TestContext};
use hydrus_legacy::objects::parsers;
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::Downloaders;
use hydrus_parse::content::ContentKind;
use hydrus_store::{Store, settings};

#[test]
fn recorded_content_kinds_and_runtime_previews() {
    for case in hydrus_testkit::fixture_json("parser_editors.json")
        .as_array()
        .unwrap()
    {
        if case["case"] != "content" {
            continue;
        }
        let object = SerialisableObject::from_tuple_str(&case["tuple"].to_string()).unwrap();
        let parser = parsers::content_parser(&object).unwrap();
        let editor = ContentEditor::new(
            &parser,
            FormulaTestData {
                context: TestContext::parse("https://example.com/post/1".into(), "0", "")
                    .unwrap()
                    .values(),
                text: format!("<p>{}</p>", case["text"].as_str().unwrap()),
                collapse_newlines: case["collapse_newlines"].as_bool().unwrap(),
            },
        );
        assert!(!editor.changed());
        assert_eq!(
            editors::CONTENT_TYPES[editor.kind_index()],
            case["kind"].as_str().unwrap()
        );
        let preview = editor.preview();
        if case["error"].is_null() {
            let results = preview
                .unwrap()
                .contents
                .into_iter()
                .map(|c| c.text)
                .collect::<Vec<_>>();
            assert_eq!(serde_json::to_value(results).unwrap(), case["results"]);
        } else {
            assert!(preview.is_err());
        }
    }
}
#[test]
fn test_context_rejects_invalid_duplicate_and_reserved_variables() {
    for (index, variables) in [
        ("-1", ""),
        ("bad", ""),
        ("0", "bad"),
        ("0", "x=1\nx=2"),
        ("0", "url=bad"),
        ("0", "post_index=1"),
        ("0", "=bad"),
    ] {
        assert!(TestContext::parse(String::new(), index, variables).is_err());
    }
    let context = TestContext::parse("https://example.com".into(), "12", "token=a=b").unwrap();
    assert_eq!(context.values()["token"], "a=b");
    assert_eq!(context.values()["post_index"], "12");
}
#[test]
fn content_change_keeps_unsupported_formula_and_note_newlines() {
    let mut parser = editors::new_content();
    parser.formula.kind = hydrus_parse::formula::FormulaKind::Static {
        text: "a\n\nb".into(),
        count: 1,
    };
    let formula = parser.formula.clone();
    let mut editor = ContentEditor::new(&parser, FormulaTestData::default());
    editor.change_kind(2);
    editor.parser.kind = ContentKind::Note {
        name: String::new(),
    };
    assert_eq!(
        editor.value().kind,
        ContentKind::Note {
            name: "note".into()
        }
    );
    assert!(!editor.test.collapse_newlines);
    assert_eq!(editor.parser.formula, formula);
    assert_eq!(
        editor.preview().unwrap().notes(),
        vec![("note".into(), "a\n\nb".into())]
    );
}
#[test]
fn native_parser_save_preserves_other_settings_and_rejects_stale_edits() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let mut draft = Draft::load(&store).unwrap();
    let mut page = editors::new_page();
    page.content_parsers.push(editors::new_content());
    draft.put(None, page).unwrap();
    draft.save(&store).unwrap();
    assert_eq!(store.snapshot().url_classes.settings().parser_keys.len(), 1);
    let mut first = Draft::load(&store).unwrap();
    let mut stale = first.clone();
    first.parsers[0].name = "changed".into();
    first.save(&store).unwrap();
    stale.parsers[0].name = "stale".into();
    assert!(stale.save(&store).is_err());
    let before: Downloaders = store.read(settings::get).unwrap();
    let mut later = Draft::load(&store).unwrap();
    later.parsers[0].name = "later".into();
    store
        .write_and_refresh(|ctx| {
            let mut classes: UrlClassSettings = settings::get(ctx.conn())?;
            classes.collapse_leading_slashes = true;
            settings::set(ctx.conn(), &classes)
        })
        .unwrap();
    later.save(&store).unwrap();
    let saved: Downloaders = store.read(settings::get).unwrap();
    let classes: UrlClassSettings = store.read(settings::get).unwrap();
    assert!(classes.collapse_leading_slashes);
    assert_eq!(saved.gugs, before.gugs);
    assert_eq!(saved.unconverted, before.unconverted);
    assert!(later.put(Some("removed-key"), editors::new_page()).is_err());
}

#[test]
fn any_namespace_toggle_retains_the_disabled_namespace() {
    let mut parser = editors::new_content();
    parser.kind = ContentKind::Tag {
        namespace: Some("artist".into()),
    };
    let mut editor = ContentEditor::new(&parser, FormulaTestData::default());
    editor.set_any_namespace(true);
    assert_eq!(editor.parser.kind, ContentKind::Tag { namespace: None });
    assert_eq!(editor.namespace_text(), "artist");
    editor.set_any_namespace(false);
    assert_eq!(editor.parser, parser);
    assert!(!editor.changed());
}

#[test]
fn changed_links_revalidate_current_class_capability_and_rollback_atomically() {
    use hydrus_core::url::strings::Conversion;
    use hydrus_core::url::{UrlClass, UrlType};
    for change in 0..5 {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let mut page = editors::new_page();
        page.key = "page".into();
        page.name = "original".into();
        let values = Downloaders {
            parsers: vec![page],
            ..Downloaders::default()
        };
        let classes = UrlClassSettings {
            url_classes: vec![UrlClass {
                key: vec![1],
                url_type: UrlType::Post,
                ..UrlClass::default()
            }],
            parser_keys: vec!["page".into()],
            ..UrlClassSettings::default()
        };
        store
            .write_and_refresh(move |ctx| {
                settings::set(ctx.conn(), &values)?;
                settings::set(ctx.conn(), &classes)
            })
            .unwrap();
        let mut draft = Draft::load(&store).unwrap();
        draft.link("01", Some("page")).unwrap();
        draft.parsers[0].name = "edited".into();
        store
            .write_and_refresh(move |ctx| {
                let mut classes: UrlClassSettings = settings::get(ctx.conn())?;
                match change {
                    0 => classes.url_classes[0].url_type = UrlType::File,
                    1 => classes.url_classes[0].url_type = UrlType::Api,
                    2 => classes.url_classes[0]
                        .api_lookup_converter
                        .conversions
                        .push(Conversion::Prepend("redirect".into())),
                    3 => classes.url_classes.clear(),
                    _ => classes.url_classes[0].name = "new class name".into(),
                }
                settings::set(ctx.conn(), &classes)
            })
            .unwrap();
        let before_classes: UrlClassSettings = store.read(settings::get).unwrap();
        if change < 4 {
            assert!(draft.save(&store).is_err());
            let after: Downloaders = store.read(settings::get).unwrap();
            assert_eq!(after.parsers[0].name, "original");
            let after: UrlClassSettings = store.read(settings::get).unwrap();
            assert_eq!(after, before_classes);
        } else {
            draft.save(&store).unwrap();
            let after: UrlClassSettings = store.read(settings::get).unwrap();
            assert_eq!(after.url_classes[0].name, "new class name");
            assert_eq!(after.parser_links, vec![("01".into(), Some("page".into()))]);
        }
    }
}
