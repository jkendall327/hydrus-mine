//! Missing archived times: the reference's global legacy/import maintenance.
use crate::{
    Result, StoreError,
    content::{ContentWriter, DomainRoles, FileTime},
};
use hydrus_core::HashId;
use rusqlite::{Connection, params};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

/// v474's approximate release, used by the reference for strict comparisons.
pub const TRACKING_STARTED_MS: i64 = 1_644_991_200_000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Population {
    Legacy,
    Import,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub hash: HashId,
    pub imported_ms: i64,
    pub deleted_ms: Option<i64>,
    pub population: Population,
    pub suggested_ms: Option<i64>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub candidates: Vec<Candidate>,
}
impl Plan {
    pub fn count(&self, population: Population) -> usize {
        self.candidates
            .iter()
            .filter(|c| c.population == population)
            .count()
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(StoreError::Invalid("Cancelled!".into()))
    } else {
        Ok(())
    }
}
/// Current media and trash use physical-storage import times; other formerly
/// local files use combined-local deletion memory, regardless of inbox flags.
pub fn scan(conn: &Connection, roles: &DomainRoles, cancel: &AtomicBool) -> Result<Plan> {
    check_cancel(cancel)?;
    let mut stmt = conn.prepare(
        "SELECT c.hash_id, s.added_ms, NULL
         FROM file_domain_current c JOIN file_domain_current s USING(hash_id)
         WHERE c.service_id IN (?1,?2) AND s.service_id=?3 AND s.added_ms IS NOT NULL
           AND NOT EXISTS(SELECT 1 FROM file_inbox i WHERE i.hash_id=c.hash_id)
           AND NOT EXISTS(SELECT 1 FROM file_archived a WHERE a.hash_id=c.hash_id AND a.archived_ms IS NOT NULL)
         UNION ALL
         SELECT d.hash_id, d.original_added_ms, d.deleted_ms FROM file_domain_deleted d
         WHERE d.service_id=?1 AND d.original_added_ms IS NOT NULL
           AND NOT EXISTS(SELECT 1 FROM file_domain_current t WHERE t.hash_id=d.hash_id AND t.service_id=?2)
           AND NOT EXISTS(SELECT 1 FROM file_archived a WHERE a.hash_id=d.hash_id AND a.archived_ms IS NOT NULL)
         ORDER BY 1")?;
    let rows = stmt.query_map(
        params![
            roles.combined_local_media,
            roles.trash,
            roles.local_file_storage
        ],
        |r| {
            Ok((
                r.get::<_, HashId>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        },
    )?;
    let mut candidates = Vec::new();
    for row in rows {
        check_cancel(cancel)?;
        let (hash, imported_ms, deleted_ms) = row?;
        let (population, suggested_ms) = match imported_ms.cmp(&TRACKING_STARTED_MS) {
            std::cmp::Ordering::Less => {
                let end = deleted_ms.unwrap_or(TRACKING_STARTED_MS);
                (
                    Population::Legacy,
                    (imported_ms <= end).then(|| {
                        (imported_ms as f64 + (end as f64 - imported_ms as f64) / 5.0).trunc()
                            as i64
                    }),
                )
            }
            std::cmp::Ordering::Greater => (Population::Import, Some(imported_ms)),
            std::cmp::Ordering::Equal => continue,
        };
        candidates.push(Candidate {
            hash,
            imported_ms,
            deleted_ms,
            population,
            suggested_ms,
        });
    }
    check_cancel(cancel)?;
    Ok(Plan { candidates })
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Repaired {
    pub legacy: usize,
    pub import: usize,
}
/// Revalidate the captured scan inside the content transaction. Existing archive
/// times, newly inboxed files and changed import/deletion memories are preserved.
/// Cancellation returns an error, so the enclosing writer rolls back atomically.
pub fn apply(
    writer: &mut ContentWriter<'_>,
    captured: &Plan,
    populations: &[Population],
    cancel: &AtomicBool,
) -> Result<Repaired> {
    let current = scan(writer.conn(), writer.roles(), cancel)?
        .candidates
        .into_iter()
        .map(|c| (c.hash, c))
        .collect::<BTreeMap<_, _>>();
    let mut repaired = Repaired::default();
    for candidate in &captured.candidates {
        check_cancel(cancel)?;
        if !populations.contains(&candidate.population)
            || current.get(&candidate.hash) != Some(candidate)
        {
            continue;
        }
        let Some(ms) = candidate.suggested_ms else {
            continue;
        };
        writer.set_file_time(&[candidate.hash], &FileTime::Archived, ms)?;
        match candidate.population {
            Population::Legacy => repaired.legacy += 1,
            Population::Import => repaired.import += 1,
        }
    }
    check_cancel(cancel)?;
    Ok(repaired)
}
