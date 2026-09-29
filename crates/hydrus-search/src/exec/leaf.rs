//! Leaves of a search plan: sets of files defined by one index each.
//!
//! Every leaf can be evaluated two ways: *scanning* its own index (all files
//! with this tag, all files with this rating) or *probing* a candidate set
//! file by file (which of these files have this tag). [`Leaf::eval`] picks
//! whichever is expected to read fewer rows.
//!
//! Some leaves also match files that have no rows at all ("fewer than 3
//! notes" includes files with none). Those [need a scope](Leaf::needs_scope):
//! they can only say which of a given set of files match.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use roaring::RoaringBitmap;
use rusqlite::ToSql;
use rusqlite::types::Value;

use hydrus_core::{
    CanvasType, ContentStatus, LabelId, NamespaceId, ServiceId, TagId, UrlDomainId, UrlId,
};
use hydrus_store::schema::MappingTables;

use super::Result;
use super::context::{DomainTable, Env};
use super::numbers::Counts;
use super::sql::{self, int_array};
use super::time::TimeRange;
use crate::predicate::Relationship;

/// Where a leaf is evaluated.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Scope<'a> {
    /// Everywhere: return every matching file (possibly outside the domain).
    All,
    /// Only these files matter; the result must be a subset.
    Within(&'a RoaringBitmap),
}

/// Stored tags looked up in one mapping table.
#[derive(Debug, Clone)]
pub(crate) struct TagLookup {
    pub service: ServiceId,
    pub status: ContentStatus,
    /// Sorted.
    pub tags: Arc<[TagId]>,
}

impl TagLookup {
    fn table(&self) -> String {
        MappingTables::new(self.service)
            .for_status(self.status)
            .to_owned()
    }
}

/// Stored tags that display in one of some namespaces, in one mapping table.
#[derive(Debug, Clone)]
pub(crate) struct NamespaceLookup {
    pub service: ServiceId,
    pub status: ContentStatus,
    pub namespaces: Arc<[NamespaceId]>,
    /// Tags in those namespaces that display outside them.
    pub excluded: Arc<HashSet<TagId>>,
    /// Tags outside those namespaces that display inside them (sorted).
    pub added: Arc<[TagId]>,
}

/// A value bound into a file condition.
#[derive(Debug, Clone)]
pub(crate) enum Param {
    Int(i64),
    Real(f64),
    Ints(Vec<i64>),
}

impl Param {
    fn to_value(&self) -> Box<dyn ToSql> {
        match self {
            Param::Int(i) => Box::new(*i),
            Param::Real(f) => Box::new(*f),
            Param::Ints(v) => Box::new(std::rc::Rc::new(
                v.iter().map(|i| Value::Integer(*i)).collect::<Vec<_>>(),
            )),
        }
    }
}

/// A condition on a row of the `files` table.
#[derive(Debug, Clone)]
pub(crate) struct FileCond {
    pub sql: String,
    pub params: Vec<Param>,
}

/// Which timestamp a time leaf tests.
#[derive(Debug, Clone)]
pub(crate) enum TimeSource {
    /// When files were added to (or deleted from) the domain's tables.
    Imported(Vec<DomainTable>),
    /// The earliest of a file's modified times.
    Modified,
    Archived,
    /// The media viewer's last-viewed time.
    LastViewed,
}

/// What a count leaf counts per file.
#[derive(Debug, Clone)]
pub(crate) enum CountSource {
    Notes,
    Urls,
    /// Views (or view time in milliseconds) summed over some viewers.
    Views {
        canvases: Vec<CanvasType>,
        viewtime: bool,
    },
    /// An inc/dec rating's value.
    IncDec(ServiceId),
    /// Duplicate-system relationships of a kind, within the domain.
    Relationships(Relationship),
    /// Distinct displayed tags (optionally only in some namespaces).
    DisplayTags(Option<Arc<[NamespaceId]>>),
}

