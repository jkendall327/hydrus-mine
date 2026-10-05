//! Emptying the trash on schedule (reference `DAEMONMaintainTrash`).
//!
//! While the trash holds more than its maximum size, its oldest files are
//! deleted for good, eight at a time; then every file trashed longer ago
//! than the maximum age goes. The reference skips the job while the user is
//! busy unless `maintain_trash_in_normal_time` (default on) is set. Automatic
//! callers capture that admission using their current GUI idle or normal-time
//! classification; explicit maintenance calls bypass the automatic gate.

use rusqlite::params;

use hydrus_core::HashId;

use crate::content::ContentWriter;
use crate::error::Result;
use crate::store::Store;

/// The old options' `trash_max_age` (hours) and `trash_max_size` (MiB);
/// `None` is no limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrashSettings {
    pub max_age_hours: Option<u64>,
    pub max_size_mb: Option<u64>,
}

impl Default for TrashSettings {
    fn default() -> Self {
        Self {
            max_age_hours: Some(72),
            max_size_mb: Some(2048),
        }
    }
}

impl crate::settings::Setting for TrashSettings {
    const KEY: &'static str = "trash";
}

/// How many files a pass deleted for good, and why.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TrashReport {
    pub over_size: usize,
    pub over_age: usize,
}

impl TrashReport {
    pub fn total(&self) -> usize {
        self.over_size + self.over_age
    }
}

/// The reference deletes in groups of eight, checking the size between them.
const CHUNK: usize = 8;

/// Clear the trash down to its limits, `batch` files per write.
pub fn maintain_trash(store: &Store, batch: usize) -> Result<TrashReport> {
    maintain_trash_with_control(store, batch, &crate::maintenance::PurgeControl::default())
}

/// An owned automatic pass can stop before admitting another group/write.
/// The caller captures its normal-time/idle admission once before this pass.
pub fn maintain_trash_with_control(
    store: &Store,
    batch: usize,
    control: &crate::maintenance::PurgeControl,
) -> Result<TrashReport> {
    if control.is_cancelled() {
        return Ok(TrashReport::default());
    }
    let settings: TrashSettings = store.read(crate::settings::get)?;
    let mut report = TrashReport::default();
    if settings.max_age_hours.is_none() && settings.max_size_mb.is_none() {
        return Ok(report);
    }
    while !control.is_cancelled() {
        let cancellation = control.clone();
        let step = store
            .write_content(move |w| clear_some_owned(w, settings, batch, Some(&cancellation)))?;
        report.over_size += step.over_size;
        report.over_age += step.over_age;
        if step.total() == 0 {
            break;
        }
    }
    Ok(report)
}

/// Delete up to about `batch` trashed files that are over the limits: the
/// oldest while the trash is too big, then any that are too old.
pub fn clear_some(
    w: &mut ContentWriter<'_>,
    settings: TrashSettings,
    batch: usize,
) -> Result<TrashReport> {
    clear_some_owned(w, settings, batch, None)
}

