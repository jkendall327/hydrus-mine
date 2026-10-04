//! Real reference panel recordings replayed through native definition drafts.

use hydrus_core::url::{AnyGug, GugOptions, UrlClassSettings};
use hydrus_gui_model::downloader_definitions::{self as definitions, Draft, Kind, RuleEdit};
use hydrus_gui_model::string_editors::MatchEditor;
use hydrus_legacy::objects::{domain, parsers};
use hydrus_legacy::serialisable::SerialisableObject;
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use serde_json::{Value, json};

fn object(value: &Value) -> SerialisableObject {
    SerialisableObject::from_tuple_str(&value.to_string()).unwrap()
}

#[test]
fn reference_rows_previews_and_vetoes_are_replayed() {
    let fixture = hydrus_testkit::fixture_json("downloader_definitions.json");
    let mut new_class = definitions::new_class();
    let expected_class = domain::url_class(&object(&fixture["new_class"])).unwrap();
    new_class.key.clone_from(&expected_class.key);
    assert_eq!(new_class, expected_class);
    let AnyGug::Single(mut expected_gug) = parsers::gug(&object(&fixture["new_gug"])).unwrap()
    else {
        panic!("single")
    };
    expected_gug.key.clear();
    assert_eq!(definitions::new_gug(), expected_gug);
    let AnyGug::Nested(mut expected_nested) =
        parsers::gug(&object(&fixture["new_nested"])).unwrap()
    else {
        panic!("nested")
    };
    expected_nested.key.clear();
    assert_eq!(definitions::new_nested(), expected_nested);
    let class = domain::url_class(&object(&fixture["class"])).unwrap();
    let classes = UrlClassSettings {
        url_classes: vec![class.clone()],
        ..Default::default()
    };
    let mut downloaders = Downloaders::default();
    let gug = parsers::gug(&object(&fixture["gug"])).unwrap();
    let nested = parsers::gug(&object(&fixture["nested"])).unwrap();
    downloaders.gugs.gugs = vec![gug.clone(), nested.clone()];
    assert_eq!(
        json!(definitions::class_row(&class, false)),
        fixture["class_row"]
    );
    assert_eq!(
        json!(definitions::gug_row(
            &gug,
            &downloaders,
            &classes,
            GugOptions::default()
        )),
        fixture["gug_row"]
    );
    assert_eq!(
        json!(definitions::gug_row(
            &nested,
            &downloaders,
            &classes,
            GugOptions::default()
        )),
        fixture["nested_row"]
    );
    for case in fixture["class_cases"].as_array().unwrap() {
        let class = domain::url_class(&object(&case["definition"])).unwrap();
        let preview = definitions::class_preview(&class, false);
        if case["veto"].is_null() {
            assert_eq!(preview.status, case["status"]);
        } else {
            assert!(
                case["status"]
                    .as_str()
                    .unwrap()
                    .starts_with("Example does not match - ")
            );
            assert_eq!(
                preview.status,
                "Example does not match - wrong.example did not match the domain mask"
            );
        }
        assert_eq!(preview.normalised, case["normalised"]);
        assert_eq!(preview.request, case["request"]);
        assert_eq!(preview.api, case["api"]);
        if case["veto"].is_null() {
            assert_eq!(preview.referral, case["referral"]);
            assert_eq!(preview.next, case["next"]);
            definitions::validate_class(&class, false).unwrap();
        } else {
            assert_eq!(
                definitions::validate_class(&class, false).unwrap_err(),
                case["veto"]
            );
        }
    }
    for case in fixture["gug_cases"].as_array().unwrap() {
        let AnyGug::Single(gug) = parsers::gug(&object(&case["definition"])).unwrap() else {
            panic!("single generator")
        };
        let (raw, matched, normalised) =
            definitions::gug_preview(&gug, &classes, GugOptions::default());
        assert_eq!(raw, case["raw"]);
        assert_eq!(matched, case["matched"]);
        assert_eq!(normalised, case["normalised"]);
        assert_eq!(json!(definitions::validate_gug(&gug).err()), case["veto"]);
    }
    let mut actions = Draft::new(classes.clone(), downloaders.clone(), Kind::Generators);
    actions.put_gug(gug.clone(), None).unwrap();
    assert_eq!(
        json!(
            actions
                .order()
                .iter()
                .map(|&i| actions.row(i))
                .collect::<Vec<_>>()
        ),
        fixture["after_duplicate"]
    );
    actions.selection.select_only(Some(0));
    let questions = fixture["delete_questions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actions.delete_question(), questions.join("\n\n"));
    actions.delete_selected();
    assert_eq!(
        json!(
            actions
                .order()
                .iter()
                .map(|&i| actions.row(i))
                .collect::<Vec<_>>()
        ),
        fixture["after_delete"]
    );
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|ctx| settings::set(ctx.conn(), &downloaders))
        .unwrap();
    actions.save(&store).unwrap();
    let saved: Downloaders = store.read(settings::get).unwrap();
    let expected = fixture["applied_gugs"].as_array().unwrap();
    assert_eq!(saved.gugs.gugs.len(), expected.len());
    for expected in expected {
        let expected = parsers::gug(&object(expected)).unwrap();
        let actual = saved
            .gugs
            .gugs
            .iter()
            .find(|actual| actual.name() == expected.name())
            .unwrap();
        // Native copied keys are random; every other field and repaired
        // nested membership comes from the recorded list's accepted actions.
        match (actual, expected) {
            (AnyGug::Single(actual), AnyGug::Single(mut expected)) => {
                expected.key.clone_from(&actual.key);
                assert_eq!(actual, &expected);
            }
            (AnyGug::Nested(actual), AnyGug::Nested(expected)) => assert_eq!(actual, &expected),
            _ => panic!("definition kind"),
        }
    }
    let mut draft = Draft::new(classes, downloaders, Kind::Classes);
    for case in fixture["checker"].as_array().unwrap() {
        assert_eq!(draft.check_url(case["url"].as_str().unwrap()), case["text"]);
    }
    assert_eq!(draft.selection.one(), Some(0));
    assert_eq!(definitions::CANCEL, fixture["questions"][0]);
    assert_eq!(definitions::REMOVE, fixture["questions"][1]);
    assert!(!fixture["cancelled"].as_bool().unwrap());
    let name_vetoes = ["", "token", "page"]
        .into_iter()
        .map(|name| definitions::validate_parameter_name(name, &["token".into()]).err())
        .collect::<Vec<_>>();
    assert_eq!(json!(name_vetoes), fixture["parameter_name_vetoes"]);
    assert_eq!(
        definitions::association_notice(hydrus_core::url::UrlType::Gallery, true).unwrap(),
        fixture["association_notices"][0]
    );
    assert_eq!(
        definitions::association_notice(hydrus_core::url::UrlType::Post, false).unwrap(),
        fixture["association_notices"][1]
    );
    assert_eq!(
        definitions::association_notice(hydrus_core::url::UrlType::Post, true),
        None
    );
    for case in fixture["api_cases"].as_array().unwrap() {
        let class = domain::url_class(&object(&case["definition"])).unwrap();
        assert_eq!(
            json!(definitions::validate_class(&class, false).err()),
            case["veto"]
        );
        let preview = definitions::class_preview(&class, false);
        assert_eq!(preview.status, case["status"]);
        assert_eq!(preview.request, case["request"]);
    }
}