/// Files with a URL matching a rule.
#[derive(Debug, Clone)]
pub(crate) enum UrlLeaf {
    Exact(UrlId),
    Domains(Vec<UrlDomainId>),
    Regex(Arc<fancy_regex::Regex>),
}

/// One indexed condition.
#[derive(Debug, Clone)]
pub(crate) enum Leaf {
    /// Exactly these files.
    Files(RoaringBitmap),
    /// Files with any of the given stored tags.
    StoredTags(Vec<TagLookup>),
    /// Files with a stored tag that displays in one of some namespaces.
    Namespaces(Vec<NamespaceLookup>),
    /// Files with any stored tag in these mapping tables.
    AnyTag(Vec<String>),
    Inbox,
    /// Files with a `files` row meeting every condition.
    FileInfo(Vec<FileCond>),
    /// Files with a content flag bit set (see `hydrus_store::media::FileFlags`).
    Flag(u32),
    /// Files with a status in a file domain.
    FileStatus {
        service: ServiceId,
        status: ContentStatus,
    },
    Time {
        source: TimeSource,
        range: TimeRange,
    },
    /// Files whose count is in `accept` (no rows counting as zero).
    Count {
        source: CountSource,
        accept: Counts,
    },
    NoteName(LabelId),
    Url(UrlLeaf),
    /// Files rated on a like/dislike or numerical service within a range.
    RatingRange {
        service: ServiceId,
        above: Option<(f64, bool)>,
        below: Option<(f64, bool)>,
    },
    /// Files with any rating on a like/dislike or numerical service.
    Rated(ServiceId),
    /// Duplicate group members that are not their group's best file.
    NonKings,
}

impl Leaf {
    /// Whether the leaf can match files without rows in its index, and so
    /// can only be evaluated within a set of candidates.
    pub fn needs_scope(&self) -> bool {
        match self {
            Leaf::Count { source, accept } => {
                accept.contains_zero() || matches!(source, CountSource::DisplayTags(_))
            }
            _ => false,
        }
    }

    /// Roughly how many rows scanning this leaf's index reads, if known.
    pub fn estimate(&self, env: &Env<'_>) -> Result<Option<u64>> {
        Ok(match self {
            Leaf::Files(files) => Some(files.len()),
            Leaf::StoredTags(lookups) => Some(tag_count_estimate(env, lookups)?),
            Leaf::Flag(bit) => {
                let n: i64 = env
                    .conn
                    .prepare_cached(&format!("SELECT count(*) FROM files WHERE flags & {bit}"))?
                    .query_row([], |r| r.get(0))?;
                Some(n.max(0) as u64)
            }
            Leaf::FileStatus { service, status } => {
                let n: i64 = env
                    .conn
                    .prepare_cached(&format!(
                        "SELECT count(*) FROM {} WHERE service_id = ?",
                        status_table(*status)
                    ))?
                    .query_row([service], |r| r.get(0))?;
                Some(n.max(0) as u64)
            }
            Leaf::NoteName(label) => {
                let n: i64 = env
                    .conn
                    .prepare_cached("SELECT count(*) FROM file_notes WHERE label_id = ?")?
                    .query_row([label], |r| r.get(0))?;
                Some(n.max(0) as u64)
            }
            Leaf::Url(UrlLeaf::Exact(url)) => {
                let n: i64 = env
                    .conn
                    .prepare_cached("SELECT count(*) FROM file_urls WHERE url_id = ?")?
                    .query_row([url], |r| r.get(0))?;
                Some(n.max(0) as u64)
            }
            Leaf::Inbox => sql::table_rows(env.conn, "file_inbox"),
            Leaf::FileInfo(_) => sql::table_rows(env.conn, "files"),
            Leaf::Rated(_) | Leaf::RatingRange { .. } => sql::table_rows(env.conn, "ratings"),
            Leaf::NonKings => sql::table_rows(env.conn, "dup_group_members"),
            Leaf::Count {
                source: CountSource::Notes,
                ..
            } => sql::table_rows(env.conn, "file_notes"),
            Leaf::Count {
                source: CountSource::Urls,
                ..
            } => sql::table_rows(env.conn, "file_urls"),
            _ => None,
        })
    }

