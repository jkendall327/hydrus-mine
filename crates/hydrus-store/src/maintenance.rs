//! Background maintenance jobs.

#[cfg(test)]
use std::path::Path;

use rusqlite::params;

use hydrus_core::{HashId, Mime, Sha256};

use crate::content::DomainRoles;
use crate::error::Result;
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

/// Permanent cancellation for one maintenance owner's interruptible waits.
#[derive(Debug, Clone, Default)]
pub struct PurgeControl(std::sync::Arc<PurgeWait>);
#[derive(Debug, Default)]
struct PurgeWait {
    cancelled: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
}
impl PurgeControl {
    /// Wake a pending wait and prevent later queue admissions for this owner.
    pub fn cancel(&self) {
        *self
            .0
            .cancelled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        self.0.changed.notify_all();
    }
    /// Whether this owner was permanently retired.
    pub fn is_cancelled(&self) -> bool {
        *self
            .0
            .cancelled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
    /// Wait outside the writer; returns false when the owner was cancelled.
    /// Bounded slices avoid platform timeout overflow for raw imported periods.
    pub fn wait(&self, duration: std::time::Duration) -> bool {
        let started = std::time::Instant::now();
        let mut cancelled = self
            .0
            .cancelled
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !*cancelled {
            let Some(left) = duration.checked_sub(started.elapsed()) else {
                return true;
            };
            if left.is_zero() {
                return true;
            }
            (cancelled, _) = self
                .0
                .changed
                .wait_timeout(cancelled, left.min(std::time::Duration::from_secs(1)))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        false
    }
}

/// Delete queued media after its logical removal commits, waiting after every
/// file/thumbnail pair (including the last), as the reference maintenance does.
pub fn purge_deleted_media(store: &Store, batch: usize) -> Result<PurgeReport> {
    purge_deleted_media_with_control(store, batch, &PurgeControl::default())
}
/// A daemon-owned pass that shutdown can wake without admitting another pair.
pub fn purge_deleted_media_with_control(
    store: &Store,
    batch: usize,
    control: &PurgeControl,
) -> Result<PurgeReport> {
    purge_with_wait(store, batch, control, &mut |duration| {
        control.wait(duration)
    })
}
fn purge_with_wait(
    store: &Store,
    batch: usize,
    control: &PurgeControl,
    wait: &mut dyn FnMut(std::time::Duration) -> bool,
) -> Result<PurgeReport> {
    if control.is_cancelled()
        || store
            .read(settings::get::<MediaOwnership>)?
            .shared_with
            .is_some()
    {
        return Ok(PurgeReport::default());
    }
    // Qt captures the period once at entry; edits affect the next pass.
    let period = store.read(crate::physical_delete::load)?.wait_ms;
    let period = std::time::Duration::from_millis(u64::try_from(period).unwrap_or(0));
    let recycle = store
        .read(settings::get::<settings::FolderSettings>)?
        .delete_to_recycle_bin;
    let ids = store.read(|conn| {
        Ok(conn
            .prepare(
                "SELECT hash_id FROM deferred_physical_deletes ORDER BY queued_ms,hash_id LIMIT ?",
            )?
            .query_map([i64::try_from(batch).unwrap_or(i64::MAX)], |row| {
                row.get::<_, HashId>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut report = PurgeReport::default();
    for id in ids {
        if control.is_cancelled() {
            break;
        }
        let snap = store.snapshot();
        let local_storage = DomainRoles::new(&snap.services)?.local_file_storage;
        let claims = store.media_claims();
        let cancellation = control.clone();
        // Each pair commits before its wait. Re-adds/queue removals and import
        // claims are rechecked on the writer, with no gap before filesystem IO.
        let step = store.write(move |ctx| {
            let conn = ctx.conn();
            if cancellation.is_cancelled()
                || settings::get::<MediaOwnership>(conn)?.shared_with.is_some()
            {
                return Ok(None);
            }
            let queued: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM deferred_physical_deletes WHERE hash_id=?)",
                [id],
                |r| r.get(0),
            )?;
            if !queued {
                return Ok(None);
            }
            let (hash, mime, stored): (Vec<u8>, Option<u8>, bool) = conn.query_row(
                "SELECT h.sha256,f.mime,EXISTS(SELECT 1 FROM file_domain_current d WHERE d.service_id=?1 AND d.hash_id=h.hash_id)
                 FROM hashes h LEFT JOIN files f USING(hash_id) WHERE h.hash_id=?2",
                params![local_storage, id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let hash = Sha256::from_slice(&hash)
                .map_err(|e| crate::StoreError::Corrupt(e.to_string()))?;
            let mut step = PurgeReport::default();
            if stored {
                step.kept = 1;
            } else {
                let Some(_claim) = claims.try_claim(hash) else {
                    return Ok(None);
                };
                let mime = mime
                    .and_then(Mime::from_code)
                    .filter(|mime| *mime != Mime::ApplicationUnknown)
                    .ok_or_else(|| crate::StoreError::Corrupt(
                        "physical delete was queued without valid file metadata".into(),
                    ))?;
                if let Some(path) = snap.storage.file_path(&hash, mime)
                    && remove_if_present(&path, recycle)?
                {
                    step.files_deleted = 1;
                }
                if let Some(path) = snap.storage.thumbnail_path(&hash)
                    && remove_if_present(&path, false)?
                {
                    step.thumbnails_deleted = 1;
                }
            }
            conn.execute("DELETE FROM deferred_physical_deletes WHERE hash_id=?", [id])?;
            Ok(Some(step))
        })?;
        if let Some(step) = step {
            report.files_deleted += step.files_deleted;
            report.thumbnails_deleted += step.thumbnails_deleted;
            report.kept += step.kept;
            // Missing physical paths and the final attempted pair still wait.
            // Retained local membership only cleans a stale queue, without IO.
            if step.kept == 0 && !wait(period) {
                break;
            }
        }
    }
    Ok(report)
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

    /// The waits the reference's deferred loop made in one recorded scenario
    /// (oracle/fixtures/physical_delete_delay.json), in milliseconds.
    fn recorded_waits(scenario: &str) -> Vec<u128> {
        let recorded = hydrus_testkit::fixture_json("physical_delete_delay.json");
        let pass = recorded["passes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["scenario"] == scenario)
            .unwrap();
        pass["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e[0] == "wait")
            .map(|e| std::time::Duration::from_secs_f64(e[1].as_f64().unwrap()).as_millis())
            .collect()
    }
    /// The delay the recorded scenario left saved, in milliseconds.
    fn recorded_saved_after(scenario: &str) -> u128 {
        let recorded = hydrus_testkit::fixture_json("physical_delete_delay.json");
        let pass = recorded["passes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["scenario"] == scenario)
            .unwrap();
        u128::from(pass["saved_after"].as_u64().unwrap())
    }

    fn three_owned_files(store: &Store) -> Vec<(HashId, std::path::PathBuf)> {
        let snap = store.snapshot();
        let local = DomainRoles::new(&snap.services).unwrap().local_file_storage;
        store.read(|conn| {
            let rows=conn.prepare("SELECT h.hash_id,h.sha256,f.mime FROM file_domain_current d JOIN hashes h USING(hash_id) JOIN files f USING(hash_id) WHERE service_id=? ORDER BY hash_id LIMIT 3")?
                .query_map([local],|r|Ok((r.get::<_,HashId>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,u8>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows.into_iter().map(|(id,hash,mime)| (id,snap.storage.file_path(&Sha256::from_slice(&hash).unwrap(),Mime::from_code(mime).unwrap()).unwrap())).collect())
        }).unwrap()
    }

    #[test]
    fn retained_local_queue_cleanup_does_not_wait_or_remove_physical_paths() {
        let (_source, _directory, store, id, path) = owned_store();
        let hash = store
            .read(|conn| crate::master::hash(conn, id))
            .unwrap()
            .unwrap();
        let thumbnail = store.snapshot().storage.thumbnail_path(&hash).unwrap();
        let had_thumbnail = thumbnail.exists();
        // Re-add normally removes this queue. An old inconsistent queue must
        // still preserve restored media without becoming a physical attempt.
        store
            .write(move |ctx| {
                ctx.conn().execute(
                    "INSERT INTO deferred_physical_deletes(hash_id,queued_ms) VALUES(?,0)",
                    [id],
                )?;
                Ok(())
            })
            .unwrap();
        let report = purge_with_wait(&store, 100, &PurgeControl::default(), &mut |_| {
            panic!("retained current media is not an attempted physical pair")
        })
        .unwrap();
        assert_eq!(
            report,
            PurgeReport {
                kept: 1,
                ..PurgeReport::default()
            }
        );
        assert!(path.is_file());
        assert_eq!(thumbnail.exists(), had_thumbnail);
        assert_eq!(
            store
                .read(|conn| Ok(conn.query_row(
                    "SELECT count(*) FROM deferred_physical_deletes WHERE hash_id=?",
                    [id],
                    |row| row.get::<_, i64>(0),
                )?))
                .unwrap(),
            0
        );
    }

    #[test]
    fn captured_wait_follows_committed_pairs_including_last_and_allows_live_readd() {
        let (_source, _directory, store, _, _) = owned_store();
        let files = three_owned_files(&store);
        assert_eq!(files.len(), 3);
        for (id, _) in &files {
            purge_file(&store, *id);
        }
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &crate::physical_delete::Preferences { wait_ms: 600 },
                )
            })
            .unwrap();
        let control = PurgeControl::default();
        let mut waits = Vec::new();
        let report = purge_with_wait(&store, 100, &control, &mut |duration| {
            waits.push(duration.as_millis());
            assert!(!files[0].1.exists());
            assert!(
                !store
                    .read(|conn| Ok(conn.query_row(
                        "SELECT EXISTS(SELECT 1 FROM deferred_physical_deletes WHERE hash_id=?)",
                        [files[0].0],
                        |r| r.get::<_, bool>(0)
                    )?))
                    .unwrap(),
                "queue clear commits before wait"
            );
            if waits.len() == 1 {
                let restored = files[1].0;
                store
                    .write_content(move |writer| {
                        writer.add_files(writer.roles().local[0], &[(restored, Some(10))])
                    })
                    .unwrap();
                store
                    .write(|ctx| {
                        settings::set(
                            ctx.conn(),
                            &crate::physical_delete::Preferences { wait_ms: 900 },
                        )
                    })
                    .unwrap();
            }
            true
        })
        .unwrap();
        assert_eq!(report.files_deleted, 2);
        assert_eq!(
            waits,
            recorded_waits("change_during_wait"),
            "per-pass policy plus final-pair wait"
        );
        assert!(files[1].1.exists());
        assert!(!files[2].1.exists());
        purge_file(&store, files[1].0);
        waits.clear();
        assert_eq!(
            purge_with_wait(&store, 100, &control, &mut |duration| {
                waits.push(duration.as_millis());
                true
            })
            .unwrap()
            .files_deleted,
            1
        );
        assert_eq!(
            waits,
            // (the delay the recorded first pass left saved is what the second pass
            // is recorded as waiting: the recording has no wait of its own for it)
            [recorded_saved_after("change_during_wait")],
            "successor pass captures edited policy and still waits after its last pair"
        );
    }

    #[test]
    fn pair_failure_keeps_its_queue_and_previous_pair_commit_and_does_not_wait() {
        let (_source, _directory, store, _, _) = owned_store();
        let files = three_owned_files(&store);
        for (id, _) in &files {
            purge_file(&store, *id);
        }
        let broken = files[1].0;
        store
            .write(move |ctx| {
                ctx.conn().execute(
                    "UPDATE files SET mime=? WHERE hash_id=?",
                    params![Mime::ApplicationUnknown as u8, broken],
                )?;
                Ok(())
            })
            .unwrap();
        let mut waits = 0;
        assert!(
            purge_with_wait(&store, 100, &PurgeControl::default(), &mut |_| {
                waits += 1;
                true
            })
            .is_err()
        );
        assert_eq!(waits, 1, "failure itself has no successful-pair wait");
        assert!(!files[0].1.exists());
        assert!(files[1].1.exists());
        assert!(files[2].1.exists());
        assert_eq!(
            store
                .read(|conn| Ok(conn.query_row(
                    "SELECT count(*) FROM deferred_physical_deletes",
                    [],
                    |r| r.get::<_, i64>(0)
                )?))
                .unwrap(),
            2
        );
    }

    #[test]
    fn missing_file_still_waits_after_its_thumbnail_and_queue_commit() {
        let (_source, _directory, store, id, path) = owned_store();
        let hash = store
            .read(|conn| crate::master::hash(conn, id))
            .unwrap()
            .unwrap();
        let thumbnail = store.snapshot().storage.thumbnail_path(&hash).unwrap();
        let had_thumbnail = thumbnail.exists();
        std::fs::remove_file(path).unwrap();
        purge_file(&store, id);
        let mut waits = Vec::new();
        let report = purge_with_wait(&store, 100, &PurgeControl::default(), &mut |period| {
            assert!(!thumbnail.exists());
            waits.push(period.as_millis());
            true
        })
        .unwrap();
        assert_eq!(report.files_deleted, 0);
        assert_eq!(report.thumbnails_deleted, usize::from(had_thumbnail));
        assert_eq!(
            waits,
            recorded_waits("missing")[..1],
            "missing physical original does not erase the final-pair delay"
        );
    }

    #[test]
    fn cancelled_owned_wait_releases_writer_and_never_admits_a_successor_pair() {
        let (_source, _directory, store, _, _) = owned_store();
        let files = three_owned_files(&store);
        for (id, _) in &files {
            purge_file(&store, *id);
        }
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &crate::physical_delete::Preferences { wait_ms: 60_000 },
                )
            })
            .unwrap();
        let control = PurgeControl::default();
        let worker_control = control.clone();
        let worker_store = store.clone();
        let (waiting, entered) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            purge_with_wait(&worker_store, 100, &worker_control, &mut |duration| {
                waiting.send(duration).unwrap();
                worker_control.wait(duration)
            })
        });
        assert_eq!(
            entered
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap(),
            std::time::Duration::from_secs(60)
        );
        // This actual writer command completes while physical maintenance waits.
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &crate::physical_delete::Preferences { wait_ms: 20 },
                )
            })
            .unwrap();
        control.cancel();
        let report = worker.join().unwrap().unwrap();
        assert_eq!(report.files_deleted, 1);
        assert!(!files[0].1.exists());
        assert!(files[1].1.exists() && files[2].1.exists());
        assert_eq!(
            purge_deleted_media_with_control(&store, 100, &control).unwrap(),
            PurgeReport::default(),
            "cancelled old owner cannot resurrect on another pass"
        );
        assert!(!control.wait(std::time::Duration::from_secs(60)));
    }

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
