//! Actual reference DB replay, transaction boundaries and reopened consumers.
use hydrus_core::{HashId, Sha256};
use hydrus_gui_model::viewing_maintenance::Operation;
use hydrus_store::{
    Store,
    settings::{self, FileViewingStatistics},
};
use serde_json::{Value, json};

fn seed(store: &Store, rows: Value) {
    store.write(move |ctx| {
        let conn = ctx.conn();
        hydrus_store::viewing_maintenance::clear(conn)?;
        for row in rows.as_array().unwrap() {
            let index = row[0].as_i64().unwrap();
            let hash = Sha256([u8::try_from(index).unwrap(); 32]);
            let id = hydrus_store::master::intern_hash(conn, &hash)?;
            assert_eq!(i64::from(id.get()), index);
            conn.execute("INSERT INTO file_viewing_stats (hash_id,canvas_type,last_viewed_ms,views,viewtime_ms) VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![id,row[1].as_i64().unwrap(),row[2].as_i64().unwrap(),row[3].as_i64().unwrap(),row[4].as_i64().unwrap()])?;
        }
        Ok(())
    }).unwrap();
}
fn state(store: &Store) -> Value {
    store.read(|conn| {
        let mut statement = conn.prepare("SELECT hash_id,canvas_type,last_viewed_ms,views,viewtime_ms FROM file_viewing_stats ORDER BY hash_id,canvas_type")?;
        let rows=statement.query_map([],|row| Ok([row.get::<_,i64>(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?]))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!(rows))
    }).unwrap()
}
// leaf: audit-media-menu-database-clear-clear-all-file-viewing-statistics
// leaf: audit-media-menu-database-clear-cull-file-viewing-statistics-based-on-current-min-max-values
#[test]
fn reference_clear_cull_questions_rules_errors_and_real_reopened_media_match() {
    let fixture = hydrus_testkit::fixture_json("viewing_maintenance.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for event in fixture["events"].as_array().unwrap() {
        seed(&store, fixture["rows"].clone());
        let bounds = &event["bounds"];
        let rules = FileViewingStatistics {
            media_min_ms: bounds[0].as_u64(),
            media_max_ms: bounds[1].as_u64(),
            preview_min_ms: bounds[2].as_u64(),
            preview_max_ms: bounds[3].as_u64(),
            ..Default::default()
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &rules))
            .unwrap();
        let operation = if event["operation"] == "clear" {
            Operation::Clear
        } else {
            Operation::Cull
        };
        assert_eq!(event["asked"][0]["message"], operation.question());
        assert_eq!(event["asked"][0]["yes_label"], "do it");
        assert_eq!(event["asked"][0]["no_label"], "forget it");
        if event["accept"].as_bool().unwrap() {
            let applied = operation.apply(&store);
            if let Some(error) = event["error"].as_str() {
                assert!(applied.unwrap_err().to_string().contains(error));
            } else {
                applied.unwrap();
                assert_eq!(event["information"][0]["message"], operation.completed());
            }
        }
        assert_eq!(state(&store), event["after"], "{event}");
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(state(&reopened), event["after"]);
        // This reads the actual media consumer, not only the maintenance rows.
        let media = reopened
            .read(|conn| hydrus_store::media::viewing_stats(conn, &[HashId(1)]))
            .unwrap();
        assert_eq!(
            media.len(),
            usize::from(event["operation"] != "clear" || !event["accept"].as_bool().unwrap())
        );
    }
}

#[test]
fn cull_reads_current_rules_and_a_preview_validation_failure_preserves_media_rows() {
    let fixture = hydrus_testkit::fixture_json("viewing_maintenance.json");
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    seed(&store, fixture["rows"].clone());
    let before = state(&store);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewingStatistics {
                    media_min_ms: Some(2000),
                    media_max_ms: Some(1000),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert!(Operation::Cull.apply(&store).is_err());
    assert_eq!(state(&store), before);
    store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &FileViewingStatistics {
                    media_min_ms: Some(2000),
                    media_max_ms: Some(60_000),
                    preview_min_ms: Some(10_001),
                    preview_max_ms: Some(10_000),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    assert!(Operation::Cull.apply(&store).is_err());
    assert_eq!(state(&store), before);
}
