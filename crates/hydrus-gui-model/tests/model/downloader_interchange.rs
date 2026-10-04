//! Reference package duplicate decisions, remapping, atomicity and cancellation.
use hydrus_core::url::{AnyGug, UrlClassSettings};
use hydrus_gui_model::downloader_interchange::{self as exchange, Auxiliary, Draft, Native};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, settings};
use serde_json::json;
fn payload() -> Vec<exchange::Definition> {
    let fixture = hydrus_testkit::fixture_json("downloader_interchange.json");
    exchange::decode_text(&fixture["reference"].to_string())
        .unwrap()
        .into_iter()
        .filter(|d| {
            matches!(
                d.native,
                Native::Class(_) | Native::Gug(_) | Native::Page(_)
            )
        })
        .collect()
}
#[test]
fn reference_duplicate_rules_remap_nested_members_and_keep_auxiliary_context() {
    let fixture = hydrus_testkit::fixture_json("downloader_interchange.json");
    let mut draft = Draft::new(
        UrlClassSettings::default(),
        Downloaders::default(),
        Auxiliary::default(),
    );
    let first = draft.import(payload()).unwrap();
    assert!(!first.added.is_empty());
    let single = draft
        .downloaders
        .gugs
        .gugs
        .iter()
        .find(|g| g.name() == "exchange gallery")
        .unwrap();
    let nested = draft
        .downloaders
        .gugs
        .gugs
        .iter()
        .find_map(|g| match g {
            AnyGug::Nested(n) if n.name == "exchange nested" => Some(n),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        nested.gugs,
        vec![(single.key().to_owned(), single.name().to_owned())]
    );
    assert_ne!(single.key(), "11".repeat(32));
    let count = draft.definitions().len();
    let duplicate = draft.import(payload()).unwrap();
    assert!(duplicate.added.is_empty());
    assert_eq!(duplicate.duplicates, count);
    assert!(
        fixture["duplicates"]["gug_ignores_key_and_name"]
            .as_bool()
            .unwrap()
    );
    assert!(
        fixture["duplicates"]["parser_ignores_name_key_examples"]
            .as_bool()
            .unwrap()
    );
    let page = draft
        .downloaders
        .parsers
        .iter_mut()
        .find(|p| p.name == "exchange page")
        .unwrap();
    page.name = "native edited page".into();
    page.content_parsers[0].name = "native edited content".into();
    let text = exchange::encode_text(&draft.definitions()).unwrap();
    assert!(text.contains("preserve child"));
    assert!(text.contains("preserve \\u65e5\\u672c"));
    assert!(text.contains("native edited content"));
    assert!(text.contains("native edited page"));
}
#[test]
fn package_invalid_and_cancelled_staging_do_not_mutate_store_and_stale_apply_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut draft = Draft::load(&store).unwrap();
    let original = draft.definitions();
    let mut mixed = payload();
    mixed.push(exchange::Definition::new(Native::Content(
        hydrus_gui_model::parser_editors::new_content(),
    )));
    assert!(draft.import(mixed).is_err());
    assert_eq!(draft.definitions(), original);
    draft.import(payload()).unwrap();
    drop(draft);
    let empty: Downloaders = store.read(settings::get).unwrap();
    assert!(empty.parsers.is_empty());
    let mut draft = Draft::load(&store).unwrap();
    draft.import(payload()).unwrap();
    store
        .write_and_refresh(|ctx| {
            let mut classes: UrlClassSettings = settings::get(ctx.conn())?;
            classes.collapse_leading_slashes = true;
            settings::set(ctx.conn(), &classes)
        })
        .unwrap();
    assert!(draft.save(&store).is_err());
    assert!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .is_empty()
    );
    let mut current = Draft::load(&store).unwrap();
    current.import(payload()).unwrap();
    current.save(&store).unwrap();
    let reopened = Draft::load(&store).unwrap();
    assert_eq!(reopened.definitions(), current.definitions());
}
#[test]
fn scoped_list_import_is_atomic_and_parser_save_preserves_original_context() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut parsers = hydrus_gui_model::parser_editors::Draft::load(&store).unwrap();
    let pages = payload()
        .into_iter()
        .filter(|d| matches!(d.native, Native::Page(_)))
        .collect::<Vec<_>>();
    parsers.import(pages).unwrap();
    parsers.save(&store).unwrap();
    let reloaded = hydrus_gui_model::parser_editors::Draft::load(&store).unwrap();
    let exported = exchange::encode_text(
        &reloaded
            .parsers
            .iter()
            .cloned()
            .map(|p| reloaded.auxiliary.definition(Native::Page(p)))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(exported.contains("preserve child"));
    let mut scoped = hydrus_gui_model::downloader_definitions::Draft::load(
        &store,
        hydrus_gui_model::downloader_definitions::Kind::Classes,
    )
    .unwrap();
    let original = scoped.classes.clone();
    assert!(scoped.import(payload()).is_err());
    assert_eq!(scoped.classes, original);
}
#[test]
fn imported_class_and_parser_link_reaches_the_live_registry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut class = hydrus_gui_model::downloader_definitions::new_class();
    class.key = vec![1; 32];
    let mut parser = hydrus_gui_model::parser_editors::new_page();
    parser.key = "02".repeat(32);
    parser.example_urls = vec![class.example_url.clone()];
    let mut content = hydrus_gui_model::parser_editors::new_content();
    content.kind = hydrus_parse::content::ContentKind::Tag {
        namespace: Some("imported".into()),
    };
    content.formula.kind = hydrus_parse::formula::FormulaKind::Static {
        text: "hello".into(),
        count: 1,
    };
    parser.content_parsers.push(content);
    let text = exchange::encode_text(&[
        exchange::Definition::new(Native::Class(Box::new(class.clone()))),
        exchange::Definition::new(Native::Page(parser)),
    ])
    .unwrap();
    let mut draft = Draft::load(&store).unwrap();
    draft.import(exchange::decode_text(&text).unwrap()).unwrap();
    draft.save(&store).unwrap();
    let registry = &store.snapshot().url_classes;
    let matched = registry.class_for(&class.example_url).unwrap();
    let link = registry
        .settings()
        .parser_links
        .iter()
        .find(|(key, _)| key == &hex::encode(&matched.key))
        .unwrap()
        .1
        .clone()
        .unwrap();
    let saved: Downloaders = store.read(settings::get).unwrap();
    let result = saved
        .parser(&link)
        .unwrap()
        .parse(&mut std::collections::BTreeMap::default(), "document")
        .unwrap();
    assert_eq!(result[0].tags(), ["imported:hello".to_owned()].into());
    assert_eq!(
        serde_json::to_value(registry.settings().parser_keys.len()).unwrap(),
        json!(1)
    );
}

