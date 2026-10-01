//! Background maintenance jobs.

#[cfg(test)]
use std::path::Path;

use rusqlite::params;

use hydrus_core::{HashId, Mime, Sha256};

use crate::content::DomainRoles;
use crate::error::Result;
use crate::master::id_array;
use crate::settings;
use crate::store::Store;
use crate::transfer::MediaOwnership;

/// What a purge did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PurgeReport {
    pub files_deleted: usize,
    pub thumbnails_deleted: usize,
    /// Queued files that came back to local storage, so were kept.
    pub kept: usize,
}

/// Delete from disk the media of files that have left local storage.
///
/// Deletes are queued by the write that removes a file from local storage
/// and done here, outside any transaction: a file is only ever deleted
/// after its removal has committed. Media shared with another install (an
/// in-place import) is never deleted.
pub fn purge_deleted_media(store: &Store, batch: usize) -> Result<PurgeReport> {
    if store
        .read(settings::get::<MediaOwnership>)?
        .shared_with
        .is_some()
    {
        return Ok(PurgeReport::default());
    }
    // (thumbnails are always deleted for good, as in the reference)
    let recycle = store
        .read(settings::get::<settings::FolderSettings>)?
        .delete_to_recycle_bin;
    let snap = store.snapshot();
    let claims = store.media_claims();
    let local_storage = DomainRoles::new(&snap.services)?.local_file_storage;
    // Runs on the writer, so no write can re-add a file between the check
    // and the delete; importers claim a file before writing it to disk.
    store.write(move |ctx| {
        let conn = ctx.conn();
        let mut report = PurgeReport::default();
        let mut stmt = conn.prepare(
            "SELECT q.hash_id, h.sha256, f.mime,
                    EXISTS (SELECT 1 FROM file_domain_current d WHERE d.service_id = ?1 AND d.hash_id = q.hash_id)
             FROM deferred_physical_deletes q JOIN hashes h USING (hash_id) LEFT JOIN files f USING (hash_id)
             ORDER BY q.queued_ms LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![local_storage, i64::try_from(batch).unwrap_or(i64::MAX)], |r| {
            Ok((
                r.get::<_, HashId>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<u8>>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })?;
        let mut done = Vec::new();
        for row in rows {
            let (id, hash, mime, stored) = row?;
            let Ok(hash) = Sha256::from_slice(&hash) else {
                done.push(id);
                continue;
            };
            if stored {
                report.kept += 1;
                done.push(id);
                continue;
            }
            // an import of this file is in progress: leave it queued
            let Some(_claim) = claims.try_claim(hash) else {
                continue;
            };
            if let Some(path) = mime.and_then(Mime::from_code).and_then(|m| snap.storage.file_path(&hash, m))
                && remove_if_present(&path, recycle)?
            {
                report.files_deleted += 1;
            }
            if let Some(path) = snap.storage.thumbnail_path(&hash)
                && remove_if_present(&path, false)?
            {
                report.thumbnails_deleted += 1;
            }
            done.push(id);
        }
        conn.prepare_cached("DELETE FROM deferred_physical_deletes WHERE hash_id IN rarray(?1)")?
            .execute([id_array(&done)])?;
        Ok(report)
    })
}

/// Drop and rebuild every derived table from the primary ones (ADR-6):
/// the subtags' word and number indexes, the notes' full-text index and
/// the autocomplete counts, so a derived-data bug is never data loss.
pub fn rebuild_caches(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM cache_subtag_words;
         DELETE FROM cache_searchable_subtags;
         DELETE FROM cache_integer_subtags;
         INSERT INTO cache_note_fts (cache_note_fts) VALUES ('delete-all');",
    )?;
    {
        let mut stmt = conn.prepare("SELECT subtag_id, subtag FROM subtags")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, hydrus_core::SubtagId>(0)?,
                r.get::<_, String>(1)?,
            ))
        })?;
        for row in rows {
            let (id, subtag) = row?;
            crate::master::index_subtag(conn, id, &subtag)?;
        }
    }
    conn.execute_batch("INSERT INTO cache_note_fts (rowid, note) SELECT note_id, note FROM notes")?;
    crate::counts::rebuild_all(conn)
}