#[test]
fn drafts_preserve_identity_links_and_current_unrelated_settings() {
    let fixture = hydrus_testkit::fixture_json("downloader_definitions.json");
    let class = domain::url_class(&object(&fixture["class"])).unwrap();
    let gug = parsers::gug(&object(&fixture["gug"])).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let classes = UrlClassSettings {
        url_classes: vec![class.clone()],
        parser_links: vec![(hex::encode(&class.key), Some("parser-key".into()))],
        parser_keys: vec!["parser-key".into()],
        collapse_leading_slashes: true,
    };
    let mut downloaders = Downloaders::default();
    downloaders.gugs.gugs.push(gug.clone());
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &classes)?;
            settings::set(ctx.conn(), &downloaders)
        })
        .unwrap();
    let mut draft = Draft::load(&store, Kind::Classes).unwrap();
    assert!(!draft.changed());
    let mut edited = class.clone();
    edited.name = "renamed gallery".into();
    edited.preferred_scheme = "http".into();
    draft.put_class(edited.clone(), Some(0)).unwrap();
    assert!(draft.changed());
    assert_eq!(draft.classes.url_classes[0].key, class.key);
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .class_for(&class.example_url)
            .unwrap()
            .name,
        class.name,
        "draft not applied"
    );
    // Another definition editor updates links and parser keys after this opens.
    store
        .write(move |ctx| {
            let mut current: UrlClassSettings = settings::get(ctx.conn())?;
            current.parser_keys.push("other-parser".into());
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    draft.save(&store).unwrap();
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .class_for(&class.example_url)
            .unwrap()
            .name,
        "renamed gallery"
    );
    assert!(
        store
            .snapshot()
            .url_classes
            .normalise(&class.example_url, false)
            .unwrap()
            .starts_with("http://")
    );
    let current: UrlClassSettings = store.read(settings::get).unwrap();
    assert_eq!(current.parser_links, draft.classes.parser_links);
    assert!(current.parser_keys.contains(&"other-parser".into()));
    assert!(current.collapse_leading_slashes);
    assert_eq!(current.url_classes[0], edited);
    draft.put_class(class, None).unwrap();
    let copy = &draft.classes.url_classes[1];
    assert_ne!(copy.key, draft.classes.url_classes[0].key);
    let copied_key = copy.key.clone();
    draft.put_class(copy.clone(), None).unwrap();
    assert_eq!(draft.classes.url_classes[2].name, "example gallery (1)");
    assert_ne!(draft.classes.url_classes[2].key, copied_key);
    draft.selection.select_many(&[1, 2]);
    draft.delete_selected();
    assert_eq!(draft.classes.url_classes.len(), 1);
    assert!(draft.selection.is_empty());
    assert_eq!(
        store
            .read::<UrlClassSettings>(settings::get)
            .unwrap()
            .url_classes
            .len(),
        1
    );
}