    /// The matching files in `scope`.
    pub fn eval(&self, env: &Env<'_>, scope: Scope<'_>) -> Result<RoaringBitmap> {
        let within = match scope {
            Scope::Within(w) => {
                if w.is_empty() {
                    return Ok(RoaringBitmap::new());
                }
                Some(w)
            }
            Scope::All => None,
        };
        let probe = match within {
            Some(w) => env.prefer_probe(w.len(), self.estimate(env)?),
            None => false,
        };
        let result = match self {
            Leaf::Files(files) => files.clone(),
            Leaf::StoredTags(lookups) => stored_tags(env, lookups, within.filter(|_| probe))?,
            Leaf::Namespaces(lookups) => namespace_tags(env, lookups, within)?,
            Leaf::AnyTag(tables) => any_tag(env, tables, within)?,
            Leaf::Inbox => simple(
                env,
                "SELECT hash_id FROM file_inbox WHERE 1",
                probe_set(within, probe),
                &[],
            )?,
            Leaf::FileInfo(conds) => file_info(env, conds, probe_set(within, probe))?,
            Leaf::Flag(bit) => simple(
                env,
                &format!("SELECT hash_id FROM files WHERE flags & {bit}"),
                probe_set(within, probe),
                &[],
            )?,
            Leaf::FileStatus { service, status } => simple(
                env,
                &format!("SELECT hash_id FROM {} WHERE service_id = ?", status_table(*status)),
                probe_set(within, probe),
                &[service],
            )?,
            Leaf::Time { source, range } => time(env, source, *range, within)?,
            Leaf::Count { source, accept } => count(env, source, accept, within, probe)?,
            Leaf::NoteName(label) => simple(
                env,
                "SELECT hash_id FROM file_notes WHERE label_id = ?",
                probe_set(within, probe),
                &[label],
            )?,
            Leaf::Url(rule) => urls(env, rule, within)?,
            Leaf::RatingRange {
                service,
                above,
                below,
            } => {
                let mut sql = "SELECT hash_id FROM ratings WHERE service_id = ?".to_owned();
                let mut params: Vec<&dyn ToSql> = vec![service];
                if let Some((value, inclusive)) = above {
                    sql += if *inclusive { " AND rating >= ?" } else { " AND rating > ?" };
                    params.push(value);
                }
                if let Some((value, inclusive)) = below {
                    sql += if *inclusive { " AND rating <= ?" } else { " AND rating < ?" };
                    params.push(value);
                }
                simple(env, &sql, probe_set(within, probe), &params)?
            }
            Leaf::Rated(service) => simple(
                env,
                "SELECT hash_id FROM ratings WHERE service_id = ?",
                probe_set(within, probe),
                &[service],
            )?,
            Leaf::NonKings => simple(
                env,
                "SELECT m.hash_id FROM dup_group_members m CROSS JOIN dup_groups g ON g.group_id = m.group_id
                 WHERE m.hash_id != g.king_hash_id",
                probe_set(within, probe),
                &[],
            )?,
        };
        Ok(match within {
            Some(w) => result & w,
            None => result,
        })
    }
}

fn probe_set(within: Option<&RoaringBitmap>, probe: bool) -> Option<&RoaringBitmap> {
    within.filter(|_| probe)
}

fn status_table(status: ContentStatus) -> &'static str {
    match status {
        ContentStatus::Current => "file_domain_current",
        ContentStatus::Deleted => "file_domain_deleted",
        ContentStatus::Pending => "file_domain_pending",
        ContentStatus::Petitioned => "file_domain_petitioned",
    }
}

