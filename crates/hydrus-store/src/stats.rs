//! Library statistics, as the reference's "mr bones" reports them
//! (`ClientDB._GetBonedStatsFromTable`).

use std::collections::HashMap;
use std::rc::Rc;

use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};

use hydrus_core::{CanvasType, HashId, ServiceId};

use crate::error::Result;
use crate::master::id_array;

/// Files to count.
#[derive(Debug, Clone)]
pub enum FileSet {
    /// Every file currently in, or deleted from (`true`), these domains.
    Domains(Vec<(ServiceId, bool)>),
    /// Exactly these files.
    Files(Vec<HashId>),
}

/// The statistics of a set of files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BonedStats {
    pub num_inbox: i64,
    pub num_archive: i64,
    pub size_inbox: i64,
    pub size_archive: i64,
    /// Count and size of the deleted files, when they were asked for.
    pub deleted: Option<(i64, i64)>,
    /// The earliest import time (current, or original of deleted files),
    /// when import times were asked for and any is known.
    pub earliest_import_ms: Option<i64>,
    /// Views and view time in the media viewer and the preview, of current
    /// and deleted files together.
    pub media_views: i64,
    pub media_viewtime_ms: i64,
    pub preview_views: i64,
    pub preview_viewtime_ms: i64,
    /// Files in duplicate groups with more than one of the files.
    pub duplicate_files: i64,
    /// Alternates groups with more than one duplicate group among the
    /// files, and the files in those duplicate groups.
    pub alternate_groups: i64,
    pub alternate_files: i64,
}

/// A file set as a SQL subquery of `hash_id`s, and its parameter.
struct Source {
    sql: String,
    ids: Option<Rc<Vec<Value>>>,
}

impl Source {
    fn new(set: &FileSet) -> Self {
        match set {
            FileSet::Domains(domains) => {
                let parts: Vec<String> = domains
                    .iter()
                    .map(|(service, deleted)| {
                        let table = if *deleted {
                            "file_domain_deleted"
                        } else {
                            "file_domain_current"
                        };
                        format!(
                            "SELECT hash_id FROM {table} WHERE service_id = {}",
                            service.get()
                        )
                    })
                    .collect();
                Self {
                    sql: parts.join(" UNION "),
                    ids: None,
                }
            }
            FileSet::Files(ids) => Self {
                sql: "SELECT value AS hash_id FROM rarray(?1)".into(),
                ids: Some(id_array(ids)),
            },
        }
    }