#[test]
fn duplicate_class_relinks_the_runtime_winner_instead_of_the_first_equivalent_row() {
    use hydrus_core::url::UrlClasses;
    let mut first = hydrus_gui_model::downloader_definitions::new_class();
    first.key = vec![1; 32];
    let mut winner = first.clone();
    winner.name = "runtime winner".into();
    winner.key = vec![2; 32];
    winner.example_url = first.example_url.replace("123456", "12345678901234567890");
    let mut parser = hydrus_gui_model::parser_editors::new_page();
    parser.example_urls = vec![first.example_url.clone()];
    let classes = UrlClassSettings {
        url_classes: vec![first.clone(), winner.clone()],
        parser_links: vec![(hex::encode(&winner.key), Some("old parser".into()))],
        ..Default::default()
    };
    assert_eq!(
        UrlClasses::new(classes.clone())
            .class_for(&first.example_url)
            .unwrap()
            .key,
        winner.key
    );
    let mut draft = Draft::new(classes, Downloaders::default(), Auxiliary::default());
    draft
        .import(vec![
            exchange::Definition::new(Native::Class(Box::new(first))),
            exchange::Definition::new(Native::Page(parser)),
        ])
        .unwrap();
    assert_eq!(draft.downloaders.parsers.len(), 1);
    assert_eq!(
        draft
            .classes
            .parser_links
            .iter()
            .find(|(key, _)| key == &hex::encode(&winner.key))
            .unwrap()
            .1,
        Some(draft.downloaders.parsers[0].key.clone())
    );
}
#[test]
fn an_api_pair_source_yields_to_its_eligible_target_during_parser_linking() {
    use hydrus_core::url::{
        StringMatch, UrlClasses,
        strings::{Conversion, PyRegex},
    };
    let mut source = hydrus_gui_model::downloader_definitions::new_class();
    source.key = vec![1; 32];
    source
        .api_lookup_converter
        .conversions
        .push(Conversion::RegexSub {
            pattern: PyRegex::new("/post/"),
            replacement: "/api/".into(),
        });
    let mut target = source.clone();
    target.name = "api target".into();
    target.key = vec![2; 32];
    target.path_components[0].0 = StringMatch::any();
    target.api_lookup_converter.conversions.clear();
    target.example_url = source.example_url.replace("/post/", "/api/");
    let mut parser = hydrus_gui_model::parser_editors::new_page();
    parser.example_urls = vec![source.example_url.clone()];
    let classes = UrlClassSettings {
        url_classes: vec![source.clone(), target.clone()],
        ..Default::default()
    };
    assert_eq!(
        UrlClasses::new(classes.clone())
            .class_for(&source.example_url)
            .unwrap()
            .key,
        source.key
    );
    let mut draft = Draft::new(classes, Downloaders::default(), Auxiliary::default());
    draft
        .import(vec![exchange::Definition::new(Native::Page(parser))])
        .unwrap();
    assert_eq!(
        draft.classes.parser_links,
        vec![(
            hex::encode(&target.key),
            Some(draft.downloaders.parsers[0].key.clone())
        )]
    );
    assert!(
        UrlClasses::new(draft.classes)
            .url_to_fetch_and_parser(&source.example_url)
            .is_ok()
    );
}
