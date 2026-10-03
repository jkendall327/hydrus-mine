//! Service review facts read from one consistent native database snapshot.

use hydrus_core::numbers::human_bytes;

use hydrus_core::{ServiceId, ServiceKey};
use hydrus_store::services::{ServiceKind, ServiceRegistry};
use hydrus_store::{Store, error::Result};
use rusqlite::Connection;

fn human_int(n: i64) -> String {
    hydrus_core::numbers::human_int(u64::try_from(n).unwrap_or(0))
}

/// One service in the review window, including its current native statistics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: ServiceId,
    pub key: ServiceKey,
    pub name: String,
    pub service_type: String,
    pub statistics: String,
    pub unavailable: String,
}

fn count(conn: &Connection, table: &str, id: ServiceId) -> Result<i64> {
    Ok(conn.query_row(
        &format!("SELECT count(*) FROM {table} WHERE service_id = ?"),
        [id],
        |r| r.get(0),
    )?)
}

fn statistics(
    conn: &Connection,
    id: ServiceId,
    kind: &ServiceKind,
    graph: &hydrus_store::display::DisplayGraph,
) -> Result<String> {
    Ok(match kind {
        ServiceKind::LocalFiles
        | ServiceKind::LocalUpdates
        | ServiceKind::Trash
        | ServiceKind::LocalFileStorage
        | ServiceKind::CombinedLocalMedia
        | ServiceKind::FileRepository(_) => {
            let (files, size): (i64, i64) = conn.query_row(
                "SELECT count(*), coalesce(sum(coalesce(f.size, 0)), 0) FROM file_domain_current d LEFT JOIN files f USING(hash_id) WHERE d.service_id = ?",
                [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let mut text = format!(
                "{} files, totalling {}",
                human_int(files),
                human_bytes(u64::try_from(size).unwrap_or(0))
            );
            if !matches!(kind, ServiceKind::LocalUpdates | ServiceKind::Trash) {
                text.push_str(&format!(
                    " - {} deleted files",
                    human_int(count(conn, "file_domain_deleted", id)?)
                ));
            }
            text
        }
        ServiceKind::LocalTags | ServiceKind::TagRepository(_) => {
            let table = hydrus_store::schema::MappingTables::new(id);
            let (mappings, files): (i64, i64) = conn.query_row(
                &format!(
                    "SELECT count(*), count(DISTINCT hash_id) FROM {}",
                    table.current
                ),
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let mut tags = graph.all_tags();
            tags.extend(
                conn.prepare(&format!(
                    "SELECT tag_id FROM {} UNION SELECT tag_id FROM {}",
                    table.current, table.pending
                ))?
                .query_map([], |r| r.get::<_, hydrus_core::TagId>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            );
            format!(
                "{} total mappings involving {} different tags on {} different files",
                human_int(mappings),
                hydrus_core::numbers::human_int(u64::try_from(tags.len()).unwrap_or(u64::MAX)),
                human_int(files)
            )
        }
        ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) => {
            format!("{} files are rated", human_int(count(conn, "ratings", id)?))
        }
        ServiceKind::RatingIncDec(_) => format!(
            "{} files are rated",
            human_int(count(conn, "ratings_incdec", id)?)
        ),
        ServiceKind::LocalNotes => {
            let files: i64 =
                conn.query_row("SELECT count(DISTINCT hash_id) FROM file_notes", [], |r| {
                    r.get(0)
                })?;
            format!("{} files have notes", human_int(files))
        }
        _ => String::new(),
    })
}

/// Read names, types and counts together, so a concurrent service change cannot mix them.
pub fn rows(store: &Store) -> Result<Vec<Row>> {
    store.read(|conn| {
        let registry = ServiceRegistry::load(conn)?;
        let graphs = hydrus_store::display::DisplayGraphs::load(conn,&registry)?;
        let mut rows = Vec::new();
        for service in registry.all() {
            let unavailable = match &service.kind {
                ServiceKind::TagRepository(_) | ServiceKind::FileRepository(_) | ServiceKind::Ipfs(_) => "Repository synchronisation, IPFS and account administration are not available yet.",
                ServiceKind::LocalTags => "Tag migration is not available yet.",
                ServiceKind::Trash => "Bulk clear trash and undelete all are not available here yet.",
                ServiceKind::RatingLike(_) | ServiceKind::RatingNumerical(_) | ServiceKind::RatingIncDec(_) => "Bulk clear ratings is not available here yet.",
                ServiceKind::ClientApi(_) => "Client API permission controls are not available here yet.",
                ServiceKind::LocalFileStorage => "Clear deleted files record is not available here yet.",
                _ => "",
            };
            rows.push(Row { id: service.id, key: service.key.clone(), name: service.name.clone(), service_type: service.service_type().name().into(), statistics: statistics(conn, service.id, &service.kind, &graphs.get(service.id))?, unavailable: unavailable.into() });
        }
        rows.sort_by(|a,b| a.service_type.cmp(&b.service_type).then_with(|| a.name.cmp(&b.name)));
        Ok(rows)
    })
}