/// Run a `SELECT hash_id ... WHERE ...` query (it must have a `WHERE`),
/// either whole or restricted to `probe` by adding
/// `AND <first column> IN rarray(...)`. Placeholders in `select` must be
/// plain `?`s.
fn simple(
    env: &Env<'_>,
    select: &str,
    probe: Option<&RoaringBitmap>,
    params: &[&dyn ToSql],
) -> Result<RoaringBitmap> {
    match probe {
        None => {
            let mut stmt = env.conn.prepare_cached(select)?;
            sql::collect(&mut stmt, params)
        }
        Some(w) => {
            // the id column is the first selected expression
            let column = select
                .trim_start()
                .strip_prefix("SELECT ")
                .and_then(|s| s.split([',', ' ']).next())
                .unwrap_or("hash_id");
            let sql = format!("{select} AND {column} IN rarray(?{})", params.len() + 1);
            let mut stmt = env.conn.prepare_cached(&sql)?;
            let mut out = RoaringBitmap::new();
            sql::for_each_chunk(w, |chunk| {
                let mut all: Vec<&dyn ToSql> = params.to_vec();
                all.push(&chunk);
                out |= sql::collect(&mut stmt, all.as_slice())?;
                Ok(())
            })?;
            Ok(out)
        }
    }
}

/// Sum of the stored tags' counts over every file ("all known files"), an
/// upper bound on how many files the lookups can return.
fn tag_count_estimate(env: &Env<'_>, lookups: &[TagLookup]) -> Result<u64> {
    let Some(all_known_files) = env.service_of_type(hydrus_core::ServiceType::CombinedFile) else {
        return Ok(u64::MAX);
    };
    let mut total = 0u64;
    for lookup in lookups {
        let column = match lookup.status {
            ContentStatus::Current => "current",
            ContentStatus::Pending => "pending",
            // no counts are kept for these; they are rare
            _ => {
                let rows = sql::table_rows(env.conn, &lookup.table());
                total = total.saturating_add(rows.unwrap_or(u64::MAX / 4));
                continue;
            }
        };
        let table = MappingTables::new(lookup.service).counts;
        let n: Option<i64> = env
            .conn
            .prepare_cached(&format!(
                "SELECT sum({column}) FROM {table} WHERE domain_id = ?1 AND tag_id IN rarray(?2)"
            ))?
            .query_row(
                rusqlite::params![
                    all_known_files.id,
                    int_array(lookup.tags.iter().map(|t| t.get()))
                ],
                |r| r.get(0),
            )?;
        total = total.saturating_add(n.unwrap_or(0).max(0) as u64);
    }
    Ok(total)
}

fn stored_tags(
    env: &Env<'_>,
    lookups: &[TagLookup],
    probe: Option<&RoaringBitmap>,
) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    for lookup in lookups {
        if lookup.tags.is_empty() {
            continue;
        }
        let table = lookup.table();
        let tags = int_array(lookup.tags.iter().map(|t| t.get()));
        match probe {
            None => {
                let mut stmt = env.conn.prepare_cached(&format!(
                    "SELECT hash_id FROM {table} WHERE tag_id IN rarray(?)"
                ))?;
                out |= sql::collect(&mut stmt, [tags])?;
            }
            Some(w) if lookup.tags.len() <= 8 => {
                // point lookups on the (tag_id, hash_id) key
                out |= sql::probe(
                    env.conn,
                    &format!(
                        "SELECT hash_id FROM {table} WHERE hash_id IN rarray(?1) AND tag_id IN rarray(?2)"
                    ),
                    &(w - &out),
                    &[&tags],
                )?;
            }
            Some(w) => {
                // read each candidate's tags through the (hash_id, tag_id) index
                let mut stmt = env.conn.prepare_cached(&format!(
                    "SELECT hash_id, tag_id FROM {table} WHERE hash_id IN rarray(?)"
                ))?;
                let remaining = w - &out;
                sql::for_each_chunk(&remaining, |chunk| {
                    let mut rows = stmt.query([chunk])?;
                    while let Some(row) = rows.next()? {
                        let tag: TagId = row.get(1)?;
                        if lookup.tags.binary_search(&tag).is_ok() {
                            out.insert(sql::hash_id(row, 0)?);
                        }
                    }
                    Ok(())
                })?;
            }
        }
    }
    Ok(out)
}

