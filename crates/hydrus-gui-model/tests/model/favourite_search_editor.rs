//! Real favourite widget snapshots and autocomplete choices replayed without Slint.
use hydrus_core::{
    ServiceKey,
    pages::{PageCollect, PageSort, PageSortBy, SortSettings},
};
use hydrus_gui_model::{
    autocomplete::Autocomplete,
    collect,
    favourites::{self, Edit},
    predicate_editors::{self, Blank},
    sort,
};
use hydrus_search::{LocationContext, Predicate, TextContext, parse_api_search, predicate_text};
use hydrus_store::{Store, settings};
use serde_json::{Value, json};

fn service(store: &Store, name: &str) -> ServiceKey {
    store
        .snapshot()
        .services
        .all()
        .find(|s| s.name == name)
        .unwrap()
        .key
        .clone()
}
fn snapshot(store: &Store, edit: &Edit, action: &str) -> Value {
    let s = store.snapshot();
    let name = |k: &ServiceKey| s.services.by_key(k).unwrap().name.clone();
    let mut current: Vec<_> = edit.search.location.current().iter().map(name).collect();
    current.sort();
    let mut deleted: Vec<_> = edit.search.location.deleted().iter().map(name).collect();
    deleted.sort();
    let sort = match &edit.sort.by {
        PageSortBy::System(code) => {
            json!({"type":"system","data":code,"ascending":edit.sort.ascending})
        }
        PageSortBy::Namespaces {
            namespaces,
            tag_display_type,
        } => {
            json!({"type":"namespaces","data":{"namespaces":namespaces,"tag_display_type":tag_display_type},"ascending":edit.sort.ascending})
        }
        PageSortBy::Rating(k) => {
            json!({"type":"rating","data":name(k),"ascending":edit.sort.ascending})
        }
    };
    let text = TextContext::from_store(&s.services, &store.read(settings::get).unwrap());
    let mut predicates: Vec<_> = edit
        .search
        .predicates
        .iter()
        .map(|p| predicate_text(p, &text))
        .collect();
    predicates.sort();
    json!({"action":action,"location":{"current":current,"deleted":deleted},"tags":{"service":name(&edit.search.tags.service),"current":edit.search.tags.include_current,"pending":edit.search.tags.include_pending},"sort":sort,"collect":{"namespaces":edit.collect.namespaces,"ratings":edit.collect.ratings.iter().map(name).collect::<Vec<_>>(),"unmatched":edit.collect.collect_unmatched,"service":name(&edit.collect.tag_context.service)},"predicates":predicates})
}

