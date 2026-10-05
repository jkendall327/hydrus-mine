//! Changes to content: file domains, tag mappings, and the metadata attached
//! to files.
//!
//! Every change goes through a [`ContentWriter`], created inside a write
//! transaction (see [`crate::Store::write_content`]). It applies changes to
//! the primary tables immediately and keeps derived data (tag counts) in step
//! incrementally; [`ContentWriter::finish`] flushes the derived changes.
//!
//! The semantics of each operation (which domains a delete cascades to, what
//! a pend does to a petitioned tag, ...) follow the reference implementation;
//! each function says which reference behaviour it implements.

mod files;
mod local_transfer;
mod mappings;
mod metadata;
pub mod tag_relations;
mod tally;

use std::collections::HashSet;

use rusqlite::Connection;

use hydrus_core::{HashId, ServiceId, ServiceType};

use crate::error::{Result, StoreError};
use crate::services::{ServiceKind, ServiceRegistry};
use crate::store::Snapshot;

pub use files::AddRows;
pub use local_transfer::TransferKind;
pub use mappings::MappingAction;
pub use metadata::{FileTime, RatingClearScope};

use tally::Tally;

/// The file domains with special roles in the file lifecycle.
#[derive(Debug, Clone)]
pub struct DomainRoles {
    /// User-facing local file domains ("my files", ...).
    pub local: Vec<ServiceId>,
    pub trash: ServiceId,
    pub combined_local_media: ServiceId,
    pub local_file_storage: ServiceId,
    pub local_updates: ServiceId,
    pub combined_deleted: ServiceId,
    pub all_known_files: Option<ServiceId>,
    /// Domains whose deletion records the combined deleted domain unions.
    pub covered_by_combined_deleted: HashSet<ServiceId>,
    /// Domains we keep tag counts for.
    pub counted: HashSet<ServiceId>,
}

impl DomainRoles {
    pub fn new(services: &ServiceRegistry) -> Result<Self> {
        let one = |t: ServiceType| {
            services
                .of_type(t)
                .next()
                .map(|s| s.id)
                .ok_or_else(|| StoreError::Corrupt(format!("there is no {t} service")))
        };
        let file_services = || {
            services
                .all()
                .filter(|s| s.service_type().is_file_service())
        };
        Ok(Self {
            local: services
                .of_type(ServiceType::LocalFileDomain)
                .map(|s| s.id)
                .collect(),
            trash: one(ServiceType::LocalFileTrashDomain)?,
            combined_local_media: one(ServiceType::CombinedLocalFileDomains)?,
            local_file_storage: one(ServiceType::HydrusLocalFileStorage)?,
            local_updates: one(ServiceType::LocalFileUpdateDomain)?,
            combined_deleted: one(ServiceType::CombinedDeletedFile)?,
            all_known_files: services
                .of_type(ServiceType::CombinedFile)
                .next()
                .map(|s| s.id),
            covered_by_combined_deleted: file_services()
                .filter(|s| {
                    matches!(
                        s.kind,
                        ServiceKind::CombinedLocalMedia
                            | ServiceKind::LocalFiles
                            | ServiceKind::LocalUpdates
                            | ServiceKind::FileRepository(_)
                            | ServiceKind::Ipfs(_)
                    )
                })
                .map(|s| s.id)
                .collect(),
            counted: file_services()
                .filter(|s| !matches!(s.kind, ServiceKind::AllKnownFiles))
                .map(|s| s.id)
                .collect(),
        })
    }

    /// Whether deleting from `domain` leaves a deletion record.
    fn keeps_deletion_records(&self, domain: ServiceId) -> bool {
        domain != self.trash
            && domain != self.combined_deleted
            && Some(domain) != self.all_known_files
    }
}

/// Applies content changes inside one write transaction.
#[derive(Debug)]
pub struct ContentWriter<'a> {
    conn: &'a Connection,
    snap: &'a Snapshot,
    roles: DomainRoles,
    now_ms: i64,
    tally: Tally,
}

impl<'a> ContentWriter<'a> {
    /// A writer that timestamps changes with `now_ms`.
    pub fn new(conn: &'a Connection, snap: &'a Snapshot, now_ms: i64) -> Result<Self> {
        Ok(Self {
            conn,
            snap,
            roles: DomainRoles::new(&snap.services)?,
            now_ms,
            tally: Tally::default(),
        })
    }

    pub fn conn(&self) -> &'a Connection {
        self.conn
    }

    pub fn snapshot(&self) -> &'a Snapshot {
        self.snap
    }

    pub fn now_ms(&self) -> i64 {
        self.now_ms
    }

    pub fn roles(&self) -> &DomainRoles {
        &self.roles
    }

    /// Flush derived changes. Must be called before the transaction commits;
    /// [`crate::Store::write_content`] does.
    pub fn finish(mut self) -> Result<()> {
        self.tally.flush(self.conn)
    }

    /// Keep counts in step with `hashes` entering or leaving `domain`.
    fn domain_changed(&mut self, domain: ServiceId, hashes: &[HashId], sign: i64) -> Result<()> {
        if hashes.is_empty() || !self.roles.counted.contains(&domain) {
            return Ok(());
        }
        let tag_services: Vec<ServiceId> =
            self.snap.services.tag_services().map(|s| s.id).collect();
        for service in tag_services {
            let graph = self.snap.display.get(service);
            self.tally
                .domain_changed(self.conn, (service, &graph), domain, hashes, sign)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
