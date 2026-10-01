//! The archive/delete filter (the reference's
//! `CanvasMediaListFilterArchiveDelete`): a page's local files one at a
//! time, each kept, deleted or skipped; when done, the kept are archived
//! and the deleted deleted, as `CommitArchiveDelete` does. Plain Rust,
//! driven by `filter_window`-like bindings and tested directly.

use std::collections::BTreeSet;
use std::sync::Arc;

use hydrus_core::{HashId, ServiceId};
use hydrus_search::LocationContext;
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use hydrus_store::services::ServiceRegistry;

/// The reason the filter's deletions record.
pub const DELETE_REASON: &str = "Deleted in Archive/Delete filter.";

/// What was decided for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Keep,
    Delete,
    Skip,
}

pub struct ArchiveDeleteFilter {
    store: Arc<Store>,
    files: Vec<HashId>,
    decisions: Vec<Option<Decision>>,
    index: usize,
    /// The file domains of the page it was launched from.
    location: LocationContext,
}

impl std::fmt::Debug for ArchiveDeleteFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveDeleteFilter")
            .field("files", &self.files.len())
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

/// Each file's current local domains (not the trash).
fn local_domains(store: &Store, files: &[HashId]) -> Vec<BTreeSet<ServiceId>> {
    let snapshot = store.snapshot();
    let Ok(roles) = DomainRoles::new(&snapshot.services) else {
        return vec![BTreeSet::new(); files.len()];
    };
    let Ok(batch) = store.read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
    else {
        return vec![BTreeSet::new(); files.len()];
    };
    files
        .iter()
        .map(|id| {
            batch
                .results
                .iter()
                .find(|m| m.hash_id == *id)
                .map(|m| {
                    m.current
                        .iter()
                        .map(|c| c.service)
                        .filter(|s| roles.local.contains(s))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect()
}

impl ArchiveDeleteFilter {
    /// Filter `files` (those in a local domain and not in the trash, as
    /// the reference takes them) from a page searching `location`; `None`
    /// if there are none.
    pub fn new(store: Arc<Store>, files: Vec<HashId>, location: LocationContext) -> Option<Self> {
        let domains = local_domains(&store, &files);
        let files: Vec<HashId> = files
            .into_iter()
            .zip(domains)
            .filter(|(_, d)| !d.is_empty())
            .map(|(id, _)| id)
            .collect();
        (!files.is_empty()).then(|| Self {
            store,
            decisions: vec![None; files.len()],
            files,
            index: 0,
            location,
        })
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// The file to decide on; `None` once every file is decided.
    pub fn current(&self) -> Option<HashId> {
        self.files.get(self.index).copied()
    }

    pub fn is_done(&self) -> bool {
        self.index >= self.files.len()
    }

    /// The window's title, e.g. `3/31`.
    pub fn caption(&self) -> String {
        format!(
            "{}/{}",
            (self.index + 1).min(self.files.len()),
            self.files.len()
        )
    }

    fn decide(&mut self, decision: Decision) {
        if let Some(slot) = self.decisions.get_mut(self.index) {
            *slot = Some(decision);
            self.index += 1;
        }
    }

    pub fn keep(&mut self) {
        self.decide(Decision::Keep);
    }

    pub fn delete(&mut self) {
        self.decide(Decision::Delete);
    }

    pub fn skip(&mut self) {
        self.decide(Decision::Skip);
    }

    /// Back to the file before, forgetting what was decided for it.
    pub fn back(&mut self) {
        if self.index > 0 {
            self.index -= 1;
            self.decisions[self.index] = None;
        }
    }

    fn decided(&self, decision: Decision) -> Vec<HashId> {
        self.files
            .iter()
            .zip(&self.decisions)
            .filter(|(_, d)| **d == Some(decision))
            .map(|(id, _)| *id)
            .collect()
    }

    pub fn kept(&self) -> Vec<HashId> {
        self.decided(Decision::Keep)
    }

    pub fn deleted(&self) -> Vec<HashId> {
        self.decided(Decision::Delete)
    }

    /// Whether anything was kept or deleted (so closing asks to commit).
    pub fn has_decisions(&self) -> bool {
        self.decisions
            .iter()
            .any(|d| matches!(d, Some(Decision::Keep | Decision::Delete)))
    }

    /// The domains deletions take the deleted files from: the first
    /// choice the reference's "filtering done?" dialog offers. The page's
    /// local domains, if it searches any; else the files' own (all local
    /// domains together, if they are in more than one).
    fn deletion_domains(&self, services: &ServiceRegistry) -> (Vec<ServiceId>, bool) {
        let Ok(roles) = DomainRoles::new(services) else {
            return (Vec::new(), false);
        };
        let page: Vec<ServiceId> = self
            .location
            .current()
            .iter()
            .filter_map(|key| services.by_key(key).ok().map(|s| s.id))
            .filter(|id| roles.local.contains(id) || *id == roles.combined_local_media)
            .collect();
        if !page.is_empty() {
            let combined = page == [roles.combined_local_media];
            return (page, combined);
        }
        let deleted = self.deleted();
        let theirs: BTreeSet<ServiceId> = local_domains(&self.store, &deleted)
            .into_iter()
            .flatten()
            .collect();
        if theirs.len() > 1 {
            (vec![roles.combined_local_media], true)
        } else {
            (theirs.into_iter().collect(), false)
        }
    }

    /// What committing would do, as the reference's dialog asks it:
    /// `keep 3 and delete 2 from my files?`.
    pub fn question(&self) -> String {
        let snapshot = self.store.snapshot();
        let (kept, deleted) = (self.kept().len(), self.deleted().len());
        let keep =
            (kept > 0).then(|| format!("keep {}", hydrus_core::numbers::human_int(kept as u64)));
        let delete = (deleted > 0).then(|| {
            let (domains, combined) = self.deletion_domains(&snapshot.services);
            let names: Vec<String> = domains
                .iter()
                .filter_map(|d| snapshot.services.get(*d).ok().map(|s| s.name.clone()))
                .collect();
            let names = names.join(", ");
            if combined {
                let n = if deleted == 1 {
                    "1".to_owned()
                } else {
                    format!("all {}", hydrus_core::numbers::human_int(deleted as u64))
                };
                format!("delete {n} from {names}, sending directly to trash")
            } else {
                format!(
                    "delete {} from {names}",
                    hydrus_core::numbers::human_int(deleted as u64)
                )
            }
        });
        match (keep, delete) {
            (Some(k), Some(d)) => format!("{k} and {d}?"),
            (Some(k), None) => format!("{k}?"),
            (None, Some(d)) => format!("{d}?"),
            (None, None) => String::new(),
        }
    }

    /// Archive the kept files and delete the deleted ones
    /// (`CommitArchiveDelete`): with the delete lock set to inbox
    /// deletees, the archived ones go back to the inbox first so they can
    /// be deleted.
    pub fn commit(&self) -> hydrus_store::Result<()> {
        let snapshot = self.store.snapshot();
        let (domains, _) = self.deletion_domains(&snapshot.services);
        let deleted = self.deleted();
        let kept = self.kept();
        self.store.write_content(move |w| {
            let lock: hydrus_store::delete_lock::DeleteLock =
                hydrus_store::settings::get(w.conn())?;
            if lock.archived && lock.reinbox_after_archive_delete {
                w.inbox(&deleted)?;
            }
            for domain in domains {
                // (only those in it, as the reference deletes them)
                let current = w.filter_current(domain, &deleted)?;
                w.delete_files(domain, &current, Some(DELETE_REASON))?;
            }
            w.archive(&kept)
        })
    }
}