fn namespace_tags(
    env: &Env<'_>,
    lookups: &[NamespaceLookup],
    within: Option<&RoaringBitmap>,
) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    for lookup in lookups {
        let table = MappingTables::new(lookup.service)
            .for_status(lookup.status)
            .to_owned();
        if let Some(w) = within {
            let remaining = w - &out;
            out |= namespace_tags_probe(env, lookup, &table, &remaining)?;
        } else {
            out |= namespace_tags_scan(env, lookup, &table)?;
        }
    }
    Ok(out)
}

/// Read each candidate's tags and check their namespaces.
fn namespace_tags_probe(
    env: &Env<'_>,
    lookup: &NamespaceLookup,
    table: &str,
    within: &RoaringBitmap,
) -> Result<RoaringBitmap> {
    let allowed: HashSet<NamespaceId> = lookup.namespaces.iter().copied().collect();
    let mut stmt = env.conn.prepare_cached(&format!(
        "SELECT m.hash_id, m.tag_id, t.namespace_id FROM {table} m CROSS JOIN tags t ON t.tag_id = m.tag_id
         WHERE m.hash_id IN rarray(?)"
    ))?;
    let mut out = RoaringBitmap::new();
    sql::for_each_chunk(within, |chunk| {
        let mut rows = stmt.query([chunk])?;
        while let Some(row) = rows.next()? {
            let tag: TagId = row.get(1)?;
            let ns: NamespaceId = row.get(2)?;
            let hit = (allowed.contains(&ns) && !lookup.excluded.contains(&tag))
                || lookup.added.binary_search(&tag).is_ok();
            if hit {
                out.insert(sql::hash_id(row, 0)?);
            }
        }
        Ok(())
    })?;
    Ok(out)
}

/// Walk the namespaces' tags through the mapping table's tag index.
fn namespace_tags_scan(
    env: &Env<'_>,
    lookup: &NamespaceLookup,
    table: &str,
) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    if !lookup.namespaces.is_empty() {
        let mut stmt = env.conn.prepare_cached(&format!(
            "SELECT m.hash_id, m.tag_id FROM tags t CROSS JOIN {table} m ON m.tag_id = t.tag_id
             WHERE t.namespace_id IN rarray(?)"
        ))?;
        let mut rows = stmt.query([int_array(lookup.namespaces.iter().map(|n| n.get()))])?;
        while let Some(row) = rows.next()? {
            let tag: TagId = row.get(1)?;
            if !lookup.excluded.contains(&tag) {
                out.insert(sql::hash_id(row, 0)?);
            }
        }
    }
    if !lookup.added.is_empty() {
        let mut stmt = env.conn.prepare_cached(&format!(
            "SELECT hash_id FROM {table} WHERE tag_id IN rarray(?)"
        ))?;
        out |= sql::collect(&mut stmt, [int_array(lookup.added.iter().map(|t| t.get()))])?;
    }
    Ok(out)
}

fn any_tag(
    env: &Env<'_>,
    tables: &[String],
    within: Option<&RoaringBitmap>,
) -> Result<RoaringBitmap> {
    let mut out = RoaringBitmap::new();
    for table in tables {
        if let Some(w) = within {
            out |= sql::probe(
                env.conn,
                &format!(
                    "SELECT value FROM rarray(?1) WHERE EXISTS (SELECT 1 FROM {table} WHERE hash_id = value)"
                ),
                &(w - &out),
                &[],
            )?;
        } else {
            let mut stmt = env
                .conn
                .prepare_cached(&format!("SELECT DISTINCT hash_id FROM {table}"))?;
            out |= sql::collect(&mut stmt, [])?;
        }
    }
    Ok(out)
}

