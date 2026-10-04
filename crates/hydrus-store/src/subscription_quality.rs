//! Read-only subscription quality reports from durable query logs and current
//! file locations. Hashes are counted once, independent of seed import status.
use crate::{Result, master, media, queues, services::ServiceRegistry};
use hydrus_core::{Sha256, service::builtin_keys};
use rusqlite::Connection;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryQuality {
    pub name: String,
    pub inbox: u64,
    pub archived: u64,
    pub deleted: u64,
}

/// Unknown stored hashes count as nonlocal; a disappeared saved log is an error.
/// Reads can be stopped between queries when the owning editor is closed.
pub fn read(
    conn: &Connection,
    selected: &[(i64, String)],
    cancel: &AtomicBool,
) -> Result<Vec<QueryQuality>> {
    let services = ServiceRegistry::load(conn)?;
    let local = services
        .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)?
        .id;
    let trash = services.builtin(builtin_keys::TRASH)?.id;
    let mut out = Vec::new();
    for (queue, name) in selected {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if queues::queue(conn, *queue)?.is_none() {
            return Err(crate::StoreError::Invalid(format!(
                "The saved query log {queue} is no longer available."
            )));
        }
        let hashes: BTreeSet<Sha256> = queues::file_seeds(conn, *queue)?
            .into_iter()
            .filter_map(|seed| seed.meta.hash("sha256")?.parse().ok())
            .collect();
        let hashes: Vec<Sha256> = hashes.into_iter().collect();
        let ids = master::hash_ids(conn, &hashes)?;
        let ids: Vec<_> = ids.into_values().collect();
        let locations = media::current_domains(conn, &ids)?;
        let inbox = media::inboxed(conn, &ids)?;
        let mut report = QueryQuality {
            name: name.clone(),
            inbox: 0,
            archived: 0,
            deleted: hashes.len() as u64,
        };
        for id in ids {
            let current = locations.get(&id).map(Vec::as_slice).unwrap_or_default();
            if current.contains(&local) && !current.contains(&trash) {
                report.deleted -= 1;
                if inbox.contains(&id) {
                    report.inbox += 1;
                } else {
                    report.archived += 1;
                }
            }
        }
        out.push(report);
    }
    Ok(out)
}
