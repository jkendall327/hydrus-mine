//! File-domain membership: adding, deleting, undeleting, trash, inbox.
//!
//! The umbrella domains are maintained explicitly, as in the reference:
//! "combined local media" is the union of the local file domains, "local file
//! storage" everything physically stored (local domains, trash, repository
//! updates), and "combined deleted" every file with a deletion record in a
//! domain it covers. A file leaving its last local domain goes to the trash;
//! leaving local file storage queues its physical deletion.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rusqlite::{OptionalExtension, params};

use hydrus_core::{HashId, ServiceId};

use super::ContentWriter;
use crate::error::Result;
use crate::master::{id_array, intern_text};

/// `(hash, time added)` pairs; a missing time is unknown.
pub type AddRows<'r> = &'r [(HashId, Option<i64>)];

/// A deletion record: when, and when the file had been added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Deletion {
    deleted_ms: Option<i64>,
    original_added_ms: Option<i64>,
}

impl ContentWriter<'_> {
    /// Which of `hashes` are current in `domain`, with the time they were added.
    fn current_in(
        &self,
        domain: ServiceId,
        hashes: &[HashId],
    ) -> Result<BTreeMap<HashId, Option<i64>>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT hash_id, added_ms FROM file_domain_current
             WHERE service_id = ?1 AND hash_id IN rarray(?2)",
        )?;
        let rows = stmt.query_map(params![domain, id_array(hashes)], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Deletion records in `domain` for those of `hashes` that have one.
    fn deleted_in(
        &self,
        domain: ServiceId,
        hashes: &[HashId],
    ) -> Result<BTreeMap<HashId, Deletion>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT hash_id, deleted_ms, original_added_ms FROM file_domain_deleted
             WHERE service_id = ?1 AND hash_id IN rarray(?2)",
        )?;
        let rows = stmt.query_map(params![domain, id_array(hashes)], |r| {
            Ok((
                r.get(0)?,
                Deletion {
                    deleted_ms: r.get(1)?,
                    original_added_ms: r.get(2)?,
                },
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Add files to a domain (reference `AddFiles`). Files already in it are
    /// left alone. Adding to a local domain takes files out of the trash and
    /// into the umbrella domains; any deletion record in the domain is
    /// cleared.
    pub fn add_files(&mut self, domain: ServiceId, rows: AddRows<'_>) -> Result<()> {
        let mut unique: BTreeMap<HashId, Option<i64>> = BTreeMap::new();
        for &(hash, added) in rows {
            unique.entry(hash).or_insert(added);
        }
        let hashes: Vec<HashId> = unique.keys().copied().collect();
        let existing = self.current_in(domain, &hashes)?;
        let new_rows: Vec<(HashId, Option<i64>)> = unique
            .into_iter()
            .filter(|(hash, _)| !existing.contains_key(hash))
            .collect();
        if new_rows.is_empty() {
            return Ok(());
        }
        crate::domains::changed(self.conn)?;
        let new: Vec<HashId> = new_rows.iter().map(|(h, _)| *h).collect();
        let roles = self.roles.clone();

        if roles.local.contains(&domain) {
            self.remove_files(roles.trash, &new, false)?;
            self.add_files(roles.combined_local_media, &new_rows)?;
            self.add_files(roles.local_file_storage, &new_rows)?;
        }
        if domain == roles.local_updates {
            self.add_files(roles.local_file_storage, &new_rows)?;
        }

        {
            let mut insert = self.conn.prepare_cached(
                "INSERT OR IGNORE INTO file_domain_current (service_id, hash_id, added_ms) VALUES (?1, ?2, ?3)",
            )?;
            for (hash, added) in &new_rows {
                insert.execute(params![domain, hash, added])?;
            }
        }
        let ids = id_array(&new);
        self.conn
            .prepare_cached(
                "DELETE FROM file_domain_pending WHERE service_id = ?1 AND hash_id IN rarray(?2)",
            )?
            .execute(params![domain, ids])?;
        if domain != roles.trash {
            self.conn
                .prepare_cached("DELETE FROM file_domain_deleted WHERE service_id = ?1 AND hash_id IN rarray(?2)")?
                .execute(params![domain, ids])?;
        }
        if domain == roles.local_file_storage {
            self.conn
                .prepare_cached(
                    "DELETE FROM deferred_physical_deletes WHERE hash_id IN rarray(?1)",
                )?
                .execute([ids])?;
        }
        self.domain_changed(domain, &new, 1)?;

        if roles.covered_by_combined_deleted.contains(&domain) {
            let still_deleted = self.deleted_anywhere_covered(&new)?;
            let no_longer: Vec<HashId> = new
                .into_iter()
                .filter(|h| !still_deleted.contains(h))
                .collect();
            self.remove_files(roles.combined_deleted, &no_longer, false)?;
        }
        Ok(())
    }

    /// Which of `hashes` have a deletion record in a domain the combined
    /// deleted domain covers.
    fn deleted_anywhere_covered(&self, hashes: &[HashId]) -> Result<BTreeSet<HashId>> {
        let covered: Vec<ServiceId> = self
            .roles
            .covered_by_combined_deleted
            .iter()
            .copied()
            .collect();
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT hash_id FROM file_domain_deleted
             WHERE hash_id IN rarray(?1) AND service_id IN rarray(?2)",
        )?;
        let rows = stmt.query_map(params![id_array(hashes), id_array(&covered)], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Remove files from a domain (reference `DeleteFiles`), cascading
    /// through the umbrella domains. With `only_if_current`, deletion records
    /// are made only for files that were actually in the domain.
    pub(crate) fn remove_files(
        &mut self,
        domain: ServiceId,
        hashes: &[HashId],
        only_if_current: bool,
    ) -> Result<()> {
        if hashes.is_empty() {
            return Ok(());
        }
        crate::domains::changed(self.conn)?;
        let roles = self.roles.clone();
        if domain == roles.local_file_storage {
            self.remove_files(roles.combined_local_media, hashes, true)?;
            self.remove_files(roles.local_updates, hashes, true)?;
            self.remove_files(roles.trash, hashes, true)?;
        }
        if domain == roles.combined_local_media {
            for &local in &roles.local {
                self.remove_files(local, hashes, true)?;
            }
        }

        let existing = self.current_in(domain, hashes)?;
        let now = Some(self.now_ms);
        if roles.keeps_deletion_records(domain) {
            let recorded: Vec<HashId> = if only_if_current {
                existing.keys().copied().collect()
            } else {
                hashes.to_vec()
            };
            let mut record = self.conn.prepare_cached(
                "INSERT OR IGNORE INTO file_domain_deleted (service_id, hash_id, deleted_ms, original_added_ms)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for hash in &recorded {
                let added = existing.get(hash).copied().flatten();
                record.execute(params![domain, hash, self.now_ms, added])?;
            }
            // Unlike the reference, a deletion record made for a file that
            // wasn't in the domain (a "pre-emptive" delete) also puts it in
            // combined deleted, which is by definition the union of deletion
            // records (see DIFFERENCES.md).
            if roles.covered_by_combined_deleted.contains(&domain) {
                let rows: Vec<_> = recorded.iter().map(|&h| (h, now)).collect();
                self.add_files(roles.combined_deleted, &rows)?;
            }
        }
        if existing.is_empty() {
            return Ok(());
        }
        let removed: Vec<HashId> = existing.keys().copied().collect();
        let ids = id_array(&removed);
        self.conn
            .prepare_cached(
                "DELETE FROM file_domain_current WHERE service_id = ?1 AND hash_id IN rarray(?2)",
            )?
            .execute(params![domain, ids])?;
        self.conn
            .prepare_cached("DELETE FROM file_domain_petitioned WHERE service_id = ?1 AND hash_id IN rarray(?2)")?
            .execute(params![domain, ids])?;
        self.domain_changed(domain, &removed, -1)?;

        if roles.local.contains(&domain) {
            let others: Vec<ServiceId> = roles
                .local
                .iter()
                .copied()
                .filter(|&d| d != domain)
                .collect();
            let mut stmt = self.conn.prepare_cached(
                "SELECT DISTINCT hash_id FROM file_domain_current
                 WHERE hash_id IN rarray(?1) AND service_id IN rarray(?2)",
            )?;
            let still_local: BTreeSet<HashId> = stmt
                .query_map(params![id_array(&removed), id_array(&others)], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let trashed: Vec<HashId> = removed
                .iter()
                .copied()
                .filter(|h| !still_local.contains(h))
                .collect();
            if !trashed.is_empty() {
                self.remove_files(roles.combined_local_media, &trashed, false)?;
                let rows: Vec<_> = trashed.iter().map(|&h| (h, now)).collect();
                self.add_files(roles.trash, &rows)?;
            }
        }
        if domain == roles.local_updates {
            self.remove_files(roles.local_file_storage, &removed, false)?;
        }
        if domain == roles.local_file_storage {
            self.archive(&removed)?;
            let mut queue = self.conn.prepare_cached(
                "INSERT OR REPLACE INTO deferred_physical_deletes (hash_id, queued_ms) VALUES (?1, ?2)",
            )?;
            for hash in &removed {
                queue.execute(params![hash, self.now_ms])?;
            }
        }
        Ok(())
    }

    /// Delete files as a user or the Client API asks: from `domain`, with an
    /// optional reason recorded for files that were in a local domain.
    /// Deleting from the trash deletes from local file storage, i.e. for good.
    pub fn delete_files(
        &mut self,
        domain: ServiceId,
        hashes: &[HashId],
        reason: Option<&str>,
    ) -> Result<()> {
        let roles = &self.roles;
        let takes_reason = roles.local.contains(&domain)
            || domain == roles.combined_local_media
            || domain == roles.local_file_storage;
        if takes_reason && let Some(reason) = reason {
            let current: Vec<HashId> = self.current_in(domain, hashes)?.into_keys().collect();
            if !current.is_empty() {
                let reason_id = intern_text(self.conn, reason)?;
                let mut set = self.conn.prepare_cached(
                    "INSERT OR REPLACE INTO file_deletion_reasons (hash_id, reason_id) VALUES (?1, ?2)",
                )?;
                for hash in current {
                    set.execute(params![hash, reason_id])?;
                }
            }
        }
        let target = if domain == self.roles.trash {
            self.roles.local_file_storage
        } else {
            domain
        };
        self.remove_files(target, hashes, false)
    }

    /// Restore deleted files (reference `UndeleteFiles`). Undeleting from an
    /// umbrella domain or the trash restores the files to every local domain
    /// they were deleted from, with their original import times.
    pub fn undelete_files(&mut self, domain: ServiceId, hashes: &[HashId]) -> Result<()> {
        let roles = &self.roles;
        let targets = if domain == roles.local_file_storage
            || domain == roles.combined_local_media
            || domain == roles.trash
        {
            roles.local.clone()
        } else {
            vec![domain]
        };
        for target in targets {
            let rows: Vec<(HashId, Option<i64>)> = self
                .deleted_in(target, hashes)?
                .into_iter()
                .map(|(hash, deletion)| (hash, deletion.original_added_ms))
                .collect();
            self.add_files(target, &rows)?;
        }
        Ok(())
    }

    /// Archive files that are in the inbox, recording when.
    pub fn archive(&mut self, hashes: &[HashId]) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT hash_id FROM file_inbox WHERE hash_id IN rarray(?1)")?;
        let inboxed: Vec<HashId> = stmt
            .query_map([id_array(hashes)], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        let mut delete = self
            .conn
            .prepare_cached("DELETE FROM file_inbox WHERE hash_id = ?1")?;
        let mut stamp = self.conn.prepare_cached(
            "INSERT OR REPLACE INTO file_archived (hash_id, archived_ms) VALUES (?1, ?2)",
        )?;
        for hash in inboxed {
            delete.execute([hash])?;
            stamp.execute(params![hash, self.now_ms])?;
        }
        Ok(())
    }

    /// Put files back in the inbox. Only files in local storage can be.
    pub fn inbox(&mut self, hashes: &[HashId]) -> Result<()> {
        let local: Vec<HashId> = self
            .current_in(self.roles.local_file_storage, hashes)?
            .into_keys()
            .collect();
        let mut insert = self
            .conn
            .prepare_cached("INSERT OR IGNORE INTO file_inbox (hash_id) VALUES (?1)")?;
        let mut clear = self
            .conn
            .prepare_cached("DELETE FROM file_archived WHERE hash_id = ?1")?;
        for hash in local {
            if insert.execute([hash])? > 0 {
                clear.execute([hash])?;
            }
        }
        Ok(())
    }

    /// Forget that files were deleted from the local domains, so importing
    /// them again isn't blocked (reference `ClearLocalDeleteRecord`). Files in
    /// the trash keep their records. `None` means every file.
    pub fn clear_local_delete_records(&mut self, hashes: Option<&[HashId]>) -> Result<()> {
        let roles = &self.roles;
        let mut domains = roles.local.clone();
        domains.extend([roles.combined_local_media, roles.local_file_storage]);
        let domains = id_array(&domains);
        let trash = roles.trash;
        crate::domains::changed(self.conn)?;
        match hashes {
            None => {
                self.conn.execute(
                    "DELETE FROM file_domain_deleted WHERE service_id IN rarray(?1)
                     AND hash_id NOT IN (SELECT hash_id FROM file_domain_current WHERE service_id = ?2)",
                    params![domains, trash],
                )?;
                self.conn.execute(
                    "DELETE FROM file_deletion_reasons
                     WHERE hash_id NOT IN (SELECT hash_id FROM file_domain_current WHERE service_id = ?1)",
                    [trash],
                )?;
            }
            Some(hashes) => {
                let in_trash = self.current_in(trash, hashes)?;
                let clearable: Vec<HashId> = hashes
                    .iter()
                    .copied()
                    .filter(|h| !in_trash.contains_key(h))
                    .collect();
                let ids = id_array(&clearable);
                self.conn.execute(
                    "DELETE FROM file_domain_deleted WHERE service_id IN rarray(?1) AND hash_id IN rarray(?2)",
                    params![domains, ids],
                )?;
                self.conn.execute(
                    "DELETE FROM file_deletion_reasons WHERE hash_id IN rarray(?1)",
                    [ids],
                )?;
            }
        }
        self.resync_combined_deleted(hashes)
    }

    /// Make the combined deleted domain match the deletion records of the
    /// domains it covers (reference `ResyncCombinedDeletedFiles`).
    pub(crate) fn resync_combined_deleted(&mut self, hashes: Option<&[HashId]>) -> Result<()> {
        let combined = self.roles.combined_deleted;
        let covered: Vec<ServiceId> = self
            .roles
            .covered_by_combined_deleted
            .iter()
            .copied()
            .collect();
        // hash -> earliest deletion time in a covered domain
        let desired: HashMap<HashId, Option<i64>> = {
            let covered = id_array(&covered);
            let row = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?));
            match hashes {
                None => self
                    .conn
                    .prepare(
                        "SELECT hash_id, min(deleted_ms) FROM file_domain_deleted
                         WHERE service_id IN rarray(?1) GROUP BY hash_id",
                    )?
                    .query_map([covered], row)?
                    .collect::<rusqlite::Result<_>>()?,
                Some(h) => self
                    .conn
                    .prepare_cached(
                        "SELECT hash_id, min(deleted_ms) FROM file_domain_deleted
                         WHERE service_id IN rarray(?1) AND hash_id IN rarray(?2) GROUP BY hash_id",
                    )?
                    .query_map(params![covered, id_array(h)], row)?
                    .collect::<rusqlite::Result<_>>()?,
            }
        };
        let existing: BTreeSet<HashId> = match hashes {
            None => {
                let mut stmt = self
                    .conn
                    .prepare("SELECT hash_id FROM file_domain_current WHERE service_id = ?1")?;
                stmt.query_map([combined], |r| r.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            }
            Some(h) => self.current_in(combined, h)?.into_keys().collect(),
        };
        let remove: Vec<HashId> = existing
            .iter()
            .copied()
            .filter(|h| !desired.contains_key(h))
            .collect();
        self.remove_files(combined, &remove, true)?;
        let mut add: Vec<(HashId, Option<i64>)> = desired
            .into_iter()
            .filter(|(h, _)| !existing.contains(h))
            .collect();
        add.sort_unstable();
        self.add_files(combined, &add)
    }

    /// Those of `hashes` currently in `domain`, in order.
    pub fn filter_current(&self, domain: ServiceId, hashes: &[HashId]) -> Result<Vec<HashId>> {
        let current = self.current_in(domain, hashes)?;
        Ok(hashes
            .iter()
            .copied()
            .filter(|h| current.contains_key(h))
            .collect())
    }

    /// Those of `hashes` with a deletion record in `domain`, in order.
    pub fn filter_deleted(&self, domain: ServiceId, hashes: &[HashId]) -> Result<Vec<HashId>> {
        let deleted = self.deleted_in(domain, hashes)?;
        Ok(hashes
            .iter()
            .copied()
            .filter(|h| deleted.contains_key(h))
            .collect())
    }

    /// When a file was deleted from `domain`, if it was.
    pub fn deletion_time(&self, domain: ServiceId, hash: HashId) -> Result<Option<Option<i64>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT deleted_ms FROM file_domain_deleted WHERE service_id = ?1 AND hash_id = ?2",
                params![domain, hash],
                |r| r.get(0),
            )
            .optional()?)
    }
}
