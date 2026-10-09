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

// leaf: audit-media-migration-pause
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
                let mut batches = 0;
                let mut last_scanned = 0;
                let done =
                    tag_migration::run_pausable(&store, &request, &cancel, &paused, 3, |p| {
                        if p.scanned > last_scanned {
                            batches += 1;
                            last_scanned = p.scanned;
                        }
                        if p.scanned == 3 {
                            paused.store(true, Ordering::Release);
                        }
                        send.send(p).unwrap();
                    })
                    .unwrap();
                (done, batches)
            })
        };
        let first = receive
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(
            first.accepted,
            case["paused"]["accepted"].as_array().unwrap().len()
        );
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
        let (done, batches) = worker.join().unwrap();
        assert_eq!(batches, case["batches"].as_u64().unwrap());
        assert_eq!(done.cancelled, case["cancelled"].as_bool().unwrap());
        assert_eq!(done.accepted, case["accepted"].as_array().unwrap().len());
        assert_eq!(done.scanned, done.accepted);
        assert!(case["done"].as_bool().unwrap());
        assert_eq!(
            case["cleanup"],
            serde_json::json!(["source", "destination"])
        );
        assert_eq!(case["text"], "done!");
        let source = store
            .snapshot()
            .services
            .by_key(&request.source)
            .unwrap()
            .id;
        let key = request.destination.clone();
        let dest = store.snapshot().services.by_key(&key).unwrap().id;
        let expected = case["accepted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| HashId(u32::try_from(value.as_u64().unwrap()).unwrap() + 1))
            .collect::<Vec<_>>();
        let actual = store
            .read(|conn| {
                Ok(conn
                    .prepare(&format!(
                        "SELECT hash_id FROM {} ORDER BY hash_id",
                        hydrus_store::schema::MappingTables::new(dest).current
                    ))?
                    .query_map([], |row| row.get::<_, HashId>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?)
            })
            .unwrap();
        assert_eq!(actual, expected);
        // Releasing the reservation is the native cleanup observable. The next
        // job succeeds and completes the missing prefix without changing source.
        tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |_| {}).unwrap();
        assert_eq!(count(&store, &request.destination, false), 11);
        assert_eq!(
            source,
            store
                .snapshot()
                .services
                .by_key(&request.source)
                .unwrap()
                .id
        );
    }
}