fn remove_if_present(path: &std::path::Path, recycle: bool) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
        Ok(_) => {
            crate::paths::delete_or_recycle(path, recycle)?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::tests::import_basic;
    use crate::transfer::{TransferMode, transfer_media};

    /// A store with its own copy of the basic fixture's media, and a file in it.
    fn owned_store() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        std::sync::Arc<Store>,
        HashId,
        std::path::PathBuf,
    ) {
        let (source, dest_dir, db) = import_basic();
        transfer_media(&db, &dest_dir.path().join("media"), TransferMode::Copy).unwrap();
        let store = Store::open(dest_dir.path()).unwrap();
        // (the fixture recycles deleted files: keep them out of the real bin)
        let folders = settings::FolderSettings {
            delete_to_recycle_bin: false,
            ..settings::FolderSettings::default()
        };
        store
            .write(move |ctx| settings::set(ctx.conn(), &folders))
            .unwrap();
        let (id, path) = store
            .read(|c| {
                let (id, hash, mime): (HashId, Vec<u8>, u8) = c.query_row(
                    "SELECT hash_id, sha256, mime FROM files JOIN hashes USING (hash_id)
                     JOIN file_domain_current USING (hash_id) WHERE service_id = ?1 LIMIT 1",
                    [DomainRoles::new(&store.snapshot().services)?.local_file_storage],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
                let hash = Sha256::from_slice(&hash).unwrap();
                let path = store
                    .snapshot()
                    .storage
                    .file_path(&hash, Mime::from_code(mime).unwrap())
                    .unwrap();
                Ok((id, path))
            })
            .unwrap();
        assert!(path.is_file());
        (source, dest_dir, store, id, path)
    }

    fn purge_file(store: &Store, id: HashId) {
        store
            .write_content(move |w| {
                let storage = w.roles().local_file_storage;
                w.delete_files(storage, &[id], None)
            })
            .unwrap();
    }

    /// Every derived table's rows, by table.
    fn derived_rows(store: &Store) -> Vec<(String, Vec<String>)> {
        store
            .read(|c| {
                let tables: Vec<String> = c
                    .prepare(
                        "SELECT name FROM sqlite_master WHERE type = 'table'
                         AND (name LIKE 'cache%' OR name LIKE '%counts%')
                         AND name NOT LIKE 'cache_note_fts%' ORDER BY name",
                    )?
                    .query_map([], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?;
                let mut out = Vec::new();
                for table in tables.into_iter().chain(["cache_note_fts".to_owned()]) {
                    let sql = if table == "cache_note_fts" {
                        "SELECT rowid FROM cache_note_fts".to_owned()
                    } else {
                        format!("SELECT * FROM {table}")
                    };
                    let mut stmt = c.prepare(&sql)?;
                    let width = stmt.column_count();
                    let mut rows: Vec<String> = stmt
                        .query_map([], |r| {
                            (0..width)
                                .map(|i| {
                                    r.get::<_, rusqlite::types::Value>(i)
                                        .map(|v| format!("{v:?}"))
                                })
                                .collect::<rusqlite::Result<Vec<_>>>()
                                .map(|v| v.join("|"))
                        })?
                        .collect::<rusqlite::Result<_>>()?;
                    rows.sort();
                    out.push((table, rows));
                }
                Ok(out)
            })
            .unwrap()
    }

    #[test]
    fn rebuilding_the_caches_gives_the_same_caches() {
        let (_source, dest_dir, _db) = import_basic();
        let store = Store::open(dest_dir.path()).unwrap();
        let before = derived_rows(&store);
        assert!(before.iter().any(|(_, rows)| !rows.is_empty()));
        store.write(|ctx| rebuild_caches(ctx.conn())).unwrap();
        assert_eq!(derived_rows(&store), before);
    }

    #[test]
    fn purges_files_that_left_local_storage() {
        let (_source, _dir, store, id, path) = owned_store();
        purge_file(&store, id);
        assert!(path.is_file(), "nothing is deleted inside the write");
        let report = purge_deleted_media(&store, 100).unwrap();
        assert_eq!(report.files_deleted, 1);
        assert!(!path.exists());
        assert_eq!(
            purge_deleted_media(&store, 100).unwrap(),
            PurgeReport::default()
        );
    }

    #[test]
    fn leaves_claimed_files_queued() {
        let (_source, _dir, store, id, path) = owned_store();
        let hash = store
            .read(|c| Ok(crate::master::hashes(c, &[id])?[&id]))
            .unwrap();
        purge_file(&store, id);
        let claim = store.media_claims().claim(hash);
        assert_eq!(purge_deleted_media(&store, 100).unwrap().files_deleted, 0);
        assert!(path.is_file());
        drop(claim);
        assert_eq!(purge_deleted_media(&store, 100).unwrap().files_deleted, 1);
    }

    #[test]
    fn never_deletes_media_shared_with_another_install() {
        let (_source, dest_dir, db) = import_basic();
        transfer_media(&db, Path::new("/unused"), TransferMode::InPlace).unwrap();
        let store = Store::open(dest_dir.path()).unwrap();
        let id: HashId = store
            .read(|c| Ok(c.query_row("SELECT hash_id FROM files LIMIT 1", [], |r| r.get(0))?))
            .unwrap();
        purge_file(&store, id);
        assert_eq!(
            purge_deleted_media(&store, 100).unwrap(),
            PurgeReport::default()
        );
    }
}
