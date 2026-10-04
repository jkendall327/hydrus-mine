//! Reference checkbox interlocks, cancellation and real-store publication.
use hydrus_core::Tag;
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_gui_model::tag_display::TagDisplayEditor;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use hydrus_store::display::{RelationKind, load_application};
use hydrus_store::tag_display::{TagDisplayFilters, TagView};
use hydrus_store::{Store, settings};

#[test]
fn staged_interlocks_match_reference() {
    let fixture = hydrus_testkit::fixture_json("tag_display.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    let steps = fixture["autocomplete_steps"].as_array().unwrap();
    for (i, step) in steps.iter().enumerate().take(3) {
        if i == 1 {
            editor.set_rule(0, true);
        }
        if i == 2 {
            editor.set_rule(1, true);
        }
        let rules = editor.current().rules;
        let v = &step["value"];
        assert_eq!(
            rules.search_namespaces_into_full_tags,
            v[4].as_bool().unwrap()
        );
        assert_eq!(
            rules.unnamespaced_search_gives_any_namespace_wildcards,
            v[5].as_bool().unwrap()
        );
        assert_eq!(
            rules.namespace_bare_fetch_all_allowed,
            v[6].as_bool().unwrap()
        );
        assert_eq!(rules.namespace_fetch_all_allowed, v[7].as_bool().unwrap());
    }
    drop(editor);
    assert!(
        !TagDisplayEditor::new(store)
            .unwrap()
            .current()
            .rules
            .namespace_fetch_all_allowed
    );
}

#[test]
fn queues_disable_reorder_reopen_counts_daemon_and_clear_filter() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let daemon = Store::open(dir.path()).unwrap();
    let snap = store.snapshot();
    let mine = snap.services.by_name("my tags").unwrap().clone();
    let other = snap.services.by_name("downloader tags").unwrap();
    let own = mine.id;
    let source = other.id;
    for (service, good) in [(own, "own ideal"), (source, "other ideal")] {
        tag_relations::apply(
            &store,
            RelationKind::Siblings,
            vec![RelationUpdate {
                service,
                left: Tag::new("old").unwrap(),
                right: Tag::new(good).unwrap(),
                action: RelationAction::Add,
            }],
        )
        .unwrap();
    }
    let bad = store
        .read(|c| hydrus_store::master::intern_tag(c, &Tag::new("old").unwrap()))
        .unwrap();
    let other_ideal = store
        .read(|c| hydrus_store::master::intern_tag(c, &Tag::new("other ideal").unwrap()))
        .unwrap();
    let file = store
        .read(|c| {
            Ok(c.query_row("SELECT hash_id FROM files LIMIT 1", [], |r| {
                r.get::<_, hydrus_core::HashId>(0)
            })?)
        })
        .unwrap();
    store
        .write_content(move |w| {
            w.update_mappings(
                own,
                &hydrus_store::content::MappingAction::Add,
                bad,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
    tag_relations::apply(
        &store,
        RelationKind::Parents,
        vec![RelationUpdate {
            service: source,
            left: Tag::new("other ideal").unwrap(),
            right: Tag::new("ancestor").unwrap(),
            action: RelationAction::Add,
        }],
    )
    .unwrap();
    let ancestor = store
        .read(|c| hydrus_store::master::intern_tag(c, &Tag::new("ancestor").unwrap()))
        .unwrap();
    let domain = snap.services.by_name("all known files").unwrap().id;
    let count = |tag| {
        store
            .read(|c| hydrus_store::counts::count(c, own, domain, tag, true))
            .unwrap()
            .current
    };
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    let i = editor
        .services()
        .iter()
        .position(|s| s.key == mine.key)
        .unwrap();
    editor.choose(i);
    editor.current_mut().parents = vec![other.key.clone()];
    editor.add_source(false, other.key.clone());
    editor.change_source(false, 1, Some(-1));
    editor.apply().unwrap();
    assert_eq!(store.snapshot().display.get(own).ideal(bad), other_ideal);
    assert_eq!(count(other_ideal), 1);
    assert_eq!(count(ancestor), 1);
    assert_eq!(count(bad), 0);
    assert!(daemon.refresh_if_changed().unwrap());
    assert_eq!(daemon.snapshot().display.get(own).ideal(bad), other_ideal);
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    editor.choose(i);
    editor.change_source(false, 0, None);
    editor.change_source(false, 0, None);
    editor.current_mut().parents.clear();
    editor.apply().unwrap();
    assert_eq!(store.snapshot().display.get(own).ideal(bad), bad);
    assert_eq!(count(other_ideal), 0);
    assert_eq!(count(ancestor), 0);
    assert_eq!(count(bad), 1);
    let app = store
        .read(|c| load_application(c, &store.snapshot().services))
        .unwrap();
    assert!(app.sources(RelationKind::Siblings, own).is_empty());
    assert!(
        TagDisplayEditor::new(daemon.clone()).unwrap().services()[i]
            .siblings
            .is_empty()
    );
    store.write(move |ctx| { let mut filters = TagDisplayFilters::default(); filters.single_media.insert(mine.key.to_hex(),TagFilter::new().with_rule("meta:",FilterRule::Blacklist)); settings::set(ctx.conn(),&filters)?; ctx.conn().execute("UPDATE settings SET value=json_set(value,'$.future','keep','$.single_media.unknown.rules.secret','blacklist') WHERE key='tag_display_filters'",[])?; Ok(()) }).unwrap();
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    let key = editor.services()[i].key.clone();
    editor.set_filter(&key, TagView::SingleMedia, TagFilter::new());
    editor.apply().unwrap();
    let filters: TagDisplayFilters = store.read(settings::get).unwrap();
    assert!(filters.single_media[&key.to_hex()].allows_everything());
    let raw: String = store
        .read(|c| {
            Ok(c.query_row(
                "SELECT value FROM settings WHERE key='tag_display_filters'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    let raw: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(raw["future"], "keep");
    assert_eq!(
        raw["single_media"]["unknown"]["rules"]["secret"],
        "blacklist"
    );
    let revision = store.snapshot().revision;
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    editor.choose(i);
    editor.current_mut().siblings = vec![hydrus_core::ServiceKey::new(vec![234; 16])];
    assert!(editor.apply().is_err());
    assert_eq!(store.snapshot().revision, revision);
    assert!(!editor.add_source(false, hydrus_core::ServiceKey::new(vec![235; 16])));
}

#[test]
fn widget_options_gate_gui_queries_only() {
    use hydrus_store::autocomplete::{AutocompleteInput, AutocompleteRules};
    use hydrus_store::tag_display_config::AutocompleteOptions;
    let key = hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::MY_TAGS.to_vec());
    let mut options = AutocompleteOptions::for_service(&key);
    let rules = AutocompleteRules::default();
    let input = AutocompleteInput::parse("ab");
    assert_eq!(options.query(&input, &rules, false).unwrap().text, "ab");
    options.exact_match_threshold = None;
    assert_eq!(options.query(&input, &rules, false).unwrap().text, "ab*");
    options.fetch_automatically = false;
    assert!(options.query(&input, &rules, false).is_none());
    assert_eq!(options.query(&input, &rules, true).unwrap().text, "ab*");
    assert_eq!(input.tag_query(&rules).unwrap().text, "ab*");
}

#[test]
fn write_autocomplete_uses_override_or_launcher_domain_and_rejects_tag_locations() {
    use hydrus_core::search::context::LocationContext;
    use hydrus_store::tag_display_config::AutocompleteWidgetSettings;
    let dir = tempfile::tempdir().unwrap();
    let legacy = hydrus_testkit::legacy_fixture("basic");
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    let empty_key = hydrus_core::ServiceKey::new(vec![29; 16]);
    let key = empty_key.clone();
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &key,
                "empty domain",
                &hydrus_store::services::ServiceKind::LocalFiles,
            )?;
            Ok(())
        })
        .unwrap();
    let snap = store.snapshot();
    let mine = snap.services.by_name("my tags").unwrap();
    let other = snap.services.by_name("downloader tags").unwrap();
    let file = store
        .read(|c| {
            Ok(c.query_row("SELECT hash_id FROM files LIMIT 1", [], |r| {
                r.get::<_, hydrus_core::HashId>(0)
            })?)
        })
        .unwrap();
    let other_id = other.id;
    store
        .write_content(move |w| {
            let tag = hydrus_store::master::intern_tag(
                w.conn(),
                &Tag::new("other service suggestion").unwrap(),
            )?;
            w.update_mappings(
                other_id,
                &hydrus_store::content::MappingAction::Add,
                tag,
                &[file],
            )?;
            Ok(())
        })
        .unwrap();
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    let i = editor
        .services()
        .iter()
        .position(|s| s.key == mine.key)
        .unwrap();
    editor.choose(i);
    editor.current_mut().autocomplete.write_tag_service = other.key.clone();
    editor.current_mut().autocomplete.write_location = LocationContext::single(
        hydrus_core::ServiceKey::new(hydrus_core::service::builtin_keys::COMBINED_FILE.to_vec()),
    );
    editor.apply().unwrap();
    let mut manage =
        hydrus_gui_model::manage_tags::ManageTags::new(store.clone(), vec![file]).unwrap();
    let i = manage
        .service_names()
        .iter()
        .position(|n| n == "my tags")
        .unwrap();
    manage.choose_service(i);
    manage.set_location(LocationContext::single(empty_key));
    manage.set_text("other service");
    assert!(
        manage
            .suggestions()
            .iter()
            .any(|(t, _)| t == "other service suggestion")
    );
    let mut editor = TagDisplayEditor::new(store.clone()).unwrap();
    let i = editor
        .services()
        .iter()
        .position(|s| s.key == mine.key)
        .unwrap();
    editor.choose(i);
    editor.current_mut().autocomplete.override_location = false;
    editor.apply().unwrap();
    manage.set_text("other service");
    assert!(
        !manage
            .suggestions()
            .iter()
            .any(|(t, _)| t == "other service suggestion")
    );
    editor.current_mut().autocomplete.write_location = LocationContext::single(mine.key.clone());
    assert!(editor.apply().is_err());
    let settings: AutocompleteWidgetSettings = store.read(settings::get).unwrap();
    assert!(!settings.options(&mine.key).override_location);
}
