//! Migration choices replay the actual Qt controls; DB outcomes replay its destination.
use hydrus_core::ServiceKey;
use hydrus_gui_model::tag_migration::{self, Action, Content, Migration, Status};
use hydrus_store::{
    Store,
    services::{self, RepositoryConfig, ServiceKind},
};
#[test]
fn reference_choices_and_second_confirmation() {
    let fixture = hydrus_testkit::fixture_json("tag_migration.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let key = ServiceKey::new(vec![123; 16]);
    let repo = key.clone();
    store
        .write_and_refresh(move |ctx| {
            services::insert(
                ctx.conn(),
                &repo,
                "repo",
                &ServiceKind::TagRepository(RepositoryConfig::default()),
            )?;
            Ok(())
        })
        .unwrap();
    let mut model = Migration::new(&store, &key, vec![]).unwrap();
    for case in fixture["matrix"].as_array().unwrap() {
        model.source = model
            .services
            .iter()
            .position(|s| s.local == (case["source"] == "local"))
            .unwrap();
        model.destination = model
            .services
            .iter()
            .position(|s| s.local == (case["destination"] == "local"))
            .unwrap();
        model.content = match case["content"].as_str().unwrap() {
            "tag mappings" => Content::Mappings,
            "tag siblings" => Content::Siblings,
            _ => Content::Parents,
        };
        model.status = match case["status"].as_str().unwrap() {
            "current" => Status::Current,
            "current and pending" => Status::CurrentAndPending,
            "pending" => Status::Pending,
            _ => Status::Deleted,
        };
        let actions = model
            .actions()
            .into_iter()
            .map(tag_migration::action_label)
            .collect::<Vec<_>>();
        let expected = case["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actions, expected, "{case}");
    }
    assert_eq!(tag_migration::LAST_CHANCE, fixture["asked"][1]["message"]);
    model.source = model.services.iter().position(|s| !s.local).unwrap();
    model.status = Status::Pending;
    model.destination = model.source;
    model.action = Action::Pend;
    model.source = model.services.iter().position(|s| s.local).unwrap();
    model.normalize();
    assert_eq!(model.status, Status::Current);
    assert!(model.actions().contains(&model.action));
}
#[test]
fn reference_database_pending_outcomes() {
    let fixture = hydrus_testkit::fixture_json("tag_migration.json");
    let legacy = hydrus_testkit::legacy_fixture("repositories");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    // Qt selected the first local service, whose real DB source is recorded.
    let key = ServiceKey::from_hex(fixture["source_service_key"].as_str().unwrap()).unwrap();
    let mut model = Migration::new(&store, &key, vec![]).unwrap();
    model.destination = model.services.iter().position(|s| !s.local).unwrap();
    model.action = Action::Pend;
    model.location = hydrus_core::search::context::LocationContext::single(
        store
            .snapshot()
            .services
            .by_name("my files")
            .unwrap()
            .key
            .clone(),
    );
    assert_eq!(model.confirmation(), fixture["asked"][0]["message"]);
    model.location = hydrus_core::search::context::LocationContext::single(ServiceKey::new(
        hydrus_core::service::builtin_keys::COMBINED_FILE.to_vec(),
    ));
    let request = model.request();
    hydrus_store::tag_migration::run(
        &store,
        &request,
        &std::sync::atomic::AtomicBool::new(false),
        5,
        |_| {},
    )
    .unwrap();
    let destination = store
        .snapshot()
        .services
        .by_key(&request.destination)
        .unwrap()
        .id;
    let table = hydrus_store::schema::MappingTables::new(destination).pending;
    for file in fixture["pending"].as_array().unwrap() {
        let hash = file["hash"]
            .as_str()
            .unwrap()
            .parse::<hydrus_core::Sha256>()
            .unwrap();
        let tags = store
            .read(|conn| {
                let id = hydrus_store::master::hash_id(conn, &hash)?.unwrap();
                let mut stmt =
                    conn.prepare(&format!("SELECT tag_id FROM {table} WHERE hash_id=?"))?;
                let ids = stmt
                    .query_map([id], |r| r.get::<_, hydrus_core::TagId>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut tags = ids
                    .into_iter()
                    .map(|id| {
                        hydrus_store::master::tag(conn, id).map(|t| t.unwrap().as_str().to_owned())
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                tags.sort();
                Ok(tags)
            })
            .unwrap();
        let expected = file["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(tags, expected, "{}", file["hash"]);
    }
}

#[test]
fn reference_database_pair_filters_and_destinations() {
    let fixture = hydrus_testkit::fixture_json("tag_migration.json");
    for case in fixture["pairs"].as_array().unwrap() {
        let legacy = hydrus_testkit::legacy_fixture("repositories");
        let dir = tempfile::tempdir().unwrap();
        hydrus_store::import::import_legacy(
            legacy.path(),
            &dir.path().join(hydrus_store::store::DB_FILE_NAME),
        )
        .unwrap();
        let store = Store::open(dir.path()).unwrap();
        let source = ServiceKey::from_hex(fixture["source_service_key"].as_str().unwrap()).unwrap();
        let destination =
            ServiceKey::from_hex(fixture["pair_destination_service_key"].as_str().unwrap())
                .unwrap();
        let kind = if case["kind"] == "tag siblings" {
            hydrus_store::display::RelationKind::Siblings
        } else {
            hydrus_store::display::RelationKind::Parents
        };
        let service = store.snapshot().services.by_key(&source).unwrap().id;
        let updates = case["initial"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| hydrus_store::content::tag_relations::RelationUpdate {
                service,
                left: hydrus_core::Tag::new(p[0].as_str().unwrap()).unwrap(),
                right: hydrus_core::Tag::new(p[1].as_str().unwrap()).unwrap(),
                action: hydrus_store::content::tag_relations::RelationAction::Add,
            })
            .collect();
        hydrus_store::content::tag_relations::apply(&store, kind, updates).unwrap();
        let mut model = Migration::new(&store, &source, vec![]).unwrap();
        model.destination = model
            .services
            .iter()
            .position(|s| s.key == destination)
            .unwrap();
        model.content = if kind == hydrus_store::display::RelationKind::Siblings {
            Content::Siblings
        } else {
            Content::Parents
        };
        model.action = Action::Add;
        model.left_filter = hydrus_core::TagFilter::new()
            .with_rule("excluded:", hydrus_core::FilterRule::Blacklist);
        hydrus_store::tag_migration::run(
            &store,
            &model.request(),
            &std::sync::atomic::AtomicBool::new(false),
            1,
            |_| {},
        )
        .unwrap();
        let service = store.snapshot().services.by_key(&destination).unwrap().id;
        let (table, left, right) = hydrus_store::content::tag_relations::columns(kind);
        let pairs = store
            .read(|conn| {
                let ids = conn
                    .prepare(&format!(
                        "SELECT {left},{right} FROM {table} WHERE service_id=? AND status=0"
                    ))?
                    .query_map([service], |r| {
                        Ok((
                            r.get::<_, hydrus_core::TagId>(0)?,
                            r.get::<_, hydrus_core::TagId>(1)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut pairs = ids
                    .into_iter()
                    .map(|(a, b)| {
                        Ok(vec![
                            hydrus_store::master::tag(conn, a)?
                                .unwrap()
                                .as_str()
                                .to_owned(),
                            hydrus_store::master::tag(conn, b)?
                                .unwrap()
                                .as_str()
                                .to_owned(),
                        ])
                    })
                    .collect::<hydrus_store::Result<Vec<_>>>()?;
                pairs.sort();
                Ok(pairs)
            })
            .unwrap();
        let graph = store.snapshot().display.get(service);
        let all = graph.all_tags();
        for tag in &all {
            assert!(graph.display_tags(*tag).count() <= all.len());
        }
        assert_eq!(
            serde_json::to_value(pairs).unwrap(),
            case["destination"]["0"]
        );
    }
}

#[test]
fn archive_inspection_choices_cancellation_and_count_summaries_match_qt() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    assert_eq!(
        recording["picker_requests"][0]["message"],
        tag_migration::SOURCE_ARCHIVE_PROMPT
    );
    assert_eq!(
        recording["picker_requests"][1]["message"],
        tag_migration::DESTINATION_ARCHIVE_PROMPT
    );
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let key = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let mut model = Migration::new(&store, &key, vec![]).unwrap();
    model.location = hydrus_core::search::context::LocationContext::single(ServiceKey::new(
        hydrus_core::service::builtin_keys::COMBINED_FILE.to_vec(),
    ));
    for case in recording["qt"].as_array().unwrap() {
        model.content = match case["kind"].as_str().unwrap() {
            "sha256" | "md5" => Content::Mappings,
            "siblings" => Content::Siblings,
            _ => Content::Parents,
        };
        model.reset_archives();
        model.source = model.services.len();
        model.destination = model.services.len();
        model.normalize();
        assert_eq!(
            serde_json::json!(
                model
                    .statuses()
                    .into_iter()
                    .map(tag_migration::status_label)
                    .collect::<Vec<_>>()
            ),
            case["statuses"]
        );
        assert_eq!(
            serde_json::json!(
                model
                    .actions()
                    .into_iter()
                    .map(tag_migration::action_label)
                    .collect::<Vec<_>>()
            ),
            case["actions"]
        );
        assert!(model.job_options().is_err());
        let path = hydrus_testkit::fixture_path(case["source"].as_str().unwrap());
        model.set_archive_path(true, &path).unwrap();
        model.set_archive_path(false, &path).unwrap();
        assert_eq!(
            model.archive_path_label(true),
            case["source"].as_str().unwrap()
        );
        let options = model.job_options().unwrap();
        assert_eq!(options.source, Some(path.clone()));
        assert_eq!(options.destination, Some(path));
        if case["kind"] == "md5" {
            assert_eq!(options.hash_kind, hydrus_core::HashKind::Md5);
            assert!(model.destination_hash_locked);
        }
        assert_eq!(model.confirmation(), case["confirmation"].as_str().unwrap());
    }
    let previous = model.archives.source.clone();
    let error = model
        .set_archive_path(
            true,
            &hydrus_testkit::fixture_path("tag_archive_siblings.db"),
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        recording["qt_warnings"][0].as_str().unwrap()
    );
    assert_eq!(model.archives.source, previous);
    model.content = Content::Siblings;
    model.count_right = true;
    let summary = model.confirmation();
    assert!(summary.contains("where the ideal tag of each pair's chain has count on"));
    model.count_either = true;
    let options = model.job_options().unwrap();
    let counts = options.counts.unwrap();
    assert!(counts.either);
    assert!(!counts.left && !counts.right);
    assert!(
        model
            .confirmation()
            .contains("where the worse or ideal tag of each pair has count on")
    );
}