    fn rows<T>(
        &self,
        conn: &Connection,
        sql: &str,
        mut f: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>> {
        let sql = sql.replace("{files}", &format!("({})", self.sql));
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(self.ids.iter()), |r| f(r))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    fn row<T>(
        &self,
        conn: &Connection,
        sql: &str,
        f: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<T> {
        let mut rows = self.rows(conn, sql, f)?;
        Ok(rows.remove(0))
    }
}

/// The statistics of `current` files, with the deleted files' count, size
/// and views if `deleted` is given. Import times are read from the domains
/// named: `current_times` for current files (their times added) and
/// `deleted_times` for deleted ones (their original times added).
pub fn boned_stats(
    conn: &Connection,
    current: &FileSet,
    current_times: Option<ServiceId>,
    deleted: Option<&FileSet>,
    deleted_times: Option<ServiceId>,
) -> Result<BonedStats> {
    let files = Source::new(current);
    let mut stats = BonedStats::default();

    // counts only files whose information is known, as the reference does
    let (num_total, size_total): (i64, Option<i64>) = files.row(
        conn,
        "SELECT count(*), sum(f.size) FROM {files} AS s CROSS JOIN files AS f ON f.hash_id = s.hash_id",
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (num_inbox, size_inbox): (i64, Option<i64>) = files.row(
        conn,
        "SELECT count(*), sum(f.size) FROM {files} AS s CROSS JOIN file_inbox AS i ON i.hash_id = s.hash_id
         CROSS JOIN files AS f ON f.hash_id = s.hash_id",
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (size_total, size_inbox) = (size_total.unwrap_or(0), size_inbox.unwrap_or(0));
    stats.num_inbox = num_inbox;
    stats.num_archive = num_total - num_inbox;
    stats.size_inbox = size_inbox;
    stats.size_archive = size_total - size_inbox;

    let deleted = deleted.map(Source::new);
    if let Some(deleted) = &deleted {
        let num: i64 = deleted.row(conn, "SELECT count(*) FROM {files}", |r| r.get(0))?;
        let size: Option<i64> = deleted.row(
            conn,
            "SELECT sum(f.size) FROM {files} AS s CROSS JOIN files AS f ON f.hash_id = s.hash_id",
            |r| r.get(0),
        )?;
        stats.deleted = Some((num, size.unwrap_or(0)));
    }

    let mut earliest = 0;
    if let Some(service) = current_times {
        let first: Option<i64> = files.row(
            conn,
            &format!(
                "SELECT min(d.added_ms) FROM {{files}} AS s CROSS JOIN file_domain_current AS d
                 ON d.hash_id = s.hash_id AND d.service_id = {}",
                service.get()
            ),
            |r| r.get(0),
        )?;
        earliest = first.unwrap_or(0);
    }
    if let (Some(deleted), Some(service)) = (&deleted, deleted_times) {
        let first: Option<i64> = deleted.row(
            conn,
            &format!(
                "SELECT min(d.original_added_ms) FROM {{files}} AS s CROSS JOIN file_domain_deleted AS d
                 ON d.hash_id = s.hash_id AND d.service_id = {}",
                service.get()
            ),
            |r| r.get(0),
        )?;
        if let Some(first) = first {
            earliest = if earliest == 0 {
                first
            } else {
                earliest.min(first)
            };
        }
    }
    stats.earliest_import_ms = (earliest > 0).then_some(earliest);

    for source in std::iter::once(&files).chain(deleted.as_ref()) {
        let rows: Vec<(i64, i64, i64)> = source.rows(
            conn,
            "SELECT v.canvas_type, sum(v.views), sum(v.viewtime_ms) FROM {files} AS s
             CROSS JOIN file_viewing_stats AS v ON v.hash_id = s.hash_id GROUP BY v.canvas_type",
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        for (canvas, views, viewtime_ms) in rows {
            if canvas == CanvasType::MediaViewer as i64 {
                stats.media_views += views;
                stats.media_viewtime_ms += viewtime_ms;
            } else if canvas == CanvasType::Preview as i64 {
                stats.preview_views += views;
                stats.preview_viewtime_ms += viewtime_ms;
            }
        }
    }

    // duplicate groups, with how many of the files each has
    let groups: HashMap<i64, i64> = files
        .rows(
            conn,
            "SELECT m.group_id, count(*) FROM {files} AS s
             CROSS JOIN dup_group_members AS m ON m.hash_id = s.hash_id GROUP BY m.group_id",
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
        .into_iter()
        .collect();
    stats.duplicate_files = groups.values().filter(|&&n| n > 1).sum();

    let group_ids: Vec<Value> = groups.keys().map(|&g| Value::Integer(g)).collect();
    let mut stmt = conn.prepare(
        "SELECT alt_group_id, group_id FROM alt_group_members WHERE group_id IN rarray(?1)",
    )?;
    let mut alternates: HashMap<i64, Vec<i64>> = HashMap::new();
    let rows = stmt.query_map([Rc::new(group_ids)], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (alt_group, group) = row?;
        alternates.entry(alt_group).or_default().push(group);
    }
    for members in alternates.values().filter(|m| m.len() > 1) {
        stats.alternate_groups += 1;
        stats.alternate_files += members.iter().map(|g| groups[g]).sum::<i64>();
    }
    Ok(stats)
}
