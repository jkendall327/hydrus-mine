//! Which files match a file search, for the endpoints that filter by one
//! (potential duplicates).
//!
//! This is the seam where the file search executor plugs in: it must turn a
//! parsed search into a [`FileFilter`] answering "which of these files
//! match?". Until the executor lands, [`interim_filter`] evaluates the
//! searches that need nothing but tags (`system:everything`, required and
//! excluded tags) and refuses anything else with a clear error.

use std::collections::HashSet;
use std::sync::Arc;

use rusqlite::{Connection, params};

use hydrus_core::{HashId, ServiceId, Tag, TagId};
use hydrus_search::{Predicate, SystemPredicate};
use hydrus_store::duplicates::FileFilter;
use hydrus_store::schema::MappingTables;
use hydrus_store::{Snapshot, master};

use crate::error::{ApiError, ApiResult};

/// A boxed [`FileFilter`] that owns what it needs.
pub type OwnedFileFilter = Box<FileFilter<'static>>;

/// A filter for `predicates` searched in `tag_service` (`None`: all known
/// tags), or `None` if every file matches.
pub fn interim_filter(
    snapshot: Arc<Snapshot>,
    predicates: Vec<Predicate>,
    tag_service: Option<ServiceId>,
) -> ApiResult<Option<OwnedFileFilter>> {
    let mut tags: Vec<(Tag, bool)> = Vec::new();
    for predicate in predicates {
        match predicate {
            Predicate::System(SystemPredicate::Everything) => {}
            Predicate::Tag { tag, inclusive } => tags.push((tag, inclusive)),
            other => {
                return Err(ApiError::server(format!(
                    "searching potential duplicates by {other:?} is not supported yet"
                )));
            }
        }
    }
    if tags.is_empty() {
        return Ok(None);
    }
    let services: Vec<ServiceId> = match tag_service {
        Some(service) => vec![service],
        None => snapshot.services.tag_services().map(|s| s.id).collect(),
    };
    Ok(Some(Box::new(
        move |conn: &Connection, candidates: &[HashId]| {
            let mut result: HashSet<HashId> = candidates.iter().copied().collect();
            for (tag, inclusive) in &tags {
                let with_tag = files_with_tag(conn, &snapshot, &services, tag, candidates)?;
                if *inclusive {
                    result.retain(|h| with_tag.contains(h));
                } else {
                    result.retain(|h| !with_tag.contains(h));
                }
            }
            Ok(result)
        },
    )))
}

/// Which of `candidates` have `tag` (as displayed, current or pending) in
/// any of `services`.
fn files_with_tag(
    conn: &Connection,
    snapshot: &Snapshot,
    services: &[ServiceId],
    tag: &Tag,
    candidates: &[HashId],
) -> hydrus_store::Result<HashSet<HashId>> {
    let Some(tag_id) = master::tag_id(conn, tag)? else {
        return Ok(HashSet::new());
    };
    let as_ints = |ids: &[u32]| -> std::rc::Rc<Vec<rusqlite::types::Value>> {
        std::rc::Rc::new(
            ids.iter()
                .map(|&i| rusqlite::types::Value::Integer(i64::from(i)))
                .collect(),
        )
    };
    let hash_ints: Vec<u32> = candidates.iter().map(|h| h.get()).collect();
    let mut out = HashSet::new();
    for &service in services {
        let stored: Vec<u32> = snapshot
            .display
            .get(service)
            .stored_tags_for(tag_id)
            .into_iter()
            .map(TagId::get)
            .collect();
        let tables = MappingTables::new(service);
        let sql = format!(
            "SELECT hash_id FROM {} WHERE hash_id IN rarray(?1) AND tag_id IN rarray(?2)
             UNION SELECT hash_id FROM {} WHERE hash_id IN rarray(?1) AND tag_id IN rarray(?2)",
            tables.current, tables.pending
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![as_ints(&hash_ints), as_ints(&stored)], |r| {
            r.get::<_, HashId>(0)
        })?;
        for row in rows {
            out.insert(row?);
        }
    }
    Ok(out)
}