#[test]
fn generator_copies_nested_repair_and_delete_are_staged() {
    let fixture = hydrus_testkit::fixture_json("downloader_definitions.json");
    let gug = parsers::gug(&object(&fixture["gug"])).unwrap();
    let nested = parsers::gug(&object(&fixture["nested"])).unwrap();
    let mut downloaders = Downloaders::default();
    downloaders.gugs.gugs = vec![gug.clone(), nested.clone()];
    downloaders.gugs.keys_to_display = vec![gug.key().into()];
    let mut draft = Draft::new(
        UrlClassSettings::default(),
        downloaders.clone(),
        Kind::Generators,
    );
    draft.put_gug(gug.clone(), None).unwrap();
    assert_ne!(draft.downloaders.gugs.gugs[2].key(), gug.key());
    assert_eq!(draft.downloaders.gugs.gugs[2].name(), "example search (1)");
    draft.put_gug(nested, Some(1)).unwrap();
    let AnyGug::Nested(n) = &draft.downloaders.gugs.gugs[1] else {
        panic!("nested")
    };
    assert_eq!(n.gugs.len(), 1, "missing members repaired on editor Apply");
    draft.selection.select_only(Some(0));
    assert!(draft.delete_question().contains("combined search"));
    draft.delete_selected();
    assert!(
        !draft
            .downloaders
            .gugs
            .keys_to_display
            .contains(&gug.key().into())
    );
    draft.kind = Kind::Nested;
    assert_eq!(
        draft.row(0)[2],
        "yes",
        "missing member remains visible in draft"
    );
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|ctx| settings::set(ctx.conn(), &downloaders))
        .unwrap();
    store
        .write(|ctx| {
            let mut current: Downloaders = settings::get(ctx.conn())?;
            current
                .unconverted
                .push(hydrus_parse::downloaders::Unconverted {
                    kind: "parser".into(),
                    name: "preserved".into(),
                    reason: "test".into(),
                });
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    draft.save(&store).unwrap();
    let stored: Downloaders = store.read(settings::get).unwrap();
    assert_eq!(stored.gugs.gugs.len(), draft.downloaders.gugs.gugs.len());
    let AnyGug::Nested(n) = &stored.gugs.gugs[0] else {
        panic!("nested")
    };
    assert!(n.gugs.is_empty(), "missing members repaired on list Apply");
    assert_eq!(stored.unconverted[0].name, "preserved");
    assert_eq!(Draft::load(&store, Kind::Nested).unwrap().row(0)[2], "");
}

#[test]
fn invalid_examples_and_defaults_cannot_be_committed() {
    let mut draft = Draft::new(
        UrlClassSettings::default(),
        Downloaders::default(),
        Kind::Classes,
    );
    assert!(
        draft
            .put_class(hydrus_core::url::UrlClass::default(), None)
            .is_err()
    );
    assert!(draft.classes.url_classes.is_empty());
    let mut gug = definitions::new_gug();
    gug.replacement_phrase.clear();
    assert!(draft.put_gug(AnyGug::Single(gug), None).is_err());
    assert!(draft.downloaders.gugs.gugs.is_empty());
    let fixed = hydrus_core::url::StringMatch {
        kind: hydrus_core::url::strings::MatchKind::Fixed("valid".into()),
        example: "valid".into(),
        ..hydrus_core::url::StringMatch::any()
    };
    let mut rule = RuleEdit {
        matcher: MatchEditor::new(&fixed),
        name: "test".into(),
        default_enabled: true,
        default: "wrong".into(),
        ephemeral: false,
        default_processor: hydrus_core::url::StringProcessor::default(),
    };
    assert_eq!(
        rule.value().unwrap_err(),
        "That default value does not match the rule!"
    );
    assert_eq!(
        rule.parameter_value().unwrap().1,
        Some("wrong".into()),
        "query defaults may be transformed by ephemeral processors"
    );
    rule.default = "valid".into();
    assert_eq!(rule.value().unwrap().1, Some("valid".into()));
    rule.default_enabled = false;
    assert_eq!(rule.value().unwrap().1, None);
}

#[test]
fn previews_use_native_encoding_options_and_nested_columns_sort_by_count() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    store
        .write(|ctx| {
            let mut network: hydrus_store::network::NetworkSettings = settings::get(ctx.conn())?;
            network.gug_percent_twenty_is_space = true;
            settings::set(ctx.conn(), &network)
        })
        .unwrap();
    let mut draft = Draft::load(&store, Kind::Generators).unwrap();
    let mut gug = definitions::new_gug();
    gug.example_search_text = "blue%20eyes".into();
    draft.put_gug(AnyGug::Single(gug), None).unwrap();
    assert_eq!(
        draft.row(0)[1],
        "https://example.com/search?q=blue+eyes&index=0"
    );
    let mut many = definitions::new_nested();
    many.gugs = vec![("a".into(), "a".into()), ("b".into(), "b".into())];
    let mut one = definitions::new_nested();
    one.gugs = vec![("z".into(), "z".into())];
    draft.downloaders.gugs.gugs = vec![AnyGug::Nested(many), AnyGug::Nested(one)];
    draft.kind = Kind::Nested;
    draft.sort_column = 1;
    assert_eq!(draft.order(), vec![1, 0]);
    draft.ascending = false;
    assert_eq!(draft.order(), vec![0, 1]);
    draft.selection.select_only(Some(1));
    assert_eq!(draft.selection.in_order(&draft.order()), vec![1]);
    let fixture = hydrus_testkit::fixture_json("downloader_definitions.json");
    let mut class = domain::url_class(&object(&fixture["class"])).unwrap();
    let mut classes = Draft::load(&store, Kind::Classes).unwrap();
    class.name = "Z gallery".into();
    classes.put_class(class.clone(), None).unwrap();
    class.name = "A gallery".into();
    classes.put_class(class.clone(), None).unwrap();
    assert_eq!(
        classes.check_url(&class.example_url),
        "Matches \"A gallery\""
    );
    assert_eq!(classes.selection.one(), Some(1));
    classes.save(&store).unwrap();
    assert_eq!(
        store
            .snapshot()
            .url_classes
            .class_for(&class.example_url)
            .unwrap()
            .name,
        "A gallery"
    );
}

