//! Real-reference row/status/question replay and transactional graph evidence.
use hydrus_core::{ServiceId, ServiceKey, Tag};
use hydrus_gui_model::tag_relationships::{Pair, RelationKind, Relationships};
use hydrus_store::Store;
use hydrus_store::content::tag_relations::{self, RelationAction, RelationUpdate};
use serde_json::Value;

fn pair(value: &Value) -> Pair {
    (
        value[0].as_str().unwrap().into(),
        value[1].as_str().unwrap().into(),
    )
}
fn update(service: ServiceId, pair: &Pair, action: RelationAction) -> RelationUpdate {
    RelationUpdate {
        service,
        left: Tag::new(&pair.0).unwrap(),
        right: Tag::new(&pair.1).unwrap(),
        action,
    }
}

#[test]
fn reference_rows_questions_conflicts_and_loops() {
    let fixture = hydrus_testkit::fixture_json("tag_relationships.json");
    for case in fixture.as_array().unwrap() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        if !case["local"].as_bool().unwrap() {
            store
                .write_and_refresh(|ctx| {
                    hydrus_store::services::insert(
                        ctx.conn(),
                        &ServiceKey::new(vec![24; 16]),
                        "a repository",
                        &hydrus_store::services::ServiceKind::TagRepository(
                            hydrus_store::services::RepositoryConfig::default(),
                        ),
                    )?;
                    Ok(())
                })
                .unwrap();
        }
        let service = store
            .snapshot()
            .services
            .by_name(if case["local"].as_bool().unwrap() {
                "my tags"
            } else {
                "a repository"
            })
            .unwrap()
            .id;
        let kind = if case["kind"] == "siblings" {
            RelationKind::Siblings
        } else {
            RelationKind::Parents
        };
        let initial = case["initial"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| update(service, &pair(p), RelationAction::Add))
            .collect();
        tag_relations::apply(&store, kind, initial).unwrap();
        let mut editor = Relationships::new(store.clone(), kind).unwrap();
        if !case["local"].as_bool().unwrap() {
            editor.choose_service(2);
        }
        assert_eq!(
            serde_json::json!([editor.sorting().0, editor.sorting().1]),
            case["default_sort"]
        );
        editor.set_filters(true, false, false);
        for step in case["steps"].as_array().unwrap() {
            if !step["step"].is_null() {
                let p = pair(&step["step"][1]);
                let only_add = step["step"][0] == "add";
                let mut answers = Vec::new();
                let mut asked = Vec::new();
                while let Err(q) = editor.enter_pairs(vec![p.clone()], only_add, &answers) {
                    asked.push(serde_json::json!({"message": q.message, "yes": q.yes, "no": q.no}));
                    answers.push(Some(if q.reason {
                        "oracle reason".into()
                    } else {
                        String::new()
                    }));
                }
                assert_eq!(Value::Array(asked), step["asked"]);
            }
            let mut rows: Vec<Value> = editor
                .rows()
                .iter()
                .map(|r| serde_json::json!([r.status, r.pair.0, r.pair.1, r.note]))
                .collect();
            rows.sort_by_key(|r| {
                (
                    r[1].as_str().unwrap().to_owned(),
                    r[2].as_str().unwrap().to_owned(),
                )
            });
            assert_eq!(
                Value::Array(rows),
                step["rows"],
                "{} {:?}",
                case["kind"],
                step["step"]
            );
            let mut updates: Vec<Value> = editor
                .updates()
                .iter()
                .map(|u| {
                    serde_json::json!([
                        match u.action {
                            RelationAction::Add => 0,
                            RelationAction::Delete => 1,
                            RelationAction::Pend(_) => 2,
                            RelationAction::RescindPend => 3,
                            RelationAction::Petition(_) => 4,
                            RelationAction::RescindPetition => 5,
                        },
                        [u.left.as_str(), u.right.as_str()],
                        match &u.action {
                            RelationAction::Pend(r) | RelationAction::Petition(r) => r.as_str(),
                            _ => "No reason given.",
                        }
                    ])
                })
                .collect();
            updates.sort_by_key(Value::to_string);
            let mut expected = step["updates"].as_array().unwrap().clone();
            expected.sort_by_key(Value::to_string);
            assert_eq!(updates, expected);
        }
        editor.enter_tags(false, "cousin").unwrap();
        editor.wipe_workspace();
        editor.set_filters(false, false, false);
        let mut rows = editor
            .rows()
            .iter()
            .map(|r| serde_json::json!([r.pair.0, r.pair.1]))
            .collect::<Vec<_>>();
        rows.sort_by_key(Value::to_string);
        let expected = &case["filtered"][if kind == RelationKind::Siblings {
            "true"
        } else {
            "false"
        }];
        assert_eq!(Value::Array(rows), *expected);
        editor.set_filters(false, false, true);
        editor.sort(1, true);
        assert_eq!(
            Value::Array(
                editor
                    .rows()
                    .iter()
                    .map(|r| serde_json::json!([r.pair.0, r.pair.1]))
                    .collect()
            ),
            case["filtered"]["true"]
        );
    }
}

