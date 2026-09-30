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
                && remove_if_present(&path)?
            {
                report.files_deleted += 1;
            }
            if let Some(path) = snap.storage.thumbnail_path(&hash)
                && remove_if_present(&path)?
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

fn remove_if_present(path: &std::path::Path) -> Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
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
