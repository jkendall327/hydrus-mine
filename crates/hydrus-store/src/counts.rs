//! Derived tag counts, for autocomplete.
//!
//! For every tag service S and every file domain D (plus "all known files"),
//! `cache_tag_counts_S` holds how many files in D have each stored tag, and
//! `cache_display_counts_S` how many files in D have each *display* tag (a
//! file counts once per display tag however many of its stored tags map to
//! it). Rows with zero counts are not stored.
//!
//! This module has the *rebuild* path; the incremental path lives with the
//! writes that change mappings, domains or relations, and tests assert the two
//! agree.

use std::fmt::Write as _;

use rusqlite::{Connection, params};

use hydrus_core::{ServiceId, TagId};

use crate::display::{DisplayGraph, DisplayGraphs};
use crate::error::Result;
use crate::schema::MappingTables;
use crate::services::{ServiceKind, ServiceRegistry};

/// Domains we keep counts for, besides "all known files".
fn counted_domains(registry: &ServiceRegistry) -> Vec<ServiceId> {
    registry
        .all()
        .filter(|s| {
            s.service_type().is_file_service() && !matches!(s.kind, ServiceKind::AllKnownFiles)
        })
        .map(|s| s.id)
        .collect()
}

/// Rebuild every tag service's counts from scratch.
pub fn rebuild_all(conn: &Connection) -> Result<()> {
    let registry = ServiceRegistry::load(conn)?;
    let graphs = DisplayGraphs::load(conn, &registry)?;
    let all_known_files = registry
        .of_type(hydrus_core::ServiceType::CombinedFile)
        .next()
        .map(|s| s.id);
    let domains = counted_domains(&registry);
    for service in registry.tag_services() {
        rebuild_service(
            conn,
            service.id,
            &graphs.get(service.id),
            all_known_files,
            &domains,
        )?;
    }
    Ok(())
}

/// Rebuild one tag service's storage and display counts.
pub fn rebuild_service(
    conn: &Connection,
    service: ServiceId,
    graph: &DisplayGraph,
    all_known_files: Option<ServiceId>,
    domains: &[ServiceId],
) -> Result<()> {
    let t = MappingTables::new(service);
    let (current, pending, counts, display_counts) =
        (&t.current, &t.pending, &t.counts, &t.display_counts);
    conn.execute_batch(&format!(
        "DELETE FROM {counts}; DELETE FROM {display_counts};"
    ))?;

    let domain_list = domains
        .iter()
        .map(|d| d.get().to_string())
        .collect::<Vec<_>>()
        .join(",");

    let mut sql = String::new();
    if let Some(akf) = all_known_files {
        write!(
            sql,
            "INSERT INTO {counts} (domain_id, tag_id, current, pending)
             SELECT {akf}, tag_id, sum(c), sum(p) FROM (
                 SELECT tag_id, count(*) AS c, 0 AS p FROM {current} GROUP BY tag_id
                 UNION ALL
                 SELECT tag_id, 0, count(*) FROM {pending} GROUP BY tag_id
             ) GROUP BY tag_id;\n"
        )
        .expect("writing to a String cannot fail");
    }
    if !domain_list.is_empty() {
        write!(
            sql,
            "INSERT INTO {counts} (domain_id, tag_id, current, pending)
             SELECT service_id, tag_id, sum(c), sum(p) FROM (
                 SELECT d.service_id, m.tag_id, count(*) AS c, 0 AS p
                 FROM {current} m JOIN file_domain_current d ON d.hash_id = m.hash_id
                 WHERE d.service_id IN ({domain_list}) GROUP BY d.service_id, m.tag_id
                 UNION ALL
                 SELECT d.service_id, m.tag_id, 0, count(*)
                 FROM {pending} m JOIN file_domain_current d ON d.hash_id = m.hash_id
                 WHERE d.service_id IN ({domain_list}) GROUP BY d.service_id, m.tag_id
             ) GROUP BY service_id, tag_id;\n"
        )
        .expect("writing to a String cannot fail");
    }
    conn.execute_batch(&sql)?;

    rebuild_display(conn, &t, graph, all_known_files, &domain_list)
}

