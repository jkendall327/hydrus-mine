//! Global maintenance replay through real storage and reopened media consumers.
use hydrus_core::{HashId, Sha256};
use hydrus_gui_model::archive_repair as model;
use hydrus_store::{
    Store,
    archive_repair::{self, Plan, Population},
    content::{DomainRoles, FileTime},
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

fn seed(store: &Store, corpus: Value) -> Vec<HashId> {
    store.write_content(move |writer| {
        let roles=writer.roles().clone();
        let mut hashes=Vec::new();
        for (i,h) in corpus["hashes"].as_array().unwrap().iter().enumerate() {
            let hash: Sha256=h.as_str().unwrap().parse().unwrap();
            let id=hydrus_store::master::intern_hash(writer.conn(), &hash)?;
            hashes.push(id);
            let imported=corpus["imports"][i].as_i64();
            if let Some(deleted)=corpus["deletes"][i.to_string()].as_i64() {
                writer.conn().execute("INSERT INTO file_domain_deleted(service_id,hash_id,deleted_ms,original_added_ms) VALUES(?1,?2,?3,?4)",rusqlite::params![roles.combined_local_media,id,deleted,imported])?;
            } else {
                for service in [roles.local_file_storage,if i==9 {roles.trash} else {roles.combined_local_media}] {
                    writer.conn().execute("INSERT INTO file_domain_current(service_id,hash_id,added_ms) VALUES(?1,?2,?3)",rusqlite::params![service,id,imported])?;
                }
            }
            if i==4 {writer.conn().execute("INSERT INTO file_inbox VALUES(?)",[id])?;}
            if i==5 {writer.set_file_time(&[id],&FileTime::Archived,corpus["already_archived"]["5"].as_i64().unwrap())?;}
        }
        Ok(hashes)
    }).unwrap()
}
fn scan(store: &Store) -> Plan {
    store
        .read(|conn| {
            archive_repair::scan(
                conn,
                &DomainRoles::new(&store.snapshot().services)?,
                &AtomicBool::new(false),
            )
        })
        .unwrap()
}
fn times(store: &Store, hashes: &[HashId]) -> Value {
    store
        .read(|conn| {
            let batch = hydrus_store::media::load(conn, &store.snapshot().services, None, hashes)?;
            Ok(json!(
                batch
                    .results
                    .iter()
                    .map(|m| m.archived.map(|t| t.millis()))
                    .collect::<Vec<_>>()
            ))
        })
        .unwrap()
}
#[test]
fn exact_global_scan_questions_population_writes_and_reopened_times_match_reference() {
    let recorded = hydrus_testkit::fixture_json("archive_time_repair.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let hashes = seed(&store, recorded["corpus"].clone());
    for event in recorded["events"].as_array().unwrap() {
        assert_eq!(times(&store, &hashes), event["before"]);
        let plan = scan(&store);
        let asked = event["asked"].as_array().unwrap();
        assert_eq!(asked[0]["message"], model::SCAN_QUESTION);
        if asked.len() > 1 {
            assert_eq!(asked[1]["message"], model::question(&plan));
            let choices = model::choices(&plan);
            assert_eq!(
                json!(choices.iter().map(|c| c.label).collect::<Vec<_>>()),
                json!(
                    asked[1]["yes_tuples"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v[0].as_str().unwrap())
                        .collect::<Vec<_>>()
                )
            );
        }
        if let Some(chosen) = event["choice"].as_array() {
            let populations = chosen
                .iter()
                .map(|v| match v.as_str().unwrap() {
                    "legacy" => Population::Legacy,
                    "import" => Population::Import,
                    _ => panic!("unknown recorded population"),
                })
                .collect::<Vec<_>>();
            store
                .write_content(move |writer| {
                    archive_repair::apply(writer, &plan, &populations, &AtomicBool::new(false))
                })
                .unwrap();
        }
        assert_eq!(times(&store, &hashes), event["after"]);
        let plan = scan(&store);
        assert_eq!(
            json!([
                plan.count(Population::Legacy),
                plan.count(Population::Import)
            ]),
            event["counts"]
        );
    }
    assert_eq!(times(&store, &hashes), recorded["reopened_archived"]);
    let both_dir = tempfile::tempdir().unwrap();
    let both_store = Store::open(both_dir.path()).unwrap();
    let both_hashes = seed(&both_store, recorded["corpus"].clone());
    let both_plan = scan(&both_store);
    assert_eq!(
        model::question(&both_plan),
        recorded["both_events"][0]["asked"][1]["message"]
    );
    both_store
        .write_content(move |writer| {
            archive_repair::apply(
                writer,
                &both_plan,
                &[Population::Legacy, Population::Import],
                &AtomicBool::new(false),
            )
        })
        .unwrap();
    assert_eq!(
        times(&both_store, &both_hashes),
        recorded["both_events"][0]["after"]
    );
    drop(store);
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(times(&store, &hashes), recorded["reopened_archived"]);
}
#[test]
fn captured_scan_preserves_intervening_edits_and_cancellation_rolls_back_content_transaction() {
    let recorded = hydrus_testkit::fixture_json("archive_time_repair.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let hashes = seed(&store, recorded["corpus"].clone());
    let captured = scan(&store);
    let ids = hashes.clone();
    store.write_content(move |writer| {
        writer.set_file_time(&[ids[0]],&FileTime::Archived,12345)?;
        writer.inbox(&[ids[1]])?;
        writer.conn().execute("UPDATE file_domain_deleted SET original_added_ms=original_added_ms+1 WHERE hash_id=?",[ids[2]])?;
        Ok(())
    }).unwrap();
    let before = times(&store, &hashes);
    let ids = hashes.clone();
    let plan = captured.clone();
    let error = store
        .write_content(move |writer| {
            let cancel = AtomicBool::new(false);
            writer.set_file_time(&[ids[3]], &FileTime::Archived, 999)?;
            cancel.store(true, Ordering::Release);
            archive_repair::apply(
                writer,
                &plan,
                &[Population::Legacy, Population::Import],
                &cancel,
            )
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "Cancelled!");
    assert_eq!(times(&store, &hashes), before);
    let done = store
        .write_content(move |writer| {
            archive_repair::apply(
                writer,
                &captured,
                &[Population::Legacy, Population::Import],
                &AtomicBool::new(false),
            )
        })
        .unwrap();
    assert_eq!((done.legacy, done.import), (1, 1));
    let after = times(&store, &hashes);
    assert_eq!(after[0], 12345);
    assert!(after[1].is_null());
    assert!(after[2].is_null());
    assert!(after[8].is_null());
    assert_eq!(after[5], before[5]);
    assert_eq!(model::choices(&Plan::default()), Vec::new());
    let cancelled = AtomicBool::new(true);
    assert!(
        store
            .read(|conn| archive_repair::scan(
                conn,
                &DomainRoles::new(&store.snapshot().services)?,
                &cancelled
            ))
            .is_err()
    );
}