fn file_info(
    env: &Env<'_>,
    conds: &[FileCond],
    probe: Option<&RoaringBitmap>,
) -> Result<RoaringBitmap> {
    let mut sql = "SELECT hash_id FROM files WHERE 1".to_owned();
    let mut values: Vec<Box<dyn ToSql>> = Vec::new();
    for cond in conds {
        sql += " AND ";
        sql += &cond.sql;
        values.extend(cond.params.iter().map(Param::to_value));
    }
    let params: Vec<&dyn ToSql> = values.iter().map(AsRef::as_ref).collect();
    simple(env, &sql, probe, &params)
}

fn time(
    env: &Env<'_>,
    source: &TimeSource,
    range: TimeRange,
    within: Option<&RoaringBitmap>,
) -> Result<RoaringBitmap> {
    let from = range.from.unwrap_or(i64::MIN);
    let to = range.to.unwrap_or(i64::MAX);
    let probe = within.filter(|w| env.prefer_probe(w.len(), None));
    match source {
        TimeSource::Imported(tables) => {
            let mut out = RoaringBitmap::new();
            for t in tables {
                let column = if t.deleted { "deleted_ms" } else { "added_ms" };
                out |= simple(
                    env,
                    &format!(
                        "SELECT hash_id FROM {} WHERE service_id = ? AND {column} BETWEEN ? AND ?",
                        t.table()
                    ),
                    probe,
                    &[&t.service, &from, &to],
                )?;
            }
            Ok(out)
        }
        TimeSource::Archived => simple(
            env,
            "SELECT hash_id FROM file_archived WHERE archived_ms BETWEEN ? AND ?",
            probe,
            &[&from, &to],
        ),
        TimeSource::LastViewed => simple(
            env,
            "SELECT hash_id FROM file_viewing_stats WHERE canvas_type = ? AND last_viewed_ms BETWEEN ? AND ?",
            probe,
            &[&CanvasType::MediaViewer.code(), &from, &to],
        ),
        TimeSource::Modified => modified(env, range, probe),
    }
}

/// Files whose earliest modified time (their own, or any web domain's) is
/// in `range`.
fn modified(
    env: &Env<'_>,
    range: TimeRange,
    probe: Option<&RoaringBitmap>,
) -> Result<RoaringBitmap> {
    if let Some(w) = probe {
        let mut earliest: HashMap<u32, i64> = HashMap::new();
        for sql in [
            "SELECT hash_id, file_modified_ms FROM files WHERE hash_id IN rarray(?) AND file_modified_ms IS NOT NULL",
            "SELECT hash_id, modified_ms FROM file_domain_modified WHERE hash_id IN rarray(?)",
        ] {
            let mut stmt = env.conn.prepare_cached(sql)?;
            sql::for_each_chunk(w, |chunk| {
                let mut rows = stmt.query([chunk])?;
                while let Some(row) = rows.next()? {
                    let t: i64 = row.get(1)?;
                    let e = earliest.entry(sql::hash_id(row, 0)?).or_insert(t);
                    *e = (*e).min(t);
                }
                Ok(())
            })?;
        }
        return Ok(earliest
            .into_iter()
            .filter(|&(_, t)| range.contains(t))
            .map(|(h, _)| h)
            .collect());
    }
    // earliest <= to  <=>  some time <= to;  earliest >= from  <=>  no time < from
    let with_time_in = |from: i64, to: i64| -> Result<RoaringBitmap> {
        let mut out = simple(
            env,
            "SELECT hash_id FROM files WHERE file_modified_ms BETWEEN ? AND ?",
            None,
            &[&from, &to],
        )?;
        out |= simple(
            env,
            "SELECT hash_id FROM file_domain_modified WHERE modified_ms BETWEEN ? AND ?",
            None,
            &[&from, &to],
        )?;
        Ok(out)
    };
    let mut out = with_time_in(i64::MIN, range.to.unwrap_or(i64::MAX))?;
    if let Some(from) = range.from {
        out -= with_time_in(i64::MIN, from.saturating_sub(1))?;
    }
    Ok(out)
}

