use hydrus_core::{HashId, Sha256, TimestampMs};
use hydrus_store::{Store, content::DomainRoles};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    store
        .write(|ctx| {
            let mut preferences: hydrus_store::delete_lock::DeleteLock =
                hydrus_store::settings::get(ctx.conn())?;
            preferences.archived = false;
            hydrus_store::settings::set(ctx.conn(), &preferences)
        })
        .unwrap();
    ([legacy, native], store)
}

pub fn files(store: &Store) -> Vec<HashId> {
    hydrus_testkit::fixture_json("selected_deletion_records.json")["corpus"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| {
            let hash: Sha256 = h.as_str().unwrap().parse().unwrap();
            store
                .read(|conn| hydrus_store::master::hash_id(conn, &hash))
                .unwrap()
                .unwrap()
        })
        .collect()
}

pub fn reset(store: &Store, files: &[HashId]) {
    let local = store.snapshot().services.by_name("my files").unwrap().id;
    let files = files.to_vec();
    store
        .write_content(move |writer| {
            let rows: Vec<_> = files
                .iter()
                .map(|&file| (file, Some(1_700_000_000_000)))
                .collect();
            writer.add_files(local, &rows)?;
            writer.delete_files(
                writer.roles().combined_local_media,
                &files[..3],
                Some("fixture deletion"),
            )?;
            writer.delete_files(
                writer.roles().local_file_storage,
                &files[..2],
                Some("fixture deletion"),
            )
        })
        .unwrap();
}

pub fn state(store: &Store, files: &[HashId]) -> Value {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).unwrap();
    store.read(|conn| {
        let deleted = hydrus_store::media::deleted_from(conn, files, roles.local_file_storage)?;
        let current = hydrus_store::media::current_domains(conn, files)?;
        let statuses = files.iter().map(|&file| {
            let state = hydrus_store::urls::file_state(conn, &snapshot.services, file)?;
            Ok(hydrus_import::status::describe(&state, "", TimestampMs(1_700_000_010_000)).0.code())
        }).collect::<hydrus_store::Result<Vec<_>>>()?;
        Ok(json!({"storage_deleted":files.iter().map(|file|deleted.contains(file)).collect::<Vec<_>>(),"trash":files.iter().map(|file|current.get(file).is_some_and(|domains|domains.contains(&roles.trash))).collect::<Vec<_>>(),"status":statuses}))
    }).unwrap()
}

pub fn queue(store: &Store) -> Vec<(HashId, i64)> {
    store
        .read(|conn| {
            Ok(conn
                .prepare(
                    "SELECT hash_id,queued_ms FROM deferred_physical_deletes ORDER BY hash_id",
                )?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?)
        })
        .unwrap()
}
