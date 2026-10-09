//! The Database menu's repair and regeneration jobs, on the native schema.
//!
//! The reference keeps many derived caches across four database files; the
//! native store keeps fewer (ADR-6): the tag storage/display counts per tag
//! service, the subtag search indexes and the in-memory display graphs. Each
//! job here regenerates or checks the native data that serves the purpose
//! the reference's job names, and returns what the user is told.

use std::collections::BTreeSet;

use rusqlite::{Connection, params};

use hydrus_core::{HashId, Mime, ServiceId, ServiceType, Sha256};

use crate::error::Result;
use crate::schema::MappingTables;
use crate::services::ServiceRegistry;
use crate::store::Store;

/// One tag service, or all of them ("all services").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagScope {
    All,
    One(ServiceId),
}

impl TagScope {
    fn services(self, registry: &ServiceRegistry) -> Vec<ServiceId> {
        match self {
            Self::All => registry.tag_services().map(|s| s.id).collect(),
            Self::One(id) => vec![id],
        }
    }
}

/// Recompute the chosen services' storage and display tag counts.
pub fn rebuild_tag_counts(conn: &Connection, scope: TagScope) -> Result<()> {
    let registry = ServiceRegistry::load(conn)?;
    let graphs = crate::display::DisplayGraphs::load(conn, &registry)?;
    for service in scope.services(&registry) {
        crate::counts::rebuild_display_service(conn, &registry, service, &graphs)?;
    }
    Ok(())
}

/// Which subtag search rows to regenerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtagIndex {
    /// Drop and rebuild the word, searchable and integer indexes.
    All,
    /// Only add rows for subtags that have none.
    MissingOnly,
    /// Drop and rebuild the searchable ("unusual character") map.
    SearchableMaps,
}

/// Regenerate the fast tag search indexes; returns how many subtags were
/// (re)indexed.
pub fn rebuild_subtag_index(conn: &Connection, which: SubtagIndex) -> Result<usize> {
    let select = match which {
        SubtagIndex::All => {
            conn.execute_batch(
                "DELETE FROM cache_subtag_words; DELETE FROM cache_searchable_subtags;
                 DELETE FROM cache_integer_subtags;",
            )?;
            "SELECT subtag_id, subtag FROM subtags"
        }
        SubtagIndex::MissingOnly => {
            "SELECT subtag_id, subtag FROM subtags s WHERE NOT EXISTS
             (SELECT 1 FROM cache_subtag_words w WHERE w.subtag_id = s.subtag_id)"
        }
        SubtagIndex::SearchableMaps => {
            conn.execute_batch("DELETE FROM cache_searchable_subtags;")?;
            "SELECT subtag_id, subtag FROM subtags"
        }
    };
    let rows = conn
        .prepare(select)?
        .query_map([], |r| {
            Ok((
                r.get::<_, hydrus_core::SubtagId>(0)?,
                r.get::<_, String>(1)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut searchable = conn.prepare_cached(
        "INSERT OR IGNORE INTO cache_searchable_subtags (searchable, subtag_id) VALUES (?, ?)",
    )?;
    for (id, subtag) in &rows {
        if which == SubtagIndex::SearchableMaps {
            let form = crate::text::searchable_subtag(subtag);
            if form != *subtag {
                searchable.execute(params![form, id])?;
            }
        } else {
            crate::master::index_subtag(conn, *id, subtag)?;
        }
    }
    Ok(rows.len())
}

/// Rescind pending mappings that are already current and petitions of
/// mappings already deleted (`fix_logically_inconsistent_mappings`); returns
/// how many were found. Counts are recomputed for services that had any.
pub fn fix_inconsistent_mappings(conn: &Connection, scope: TagScope) -> Result<usize> {
    let registry = ServiceRegistry::load(conn)?;
    let mut total = 0;
    for service in scope.services(&registry) {
        let t = MappingTables::new(service);
        let fixed = conn.execute(
            &format!(
                "DELETE FROM {p} WHERE EXISTS (SELECT 1 FROM {c} WHERE {c}.tag_id = {p}.tag_id AND {c}.hash_id = {p}.hash_id)",
                p = t.pending,
                c = t.current
            ),
            [],
        )? + conn.execute(
            &format!(
                "DELETE FROM {p} WHERE EXISTS (SELECT 1 FROM {d} WHERE {d}.tag_id = {p}.tag_id AND {d}.hash_id = {p}.hash_id)",
                p = t.petitioned,
                d = t.deleted
            ),
            [],
        )?;
        if fixed > 0 {
            rebuild_tag_counts(conn, TagScope::One(service))?;
        }
        total += fixed;
    }
    Ok(total)
}

/// A count row: domain, tag, current, pending, and whether a display count.
type CountRow = (i64, i64, i64, i64, bool);

/// Check the chosen services' counts against their mappings and repair any
/// that differ; returns, per service name, how many count rows were wrong.
pub fn resync_tag_counts(conn: &Connection, scope: TagScope) -> Result<Vec<(String, usize)>> {
    let registry = ServiceRegistry::load(conn)?;
    let mut desynced = Vec::new();
    for service in scope.services(&registry) {
        let t = MappingTables::new(service);
        let snapshot = |conn: &Connection| -> Result<BTreeSet<CountRow>> {
            let mut rows = BTreeSet::new();
            for (table, display) in [(&t.counts, false), (&t.display_counts, true)] {
                let mut stmt = conn.prepare(&format!(
                    "SELECT domain_id, tag_id, current, pending FROM {table}"
                ))?;
                for row in stmt.query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, display))
                })? {
                    rows.insert(row?);
                }
            }
            Ok(rows)
        };
        let before = snapshot(conn)?;
        rebuild_tag_counts(conn, TagScope::One(service))?;
        let after = snapshot(conn)?;
        let wrong = before.symmetric_difference(&after).count();
        if wrong > 0 {
            desynced.push((registry.get(service)?.name.clone(), wrong));
        }
    }
    Ok(desynced)
}