fn count(
    env: &Env<'_>,
    source: &CountSource,
    accept: &Counts,
    within: Option<&RoaringBitmap>,
    probe: bool,
) -> Result<RoaringBitmap> {
    let restrict = match source {
        CountSource::DisplayTags(_) => within,
        _ if probe || accept.contains_zero() => within,
        _ => None,
    };
    let counts = counts(env, source, restrict)?;
    if accept.contains_zero() {
        let w = within.expect("a leaf that needs a scope is evaluated within one");
        let mut out = w.clone();
        for (hash, n) in counts {
            if !accept.contains(n) {
                out.remove(hash);
            }
        }
        Ok(out)
    } else {
        Ok(counts
            .into_iter()
            .filter(|&(_, n)| accept.contains(n))
            .map(|(h, _)| h)
            .collect())
    }
}

/// Per-file counts for files with at least one row (restricted to `within`
/// when given).
fn counts(
    env: &Env<'_>,
    source: &CountSource,
    within: Option<&RoaringBitmap>,
) -> Result<Vec<(u32, u64)>> {
    let grouped = |select: &str, group: &str, params: &[&dyn ToSql]| -> Result<Vec<(u32, u64)>> {
        let mut out = Vec::new();
        let mut read = |stmt: &mut rusqlite::Statement<'_>, params: &[&dyn ToSql]| -> Result<()> {
            let mut rows = stmt.query(params)?;
            while let Some(row) = rows.next()? {
                let n: i64 = row.get(1)?;
                if n > 0 {
                    out.push((sql::hash_id(row, 0)?, n as u64));
                }
            }
            Ok(())
        };
        match within {
            None => {
                let mut stmt = env.conn.prepare_cached(&format!("{select} {group}"))?;
                read(&mut stmt, params)?;
            }
            Some(w) => {
                let mut stmt = env.conn.prepare_cached(&format!(
                    "{select} AND hash_id IN rarray(?{}) {group}",
                    params.len() + 1
                ))?;
                sql::for_each_chunk(w, |chunk| {
                    let mut all = params.to_vec();
                    all.push(&chunk);
                    read(&mut stmt, &all)
                })?;
            }
        }
        Ok(out)
    };
    match source {
        CountSource::Notes => grouped(
            "SELECT hash_id, count(*) FROM file_notes WHERE 1",
            "GROUP BY hash_id",
            &[],
        ),
        CountSource::Urls => grouped(
            "SELECT hash_id, count(*) FROM file_urls WHERE 1",
            "GROUP BY hash_id",
            &[],
        ),
        CountSource::Views { canvases, viewtime } => {
            let column = if *viewtime { "viewtime_ms" } else { "views" };
            let canvases = int_array(canvases.iter().map(|c| u32::from(c.code())));
            grouped(
                &format!(
                    "SELECT hash_id, sum({column}) FROM file_viewing_stats WHERE canvas_type IN rarray(?1)"
                ),
                "GROUP BY hash_id",
                &[&canvases],
            )
        }
        CountSource::IncDec(service) => grouped(
            "SELECT hash_id, rating FROM ratings_incdec WHERE service_id = ?1",
            "",
            &[service],
        ),
        CountSource::Relationships(relationship) => {
            let all = super::dupes::relationship_counts(env, *relationship)?;
            Ok(match within {
                Some(w) => all.into_iter().filter(|(h, _)| w.contains(*h)).collect(),
                None => all.into_iter().collect(),
            })
        }
        CountSource::DisplayTags(namespaces) => {
            let w = within.expect("tag counts are always evaluated within candidates");
            display_tag_counts(env, namespaces.as_deref(), w)
        }
    }
}

