//! The archived-file delete lock (reference `ClientDBFileDeleteLock` and
//! the `delete_lock_*` options): while it is on, archived files can't be
//! deleted for good. They can still go to the trash, where they wait,
//! unless they are inboxed again.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use hydrus_core::{HashId, ServiceId};

use crate::error::Result;
use crate::master::id_array;

/// The lock and when files are inboxed again so they can go
/// (`delete_lock_for_archived_files` and `delete_lock_reinbox_deletees_*`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeleteLock {
    /// Archived files can't be deleted for good.
    pub archived: bool,
    /// Inbox the files the archive/delete filter deletes.
    pub reinbox_after_archive_delete: bool,
    /// Inbox the files the duplicate filter (or the Client API, merging
    /// as the filter does) deletes.
    pub reinbox_after_duplicate_filter: bool,
    /// Inbox the files duplicates auto-resolution deletes.
    pub reinbox_in_auto_resolution: bool,
}

impl crate::settings::Setting for DeleteLock {
    const KEY: &'static str = "delete_lock";
}

impl DeleteLock {
    /// The reference's option names, with this setting's fields.
    pub fn by_option_name(&mut self) -> [(&'static str, &mut bool); 4] {
        [
            ("delete_lock_for_archived_files", &mut self.archived),
            (
                "delete_lock_reinbox_deletees_after_archive_delete",
                &mut self.reinbox_after_archive_delete,
            ),
            (
                "delete_lock_reinbox_deletees_after_duplicate_filter",
                &mut self.reinbox_after_duplicate_filter,
            ),
            (
                "delete_lock_reinbox_deletees_in_auto_resolution",
                &mut self.reinbox_in_auto_resolution,
            ),
        ]
    }
}

/// Which option, if any, inboxes an archived file that a duplicate
/// decision deletes, so that it can go for good: as the reference's merge
/// path has it, the duplicate filter's (which the Client API shares when it
/// merges) or auto-resolution's; a Client API delete without a merge has
/// none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reinbox {
    Never,
    AfterDuplicateFilter,
    InAutoResolution,
}

impl Reinbox {
    /// Whether a locked file this deletes is inboxed first.
    pub fn applies(self, lock: &DeleteLock) -> bool {
        lock.archived
            && match self {
                Self::Never => false,
                Self::AfterDuplicateFilter => lock.reinbox_after_duplicate_filter,
                Self::InAutoResolution => lock.reinbox_in_auto_resolution,
            }
    }
}

/// Which of `hashes` the lock keeps from being deleted for good: with the
/// lock on, those in local file storage and not in the inbox
/// (`IsPhysicalDeleteLocked`).
pub fn locked(
    conn: &Connection,
    local_file_storage: ServiceId,
    hashes: &[HashId],
) -> Result<Vec<HashId>> {
    let lock: DeleteLock = crate::settings::get(conn)?;
    if !lock.archived || hashes.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare_cached(
        "SELECT hash_id FROM file_domain_current d
         WHERE service_id = ?1 AND hash_id IN rarray(?2)
         AND NOT EXISTS (SELECT 1 FROM file_inbox i WHERE i.hash_id = d.hash_id)",
    )?;
    let rows = stmt.query_map(params![local_file_storage, id_array(hashes)], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What a query of files that may be deleted for good adds about the
/// `column` of file ids (`GetPhysicalFileDeleteLockSQLitePredicates`): with
/// the lock on, that they are in the inbox.
pub fn sql_condition(conn: &Connection, column: &str) -> Result<Option<String>> {
    let lock: DeleteLock = crate::settings::get(conn)?;
    Ok(lock
        .archived
        .then(|| format!("EXISTS (SELECT 1 FROM file_inbox WHERE file_inbox.hash_id = {column})")))
}
