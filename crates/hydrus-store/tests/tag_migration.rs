//! Stable-source batching, durable cancellation and derived-cache boundaries.
use hydrus_core::{HashId, ServiceKey, Sha256, Tag, TagFilter, TagId};
use hydrus_store::{
    Store,
    content::MappingAction,
    services::{self, ServiceKind},
    tag_migration::{self, Action, Content, Request, Scope, Status},
};
use std::sync::atomic::{AtomicBool, Ordering};
fn setup() -> (tempfile::TempDir, std::sync::Arc<Store>, Request) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let source = store
        .snapshot()
        .services
        .by_name("my tags")
        .unwrap()
        .key
        .clone();
    let destination = ServiceKey::new(vec![128; 16]);
    let key = destination.clone();
    store
        .write_and_refresh(move |ctx| {
            services::insert(ctx.conn(), &key, "destination", &ServiceKind::LocalTags)?;
            Ok(())
        })
        .unwrap();
    let sid = store.snapshot().services.by_key(&source).unwrap().id;
    store
        .write_content(move |writer| {
            let tag =
                hydrus_store::master::intern_tag(writer.conn(), &Tag::new("series:test").unwrap())?;
            for n in 0_u8..11 {
                let hash = hydrus_store::master::intern_hash(writer.conn(), &Sha256([n; 32]))?;
                writer.update_mappings(sid, &MappingAction::Add, tag, &[hash])?;
            }
            Ok(())
        })
        .unwrap();
    let request = Request {
        source,
        destination,
        content: Content::Mappings,
        status: Status::Current,
        action: Action::Add,
        scope: Scope::Location(hydrus_core::search::context::LocationContext::single(
            ServiceKey::new(hydrus_core::service::builtin_keys::COMBINED_FILE.to_vec()),
        )),
        left_filter: TagFilter::default(),
        right_filter: TagFilter::default(),
        reason: "test".into(),
    };
    (dir, store, request)
}
fn count(store: &Store, key: &ServiceKey, deleted: bool) -> i64 {
    let id = store.snapshot().services.by_key(key).unwrap().id;
    let table = hydrus_store::schema::MappingTables::new(id);
    store
        .read(|conn| {
            Ok(conn.query_row(
                &format!(
                    "SELECT count(*) FROM {}",
                    if deleted {
                        table.deleted
                    } else {
                        table.current
                    }
                ),
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap()
}
#[test]
fn same_service_delete_and_clear_never_skip_mutated_source_rows() {
    let (_dir, store, mut request) = setup();
    request.destination = request.source.clone();
    request.action = Action::Delete;
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    assert_eq!(p.accepted, 11);
    assert_eq!(count(&store, &request.source, false), 0);
    assert_eq!(count(&store, &request.source, true), 11);
    request.status = Status::Deleted;
    request.action = Action::ClearDeletion;
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |_| {}).unwrap();
    assert_eq!(p.accepted, 11);
    assert_eq!(count(&store, &request.source, true), 0);
}
#[test]
fn cancellation_is_a_durable_prefix_and_incremental_counts_match_rebuild() {
    let (dir, store, request) = setup();
    let cancel = AtomicBool::new(false);
    let p = tag_migration::run(&store, &request, &cancel, 3, |p| {
        if p.scanned >= 3 {
            cancel.store(true, Ordering::Release);
        }
    })
    .unwrap();
    assert_eq!(p.accepted, 3);
    assert!(p.cancelled);
    assert_eq!(count(&store, &request.destination, false), 3);
    let id = store
        .snapshot()
        .services
        .by_key(&request.destination)
        .unwrap()
        .id;
    let table = hydrus_store::schema::MappingTables::new(id);
    let before = store
        .read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT domain_id,tag_id,current,pending FROM {} ORDER BY domain_id,tag_id",
                table.counts
            ))?;
            Ok(stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    store
        .write(|ctx| hydrus_store::counts::rebuild_all(ctx.conn()))
        .unwrap();
    let after = store
        .read(|conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT domain_id,tag_id,current,pending FROM {} ORDER BY domain_id,tag_id",
                table.counts
            ))?;
            Ok(stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    assert_eq!(before, after);
    drop(store);
    let reopened = Store::open(dir.path()).unwrap();
    assert_eq!(count(&reopened, &request.destination, false), 3);
}
#[test]
fn registry_changes_stop_the_next_batch_and_keep_committed_prefix() {
    let (_dir, store, request) = setup();
    let destination = request.destination.clone();
    let remove = store.clone();
    let result = tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |p| {
        if p.scanned == 3 {
            let key = destination.clone();
            remove
                .write_and_refresh(move |ctx| {
                    ctx.conn().execute(
                        "UPDATE services SET service_key=?1 WHERE service_key=?2",
                        rusqlite::params![ServiceKey::new(vec![99; 16]), key],
                    )?;
                    Ok(())
                })
                .unwrap();
        }
    });
    assert!(result.is_err());
    assert_eq!(count(&store, &ServiceKey::new(vec![99; 16]), false), 3);
}
#[test]
fn selected_files_and_filters_are_applied_before_destination_changes() {
    let (_dir, store, mut request) = setup();
    request.scope = Scope::Files(vec![HashId(1), HashId(3)]);
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    assert_eq!(p.accepted, 2);
    request.left_filter = TagFilter::new().with_rule("series:", hydrus_core::FilterRule::Blacklist);
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    assert_eq!(p.accepted, 0);
}
#[test]
fn pair_migration_publishes_graph_and_rebuilt_counts() {
    let (_dir, store, mut request) = setup();
    let source = store
        .snapshot()
        .services
        .by_key(&request.source)
        .unwrap()
        .id;
    hydrus_store::content::tag_relations::apply(
        &store,
        hydrus_store::display::RelationKind::Parents,
        vec![hydrus_store::content::tag_relations::RelationUpdate {
            service: source,
            left: Tag::new("series:test").unwrap(),
            right: Tag::new("parent").unwrap(),
            action: hydrus_store::content::tag_relations::RelationAction::Add,
        }],
    )
    .unwrap();
    tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    request.content = Content::Parents;
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 1, |_| {}).unwrap();
    assert_eq!(p.accepted, 1);
    let destination = store
        .snapshot()
        .services
        .by_key(&request.destination)
        .unwrap()
        .id;
    let tags = store
        .read(|conn| hydrus_store::master::tag_id(conn, &Tag::new("parent").unwrap()))
        .unwrap()
        .unwrap();
    assert!(
        store
            .snapshot()
            .display
            .get(destination)
            .display_tags(TagId(1))
            .any(|t| t == tags)
    );
    let before = store
        .read(|conn| {
            Ok(conn.query_row(
                &format!(
                    "SELECT sum(current) FROM {}",
                    hydrus_store::schema::MappingTables::new(destination).display_counts
                ),
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    store
        .write(|ctx| hydrus_store::counts::rebuild_all(ctx.conn()))
        .unwrap();
    let after = store
        .read(|conn| {
            Ok(conn.query_row(
                &format!(
                    "SELECT sum(current) FROM {}",
                    hydrus_store::schema::MappingTables::new(destination).display_counts
                ),
                [],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .unwrap();
    assert_eq!(before, after);
}
#[test]
fn repository_union_deduplicates_and_petition_reason_is_persisted() {
    let (_dir, store, mut request) = setup();
    let repo = ServiceKey::new(vec![44; 16]);
    let key = repo.clone();
    store
        .write_and_refresh(move |ctx| {
            services::insert(
                ctx.conn(),
                &key,
                "repository",
                &ServiceKind::TagRepository(services::RepositoryConfig::default()),
            )?;
            Ok(())
        })
        .unwrap();
    request.destination = repo.clone();
    request.action = Action::Pend;
    tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |_| {}).unwrap();
    let id = store.snapshot().services.by_key(&repo).unwrap().id;
    let table = hydrus_store::schema::MappingTables::new(id);
    store
        .write(move |ctx| {
            ctx.conn().execute(
                &format!(
                    "INSERT INTO {} (tag_id,hash_id) SELECT tag_id,hash_id FROM {}",
                    table.current, table.pending
                ),
                [],
            )?;
            Ok(())
        })
        .unwrap();
    request.source = repo;
    request.status = Status::CurrentAndPending;
    request.destination = ServiceKey::new(vec![128; 16]);
    request.action = Action::Add;
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    assert_eq!(p.accepted, 11);
    assert_eq!(count(&store, &request.destination, false), 11);
    request.destination = request.source.clone();
    request.status = Status::Current;
    request.action = Action::Petition;
    request.reason = "oracle migration reason".into();
    tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    // Existing pending entries block petitions, as the reference state machine does.
    let id = store
        .snapshot()
        .services
        .by_key(&request.source)
        .unwrap()
        .id;
    store
        .write_content(move |writer| {
            writer.update_mappings(
                id,
                &MappingAction::RescindPend,
                TagId(1),
                &(1..=11).map(HashId).collect::<Vec<_>>(),
            )
        })
        .unwrap();
    tag_migration::run(&store, &request, &AtomicBool::new(false), 2, |_| {}).unwrap();
    let table = hydrus_store::schema::MappingTables::new(id).petitioned;
    let reasons = store.read(|conn| {
        Ok(conn
            .prepare(&format!(
                "SELECT DISTINCT text FROM {table} JOIN texts ON texts.text_id={table}.reason_id"
            ))?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    });
    // Reasons use the primary free-text intern table, not an ephemeral GUI value.
    assert_eq!(
        reasons.unwrap(),
        vec!["oracle migration reason".to_string()]
    );
}

#[test]
fn source_snapshot_excludes_later_inserts_and_retains_later_removed_rows() {
    let (_dir, store, request) = setup();
    let source = store
        .snapshot()
        .services
        .by_key(&request.source)
        .unwrap()
        .id;
    let change = store.clone();
    let p = tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |p| {
        if p.scanned == 3 {
            change
                .write_content(move |writer| {
                    writer.update_mappings(
                        source,
                        &MappingAction::Delete,
                        TagId(1),
                        &[HashId(11)],
                    )?;
                    let hash = hydrus_store::master::intern_hash(writer.conn(), &Sha256([88; 32]))?;
                    writer.update_mappings(source, &MappingAction::Add, TagId(1), &[hash])?;
                    Ok(())
                })
                .unwrap();
        }
    })
    .unwrap();
    assert_eq!(p.accepted, 11);
    let id = store
        .snapshot()
        .services
        .by_key(&request.destination)
        .unwrap()
        .id;
    let table = hydrus_store::schema::MappingTables::new(id).current;
    let ids = store
        .read(|conn| {
            Ok(conn
                .prepare(&format!("SELECT hash_id FROM {table} ORDER BY hash_id"))?
                .query_map([], |r| r.get::<_, HashId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .unwrap();
    assert_eq!(ids, (1..=11).map(HashId).collect::<Vec<_>>());
}
#[test]
fn overlapping_migration_is_rejected_without_waiting_for_a_reader() {
    let (_dir, store, request) = setup();
    let cancel = AtomicBool::new(false);
    let mut rejected = false;
    tag_migration::run(&store, &request, &cancel, 3, |p| {
        if p.scanned == 3 {
            rejected =
                tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |_| {}).is_err();
            // Ordinary store reads still succeed while the first snapshot is held.
            assert_eq!(count(&store, &request.destination, false), 3);
            cancel.store(true, Ordering::Release);
        }
    })
    .unwrap();
    assert!(rejected);
    tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |_| {}).unwrap();
    assert_eq!(count(&store, &request.destination, false), 11);
}

#[test]
fn pause_waits_after_commit_resume_continues_and_cancel_wakes_without_next_batch() {
    let recording = hydrus_testkit::fixture_json("tag_migration_pause.json");
    for case in recording.as_array().unwrap() {
        let (_dir, store, request) = setup();
        let paused = std::sync::Arc::new(AtomicBool::new(false));
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let (send, receive) = std::sync::mpsc::channel();
        let worker = {
            let store = store.clone();
            let request = request.clone();
            let paused = paused.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                tag_migration::run_pausable(&store, &request, &cancel, &paused, 3, |p| {
                    if p.scanned == 3 {
                        paused.store(true, Ordering::Release);
                    }
                    send.send(p).unwrap();
                })
                .unwrap()
            })
        };
        let first = receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(first.accepted, 3);
        assert_eq!(count(&store, &request.destination, false), 3);
        assert!(
            receive
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );
        assert!(case["paused"]["paused"].as_bool().unwrap());
        if case["action"] == "cancel" {
            cancel.store(true, Ordering::Release);
        } else {
            paused.store(false, Ordering::Release);
        }
        // Completion is bounded even when cancellation leaves paused=true.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !worker.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(worker.is_finished());
        let done = worker.join().unwrap();
        assert_eq!(done.cancelled, case["cancelled"].as_bool().unwrap());
        assert_eq!(done.accepted, if done.cancelled { 3 } else { 11 });
        assert_eq!(
            count(&store, &request.destination, false),
            i64::try_from(done.accepted).unwrap()
        );
    }
}