#[test]
fn per_service_inputs_cancel_import_and_selection() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &ServiceKey::new(vec![22; 16]),
                "other tags",
                &hydrus_store::services::ServiceKind::LocalTags,
            )?;
            Ok(())
        })
        .unwrap();
    let mut editor = Relationships::new(store.clone(), RelationKind::Parents).unwrap();
    editor.enter_tags(false, " Sword \n Shield ").unwrap();
    assert!(!editor.can_add());
    editor.enter_tags(true, "Weapon\nObject").unwrap();
    assert!(editor.can_add());
    assert_eq!(
        editor.apply_question().unwrap().message,
        "Are you sure you want to OK? You have an uncommitted pair."
    );
    editor.add(&[]).unwrap();
    assert!(!editor.can_add());
    assert_eq!(editor.rows().len(), 4);
    editor.click(0, false, false);
    editor.click(2, false, true);
    assert_eq!(editor.export().lines().count(), 6);
    editor.click(3, true, false);
    assert_eq!(editor.export().lines().count(), 8);
    editor.delete(&[None, None, None, None]).unwrap();
    assert_eq!(editor.rows().len(), 4);
    editor.delete(&[Some(String::new())]).unwrap();
    assert!(editor.rows().is_empty());
    assert!(editor.import(" Red\n Colour\nodd", &[]).unwrap());
    editor.choose_service(1);
    assert!(editor.rows().is_empty());
    editor.enter_tags(false, "left").unwrap();
    editor.enter_tags(true, "right").unwrap();
    editor.add(&[]).unwrap();
    editor.choose_service(0);
    assert_eq!(editor.rows().len(), 1);
    editor.wipe_workspace();
    assert!(editor.rows().is_empty());
    editor.set_filters(false, true, false);
    assert_eq!(editor.rows().len(), 1);
    // Dropping the entire staged dialog commits neither service.
    drop(editor);
    let mut reloaded = Relationships::new(store, RelationKind::Parents).unwrap();
    reloaded.set_filters(true, false, false);
    assert!(reloaded.rows().is_empty());
    reloaded.choose_service(1);
    reloaded.set_filters(true, false, false);
    assert!(reloaded.rows().is_empty());
}