fn archive_hashes(store: &Store, recording: &serde_json::Value) -> Vec<HashId> {
    let known = recording["known_sha256"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().parse::<Sha256>().unwrap())
        .collect::<Vec<_>>();
    let digests = (0..known.len())
        .map(|i| {
            ["md5", "sha1", "sha512"].map(|kind| {
                let archive = recording["archives"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["kind"] == kind)
                    .unwrap();
                let row = archive["mappings"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| {
                        row["tags"]
                            .as_array()
                            .unwrap()
                            .contains(&serde_json::json!(format!("archive:known-{i}")))
                    })
                    .unwrap();
                hex::decode(row["hash"].as_str().unwrap()).unwrap()
            })
        })
        .collect::<Vec<_>>();
    store.write(move|ctx| {
        known.iter().zip(digests).map(|(sha,[md5,sha1,sha512])| {
            let id=hydrus_store::master::intern_hash(ctx.conn(),sha)?;
            ctx.conn().execute("INSERT OR REPLACE INTO hash_digests(hash_id,md5,sha1,sha512) VALUES(?1,?2,?3,?4)",rusqlite::params![id,md5,sha1,sha512])?;
            Ok(id)
        }).collect()
    }).unwrap()
}
fn destination_rows(store: &Store, key: &ServiceKey) -> serde_json::Value {
    let service = store.snapshot().services.by_key(key).unwrap().id;
    let table = hydrus_store::schema::MappingTables::new(service).current;
    let rows = store
        .read(|conn| {
            let ids = conn
                .prepare(&format!("SELECT tag_id,hash_id FROM {table}"))?
                .query_map([], |r| Ok((r.get::<_, TagId>(0)?, r.get::<_, HashId>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut result = std::collections::BTreeMap::<String, Vec<String>>::new();
            for (tag, hash) in ids {
                result
                    .entry(hydrus_store::master::hash(conn, hash)?.unwrap().to_hex())
                    .or_default()
                    .push(
                        hydrus_store::master::tag(conn, tag)?
                            .unwrap()
                            .as_str()
                            .to_owned(),
                    );
            }
            Ok(result
                .into_iter()
                .map(|(hash, mut tags)| {
                    tags.sort();
                    serde_json::json!({"hash":hash,"tags":tags})
                })
                .collect::<Vec<_>>())
        })
        .unwrap();
    serde_json::json!(rows)
}
fn archive_rows(path: &std::path::Path) -> serde_json::Value {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut grouped = std::collections::BTreeMap::<String, Vec<String>>::new();
    for row in conn
        .prepare("SELECT hash,tag FROM mappings JOIN hashes USING(hash_id) JOIN tags USING(tag_id)")
        .unwrap()
        .query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })
        .unwrap()
    {
        let (hash, tag) = row.unwrap();
        grouped.entry(hex::encode(hash)).or_default().push(tag);
    }
    serde_json::json!(
        grouped
            .into_iter()
            .map(|(hash, mut tags)| {
                tags.sort();
                serde_json::json!({"hash":hash,"tags":tags})
            })
            .collect::<Vec<_>>()
    )
}
// leaf: migration-archive-destination
// leaf: migration-archive-source
#[test]
fn actual_python_archives_import_convert_scope_filter_and_reopen() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    for case in recording["conversion"].as_array().unwrap() {
        let (dir, store, mut request) = setup();
        let hashes = archive_hashes(&store, &recording);
        if case["selected"].as_bool().unwrap() {
            request.scope = Scope::Files(hashes[..1].to_vec());
        }
        if let Some(domain) = case.get("domain") {
            let service_id = store.snapshot().services.by_name("my files").unwrap().id;
            let membership = recording["domain_membership"].as_array().unwrap().clone();
            store.write(move|ctx| {
                for row in membership {
                    let sha=row["hash"].as_str().unwrap().parse::<Sha256>().unwrap();
                    let hash=hydrus_store::master::hash_id(ctx.conn(),&sha)?.unwrap();
                    for (field,table) in [("current","file_domain_current"),("deleted","file_domain_deleted")] {
                        if row[field].as_bool().unwrap() {ctx.conn().execute(&format!("INSERT OR IGNORE INTO {table}(service_id,hash_id) VALUES(?1,?2)"),rusqlite::params![service_id,hash])?;}
                    }
                }
                Ok(())
            }).unwrap();
            let key = store
                .snapshot()
                .services
                .by_name("my files")
                .unwrap()
                .key
                .clone();
            request.scope = Scope::Location(if domain == "current" {
                hydrus_core::search::context::LocationContext::single(key)
            } else {
                hydrus_core::search::context::LocationContext::new(vec![], vec![key])
            });
        }
        request.left_filter =
            TagFilter::new().with_rule("creator:", hydrus_core::FilterRule::Blacklist);
        let path = hydrus_testkit::fixture_path(format!(
            "tag_archive_{}.db",
            case["kind"].as_str().unwrap()
        ));
        let before = std::fs::read(&path).unwrap();
        let kind = case["desired"]
            .as_str()
            .unwrap()
            .parse::<hydrus_core::HashKind>()
            .unwrap();
        let output = dir.path().join("converted.db");
        let options = tag_migration::Options {
            source: Some(path.clone()),
            destination: (kind != hydrus_core::HashKind::Sha256).then(|| output.clone()),
            hash_kind: kind,
            ..Default::default()
        };
        let done = tag_migration::run_job(
            &store,
            &request,
            &options,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            2,
            |_| {},
        )
        .unwrap();
        assert_eq!(
            if options.destination.is_some() {
                archive_rows(&output)
            } else {
                destination_rows(&store, &request.destination)
            },
            case["mappings"]
        );
        assert_eq!(
            done.accepted,
            case["mappings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["tags"].as_array().unwrap().len())
                .sum::<usize>()
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "source archive is read-only"
        );
        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(
            if options.destination.is_some() {
                archive_rows(&output)
            } else {
                destination_rows(&reopened, &request.destination)
            },
            case["mappings"]
        );
    }
}
// leaf: migration-archive-destination
#[test]
fn archive_exports_preserve_metadata_merge_and_cancelled_committed_prefix() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    let (dir, store, mut request) = setup();
    archive_hashes(&store, &recording);
    let source = tag_migration::Options {
        source: Some(hydrus_testkit::fixture_path("tag_archive_sha256.db")),
        ..Default::default()
    };
    tag_migration::run_job(
        &store,
        &request,
        &source,
        &AtomicBool::new(false),
        &AtomicBool::new(false),
        2,
        |_| {},
    )
    .unwrap();
    request.source = request.destination.clone();
    for kind in [
        hydrus_core::HashKind::Sha256,
        hydrus_core::HashKind::Md5,
        hydrus_core::HashKind::Sha1,
        hydrus_core::HashKind::Sha512,
    ] {
        let name = match kind {
            hydrus_core::HashKind::Sha256 => "sha256",
            hydrus_core::HashKind::Md5 => "md5",
            hydrus_core::HashKind::Sha1 => "sha1",
            hydrus_core::HashKind::Sha512 => "sha512",
        };
        let destination = dir.path().join(format!("export-{name}.db"));
        let options = tag_migration::Options {
            destination: Some(destination.clone()),
            hash_kind: kind,
            ..Default::default()
        };
        let cancel = AtomicBool::new(false);
        let done = tag_migration::run_job(
            &store,
            &request,
            &options,
            &cancel,
            &AtomicBool::new(false),
            2,
            |p| {
                if p.scanned >= 2 {
                    cancel.store(true, Ordering::Release);
                }
            },
        )
        .unwrap();
        assert!(done.cancelled);
        let conn = rusqlite::Connection::open(&destination).unwrap();
        let prefix: usize = usize::try_from(
            conn.query_row("SELECT count(*) FROM mappings", [], |r| r.get::<_, i64>(0))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(prefix, done.accepted);
        drop(conn);
        // Existing metadata wins even if the caller requests a different kind.
        let options = tag_migration::Options {
            hash_kind: hydrus_core::HashKind::Sha512,
            ..options
        };
        tag_migration::run_job(
            &store,
            &request,
            &options,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            3,
            |_| {},
        )
        .unwrap();
        assert_eq!(
            tag_migration::archive::inspect(&destination, Content::Mappings).unwrap(),
            tag_migration::archive::Metadata::Mappings(kind)
        );
        let conn = rusqlite::Connection::open(&destination).unwrap();
        let mut grouped = std::collections::BTreeMap::<String, Vec<String>>::new();
        for row in conn
            .prepare(
                "SELECT hash,tag FROM mappings JOIN hashes USING(hash_id) JOIN tags USING(tag_id)",
            )
            .unwrap()
            .query_map([], |r| {
                Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
        {
            let (hash, tag) = row.unwrap();
            grouped.entry(hex::encode(hash)).or_default().push(tag);
        }
        let actual = grouped
            .into_iter()
            .map(|(hash, mut tags)| {
                tags.sort();
                serde_json::json!({"hash":hash,"tags":tags})
            })
            .collect::<Vec<_>>();
        let expected = recording["archives"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["kind"] == name)
            .unwrap();
        let expected = expected["mappings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                kind == hydrus_core::HashKind::Sha256
                    || !row["tags"]
                        .as_array()
                        .unwrap()
                        .contains(&serde_json::json!("archive:unknown"))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(serde_json::json!(actual), serde_json::json!(expected));
    }
}
#[test]
fn pair_count_gates_replay_real_current_pending_and_terminal_ideal_counts() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    let (dir, store, mut request) = setup();
    let local = request.source.clone();
    let repo = ServiceKey::new(vec![66; 16]);
    let key = repo.clone();
    store
        .write_and_refresh(move |ctx| {
            services::insert(
                ctx.conn(),
                &key,
                "count repository",
                &ServiceKind::TagRepository(services::RepositoryConfig::default()),
            )?;
            Ok(())
        })
        .unwrap();
    let sid = store.snapshot().services.by_key(&local).unwrap().id;
    let rid = store.snapshot().services.by_key(&repo).unwrap().id;
    store
        .write_content(move |writer| {
            for (service, action, text) in [
                (sid, MappingAction::Add, "archive:left"),
                (sid, MappingAction::Add, "archive:ideal"),
                (rid, MappingAction::Pend, "archive:pending"),
            ] {
                let tag =
                    hydrus_store::master::intern_tag(writer.conn(), &Tag::new(text).unwrap())?;
                writer.update_mappings(service, &action, tag, &[HashId(1)])?;
            }
            Ok(())
        })
        .unwrap();
    hydrus_store::content::tag_relations::apply(
        &store,
        hydrus_store::display::RelationKind::Siblings,
        vec![hydrus_store::content::tag_relations::RelationUpdate {
            service: sid,
            left: Tag::new("archive:right").unwrap(),
            right: Tag::new("archive:ideal").unwrap(),
            action: hydrus_store::content::tag_relations::RelationAction::Add,
        }],
    )
    .unwrap();
    for (content, kind) in [
        ("siblings", hydrus_store::display::RelationKind::Siblings),
        ("parents", hydrus_store::display::RelationKind::Parents),
    ] {
        let archive = recording["archives"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["kind"] == content)
            .unwrap();
        let updates = archive["pairs"]
            .as_array()
            .unwrap()
            .iter()
            .map(
                |pair| hydrus_store::content::tag_relations::RelationUpdate {
                    service: sid,
                    left: Tag::new(pair[0].as_str().unwrap()).unwrap(),
                    right: Tag::new(pair[1].as_str().unwrap()).unwrap(),
                    action: hydrus_store::content::tag_relations::RelationAction::Add,
                },
            )
            .collect();
        hydrus_store::content::tag_relations::apply(&store, kind, updates).unwrap();
    }
    for (index, case) in recording["counts"].as_array().unwrap().iter().enumerate() {
        request.content = if case["content"] == "siblings" {
            Content::Siblings
        } else {
            Content::Parents
        };
        for archive_source in [true, false] {
            let destination = dir
                .path()
                .join(format!("counts-{index}-{archive_source}.db"));
            let options = tag_migration::Options {
                source: archive_source.then(|| {
                    hydrus_testkit::fixture_path(format!(
                        "tag_archive_{}.db",
                        case["content"].as_str().unwrap()
                    ))
                }),
                destination: Some(destination.clone()),
                counts: Some(tag_migration::PairCounts {
                    service: if case["service"] == "local" {
                        local.clone()
                    } else {
                        repo.clone()
                    },
                    left: case["left"].as_bool().unwrap(),
                    right: case["right"].as_bool().unwrap(),
                    either: case["either"].as_bool().unwrap(),
                }),
                ..Default::default()
            };
            let done = tag_migration::run_job(
                &store,
                &request,
                &options,
                &AtomicBool::new(false),
                &AtomicBool::new(false),
                2,
                |_| {},
            )
            .unwrap();
            let conn = rusqlite::Connection::open(destination).unwrap();
            let rows=conn.prepare("SELECT a.tag,b.tag FROM pairs p JOIN tags a ON p.tag_id_1=a.tag_id JOIN tags b ON p.tag_id_2=b.tag_id ORDER BY a.tag,b.tag").unwrap().query_map([],|r|Ok([r.get::<_,String>(0)?,r.get::<_,String>(1)?])).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
            assert_eq!(serde_json::json!(rows), case["pairs"]);
            assert_eq!(done.accepted, rows.len());
        }
    }
}
// leaf: migration-archive-source
#[test]
fn archive_validation_rejects_wrong_types_and_unsupported_hashes_before_import() {
    let (dir, store, mut request) = setup();
    let mut options = tag_migration::Options {
        source: Some(hydrus_testkit::fixture_path("tag_archive_siblings.db")),
        ..Default::default()
    };
    request.content = Content::Parents;
    assert!(
        tag_migration::run_job(
            &store,
            &request,
            &options,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            2,
            |_| {}
        )
        .unwrap_err()
        .to_string()
        .contains("not a tag parents archive")
    );
    request.content = Content::Mappings;
    let path = dir.path().join("unsupported.db");
    std::fs::copy(hydrus_testkit::fixture_path("tag_archive_sha256.db"), &path).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("UPDATE hash_type SET hash_type=99", [])
        .unwrap();
    drop(conn);
    options.source = Some(path);
    assert!(
        tag_migration::run_job(
            &store,
            &request,
            &options,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            2,
            |_| {}
        )
        .unwrap_err()
        .to_string()
        .contains("unsupported archive hash type")
    );
    assert_eq!(count(&store, &request.destination, false), 0);
    options.source = Some(dir.path().join("absent.db"));
    assert!(
        tag_migration::run_job(
            &store,
            &request,
            &options,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            2,
            |_| {}
        )
        .is_err()
    );
    assert!(!options.source.unwrap().exists());
}

#[test]
fn pair_count_gates_observe_reference_concurrent_mapping_changes_between_batches() {
    let recording = hydrus_testkit::fixture_json("tag_archives.json");
    let case = &recording["dynamic_counts"];
    let (dir, store, mut request) = setup();
    request.content = Content::Parents;
    let service = store
        .snapshot()
        .services
        .by_key(&request.source)
        .unwrap()
        .id;
    store
        .write_content(move |writer| {
            let tag = hydrus_store::master::intern_tag(
                writer.conn(),
                &Tag::new("archive:left").unwrap(),
            )?;
            writer.update_mappings(service, &MappingAction::Add, tag, &[HashId(1)])
        })
        .unwrap();
    let output = dir.path().join("live-counts.db");
    let options = tag_migration::Options {
        source: Some(hydrus_testkit::fixture_path("tag_archive_parents.db")),
        destination: Some(output.clone()),
        counts: Some(tag_migration::PairCounts {
            service: request.source.clone(),
            left: true,
            right: false,
            either: false,
        }),
        ..Default::default()
    };
    let added = case["added_after_first_batch"].as_str().unwrap().to_owned();
    let observer = store.clone();
    let done = tag_migration::run_job(
        &store,
        &request,
        &options,
        &AtomicBool::new(false),
        &AtomicBool::new(false),
        usize::try_from(case["batch_size"].as_u64().unwrap()).unwrap(),
        |p| {
            if p.scanned == 1 {
                let text = added.clone();
                observer
                    .write_content(move |writer| {
                        let tag = hydrus_store::master::intern_tag(
                            writer.conn(),
                            &Tag::new(&text).unwrap(),
                        )?;
                        writer.update_mappings(service, &MappingAction::Add, tag, &[HashId(1)])
                    })
                    .unwrap();
            }
        },
    )
    .unwrap();
    let conn = rusqlite::Connection::open(output).unwrap();
    let pairs=conn.prepare("SELECT a.tag,b.tag FROM pairs p JOIN tags a ON p.tag_id_1=a.tag_id JOIN tags b ON p.tag_id_2=b.tag_id ORDER BY a.tag,b.tag").unwrap().query_map([],|r|Ok([r.get::<_,String>(0)?,r.get::<_,String>(1)?])).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(serde_json::json!(pairs), case["pairs"]);
    assert_eq!(done.accepted, pairs.len());
}

#[test]
fn published_job_events_match_reference_phases_and_committed_prefixes() {
    use tag_migration::{Event, Options};
    let recording = hydrus_testkit::fixture_json("tag_migration_progress.json");
    for case in recording.as_array().unwrap() {
        for archive_destination in [false, true] {
            let (dir, store, request) = setup();
            let options = Options {
                destination: archive_destination.then(|| dir.path().join("output.db")),
                ..Options::default()
            };
            let cancel = AtomicBool::new(false);
            let mut events = Vec::new();
            let mut accepted = Vec::new();
            let done = tag_migration::run_job_events(
                &store,
                &request,
                &options,
                &cancel,
                &AtomicBool::new(false),
                3,
                |event| {
                    let phase = match event {
                        Event::PreparingSource => "preparing source",
                        Event::PreparingDestination => "preparing destination",
                        Event::BeginningWork => "beginning work",
                        Event::Batch { progress, .. } => {
                            accepted.push(progress.accepted);
                            if case["action"] == "cancel" {
                                cancel.store(true, Ordering::Release);
                            }
                            "batch"
                        }
                        Event::CleaningSource => "done, cleaning up source",
                        Event::CleaningDestination => "done, cleaning up destination",
                        Event::Done(_) => "done!",
                    };
                    events.push(phase.to_owned());
                },
            )
            .unwrap();
            let expected = case["timeline"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    let text = entry["status"].as_str().unwrap();
                    if text.ends_with("rows/s") {
                        "batch"
                    } else {
                        text
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(events, expected);
            let mut total = 0;
            let expected = case["speed_inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| {
                    total += usize::try_from(entry["rows"].as_u64().unwrap()).unwrap();
                    total
                })
                .collect::<Vec<_>>();
            assert_eq!(accepted, expected);
            assert_eq!(done.accepted, case["accepted"].as_array().unwrap().len());
            assert_eq!(done.cancelled, case["cancelled"].as_bool().unwrap());
            if let Some(path) = &options.destination {
                let metadata = tag_migration::archive::inspect(path, Content::Mappings).unwrap();
                assert_eq!(
                    metadata,
                    tag_migration::archive::Metadata::Mappings(hydrus_core::HashKind::Sha256)
                );
                let rows = rusqlite::Connection::open(path)
                    .unwrap()
                    .query_row("SELECT count(*) FROM mappings", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .unwrap();
                assert_eq!(usize::try_from(rows).unwrap(), done.accepted);
            } else {
                assert_eq!(
                    usize::try_from(count(&store, &request.destination, false)).unwrap(),
                    done.accepted
                );
            }
            // Both paths release the reservation when the completed call
            // returns, allowing the next normal migration to finish.
            tag_migration::run(&store, &request, &AtomicBool::new(false), 3, |_| {}).unwrap();
        }
    }
}