fn clear_some_owned(
    w: &mut ContentWriter<'_>,
    settings: TrashSettings,
    batch: usize,
    control: Option<&crate::maintenance::PurgeControl>,
) -> Result<TrashReport> {
    let cancelled = || control.is_some_and(crate::maintenance::PurgeControl::is_cancelled);
    if cancelled() {
        return Ok(TrashReport::default());
    }
    let trash = w.roles().trash;
    let storage = w.roles().local_file_storage;
    // (files the delete lock holds stay, though they count towards the size)
    let deletable =
        crate::delete_lock::sql_condition(w.conn(), "d.hash_id")?.unwrap_or_else(|| "1".into());
    let mut report = TrashReport::default();
    if let Some(mb) = settings.max_size_mb {
        let max = i64::try_from(mb.saturating_mul(1_048_576)).unwrap_or(i64::MAX);
        let rows: Vec<(HashId, i64, bool)> = {
            let mut stmt = w.conn().prepare(&format!(
                "SELECT d.hash_id, COALESCE(f.size, 0), {deletable} FROM file_domain_current d
                 LEFT JOIN files f USING (hash_id)
                 WHERE d.service_id = ?1 ORDER BY d.added_ms, d.hash_id"
            ))?;
            let rows = stmt.query_map([trash], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut total: i64 = rows.iter().map(|&(_, size, _)| size).sum();
        let rows: Vec<(HashId, i64)> = rows
            .into_iter()
            .filter(|&(_, _, deletable)| deletable)
            .map(|(id, size, _)| (id, size))
            .collect();
        for chunk in rows.chunks(CHUNK) {
            if cancelled() || total <= max || report.over_size >= batch {
                break;
            }
            let ids: Vec<HashId> = chunk.iter().map(|&(id, _)| id).collect();
            w.delete_files(storage, &ids, None)?;
            total -= chunk.iter().map(|&(_, size)| size).sum::<i64>();
            report.over_size += ids.len();
        }
        if total > max {
            // more to do next write, before the age limit
            return Ok(report);
        }
    }
    if cancelled() {
        return Ok(report);
    }
    if let Some(hours) = settings.max_age_hours {
        // (the reference cuts off at a whole second)
        let age = i64::try_from(hours.saturating_mul(3600)).unwrap_or(i64::MAX);
        let cutoff_ms = (w.now_ms() / 1000).saturating_sub(age).saturating_mul(1000);
        let limit = batch.saturating_sub(report.over_size);
        let ids: Vec<HashId> = {
            let mut stmt = w.conn().prepare(&format!(
                "SELECT hash_id FROM file_domain_current d
                 WHERE service_id = ?1 AND added_ms < ?2 AND {deletable} LIMIT ?3"
            ))?;
            let rows = stmt.query_map(
                params![trash, cutoff_ms, i64::try_from(limit).unwrap_or(i64::MAX)],
                |r| r.get(0),
            )?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        for chunk in ids.chunks(CHUNK) {
            if cancelled() {
                break;
            }
            w.delete_files(storage, chunk, None)?;
            report.over_age += chunk.len();
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::tests::import_basic;

    const HOUR_MS: i64 = 3_600_000;

    /// A store whose trash holds ten files, oldest first, each 200,000 bytes,
    /// trashed 9.5 to 0.5 hours ago (half an hour off the limits, so a slow
    /// run doesn't cross one).
    fn trashed_store() -> (
        tempfile::TempDir,
        tempfile::TempDir,
        std::sync::Arc<Store>,
        Vec<HashId>,
    ) {
        let (source, dest_dir, _db) = import_basic();
        let store = Store::open(dest_dir.path()).unwrap();
        let files = store
            .write_content(|w| {
                let roles = w.roles().clone();
                let ids_in = |w: &ContentWriter<'_>, domain| -> Result<Vec<HashId>> {
                    let mut stmt = w.conn().prepare(
                        "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 ORDER BY hash_id",
                    )?;
                    let rows = stmt.query_map([domain], |r| r.get(0))?;
                    Ok(rows.collect::<rusqlite::Result<_>>()?)
                };
                let already = ids_in(w, roles.trash)?;
                w.delete_files(roles.trash, &already, None)?;
                let files: Vec<HashId> = ids_in(w, roles.combined_local_media)?
                    .into_iter()
                    .take(10)
                    .collect();
                w.delete_files(roles.combined_local_media, &files, None)?;
                let now = w.now_ms();
                for (i, &id) in files.iter().enumerate() {
                    let age = 10 - i64::try_from(i).unwrap();
                    w.conn().execute(
                        "UPDATE file_domain_current SET added_ms = ?1 WHERE service_id = ?2 AND hash_id = ?3",
                        params![now - age * HOUR_MS + HOUR_MS / 2, roles.trash, id],
                    )?;
                    w.conn()
                        .execute("UPDATE files SET size = 200000 WHERE hash_id = ?1", [id])?;
                }
                Ok(files)
            })
            .unwrap();
        (source, dest_dir, store, files)
    }

    fn in_trash(store: &Store) -> Vec<HashId> {
        let trash = crate::content::DomainRoles::new(&store.snapshot().services)
            .unwrap()
            .trash;
        store
            .read(|c| {
                let mut stmt = c.prepare(
                    "SELECT hash_id FROM file_domain_current WHERE service_id = ?1 ORDER BY hash_id",
                )?;
                let rows = stmt.query_map([trash], |r| r.get(0))?;
                Ok(rows.collect::<rusqlite::Result<_>>()?)
            })
            .unwrap()
    }

    fn set(store: &Store, settings: TrashSettings) {
        store
            .write(move |ctx| crate::settings::set(ctx.conn(), &settings))
            .unwrap();
    }

    #[test]
    fn files_trashed_too_long_ago_are_deleted_for_good() {
        let (_source, _dir, store, files) = trashed_store();
        set(
            &store,
            TrashSettings {
                max_age_hours: Some(3),
                max_size_mb: None,
            },
        );
        let report = maintain_trash(&store, 3).unwrap();
        assert_eq!(
            report,
            TrashReport {
                over_size: 0,
                over_age: 7
            }
        );
        // the three trashed in the last three hours stay
        assert_eq!(in_trash(&store), files[7..]);
        let gone = crate::master::id_array(&files[..7]);
        let queued: i64 = store
            .read(move |c| {
                Ok(c.query_row(
                    "SELECT count(*) FROM deferred_physical_deletes WHERE hash_id IN rarray(?1)",
                    [gone],
                    |r| r.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(queued, 7, "their media is queued for deletion");
    }

    #[test]
    fn an_oversized_trash_loses_its_oldest_files_eight_at_a_time() {
        let (_source, _dir, store, files) = trashed_store();
        set(
            &store,
            TrashSettings {
                max_age_hours: None,
                max_size_mb: Some(1),
            },
        );
        // 2,000,000 bytes against 1 MiB: deleting the oldest eight leaves 400,000
        let report = maintain_trash(&store, 256).unwrap();
        assert_eq!(
            report,
            TrashReport {
                over_size: 8,
                over_age: 0
            }
        );
        assert_eq!(in_trash(&store), files[8..]);
    }

    #[test]
    fn the_size_limit_goes_first_then_the_age_limit() {
        let (_source, _dir, store, files) = trashed_store();
        set(
            &store,
            TrashSettings {
                max_age_hours: Some(1),
                max_size_mb: Some(1),
            },
        );
        let report = maintain_trash(&store, 256).unwrap();
        assert_eq!(
            report,
            TrashReport {
                over_size: 8,
                over_age: 1
            }
        );
        assert_eq!(in_trash(&store), files[9..]);
    }

    #[test]
    fn the_delete_lock_keeps_archived_files_in_the_trash() {
        let (_source, _dir, store, files) = trashed_store();
        set(
            &store,
            TrashSettings {
                max_age_hours: Some(3),
                max_size_mb: Some(1),
            },
        );
        let archived = [files[0], files[1], files[5]];
        let all = files.clone();
        store
            .write_content(move |w| {
                w.inbox(&all)?;
                w.archive(&archived)?;
                let lock = crate::delete_lock::DeleteLock {
                    archived: true,
                    ..Default::default()
                };
                crate::settings::set(w.conn(), &lock)
            })
            .unwrap();
        // the archived files count towards the size but can't go: the seven
        // others go (2,000,000 bytes less 1,400,000 is under 1 MiB), and of
        // the files over three hours old only the archived ones are left
        let report = maintain_trash(&store, 256).unwrap();
        assert_eq!(
            report,
            TrashReport {
                over_size: 7,
                over_age: 0
            }
        );
        assert_eq!(in_trash(&store), archived);
        assert_eq!(maintain_trash(&store, 256).unwrap().total(), 0);
    }

    #[test]
    fn cancelled_writer_admission_leaves_trash_and_physical_queue_untouched() {
        let (_source, _dir, store, files) = trashed_store();
        let cancellation = crate::maintenance::PurgeControl::default();
        cancellation.cancel();
        let report = store
            .write_content(move |w| {
                let queued: i64 = w.conn().query_row(
                    "SELECT count(*) FROM deferred_physical_deletes",
                    [],
                    |r| r.get(0),
                )?;
                let report = clear_some_owned(
                    w,
                    TrashSettings {
                        max_age_hours: Some(0),
                        max_size_mb: Some(0),
                    },
                    256,
                    Some(&cancellation),
                )?;
                let after: i64 = w.conn().query_row(
                    "SELECT count(*) FROM deferred_physical_deletes",
                    [],
                    |r| r.get(0),
                )?;
                assert_eq!(after, queued);
                Ok(report)
            })
            .unwrap();
        assert_eq!(report.total(), 0);
        assert_eq!(in_trash(&store), files);
        // The same actual writer remains usable by a separately admitted pass.
        set(
            &store,
            TrashSettings {
                max_age_hours: Some(0),
                max_size_mb: None,
            },
        );
        assert_eq!(maintain_trash(&store, 256).unwrap().total(), files.len());
        assert!(in_trash(&store).is_empty());
    }

    #[test]
    fn no_limits_leave_the_trash_alone() {
        let (_source, _dir, store, files) = trashed_store();
        set(
            &store,
            TrashSettings {
                max_age_hours: None,
                max_size_mb: None,
            },
        );
        assert_eq!(maintain_trash(&store, 256).unwrap().total(), 0);
        assert_eq!(in_trash(&store), files);
    }
}