#[test]
fn apply_republishes_graphs_and_counts_atomically_and_rolls_back_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let service = store.snapshot().services.by_name("my tags").unwrap().id;
    let ids = store
        .write_content(move |w| {
            let a = hydrus_store::master::intern_tag(w.conn(), &Tag::new("old").unwrap())?;
            let b = hydrus_store::master::intern_tag(w.conn(), &Tag::new("ideal").unwrap())?;
            let parent = hydrus_store::master::intern_tag(w.conn(), &Tag::new("parent").unwrap())?;
            let hash = hydrus_store::master::intern_hash(w.conn(), &hydrus_core::Sha256([55; 32]))?;
            w.update_mappings(
                service,
                &hydrus_store::content::MappingAction::Add,
                a,
                &[hash],
            )?;
            Ok((a, b, parent))
        })
        .unwrap();
    let count = |tag| {
        store
            .read(|conn| {
                let tables = hydrus_store::schema::MappingTables::new(service);
                Ok(conn.query_row(
                    &format!(
                        "SELECT COALESCE(sum(current),0) FROM {} WHERE tag_id=?1",
                        tables.display_counts
                    ),
                    [tag],
                    |r| r.get::<_, i64>(0),
                )?)
            })
            .unwrap()
    };
    assert_eq!(count(ids.0), 1);
    let old = store.snapshot();
    let mut editor = Relationships::new(store.clone(), RelationKind::Siblings).unwrap();
    editor.enter_tags(false, "old").unwrap();
    editor.enter_tags(true, "ideal").unwrap();
    editor.add(&[]).unwrap();
    assert_eq!(store.snapshot().display.get(service).ideal(ids.0), ids.0);
    editor.apply().unwrap();
    assert_eq!(store.snapshot().display.get(service).ideal(ids.0), ids.1);
    assert_eq!(count(ids.0), 0);
    assert_eq!(count(ids.1), 1);
    assert_eq!(
        old.display.get(service).ideal(ids.0),
        ids.0,
        "existing snapshots stay immutable"
    );
    let snapshot = store.snapshot();
    let invalid = vec![
        update(
            service,
            &("ideal".into(), "parent".into()),
            RelationAction::Add,
        ),
        update(
            ServiceId(u32::MAX),
            &("x".into(), "y".into()),
            RelationAction::Add,
        ),
    ];
    assert!(tag_relations::apply(&store, RelationKind::Parents, invalid).is_err());
    assert!(std::sync::Arc::ptr_eq(&snapshot, &store.snapshot()));
    let mut parents = Relationships::new(store.clone(), RelationKind::Parents).unwrap();
    parents.set_filters(true, false, false);
    assert!(parents.rows().is_empty());
    parents.enter_tags(false, "old").unwrap();
    parents.enter_tags(true, "parent").unwrap();
    parents.add(&[]).unwrap();
    parents.apply().unwrap();
    assert_eq!(
        store.snapshot().display.get(service).ancestors(ids.0),
        &[ids.2]
    );
    assert_eq!(count(ids.2), 1);
    let mut sibling = Relationships::new(store.clone(), RelationKind::Siblings).unwrap();
    sibling.set_filters(true, false, false);
    sibling.click(0, false, false);
    sibling.delete(&[]).unwrap();
    sibling.apply().unwrap();
    assert_eq!(store.snapshot().display.get(service).ideal(ids.0), ids.0);
    assert_eq!(count(ids.0), 1);
    assert_eq!(count(ids.1), 0);
    assert_eq!(
        store.snapshot().display.get(service).ancestors(ids.0),
        &[ids.2]
    );
    assert_eq!(count(ids.2), 1);
}