// leaf: audit-options-favourite-edit-collect
// leaf: audit-options-favourite-edit-domains
// leaf: audit-options-favourite-edit-sort
#[test]
fn favourite_controls_replay_reference_domain_sort_collect_and_predicate_values() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let f = hydrus_testkit::fixture_json("favourite_search_editor.json");
    let sorts: SortSettings = store.read(settings::get).unwrap();
    let location = LocationContext::single(service(&store, "my files"));
    let row = favourites::new_search(location.clone());
    let mut edit = Edit::new(&row, &sorts.default_sort, &sorts.default_collect);
    edit.save_sort = true;
    edit.save_collect = true;
    let choices = collect::choices(&store);
    let text = TextContext::from_store(
        &store.snapshot().services,
        &store.read(settings::get).unwrap(),
    );
    for event in f["events"].as_array().unwrap() {
        let action = event["action"].as_str().unwrap();
        match action {
            "initial" => (),
            "trash" => edit.choose_location(
                &store.snapshot().services,
                LocationContext::single(service(&store, "trash")),
            ),
            "all_files" => edit.choose_location(
                &store.snapshot().services,
                LocationContext::single(service(&store, "all known files")),
            ),
            "all_tags" => edit.choose_tags(service(&store, "all known tags"), &location),
            "local_tags" => edit.choose_tags(
                service(&store, f["local_tag_service"].as_str().unwrap()),
                &location,
            ),
            "exclude_current" => edit.search.tags.include_current = false,
            "exclude_pending" => edit.search.tags.include_pending = false,
            "include_current" => edit.search.tags.include_current = true,
            "include_pending" => edit.search.tags.include_pending = true,
            "multiple_deleted" => {
                let my = service(&store, "my files");
                edit.choose_location(
                    &store.snapshot().services,
                    LocationContext::new([my.clone()], [my]),
                );
            }
            "sort_width_default" => {
                let choice = sort::page_choices(&store, &edit.sort.by)
                    .into_iter()
                    .find(|c| c.by == PageSortBy::System(event["sort"]["data"].as_i64().unwrap()))
                    .unwrap();
                edit.choose_sort(&choice);
            }
            "sort_width_descending" => edit.sort.ascending = false,
            "sort_namespace" => {
                let ns = &event["sort"]["data"]["namespaces"];
                let choice = sort::page_choices(&store, &edit.sort.by).into_iter().find(|c| matches!(&c.by, PageSortBy::Namespaces { namespaces, .. } if json!(namespaces) == *ns)).unwrap();
                edit.choose_sort(&choice);
            }
            "sort_rating" => {
                let choice = sort::page_choices(&store, &edit.sort.by)
                    .into_iter()
                    .find(|c| {
                        c.by == PageSortBy::Rating(service(
                            &store,
                            f["rating_service"].as_str().unwrap(),
                        ))
                    })
                    .unwrap();
                edit.choose_sort(&choice);
            }
            "collect_namespace_rating" => {
                for (i, c) in choices.iter().enumerate() {
                    if c.name == "creator" || c.name == f["rating_service"].as_str().unwrap() {
                        edit.collect = collect::toggled(&choices, &edit.collect, i, true);
                    }
                }
            }
            "unmatched_separate" => {
                edit.collect = collect::with_unmatched(&choices, &edit.collect, false);
            }
            "collect_local_tags" => {
                edit.collect.tag_context.service =
                    service(&store, f["local_tag_service"].as_str().unwrap());
                edit.collect.tag_context.display_service = edit.collect.tag_context.service.clone();
            }
            "my_files" => edit.choose_location(&store.snapshot().services, location.clone()),
            "autocomplete_blue_eyes" => {
                let mut ac = Autocomplete::new(store.clone());
                ac.set_context(&edit.search.location, &edit.search.tags);
                ac.set_text("blue");
                let mut suggestions: Vec<_> = ac
                    .suggestions()
                    .iter()
                    .map(|s| s.predicate.clone())
                    .collect();
                suggestions.sort();
                assert_eq!(f["autocomplete_suggestions"], json!(suggestions));
                let chosen = ac
                    .suggestions()
                    .iter()
                    .find(|s| s.predicate == "blue eyes")
                    .unwrap();
                hydrus_search::enter_predicates(
                    &mut edit.search.predicates,
                    &parse_api_search(&json!([chosen.predicate])).unwrap(),
                    &text,
                );
            }
            "system_limit_child" => {
                let context = predicate_editors::Context::new(
                    &store.snapshot().services,
                    Vec::new(),
                    hydrus_search::Clock::system().today(),
                );
                let editor = predicate_editors::Editor::new(Blank::Limit, &context);
                let mut panel = editor.pages[0].panels[0].clone();
                let n = panel
                    .fields
                    .iter()
                    .position(|f| matches!(f, predicate_editors::Field::Number { .. }))
                    .unwrap();
                panel.set_number(n, 7);
                let made: Vec<Predicate> = panel.predicates(&context).unwrap();
                hydrus_search::enter_predicates(&mut edit.search.predicates, &made, &text);
            }
            _ => panic!("unknown reference action {action}"),
        }
        assert_eq!(*event, snapshot(&store, &edit, action), "{action}");
    }
    let saved = edit.value();
    assert_eq!(
        saved.collect.as_ref().unwrap().tag_context,
        edit.collect.tag_context
    );
    edit.save_sort = false;
    edit.save_collect = false;
    assert!(edit.value().sort.is_none() && edit.value().collect.is_none());
    edit.save_sort = true;
    edit.save_collect = true;
    assert_eq!(edit.value().sort, saved.sort);
    assert_eq!(edit.value().collect, saved.collect);
    // Existing native JSON remains readable with combined/current/pending collection defaults.
    let old: PageCollect = serde_json::from_value(
        json!({"namespaces":["series"],"ratings":[],"collect_unmatched":false}),
    )
    .unwrap();
    assert_eq!(old.tag_context, hydrus_search::TagContext::default());
    let roundtrip: PageSort =
        serde_json::from_str(&serde_json::to_string(&edit.sort).unwrap()).unwrap();
    assert_eq!(roundtrip, edit.sort);
}
