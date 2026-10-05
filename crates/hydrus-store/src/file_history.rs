//! The reference's sampled cumulative file-history series for one file domain.
use crate::{Result, StoreError, content::DomainRoles};
use hydrus_core::{HashId, ServiceId};
use rusqlite::{Connection, params};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct History {
    pub current: Vec<(i64, i64)>,
    pub deleted: Vec<(i64, i64)>,
    pub inbox: Vec<(i64, i64)>,
    pub archive: Vec<(i64, i64)>,
}
impl History {
    pub fn series(&self) -> [&[(i64, i64)]; 4] {
        [&self.current, &self.inbox, &self.archive, &self.deleted]
    }
}
/// Independent current/deleted query results. None means the entire domain.
#[derive(Debug, Clone)]
pub struct Scope {
    pub service: ServiceId,
    pub current: Option<BTreeSet<HashId>>,
    pub deleted: Option<BTreeSet<HashId>>,
}
impl Scope {
    pub fn entire(service: ServiceId) -> Self {
        Self {
            service,
            current: None,
            deleted: None,
        }
    }
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Acquire) {
        Err(StoreError::Invalid("Cancelled!".into()))
    } else {
        Ok(())
    }
}
fn gap(first: i64, last: i64, steps: i64) -> i64 {
    i64::try_from((i128::from(last) - i128::from(first)).div_euclid(i128::from(steps)))
        .unwrap_or(i64::MAX)
        .max(1)
}
/// Preserve the reference's sample boundaries, including omitted terminal events
/// and its reverse-series repeated event timestamps rather than bucket times.
pub fn load(
    conn: &Connection,
    roles: &DomainRoles,
    scope: &Scope,
    steps: i64,
    cancel: &AtomicBool,
) -> Result<History> {
    if steps <= 0 {
        return Err(StoreError::Invalid(
            "history sampling steps must be positive".into(),
        ));
    }
    cancelled(cancel)?;
    let mut current = Vec::new();
    let mut stmt=conn.prepare("SELECT c.hash_id,c.added_ms,EXISTS(SELECT 1 FROM file_inbox i WHERE i.hash_id=c.hash_id),EXISTS(SELECT 1 FROM file_domain_current m WHERE m.hash_id=c.hash_id AND m.service_id=?2) FROM file_domain_current c WHERE c.service_id=?1")?;
    for row in stmt.query_map(params![scope.service, roles.combined_local_media], |r| {
        Ok((
            r.get::<_, HashId>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, bool>(2)?,
            r.get::<_, bool>(3)?,
        ))
    })? {
        cancelled(cancel)?;
        let row = row?;
        if scope
            .current
            .as_ref()
            .is_none_or(|ids| ids.contains(&row.0))
        {
            current.push(row);
        }
    }
    let mut deleted = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT hash_id,original_added_ms,deleted_ms FROM file_domain_deleted WHERE service_id=?",
    )?;
    for row in stmt.query_map([scope.service], |r| {
        Ok((
            r.get::<_, HashId>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, Option<i64>>(2)?,
        ))
    })? {
        cancelled(cancel)?;
        let row = row?;
        if scope
            .deleted
            .as_ref()
            .is_none_or(|ids| ids.contains(&row.0))
        {
            deleted.push(row);
        }
    }
    let archive = conn
        .prepare("SELECT hash_id,archived_ms FROM file_archived WHERE archived_ms IS NOT NULL")?
        .query_map([], |r| Ok((r.get::<_, HashId>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let mut imports = current
        .iter()
        .filter_map(|r| r.1)
        .chain(deleted.iter().filter_map(|r| r.1))
        .map(|t| t.div_euclid(1000))
        .collect::<Vec<_>>();
    imports.sort_unstable();
    let mut deletes = deleted
        .iter()
        .filter_map(|r| r.2)
        .map(|t| t.div_euclid(1000))
        .collect::<Vec<_>>();
    deletes.sort_unstable();
    let mut events = imports
        .iter()
        .map(|t| (*t, 1))
        .chain(deletes.iter().map(|t| (*t, -1)))
        .collect::<Vec<_>>();
    events.sort_unstable();
    let mut history = History::default();
    if let (Some(first), Some(last)) = (events.first(), events.last()) {
        history.current.push((first.0, 0));
        let step = gap(first.0, last.0, steps);
        let mut time = first.0;
        let mut count = 0;
        for (timestamp, delta) in events {
            while timestamp > time.saturating_add(step) {
                cancelled(cancel)?;
                history.current.push((time, count));
                time += step;
            }
            count += delta;
        }
    }
    if let (Some(first), Some(last)) = (deletes.first(), deletes.last()) {
        let step = gap(*first, *last, steps);
        let mut time = *first;
        let mut count = deleted.iter().filter(|r| r.2.is_none()).count() as i64;
        for timestamp in &deletes {
            while *timestamp > time.saturating_add(step) {
                cancelled(cancel)?;
                history.deleted.push((time, count));
                time += step;
            }
            count += 1;
        }
    }
    let entire_storage = scope.service == roles.local_file_storage && scope.current.is_none();
    let mut inbox = if entire_storage {
        conn.query_row("SELECT count(*) FROM file_inbox", [], |r| {
            r.get::<_, i64>(0)
        })?
    } else {
        current.iter().filter(|r| r.2).count() as i64
    };
    let archiveable = if scope.service == roles.combined_local_media {
        current.iter().filter(|r| r.1.is_some()).count()
    } else {
        current.iter().filter(|r| r.3).count()
    } as i64;
    let mut archived = archiveable - inbox;
    let mut archives = Vec::new();
    if entire_storage {
        archives.extend(archive.values().map(|t| t.div_euclid(1000)));
    } else {
        archives.extend(
            current
                .iter()
                .filter_map(|r| archive.get(&r.0))
                .map(|t| t.div_euclid(1000)),
        );
        for (id, _, delete) in &deleted {
            if let Some(archived) = archive.get(id) {
                archives.push(
                    delete
                        .map_or(*archived, |t| t.min(*archived))
                        .div_euclid(1000),
                );
            } else if let Some(t) = delete {
                archives.push(t.div_euclid(1000));
            }
        }
    }
    archives.sort_unstable();
    if let Some(first) = archives.first().copied() {
        let mut events = archives
            .iter()
            .map(|t| (*t, 1, -1))
            .chain(imports.iter().filter(|t| **t >= first).map(|t| (*t, -1, 0)))
            .chain(deletes.iter().filter(|t| **t >= first).map(|t| (*t, 0, 1)))
            .collect::<Vec<_>>();
        events.sort_unstable_by(|a, b| b.cmp(a));
        if let (Some(first), Some(last)) = (events.first(), events.last()) {
            let step = gap(last.0, first.0, steps);
            let mut time = first.0;
            for (timestamp, inbox_delta, archive_delta) in events {
                while timestamp < time.saturating_sub(step) {
                    cancelled(cancel)?;
                    history.inbox.push((timestamp, inbox));
                    history.archive.push((timestamp, archived));
                    time -= step;
                }
                inbox += inbox_delta;
                archived += archive_delta;
            }
            history.inbox.reverse();
            history.archive.reverse();
        }
    }
    cancelled(cancel)?;
    Ok(history)
}
