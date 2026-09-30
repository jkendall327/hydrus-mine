//! Content waiting to be uploaded to a repository: how much of it there is,
//! and forgetting it (`ClientDB._GetNumsPending`, `_DeletePending`).

use std::collections::BTreeMap;

use rusqlite::Connection;

use hydrus_core::{ContentStatus, HashId, ServiceId, ServiceKey, ServiceType, TagId};

use crate::content::MappingAction;
use crate::error::Result;
use crate::schema::MappingTables;
use crate::services::ServiceRegistry;
use crate::store::Store;

/// A repository's counts of pending and petitioned content, named as the
/// Client API names them.
pub type PendingCounts = Vec<(&'static str, i64)>;

/// For each repository (tag and file repositories, IPFS), its counts of
/// pending and petitioned content.
pub fn counts(
    conn: &Connection,
    registry: &ServiceRegistry,
) -> Result<Vec<(ServiceKey, PendingCounts)>> {
    let count = |sql: &str, service: ServiceId| -> Result<i64> {
        Ok(conn.query_row(sql, [service], |r| r.get(0))?)
    };
    let relations = |table: &str, service: ServiceId, status: ContentStatus| -> Result<i64> {
        Ok(conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE service_id = ?1 AND status = ?2"),
            rusqlite::params![service, status.code()],
            |r| r.get(0),
        )?)
    };
    let mut out = Vec::new();
    for service in registry.all() {
        let id = service.id;
        let counts = match service.service_type() {
            ServiceType::TagRepository => {
                let t = MappingTables::new(id);
                let rows = |table: &str| -> Result<i64> {
                    Ok(conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?)
                };
                vec![
                    ("pending_tag_mappings", rows(&t.pending)?),
                    ("petitioned_tag_mappings", rows(&t.petitioned)?),
                    (
                        "pending_tag_siblings",
                        relations("tag_siblings", id, ContentStatus::Pending)?,
                    ),
                    (
                        "petitioned_tag_siblings",
                        relations("tag_siblings", id, ContentStatus::Petitioned)?,
                    ),
                    (
                        "pending_tag_parents",
                        relations("tag_parents", id, ContentStatus::Pending)?,
                    ),
                    (
                        "petitioned_tag_parents",
                        relations("tag_parents", id, ContentStatus::Petitioned)?,
                    ),
                ]
            }
            ServiceType::FileRepository | ServiceType::Ipfs => vec![
                (
                    "pending_files",
                    count(
                        "SELECT count(*) FROM file_domain_pending WHERE service_id = ?",
                        id,
                    )?,
                ),
                (
                    "petitioned_files",
                    count(
                        "SELECT count(*) FROM file_domain_petitioned WHERE service_id = ?",
                        id,
                    )?,
                ),
            ],
            _ => continue,
        };
        out.push((service.key.clone(), counts));
    }
    Ok(out)
}

/// Forget everything pending or petitioned on a repository: its proposed
/// tags (the tag counts follow), sibling and parent proposals (so the tags
/// display without them), and proposed files.
pub fn forget(store: &Store, service: ServiceId) -> Result<()> {
    let service_type = store.snapshot().services.get(service)?.service_type();
    if service_type == ServiceType::TagRepository {
        store.write_content(move |writer| {
            let t = MappingTables::new(service);
            for (table, action) in [
                (&t.pending, MappingAction::RescindPend),
                (&t.petitioned, MappingAction::RescindPetition),
            ] {
                let mut by_tag: BTreeMap<TagId, Vec<HashId>> = BTreeMap::new();
                let mut stmt = writer
                    .conn()
                    .prepare(&format!("SELECT tag_id, hash_id FROM {table}"))?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                for row in rows {
                    let (tag, hash) = row?;
                    by_tag.entry(tag).or_default().push(hash);
                }
                drop(stmt);
                for (tag, hashes) in by_tag {
                    writer.update_mappings(service, &action, tag, &hashes)?;
                }
            }
            Ok(())
        })?;
    }
    store.write_and_refresh(move |ctx| {
        let conn = ctx.conn();
        let proposed = [ContentStatus::Pending.code(), ContentStatus::Petitioned.code()];
        let mut relations_changed = 0;
        for table in ["tag_siblings", "tag_parents"] {
            relations_changed += conn.execute(
                &format!("DELETE FROM {table} WHERE service_id = ?1 AND status IN (?2, ?3)"),
                rusqlite::params![service, proposed[0], proposed[1]],
            )?;
        }
        conn.execute(
            "DELETE FROM file_domain_pending WHERE service_id = ?",
            [service],
        )?;
        conn.execute(
            "DELETE FROM file_domain_petitioned WHERE service_id = ?",
            [service],
        )?;
        if relations_changed > 0 {
            // the tags of every service that applies these relations display
            // differently now
            let registry = ServiceRegistry::load(conn)?;
            let graphs = crate::display::DisplayGraphs::load(conn, &registry)?;
            let mut stmt = conn.prepare(
                "SELECT DISTINCT display_service_id FROM tag_display_application WHERE source_service_id = ?",
            )?;
            let affected: Vec<ServiceId> = stmt
                .query_map([service], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            for display in affected {
                crate::counts::rebuild_display_service(conn, &registry, display, &graphs)?;
            }
        }
        Ok(())
    })
}
