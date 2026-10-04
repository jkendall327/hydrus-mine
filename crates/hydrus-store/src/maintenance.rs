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

/// Regenerate only the selected tags' connected display/storage caches from
/// primary data. Native display graphs publish atomically with the repaired
/// counts, so callers do not observe a temporary loss of relationships.
pub fn regenerate_tag_display(store: &Store, tags: &[hydrus_core::Tag]) -> Result<()> {
    let tags = tags.to_vec();
    store.write_and_refresh(move |ctx| {
        let conn = ctx.conn();
        let registry = crate::services::ServiceRegistry::load(conn)?;
        let graphs = crate::display::DisplayGraphs::load(conn, &registry)?;
        let selected = tags.iter().filter_map(|tag| crate::master::tag_id(conn, tag).transpose()).collect::<Result<Vec<_>>>()?;
        let affected = crate::counts::rebuild_tags(conn, &registry, &graphs, &selected)?;
        let affected: Vec<_> = affected.into_iter().collect();
        let subtags = conn.prepare(
            "SELECT DISTINCT s.subtag_id,s.subtag FROM tags t JOIN subtags s USING (subtag_id) WHERE t.tag_id IN rarray(?1)"
        )?.query_map([crate::master::id_array(&affected)], |row| Ok((row.get::<_,hydrus_core::SubtagId>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, subtag) in subtags {
            for table in ["cache_subtag_words", "cache_searchable_subtags", "cache_integer_subtags"] {
                conn.execute(&format!("DELETE FROM {table} WHERE subtag_id=?1"), [id])?;
            }
            crate::master::index_subtag(conn,id,&subtag)?;
        }
        Ok(())
    })
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
    fn selected_tag_regeneration_repairs_connected_counts_without_changing_primary_data() {
        use crate::content::{
            MappingAction,
            tag_relations::{self, RelationAction, RelationUpdate},
        };
        use crate::display::RelationKind;
        use crate::schema::MappingTables;
        use hydrus_core::Tag;
        let (_source, dest_dir, _db) = import_basic();
        let store = Store::open(dest_dir.path()).unwrap();
        let registry = store.snapshot().services.clone();
        let service = registry.by_name("my tags").unwrap().id;
        let second = registry.by_name("second tags").unwrap().id;
        let all = registry
            .of_type(hydrus_core::ServiceType::CombinedFile)
            .next()
            .unwrap()
            .id;
        let files: Vec<HashId> = store
            .read(|conn| {
                Ok(conn
                    .prepare("SELECT hash_id FROM files ORDER BY hash_id LIMIT 2")?
                    .query_map([], |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?)
            })
            .unwrap();
        let ids = store
            .write_content(move |writer| {
                let mut ids = Vec::new();
                for text in [
                    "parity:repair old",
                    "parity:repair ideal",
                    "parity:repair parent",
                    "parity:untouched",
                ] {
                    ids.push(crate::master::intern_tag(
                        writer.conn(),
                        &Tag::new(text).unwrap(),
                    )?);
                }
                for tag in [ids[0], ids[1], ids[3]] {
                    writer.update_mappings(service, &MappingAction::Add, tag, &files[..1])?;
                }
                writer.update_mappings(service, &MappingAction::Pend, ids[0], &files[1..])?;
                writer.update_mappings(second, &MappingAction::Add, ids[0], &files[1..])?;
                Ok(ids)
            })
            .unwrap();
        for service in [service, second] {
            for (kind, left, right) in [
                (
                    RelationKind::Siblings,
                    "parity:repair old",
                    "parity:repair ideal",
                ),
                (
                    RelationKind::Parents,
                    "parity:repair ideal",
                    "parity:repair parent",
                ),
            ] {
                tag_relations::apply(
                    &store,
                    kind,
                    vec![RelationUpdate {
                        service,
                        left: Tag::new(left).unwrap(),
                        right: Tag::new(right).unwrap(),
                        action: RelationAction::Add,
                    }],
                )
                .unwrap();
            }
        }
        let before = derived_rows(&store);
        let primary = |conn: &rusqlite::Connection| -> Result<Vec<String>> {
            let mut out = Vec::new();
            for service in [service, second] {
                let tables = MappingTables::new(service);
                for table in [tables.current, tables.pending] {
                    out.extend(
                        conn.prepare(&format!(
                            "SELECT tag_id,hash_id FROM {table} ORDER BY tag_id,hash_id"
                        ))?
                        .query_map([], |row| {
                            Ok(format!(
                                "{table}:{}:{}",
                                row.get::<_, u32>(0)?,
                                row.get::<_, u32>(1)?
                            ))
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?,
                    );
                }
            }
            Ok(out)
        };
        let primary_before = store.read(primary).unwrap();
        let ids_copy = ids.clone();
        store
            .write(move |ctx| {
                for service in [service, second] {
                    let tables = MappingTables::new(service);
                    for table in [&tables.counts, &tables.display_counts] {
                        ctx.conn().execute(
                            &format!("DELETE FROM {table} WHERE tag_id IN rarray(?1)"),
                            [crate::master::id_array(&ids_copy[..3])],
                        )?;
                    }
                }
                Ok(())
            })
            .unwrap();
        regenerate_tag_display(
            &store,
            &[
                Tag::new("parity:repair old").unwrap(),
                Tag::new("parity:unknown repair").unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(derived_rows(&store), before);
        assert_eq!(store.read(primary).unwrap(), primary_before);
        // Corrupt an unrelated count: a selected repair must deliberately leave it alone.
        let tables = MappingTables::new(service);
        let unrelated = ids[3];
        store
            .write(move |ctx| {
                ctx.conn().execute(
                    &format!(
                        "UPDATE {} SET current=93 WHERE tag_id=?1 AND domain_id=?2",
                        tables.counts
                    ),
                    params![unrelated, all],
                )?;
                Ok(())
            })
            .unwrap();
        regenerate_tag_display(&store, &[Tag::new("parity:repair parent").unwrap()]).unwrap();
        assert_eq!(
            store
                .read(|conn| crate::counts::count(conn, service, all, ids[3], false))
                .unwrap()
                .current,
            93
        );
        assert_eq!(
            store
                .read(|conn| crate::counts::count(conn, service, all, ids[1], true))
                .unwrap(),
            crate::counts::TagCount {
                current: 1,
                pending: 1
            }
        );
        assert_eq!(
            store
                .read(|conn| crate::counts::count(conn, second, all, ids[2], true))
                .unwrap()
                .current,
            1
        );
        assert_eq!(store.read(primary).unwrap(), primary_before);
        assert!(
            store
                .read(|conn| crate::master::tag_id(
                    conn,
                    &Tag::new("parity:unknown repair").unwrap()
                ))
                .unwrap()
                .is_none()
        );
        drop(store);
        let reopened = Store::open(dest_dir.path()).unwrap();
        assert_eq!(
            reopened
                .read(|conn| crate::counts::count(conn, service, all, ids[1], true))
                .unwrap(),
            crate::counts::TagCount {
                current: 1,
                pending: 1
            }
        );
        assert_eq!(reopened.read(primary).unwrap(), primary_before);
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