/// Gather the query planner's statistics: `ANALYZE` in full, or SQLite's
/// own choice of the tables that are due (`PRAGMA optimize`).
pub fn analyze(conn: &Connection, full: bool) -> Result<()> {
    conn.execute_batch(if full { "ANALYZE;" } else { "PRAGMA optimize;" })?;
    Ok(())
}

/// When each table was last analysed and how many rows it had then (the
/// reference's `analyze_timestamps`): table name to (rows, milliseconds).
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AnalyzeTimestamps(pub std::collections::BTreeMap<String, (i64, i64)>);

impl crate::settings::Setting for AnalyzeTimestamps {
    const KEY: &'static str = "analyze_timestamps";
}

/// The reference's re-analysis schedule: tables of at most this many rows,
/// whether they are analysed at once while looking (small ones are cheap),
/// and how long after the last analysis.
const ANALYZE_BOUNDARIES: [(i64, bool, i64); 4] = [
    (100, true, 6 * 3600),
    (10_000, true, 3 * 86_400),
    (100_000, false, 3 * 30 * 86_400),
    (10_000_000, false, 12 * 30 * 86_400),
];

fn analysable_tables(conn: &Connection) -> Result<Vec<String>> {
    Ok(conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             AND sql NOT LIKE 'CREATE VIRTUAL%' ORDER BY name",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?)
}

fn has_at_least(conn: &Connection, table: &str, rows: i64) -> Result<bool> {
    let quoted = table.replace('"', "\"\"");
    Ok(conn
        .query_row(
            &format!("SELECT COUNT(*) FROM (SELECT 1 FROM \"{quoted}\" LIMIT ?1)"),
            [rows],
            |r| r.get::<_, i64>(0),
        )?
        >= rows)
}

/// The tables due an analysis at `now_ms` (`GetTableNamesDueAnalysis`):
/// those never analysed, and those whose time under the reference's
/// schedule has passed, except small ones the reference analyses at once
/// while looking (returned second). As in the reference, a table can be due
/// under more than one boundary, and is listed once for each.
pub fn tables_due_analysis_at(conn: &Connection, now_ms: i64) -> Result<(Vec<String>, Vec<String>)> {
    let seen: AnalyzeTimestamps = crate::settings::get(conn)?;
    let now = now_ms.div_euclid(1000);
    let mut due = Vec::new();
    let mut at_once = Vec::new();
    for name in analysable_tables(conn)? {
        let Some(&(rows, at_ms)) = seen.0.get(&name) else {
            due.push(name);
            continue;
        };
        let at = at_ms.div_euclid(1000);
        for (limit, immediate, period) in ANALYZE_BOUNDARIES {
            if rows > limit || now <= at + period {
                continue;
            }
            if immediate && !has_at_least(conn, &name, limit)? {
                at_once.push(name.clone());
            } else {
                due.push(name.clone());
            }
        }
    }
    Ok((due, at_once))
}

/// The tables due an analysis now (see [`tables_due_analysis_at`]).
pub fn tables_due_analysis(conn: &Connection) -> Result<Vec<String>> {
    Ok(tables_due_analysis_at(conn, hydrus_core::TimestampMs::now().0)?.0)
}

/// Analyse one table and remember it (`AnalyzeTable`): a table analysed
/// with rows before that is empty now is not analysed again, only its time
/// is renewed.
pub fn analyze_table(conn: &Connection, table: &str, now_ms: i64) -> Result<()> {
    let mut seen: AnalyzeTimestamps = crate::settings::get(conn)?;
    let mut rows = seen.0.get(table).map_or(0, |&(rows, _)| rows);
    let quoted = table.replace('"', "\"\"");
    if !(rows > 0 && !has_at_least(conn, table, 1)?) {
        conn.execute_batch(&format!("ANALYZE \"{quoted}\";"))?;
        rows = conn.query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |r| {
            r.get(0)
        })?;
    }
    seen.0.insert(table.to_owned(), (rows, now_ms));
    crate::settings::set(conn, &seen)
}

