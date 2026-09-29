//! Tag mappings: the per-(tag, file) state machine of a tag service.
//!
//! A mapping can be current, deleted, pending (proposed to a repository) and
//! petitioned (proposed for removal). Local tag services use add/delete;
//! repositories use the pend/petition actions. Each action is a no-op where
//! the reference makes it one (e.g. pending something already current).

use std::collections::BTreeSet;

use rusqlite::params;

use hydrus_core::{HashId, ServiceId, TagId};

use super::ContentWriter;
use super::tally::{Column, MappingChange};
use crate::error::{Result, StoreError};
use crate::master::{id_array, intern_text};
use crate::schema::MappingTables;

/// A change to mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingAction {
    /// Make current (clearing any deletion record or pending proposal).
    Add,
    /// Make deleted (clearing any petition).
    Delete,
    /// Propose adding, unless already current, pending or petitioned.
    Pend,
    RescindPend,
    /// Propose removing, with a reason, unless pending or already petitioned.
    Petition {
        reason: String,
    },
    RescindPetition,
}

impl ContentWriter<'_> {
    /// Which of `hashes` have `tag` in `table`.
    fn with_tag(&self, table: &str, tag: TagId, hashes: &[HashId]) -> Result<BTreeSet<HashId>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT hash_id FROM {table} WHERE tag_id = ?1 AND hash_id IN rarray(?2)"
        ))?;
        let rows = stmt.query_map(params![tag, id_array(hashes)], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn delete_tag(&self, table: &str, tag: TagId, hashes: &[HashId]) -> Result<()> {
        self.conn
            .prepare_cached(&format!(
                "DELETE FROM {table} WHERE tag_id = ?1 AND hash_id IN rarray(?2)"
            ))?
            .execute(params![tag, id_array(hashes)])?;
        Ok(())
    }

    fn insert_tag(&self, table: &str, tag: TagId, hashes: &[HashId]) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "INSERT OR IGNORE INTO {table} (tag_id, hash_id) VALUES (?1, ?2)"
        ))?;
        for hash in hashes {
            stmt.execute(params![tag, hash])?;
        }
        Ok(())
    }

    fn counts_changed(
        &mut self,
        service: ServiceId,
        column: Column,
        tag: TagId,
        hashes: &[HashId],
        sign: i64,
    ) -> Result<()> {
        let graph = self.snap.display.get(service);
        let domains = super::tally::Domains {
            all_known_files: self.roles.all_known_files,
            counted: &self.roles.counted,
        };
        let change = MappingChange {
            column,
            tag,
            hashes,
            sign,
        };
        self.tally
            .mapping_changed(self.conn, &domains, (service, &graph), &change)
    }

    /// Apply `action` for `tag` on `hashes` in tag service `service`.
    /// Returns how many mappings changed.
    pub fn update_mappings(
        &mut self,
        service: ServiceId,
        action: &MappingAction,
        tag: TagId,
        hashes: &[HashId],
    ) -> Result<usize> {
        if !self.snap.services.get(service)?.kind.has_mappings() {
            return Err(StoreError::Invalid(format!(
                "service {service} does not hold tags"
            )));
        }
        let mut hashes = hashes.to_vec();
        hashes.sort_unstable();
        hashes.dedup();
        let t = MappingTables::new(service);
        let without = |set: &BTreeSet<HashId>| -> Vec<HashId> {
            hashes
                .iter()
                .copied()
                .filter(|h| !set.contains(h))
                .collect()
        };
        match action {
            MappingAction::Add => {
                let targets = without(&self.with_tag(&t.current, tag, &hashes)?);
                if targets.is_empty() {
                    return Ok(0);
                }
                let was_pending: Vec<HashId> = self
                    .with_tag(&t.pending, tag, &targets)?
                    .into_iter()
                    .collect();
                self.delete_tag(&t.deleted, tag, &targets)?;
                self.delete_tag(&t.pending, tag, &targets)?;
                self.insert_tag(&t.current, tag, &targets)?;
                self.counts_changed(service, Column::Current, tag, &targets, 1)?;
                self.counts_changed(service, Column::Pending, tag, &was_pending, -1)?;
                Ok(targets.len())
            }
            MappingAction::Delete => {
                let targets = without(&self.with_tag(&t.deleted, tag, &hashes)?);
                if targets.is_empty() {
                    return Ok(0);
                }
                let was_current: Vec<HashId> = self
                    .with_tag(&t.current, tag, &targets)?
                    .into_iter()
                    .collect();
                self.delete_tag(&t.current, tag, &targets)?;
                self.delete_tag(&t.petitioned, tag, &targets)?;
                self.insert_tag(&t.deleted, tag, &targets)?;
                self.counts_changed(service, Column::Current, tag, &was_current, -1)?;
                Ok(targets.len())
            }
            MappingAction::Pend => {
                let mut blocked = self.with_tag(&t.current, tag, &hashes)?;
                blocked.extend(self.with_tag(&t.pending, tag, &hashes)?);
                blocked.extend(self.with_tag(&t.petitioned, tag, &hashes)?);
                let targets = without(&blocked);
                self.insert_tag(&t.pending, tag, &targets)?;
                self.counts_changed(service, Column::Pending, tag, &targets, 1)?;
                Ok(targets.len())
            }
            MappingAction::RescindPend => {
                let targets: Vec<HashId> = self
                    .with_tag(&t.pending, tag, &hashes)?
                    .into_iter()
                    .collect();
                self.delete_tag(&t.pending, tag, &targets)?;
                self.counts_changed(service, Column::Pending, tag, &targets, -1)?;
                Ok(targets.len())
            }
            MappingAction::Petition { reason } => {
                let mut blocked = self.with_tag(&t.pending, tag, &hashes)?;
                blocked.extend(self.with_tag(&t.petitioned, tag, &hashes)?);
                let targets = without(&blocked);
                if targets.is_empty() {
                    return Ok(0);
                }
                let reason_id = intern_text(self.conn, reason)?;
                let mut stmt = self.conn.prepare_cached(&format!(
                    "INSERT OR IGNORE INTO {} (tag_id, hash_id, reason_id) VALUES (?1, ?2, ?3)",
                    t.petitioned
                ))?;
                for hash in &targets {
                    stmt.execute(params![tag, hash, reason_id])?;
                }
                Ok(targets.len())
            }
            MappingAction::RescindPetition => {
                let targets: Vec<HashId> = self
                    .with_tag(&t.petitioned, tag, &hashes)?
                    .into_iter()
                    .collect();
                self.delete_tag(&t.petitioned, tag, &targets)?;
                Ok(targets.len())
            }
        }
    }
}