/// Display counts: identical to storage counts for tags the display graph
/// doesn't touch; recounted with de-duplication for tags it does.
fn rebuild_display(
    conn: &Connection,
    t: &MappingTables,
    graph: &DisplayGraph,
    all_known_files: Option<ServiceId>,
    domain_list: &str,
) -> Result<()> {
    let (current, pending, counts, display_counts) =
        (&t.current, &t.pending, &t.counts, &t.display_counts);
    if graph.is_empty() {
        conn.execute_batch(&format!(
            "INSERT INTO {display_counts} SELECT * FROM {counts};"
        ))?;
        return Ok(());
    }

    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS display_map (storage_tag_id INTEGER NOT NULL, display_tag_id INTEGER NOT NULL,
             PRIMARY KEY (storage_tag_id, display_tag_id)) WITHOUT ROWID;
         CREATE TEMP TABLE IF NOT EXISTS graph_tags (tag_id INTEGER PRIMARY KEY);
         DELETE FROM display_map; DELETE FROM graph_tags;",
    )?;
    {
        let mut insert_map =
            conn.prepare_cached("INSERT OR IGNORE INTO display_map VALUES (?, ?)")?;
        let mut insert_tag = conn.prepare_cached("INSERT OR IGNORE INTO graph_tags VALUES (?)")?;
        for stored in graph.all_tags() {
            insert_tag.execute([stored])?;
            for display in graph.display_tags(stored) {
                insert_tag.execute([display])?;
                insert_map.execute(params![stored, display])?;
            }
        }
    }

    let mut sql = format!(
        "INSERT INTO {display_counts} SELECT * FROM {counts} WHERE tag_id NOT IN (SELECT tag_id FROM graph_tags);\n"
    );
    let mut parts = Vec::new();
    if let Some(akf) = all_known_files {
        parts.push(format!(
            "SELECT {akf} AS domain_id, dm.display_tag_id AS tag_id, count(DISTINCT m.hash_id) AS c, 0 AS p
             FROM display_map dm JOIN {current} m ON m.tag_id = dm.storage_tag_id GROUP BY dm.display_tag_id"
        ));
        parts.push(format!(
            "SELECT {akf}, dm.display_tag_id, 0, count(DISTINCT m.hash_id)
             FROM display_map dm JOIN {pending} m ON m.tag_id = dm.storage_tag_id GROUP BY dm.display_tag_id"
        ));
    }
    if !domain_list.is_empty() {
        for (table, current_col, pending_col) in [
            (current, "count(DISTINCT m.hash_id)", "0"),
            (pending, "0", "count(DISTINCT m.hash_id)"),
        ] {
            parts.push(format!(
                "SELECT d.service_id, dm.display_tag_id, {current_col}, {pending_col}
                 FROM display_map dm JOIN {table} m ON m.tag_id = dm.storage_tag_id
                 JOIN file_domain_current d ON d.hash_id = m.hash_id
                 WHERE d.service_id IN ({domain_list})
                 GROUP BY d.service_id, dm.display_tag_id"
            ));
        }
    }
    if !parts.is_empty() {
        write!(
            sql,
            "INSERT INTO {display_counts} (domain_id, tag_id, current, pending)
             SELECT domain_id, tag_id, sum(c), sum(p) FROM ({}) GROUP BY domain_id, tag_id;\n",
            parts.join("\nUNION ALL\n")
        )
        .expect("writing to a String cannot fail");
    }
    conn.execute_batch(&sql)?;
    conn.execute_batch("DELETE FROM display_map; DELETE FROM graph_tags;")?;
    Ok(())
}

/// A tag's counts in one domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TagCount {
    pub current: u64,
    pub pending: u64,
}

/// Read one tag's storage or display count in a domain.
pub fn count(
    conn: &Connection,
    service: ServiceId,
    domain: ServiceId,
    tag: TagId,
    display: bool,
) -> Result<TagCount> {
    let t = MappingTables::new(service);
    let table = if display {
        &t.display_counts
    } else {
        &t.counts
    };
    let row = conn
        .prepare_cached(&format!(
            "SELECT current, pending FROM {table} WHERE domain_id = ? AND tag_id = ?"
        ))?
        .query_row(params![domain, tag], |r| {
            Ok(TagCount {
                current: r.get::<_, i64>(0)? as u64,
                pending: r.get::<_, i64>(1)? as u64,
            })
        });
    match row {
        Ok(c) => Ok(c),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(TagCount::default()),
        Err(e) => Err(e.into()),
    }
}
