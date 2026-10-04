//! Exact owned sort-cog service/view menus, checks and metadata preservation.
use hydrus_core::{
    pages::{PageSort, PageSortBy},
    search::context::TagContext,
};
use hydrus_gui_model::sort_cog::{self, Action};
use serde_json::{Value, json};
fn menu(store: &hydrus_store::Store, sort: &PageSort) -> Value {
    json!(sort_cog::groups(sort).iter().enumerate().map(|(group, name)| json!({"menu":name,"entries":sort_cog::entries(store,sort,group).iter().map(|entry| if entry.action.is_none() { json!("---") } else { json!({"check":entry.label,"checked":entry.checked}) }).collect::<Vec<_>>() })).collect::<Vec<_>>())
}
fn sort(value: &Value) -> PageSort {
    PageSort {
        ascending: value["order"] == 0,
        by: if value["type"] == "system" {
            PageSortBy::System(value["data"].as_i64().unwrap())
        } else {
            PageSortBy::Namespaces {
                namespaces: value["data"]["namespaces"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_str().unwrap().into())
                    .collect(),
                tag_display_type: value["data"]["tag_display_type"].as_i64().unwrap(),
            }
        },
        tag_context: TagContext {
            service: hydrus_core::ServiceKey::from_hex(
                value["tag_context"]["service"].as_str().unwrap(),
            )
            .unwrap(),
            display_service: hydrus_core::ServiceKey::from_hex(
                value["tag_context"]["display_service"].as_str().unwrap(),
            )
            .unwrap(),
            include_current: value["tag_context"]["include_current"].as_bool().unwrap(),
            include_pending: value["tag_context"]["include_pending"].as_bool().unwrap(),
        },
    }
}
#[test]
fn menus_replay_service_order_separators_views_checks_and_full_context_preservation() {
    let source = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        source.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = hydrus_store::Store::open(native.path()).unwrap();
    let fixture = hydrus_testkit::fixture_json("sort_cogs.json");
    for case in fixture["cases"].as_array().unwrap() {
        let expected = sort(&case["sort"]);
        let mut actual = expected.clone();
        actual.tag_context.service = TagContext::default().service;
        if let PageSortBy::Namespaces {
            tag_display_type, ..
        } = &mut actual.by
        {
            *tag_display_type = 1;
        }
        let before = actual.clone();
        assert!(sort_cog::choose(
            &mut actual,
            &Action::Service(expected.tag_context.service.clone())
        ));
        if let PageSortBy::Namespaces {
            tag_display_type, ..
        } = expected.by
        {
            assert!(sort_cog::choose(
                &mut actual,
                &Action::View(tag_display_type)
            ));
        }
        assert_eq!(actual, expected);
        assert_eq!(
            actual.tag_context.display_service,
            before.tag_context.display_service
        );
        assert_eq!(menu(&store, &actual), case["menu"]);
        assert!(sort_cog::entries(&store, &actual, 99).is_empty());
        let stable = actual.clone();
        assert!(!sort_cog::choose(&mut actual, &Action::View(99)));
        assert_eq!(actual, stable);
    }
    let mut system = PageSort {
        by: PageSortBy::System(0),
        ascending: true,
        tag_context: TagContext::default(),
    };
    assert!(sort_cog::groups(&system).is_empty());
    assert!(!sort_cog::choose(
        &mut system,
        &Action::Service(TagContext::default().service)
    ));
}