/// Analyse the tables due (those the reference analyses at once first)
/// one at a time until `stop` passes; returns how many of the due ones.
pub fn analyze_due_tables(conn: &Connection, stop: std::time::Instant) -> Result<usize> {
    let now = || hydrus_core::TimestampMs::now().0;
    let (due, at_once) = tables_due_analysis_at(conn, now())?;
    for table in &at_once {
        analyze_table(conn, table, now())?;
    }
    let mut done = 0;
    for table in &due {
        if std::time::Instant::now() >= stop {
            break;
        }
        analyze_table(conn, table, now())?;
        done += 1;
    }
    Ok(done)
}

/// Delete file-URL rows whose URL no longer exists; returns how many.
pub fn clear_orphan_url_mappings(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM file_urls WHERE NOT EXISTS (SELECT 1 FROM urls WHERE urls.url_id = file_urls.url_id)",
        [],
    )?)
}

/// Drop per-service tables whose tag service no longer exists; returns their
/// names.
pub fn clear_orphan_tables(conn: &Connection) -> Result<Vec<String>> {
    let registry = ServiceRegistry::load(conn)?;
    let tag_services: BTreeSet<i64> = registry
        .tag_services()
        .map(|s| i64::from(s.id.get()))
        .collect();
    let names = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut dropped = Vec::new();
    for name in names {
        if let Some(id) = per_service_table_id(&name)
            && !tag_services.contains(&id)
        {
            conn.execute_batch(&format!("DROP TABLE \"{name}\";"))?;
            dropped.push(name);
        }
    }
    Ok(dropped)
}