/// How many distinct tags each file displays (in `namespaces`, if given),
/// across every searched service and status.
pub(crate) fn display_tag_counts(
    env: &Env<'_>,
    namespaces: Option<&[NamespaceId]>,
    within: &RoaringBitmap,
) -> Result<Vec<(u32, u64)>> {
    let allowed: Option<HashSet<NamespaceId>> = namespaces.map(|n| n.iter().copied().collect());
    let graph_namespaces = if allowed.is_some() {
        Some(env.graph_namespaces()?)
    } else {
        None
    };
    let mut out = Vec::new();
    let mut per_file: HashMap<u32, HashSet<TagId>> = HashMap::new();
    sql::for_each_chunk(within, |chunk| {
        per_file.clear();
        for (service, table) in env.tags.tables() {
            let mut stmt = env.conn.prepare_cached(&format!(
                "SELECT m.hash_id, m.tag_id, t.namespace_id FROM {table} m CROSS JOIN tags t ON t.tag_id = m.tag_id
                 WHERE m.hash_id IN rarray(?)"
            ))?;
            let mut rows = stmt.query([chunk.clone()])?;
            while let Some(row) = rows.next()? {
                let hash = sql::hash_id(row, 0)?;
                let stored: TagId = row.get(1)?;
                let stored_ns: NamespaceId = row.get(2)?;
                let displayed = per_file.entry(hash).or_default();
                for tag in service.graph.display_tags(stored) {
                    let keep = match (&allowed, graph_namespaces) {
                        (None, _) => true,
                        (Some(allowed), Some(graph_ns)) => {
                            let ns = if tag == stored {
                                stored_ns
                            } else {
                                graph_ns.get(&tag).copied().unwrap_or(stored_ns)
                            };
                            allowed.contains(&ns)
                        }
                        (Some(_), None) => unreachable!("namespaces are loaded when filtering"),
                    };
                    if keep {
                        displayed.insert(tag);
                    }
                }
            }
        }
        out.extend(
            per_file
                .iter()
                .filter(|(_, tags)| !tags.is_empty())
                .map(|(&h, tags)| (h, tags.len() as u64)),
        );
        Ok(())
    })?;
    Ok(out)
}

fn urls(env: &Env<'_>, rule: &UrlLeaf, within: Option<&RoaringBitmap>) -> Result<RoaringBitmap> {
    match rule {
        UrlLeaf::Exact(url) => {
            let probe = within.filter(|w| env.prefer_probe(w.len(), Some(64)));
            simple(
                env,
                "SELECT hash_id FROM file_urls WHERE url_id = ?",
                probe,
                &[url],
            )
        }
        UrlLeaf::Domains(domains) => {
            if domains.is_empty() {
                return Ok(RoaringBitmap::new());
            }
            let domains = int_array(domains.iter().map(|d| d.get()));
            let mut stmt = env.conn.prepare_cached(
                "SELECT f.hash_id FROM urls u CROSS JOIN file_urls f ON f.url_id = u.url_id WHERE u.domain_id IN rarray(?)",
            )?;
            sql::collect(&mut stmt, [domains])
        }
        UrlLeaf::Regex(regex) => {
            let matches = |url: &str| regex.is_match(url).unwrap_or(false);
            if let Some(w) = within.filter(|w| env.prefer_probe(w.len(), None)) {
                let mut out = RoaringBitmap::new();
                let mut stmt = env.conn.prepare_cached(
                    "SELECT f.hash_id, u.url FROM file_urls f CROSS JOIN urls u ON u.url_id = f.url_id
                     WHERE f.hash_id IN rarray(?)",
                )?;
                sql::for_each_chunk(w, |chunk| {
                    let mut rows = stmt.query([chunk])?;
                    while let Some(row) = rows.next()? {
                        let url: String = row.get(1)?;
                        if matches(&url) {
                            out.insert(sql::hash_id(row, 0)?);
                        }
                    }
                    Ok(())
                })?;
                return Ok(out);
            }
            let mut ids = Vec::new();
            let mut stmt = env.conn.prepare_cached("SELECT url_id, url FROM urls")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let url: String = row.get(1)?;
                if matches(&url) {
                    ids.push(row.get::<_, u32>(0)?);
                }
            }
            let mut stmt = env
                .conn
                .prepare_cached("SELECT hash_id FROM file_urls WHERE url_id IN rarray(?)")?;
            sql::collect(&mut stmt, [int_array(ids)])
        }
    }
}