#[test]
fn editor_controls_change_matching_normalisation_and_validate_pagination() {
    use hydrus_core::url::class::GalleryIndexPosition;
    use hydrus_gui_model::downloader_definitions::{DefinitionEditor, EditValue};

    let draft = Draft::new(
        UrlClassSettings::default(),
        Downloaders::default(),
        Kind::Classes,
    );
    let mut editor =
        DefinitionEditor::new(EditValue::Class(Box::new(definitions::new_class())), &draft);
    assert!(editor.validate().is_ok());
    editor.choose(1, 1);
    editor.choose(2, 0);
    editor.choose(20, 2);
    editor.text(21, "id".into());
    editor.text(22, "2".into());
    editor.toggle(5, true);
    editor.toggle(6, true);
    editor.text(
        40,
        "https://sub.hostname.com/post/page.php?s=view&id=123456".into(),
    );
    assert!(editor.validate().is_ok());
    let EditValue::Class(class) = &editor.value else {
        panic!("class")
    };
    assert!(!class.should_be_associated_with_files);
    assert_eq!(
        class.gallery_index.as_ref().unwrap().position,
        GalleryIndexPosition::Parameter("id".into())
    );
    let preview = definitions::class_preview(class, false);
    assert_eq!(
        preview.normalised,
        "http://sub.hostname.com/post/page.php?id=123456&s=view"
    );
    assert_eq!(
        preview.next,
        "http://sub.hostname.com/post/page.php?id=123458&s=view"
    );
    editor.text(22, "0".into());
    assert_eq!(
        editor.validate().unwrap_err(),
        "Please enter a page delta from 1 to 65536."
    );
    editor.text(22, "65537".into());
    assert!(editor.validate().is_err());
    editor.text(22, "65536".into());
    assert!(editor.validate().is_ok());
    editor.choose(20, 1);
    editor.text(21, "invalid".into());
    assert_eq!(
        editor.validate().unwrap_err(),
        "Please enter a path component index."
    );
    editor.choose(20, 0);
    editor.toggle(5, false);
    assert!(editor.validate().is_err());
    editor.text(
        40,
        "https://hostname.com/post/page.php?id=123456&s=view".into(),
    );
    assert!(editor.validate().is_ok());

    let mut gug = DefinitionEditor::new(
        EditValue::Gug(AnyGug::Single(definitions::new_gug())),
        &draft,
    );
    gug.text(1, "https://example.com/search?q=%tags%".into());
    gug.text(3, "+".into());
    gug.text(5, "blue eyes".into());
    assert!(
        gug.preview()
            .contains("raw url: https://example.com/search?q=blue+eyes")
    );
    gug.text(2, "%missing%".into());
    assert_eq!(
        gug.validate().unwrap_err(),
        "Please ensure your generator can make an example url!"
    );
}