#[test]
fn repository_changes_keep_reasons_and_rescinds_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let repository = store
        .write_and_refresh(|ctx| {
            hydrus_store::services::insert(
                ctx.conn(),
                &ServiceKey::new(vec![33; 16]),
                "repository",
                &hydrus_store::services::ServiceKind::TagRepository(
                    hydrus_store::services::RepositoryConfig::default(),
                ),
            )
        })
        .unwrap();
    let initial = ("before".into(), "ideal".into());
    tag_relations::apply(
        &store,
        RelationKind::Siblings,
        vec![update(repository, &initial, RelationAction::Add)],
    )
    .unwrap();
    let mut editor = Relationships::new(store.clone(), RelationKind::Siblings).unwrap();
    editor.choose_service(2);
    editor.set_filters(true, false, false);
    editor
        .enter_pairs(
            vec![("new".into(), "ideal".into())],
            true,
            &[Some("same tag".into())],
        )
        .unwrap();
    editor
        .enter_pairs(vec![initial.clone()], false, &[Some("wrong tag".into())])
        .unwrap();
    editor.apply().unwrap();
    let stored_reasons = store.read(|conn| {
        Ok(conn.prepare("SELECT text FROM tag_siblings JOIN texts ON reason_id=text_id WHERE service_id=?1 ORDER BY text")?.query_map([repository], |r| r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?)
    }).unwrap();
    assert_eq!(stored_reasons, ["same tag", "wrong tag"]);
    let mut reopened = Relationships::new(store.clone(), RelationKind::Siblings).unwrap();
    reopened.choose_service(2);
    reopened.set_filters(true, false, false);
    let rows = reopened.rows();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "(-) ");
    assert_eq!(rows[0].note, "");
    assert_eq!(rows[1].status, "(+) ");
    assert_eq!(rows[1].note, "");
    reopened.click(0, false, false);
    reopened.click(1, false, true);
    let q = reopened.delete(&[]).unwrap_err();
    assert!(q.message.contains("pending"));
    reopened
        .delete(&[Some(String::new()), Some(String::new())])
        .unwrap();
    reopened.apply().unwrap();
    let mut final_editor = Relationships::new(store, RelationKind::Siblings).unwrap();
    final_editor.choose_service(2);
    final_editor.set_filters(true, false, false);
    assert_eq!(final_editor.rows().len(), 1);
    assert_eq!(final_editor.rows()[0].status, "");
    assert!(final_editor.updates().is_empty());
}

#[test]
fn changes_refresh_other_services_applying_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let snapshot = store.snapshot();
    let display = snapshot.services.by_name("my tags").unwrap().id;
    let source = snapshot.services.by_name("downloader tags").unwrap().id;
    let (a,b) = store.write_and_refresh(move |ctx| {
        let a = hydrus_store::master::intern_tag(ctx.conn(), &Tag::new("alias").unwrap())?; let b = hydrus_store::master::intern_tag(ctx.conn(), &Tag::new("ideal").unwrap())?;
        ctx.conn().execute("INSERT INTO tag_display_application (display_service_id,kind,position,source_service_id) VALUES (?1,0,0,?2)", rusqlite::params![display,source])?;
        Ok((a,b))
    }).unwrap();
    tag_relations::apply(
        &store,
        RelationKind::Siblings,
        vec![update(
            source,
            &("alias".into(), "ideal".into()),
            RelationAction::Add,
        )],
    )
    .unwrap();
    assert_eq!(store.snapshot().display.get(display).ideal(a), b);
    tag_relations::apply(
        &store,
        RelationKind::Siblings,
        vec![update(
            source,
            &("alias".into(), "ideal".into()),
            RelationAction::Delete,
        )],
    )
    .unwrap();
    assert_eq!(store.snapshot().display.get(display).ideal(a), a);
}

#[test]
fn invalid_batch_graphs_are_reported_and_leave_valid_relationships_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for kind in [RelationKind::Parents, RelationKind::Siblings] {
        let mut editor = Relationships::new(store.clone(), kind).unwrap();
        let pairs = vec![("one".into(), "two".into()), ("two".into(), "one".into())];
        assert!(
            editor
                .enter_pairs(pairs.clone(), true, &[])
                .unwrap_err()
                .message
                .contains("cycle")
        );
        editor
            .enter_pairs(pairs, true, &[Some(String::new())])
            .unwrap();
        assert!(editor.updates().is_empty());
        editor
            .enter_pairs(vec![("one".into(), "two".into())], true, &[])
            .unwrap();
        editor.apply().unwrap();
        let q = editor.import("one\none", &[]).unwrap_err();
        assert!(q.message.contains("self-referencing"));
        editor.import("one\none", &[Some(String::new())]).unwrap();
        assert_eq!(editor.updates().len(), 1);
    }
    let mut editor = Relationships::new(store, RelationKind::Siblings).unwrap();
    let pairs = vec![
        ("alias".into(), "first".into()),
        ("alias".into(), "second".into()),
    ];
    assert!(
        editor
            .enter_pairs(pairs, true, &[])
            .unwrap_err()
            .message
            .contains("conflicting sibling")
    );
}
