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

fn mixed_login_payload() -> Vec<exchange::Definition> {
    let fixture = hydrus_testkit::fixture_json("mixed_login_packages.json");
    exchange::decode_text(&fixture["reference"].to_string()).unwrap()
}

fn recorded_login_manager() -> hydrus_parse::login::LoginManager {
    use hydrus_parse::login::{Access, DomainLogin, LoginManager, Validity};
    let fixture = hydrus_testkit::fixture_json("mixed_login_packages.json");
    let Native::Login(mut old) = mixed_login_payload()
        .into_iter()
        .find(|definition| matches!(definition.native, Native::Login(_)))
        .unwrap()
        .native
    else {
        unreachable!()
    };
    let before = &fixture["before"]["domains"]["packages.example"];
    old.key = before[0].as_str().unwrap().into();
    old.examples.truncate(1);
    old.examples[0].access = Access::UserPreferences;
    old.examples[0].description = before[4].as_str().unwrap().into();
    LoginManager {
        scripts: vec![old],
        domains: [(
            "packages.example".into(),
            DomainLogin {
                script_key: before[0].as_str().unwrap().into(),
                script_name: before[1].as_str().unwrap().into(),
                credentials: serde_json::from_value(before[2].clone()).unwrap(),
                access: Access::UserPreferences,
                description: before[4].as_str().unwrap().into(),
                active: before[5].as_bool().unwrap(),
                validity: Validity::Invalid,
                validity_error: before[7].as_str().unwrap().into(),
                no_work_until: before[8].as_i64().unwrap(),
                delay_reason: before[9].as_str().unwrap().into(),
            },
        )]
        .into(),
    }
}

#[test]
fn mixed_login_import_replays_qt_duplicates_links_and_saved_consumer_without_exporting_credentials()
{
    let fixture = hydrus_testkit::fixture_json("mixed_login_packages.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let original = recorded_login_manager();
    let saved = original.clone();
    store
        .write(move |ctx| hydrus_store::logins::save(ctx.conn(), &saved))
        .unwrap();
    let mut draft = Draft::load(&store).unwrap();
    let review = draft.import(mixed_login_payload()).unwrap();
    assert_eq!(
        review.added.len(),
        fixture["mixed"].as_array().unwrap().len()
    );
    assert!(review.text().contains("Login Script: mixed package login"));
    assert_eq!(store.read(hydrus_store::logins::load).unwrap(), original);
    let mut concurrent = original.clone();
    let domain = concurrent.domains.get_mut("packages.example").unwrap();
    domain
        .credentials
        .insert("username".into(), "concurrentdummyuser".into());
    domain.active = true;
    let saved = concurrent.clone();
    store
        .write(move |ctx| hydrus_store::logins::save(ctx.conn(), &saved))
        .unwrap();
    draft.save(&store).unwrap();
    let reopened = Draft::load(&store).unwrap();
    let manager = store.read(hydrus_store::logins::load).unwrap();
    assert_eq!(
        manager.scripts.len(),
        fixture["accepted"]["scripts"].as_array().unwrap().len()
    );
    let current = manager.domains.get("packages.example").unwrap();
    let script = manager.script(current).unwrap();
    assert_eq!(script.name, "mixed package login");
    assert_ne!(script.key, "71".repeat(32));
    assert_ne!(script.key, original.scripts[0].key);
    assert_eq!(
        current.credentials,
        concurrent.domains["packages.example"].credentials
    );
    assert!(current.active);
    assert_eq!(
        current.validity_error,
        original.domains["packages.example"].validity_error
    );
    assert_eq!(
        current.no_work_until,
        original.domains["packages.example"].no_work_until
    );
    assert_eq!(
        current.delay_reason,
        original.domains["packages.example"].delay_reason
    );
    assert_eq!(
        current.access.code(),
        fixture["accepted"]["domains"]["packages.example"][3]
            .as_i64()
            .unwrap()
    );
    assert_eq!(
        current.description,
        fixture["accepted"]["domains"]["packages.example"][4]
            .as_str()
            .unwrap()
    );
    assert!(!manager.domains.contains_key("unconfigured.example"));
    script.check_valid().unwrap();
    script.check_credentials(&current.credentials).unwrap();
    assert!(
        store
            .read(settings::get::<hydrus_store::network::LoginDomains>)
            .unwrap()
            .0
            .contains(&"packages.example".into())
    );
    let definitions = reopened.definitions();
    let selected: std::collections::BTreeSet<_> = definitions
        .iter()
        .enumerate()
        .filter_map(|(i, definition)| {
            matches!(&definition.native, Native::Login(script) if script.key == current.script_key)
                .then_some(i)
        })
        .collect();
    let login_only = reopened.export(&selected);
    assert_eq!(
        login_only.len(),
        fixture["login_only"].as_array().unwrap().len()
    );
    let text = exchange::encode_text(&login_only).unwrap();
    assert!(!text.contains("concurrentdummyuser"));
    assert!(!text.contains("dummy-retained-password"));
    let decoded = hydrus_downloader_exchange::logins::decode_text(&text).unwrap();
    assert_eq!(&decoded[0], script);
    let nested = definitions
        .iter()
        .position(|definition| matches!(&definition.native, Native::Gug(AnyGug::Nested(_))))
        .unwrap();
    let with_dependencies = reopened.export(&[nested].into());
    assert_eq!(with_dependencies.len(), 5);
    assert!(
        !with_dependencies
            .iter()
            .any(|definition| matches!(definition.native, Native::Login(_)))
    );
    let mut duplicate = reopened.clone();
    let review = duplicate.import(mixed_login_payload()).unwrap();
    assert_eq!(review.duplicates, 6);
    assert!(review.added.is_empty());
}

#[test]
fn mixed_login_cancel_malformed_and_stale_scripts_never_partially_save_downloaders() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut cancelled = Draft::load(&store).unwrap();
    cancelled.import(mixed_login_payload()).unwrap();
    drop(cancelled);
    assert!(
        store
            .read(hydrus_store::logins::load)
            .unwrap()
            .scripts
            .is_empty()
    );
    let mut stale = Draft::load(&store).unwrap();
    let original = stale.definitions();
    let mut unsupported = mixed_login_payload();
    unsupported.push(exchange::Definition::new(Native::Content(
        hydrus_gui_model::parser_editors::new_content(),
    )));
    assert!(stale.import(unsupported).is_err());
    assert_eq!(stale.definitions(), original);
    stale.import(mixed_login_payload()).unwrap();
    store
        .write(|ctx| {
            let mut manager = hydrus_store::logins::load(ctx.conn())?;
            manager
                .scripts
                .push(hydrus_parse::login::LoginScript::default());
            hydrus_store::logins::save(ctx.conn(), &manager)
        })
        .unwrap();
    assert!(stale.save(&store).is_err());
    assert!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .is_empty()
    );
    assert!(
        store
            .read::<UrlClassSettings>(settings::get)
            .unwrap()
            .url_classes
            .is_empty()
    );
    assert_eq!(
        store
            .read(hydrus_store::logins::load)
            .unwrap()
            .scripts
            .len(),
        1
    );
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