/// The service id of a per-tag-service table's name.
fn per_service_table_id(name: &str) -> Option<i64> {
    let id = if let Some(rest) = name.strip_prefix("mappings_") {
        let (id, status) = rest.split_once('_')?;
        ["current", "deleted", "pending", "petitioned"]
            .contains(&status)
            .then_some(id)?
    } else {
        name.strip_prefix("cache_tag_counts_")
            .or_else(|| name.strip_prefix("cache_display_counts_"))?
    };
    id.parse().ok()
}

/// A kind of interned id ("get tables using definitions").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Definition {
    Hash,
    Tag,
}

/// Every table and column holding this kind of id, as `(table, column)`.
pub fn tables_using(conn: &Connection, definition: Definition) -> Result<Vec<(String, String)>> {
    let suffix = match definition {
        Definition::Hash => "hash_id",
        Definition::Tag => "tag_id",
    };
    let tables = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut found = Vec::new();
    for table in tables {
        let columns = conn
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for column in columns {
            if column == suffix || column.ends_with(&format!("_{suffix}")) {
                found.push((table.clone(), column));
            }
        }
    }
    Ok(found)
}

/// Rename tags whose stored text is not what cleaning makes of it (or that
/// clean to nothing, which become "unrecoverable invalid tag"), to a name no
/// other tag has; returns the renames, old and new.
pub fn repair_invalid_tags(conn: &Connection) -> Result<Vec<(String, String)>> {
    let tags = conn
        .prepare(
            "SELECT t.tag_id, n.namespace, s.subtag FROM tags t
             JOIN namespaces n USING (namespace_id) JOIN subtags s USING (subtag_id) ORDER BY t.tag_id",
        )?
        .query_map([], |r| {
            Ok((
                r.get::<_, hydrus_core::TagId>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut renamed = Vec::new();
    for (id, namespace, subtag) in tags {
        let tag = hydrus_core::tag::combine_tag(&namespace, &subtag);
        let cleaned = hydrus_core::tag::clean_tag_checked(&tag)
            .unwrap_or_else(|| "unrecoverable invalid tag".to_owned());
        if tag == cleaned {
            continue;
        }
        let exists = |text: &str| -> Result<bool> {
            let (namespace, subtag) = hydrus_core::tag::split_tag(text);
            Ok(conn
                .prepare_cached(
                    "SELECT 1 FROM tags JOIN namespaces USING (namespace_id) JOIN subtags USING (subtag_id)
                     WHERE namespace = ?1 AND subtag = ?2",
                )?
                .exists(params![namespace, subtag])?)
        };
        let mut new = cleaned.clone();
        let mut i = 1;
        while exists(&new)? {
            new = format!("{cleaned} ({i})");
            i += 1;
        }
        let (namespace, subtag) = hydrus_core::tag::split_tag(&new);
        let namespace_id = crate::master::intern_namespace(conn, namespace)?;
        let subtag_id = crate::master::intern_subtag(conn, subtag)?;
        conn.execute(
            "UPDATE tags SET namespace_id = ?1, subtag_id = ?2 WHERE tag_id = ?3",
            params![namespace_id, subtag_id, id],
        )?;
        renamed.push((tag, new));
    }
    Ok(renamed)
}

/// Repair file records that are in a specific local domain but not its
/// umbrella, or in an umbrella but none of its components
/// (`clear_orphan_file_records`); returns the messages the reference shows.
pub fn clear_orphan_file_records(store: &Store) -> Result<Vec<String>> {
    let snapshot = store.snapshot();
    let services = &snapshot.services;
    let ids = |types: &[ServiceType]| -> Vec<ServiceId> {
        types
            .iter()
            .flat_map(|t| services.of_type(*t).map(|s| s.id))
            .collect()
    };
    let one = |t: ServiceType| services.of_type(t).next().map(|s| s.id);
    let mut jobs = Vec::new();
    if let Some(master) = one(ServiceType::CombinedLocalFileDomains) {
        jobs.push((
            ids(&[ServiceType::LocalFileDomain]),
            master,
            "combined local file domains umbrella",
        ));
    }
    if let Some(master) = one(ServiceType::HydrusLocalFileStorage) {
        jobs.push((
            ids(&[
                ServiceType::LocalFileTrashDomain,
                ServiceType::CombinedLocalFileDomains,
                ServiceType::LocalFileUpdateDomain,
            ]),
            master,
            "hydrus local file storage umbrella",
        ));
    }
    let mut messages = Vec::new();
    for (components, master, description) in jobs {
        let in_components: Vec<(HashId, Vec<u8>, u8, Option<i64>)> = store.read(|conn| {
            Ok(conn
                .prepare(
                    "SELECT c.hash_id, h.sha256, f.mime, min(c.added_ms) FROM file_domain_current c
                     JOIN hashes h USING (hash_id) LEFT JOIN files f USING (hash_id)
                     WHERE c.service_id IN rarray(?1) AND NOT EXISTS
                       (SELECT 1 FROM file_domain_current m WHERE m.service_id = ?2 AND m.hash_id = c.hash_id)
                     GROUP BY c.hash_id",
                )?
                .query_map(params![crate::master::id_array(&components), master], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<u8>>(2)?.unwrap_or(0), r.get(3)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?)
        })?;
        // Only a record whose file is on disk can be recovered.
        let (recover, forget): (Vec<_>, Vec<_>) =
            in_components.into_iter().partition(|(_, hash, mime, _)| {
                let (Ok(hash), Some(mime)) = (Sha256::from_slice(hash), Mime::from_code(*mime))
                else {
                    return false;
                };
                snapshot
                    .storage
                    .file_path(&hash, mime)
                    .is_some_and(|path| path.exists())
            });
        let recover: Vec<(HashId, Option<i64>)> =
            recover.into_iter().map(|(id, _, _, at)| (id, at)).collect();
        let forget: Vec<HashId> = forget.into_iter().map(|(id, ..)| id).collect();
        let component_ids = components.clone();
        let (recovered, forgotten, surplus) = store.write(move |ctx| {
            let conn = ctx.conn();
            let list = crate::master::id_array(&component_ids);
            for (id, added) in &recover {
                conn.execute(
                    "INSERT OR IGNORE INTO file_domain_current (service_id, hash_id, added_ms) VALUES (?1, ?2, ?3)",
                    params![master, id, added],
                )?;
            }
            conn.execute(
                "DELETE FROM file_domain_current WHERE service_id IN rarray(?1) AND hash_id IN rarray(?2)",
                params![list, crate::master::id_array(&forget)],
            )?;
            let surplus = conn.execute(
                "DELETE FROM file_domain_current WHERE service_id = ?1 AND NOT EXISTS
                 (SELECT 1 FROM file_domain_current c WHERE c.service_id IN rarray(?2) AND c.hash_id = file_domain_current.hash_id)",
                params![master, list],
            )?;
            if !recover.is_empty() || !forget.is_empty() || surplus > 0 {
                crate::counts::rebuild_all(conn)?;
            }
            Ok((recover.len(), forget.len(), surplus))
        })?;
        let n = hydrus_core::numbers::human_int;
        if recovered > 0 {
            messages.push(format!(
                "Found and recovered {} records for files that were safely in specific component services components but not the master \"{description}\".",
                n(recovered as u64)
            ));
        }
        if forgotten > 0 {
            messages.push(format!(
                "Found and deleted {} records for files that were in specific service components but not the master \"{description}\".",
                n(forgotten as u64)
            ));
        }
        if surplus > 0 {
            messages.push(format!(
                "Found and deleted {} records for files that were in the master \"{description}\" but not it its specific service components.",
                n(surplus as u64)
            ));
        }
    }
    if messages.is_empty() {
        messages.push("No orphan file records found!".to_owned());
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::tests::import_basic;

    fn basic() -> (tempfile::TempDir, tempfile::TempDir, std::sync::Arc<Store>) {
        let (source, dest, _) = import_basic();
        let store = Store::open(dest.path()).unwrap();
        (source, dest, store)
    }

    #[test]
    fn per_service_table_names() {
        assert_eq!(per_service_table_id("mappings_12_current"), Some(12));
        assert_eq!(per_service_table_id("mappings_3_petitioned"), Some(3));
        assert_eq!(per_service_table_id("cache_display_counts_7"), Some(7));
        assert_eq!(per_service_table_id("mappings_x_current"), None);
        assert_eq!(per_service_table_id("mappings_1_other"), None);
        assert_eq!(per_service_table_id("file_urls"), None);
    }

    #[test]
    fn basic_jobs_find_nothing_and_regeneration_keeps_counts() {
        let (_source, _dest, store) = basic();
        let counts = |store: &Store| {
            store
                .read(|conn| {
                    let registry = ServiceRegistry::load(conn)?;
                    let mut all = Vec::new();
                    for s in registry.tag_services() {
                        let t = MappingTables::new(s.id);
                        let n: i64 = conn.query_row(
                            &format!(
                                "SELECT count(*) + coalesce(sum(current),0) FROM {}",
                                t.display_counts
                            ),
                            [],
                            |r| r.get(0),
                        )?;
                        all.push(n);
                    }
                    Ok(all)
                })
                .unwrap()
        };
        let before = counts(&store);
        let (fixed, desynced, orphans, renamed) = store
            .write(|ctx| {
                let conn = ctx.conn();
                rebuild_tag_counts(conn, TagScope::All)?;
                rebuild_subtag_index(conn, SubtagIndex::All)?;
                rebuild_subtag_index(conn, SubtagIndex::MissingOnly)?;
                rebuild_subtag_index(conn, SubtagIndex::SearchableMaps)?;
                analyze(conn, false)?;
                analyze(conn, true)?;
                Ok((
                    fix_inconsistent_mappings(conn, TagScope::All)?,
                    resync_tag_counts(conn, TagScope::All)?,
                    clear_orphan_tables(conn)?,
                    repair_invalid_tags(conn)?,
                ))
            })
            .unwrap();
        assert_eq!(
            (fixed, desynced, orphans, renamed),
            (0, vec![], vec![], vec![])
        );
        assert_eq!(counts(&store), before);
        assert_eq!(
            clear_orphan_file_records(&store).unwrap(),
            ["No orphan file records found!"]
        );
        let hashes = store
            .read(|conn| tables_using(conn, Definition::Hash))
            .unwrap();
        assert!(hashes.contains(&("file_urls".to_owned(), "hash_id".to_owned())));
        let tags = store
            .read(|conn| tables_using(conn, Definition::Tag))
            .unwrap();
        assert!(tags.iter().all(|(_, c)| c.ends_with("tag_id")));
    }

    #[test]
    fn inconsistent_mappings_and_orphans_are_repaired() {
        let (_source, _dest, store) = basic();
        let (fixed, urls, tables) = store
            .write(|ctx| {
                let conn = ctx.conn();
                let registry = ServiceRegistry::load(conn)?;
                let service = registry.tag_services().next().unwrap().id;
                let t = MappingTables::new(service);
                let (tag, hash): (i64, i64) = conn.query_row(
                    &format!("SELECT tag_id, hash_id FROM {} LIMIT 1", t.current),
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                conn.execute(
                    &format!("INSERT INTO {} VALUES (?1, ?2)", t.pending),
                    [tag, hash],
                )?;
                conn.execute("INSERT INTO file_urls VALUES (?1, 999999)", [hash])?;
                conn.execute_batch("CREATE TABLE mappings_9999_current (tag_id INTEGER);")?;
                Ok((
                    fix_inconsistent_mappings(conn, TagScope::One(service))?,
                    clear_orphan_url_mappings(conn)?,
                    clear_orphan_tables(conn)?,
                ))
            })
            .unwrap();
        assert_eq!(
            (fixed, urls, tables),
            (1, 1, vec!["mappings_9999_current".to_owned()])
        );
    }
}
