//! The archive/delete filter (the reference's
//! `CanvasMediaListFilterArchiveDelete`): a page's local files one at a
//! time, each kept, deleted or skipped; when done, the kept are archived
//! and the deleted deleted, as `CommitArchiveDelete` does. Plain Rust,
//! driven by `filter_window`-like bindings and tested directly.

use std::collections::BTreeSet;
use std::sync::Arc;

use hydrus_core::{HashId, ServiceId, ServiceKey};
use hydrus_search::LocationContext;
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;

/// The reason the filter's deletions record.
pub const DELETE_REASON: &str = "Deleted in Archive/Delete filter.";

/// What was decided for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Keep,
    Delete,
    Skip,
}

/// One reference finish-button label and its captured deletion service identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionChoice {
    pub label: String,
    domains: Vec<(ServiceId, ServiceKey)>,
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

    pub fn skipped(&self) -> Vec<HashId> {
        self.decided(Decision::Skip)
    }
    /// Read saved policy on acceptance, preserving the independent skipped preference.
    pub fn removed_from_view(&self) -> Vec<HashId> {
        let preferences = self
            .store
            .read(hydrus_store::settings::get::<hydrus_store::settings::FileViewRemoval>)
            .unwrap_or_default();
        if !preferences.filtered {
            return Vec::new();
        }
        self.files
            .iter()
            .zip(&self.decisions)
            .filter_map(|(&file, decision)| {
                matches!(decision, Some(Decision::Keep | Decision::Delete))
                    .then_some(file)
                    .or_else(|| {
                        (preferences.skipped && *decision == Some(Decision::Skip)).then_some(file)
                    })
            })
            .collect()
    }
    pub fn return_file(&self) -> Option<HashId> {
        let preferences = self
            .store
            .read(hydrus_store::settings::get::<hydrus_store::settings::FileViewRemoval>)
            .unwrap_or_default();
        if preferences.filtered && !preferences.skipped {
            self.skipped()
                .first()
                .copied()
                .or_else(|| self.files.first().copied())
        } else {
            self.files.first().copied()
        }
    }
    /// Whether anything was kept or deleted (so closing asks to commit).
    pub fn has_decisions(&self) -> bool {
        self.decisions
            .iter()
            .any(|d| matches!(d, Some(Decision::Keep | Decision::Delete)))
    }

    /// Real finish-dialog alternatives, deduped by the complete current context.
    /// Each captured key is checked again inside the committing transaction.
    pub fn deletion_choices(&self) -> Vec<DeletionChoice> {
        let snapshot = self.store.snapshot();
        let Ok(roles) = DomainRoles::new(&snapshot.services) else {
            return Vec::new();
        };
        let deleted = self.deleted();
        if deleted.is_empty() {
            return Vec::new();
        }
        let memberships = local_domains(&self.store, &deleted);
        let preferences = self
            .store
            .read(hydrus_store::archive_delete_preferences::load)
            .unwrap_or_default();
        let page: Vec<ServiceId> = self
            .location
            .current()
            .iter()
            .filter_map(|key| snapshot.services.by_key(key).ok().map(|s| s.id))
            .filter(|id| roles.local.contains(id) || *id == roles.combined_local_media)
            .collect();
        let own: BTreeSet<ServiceId> = memberships.iter().flatten().copied().collect();
        let mut contexts = Vec::new();
        if preferences.all_domains {
            contexts.push(vec![roles.combined_local_media]);
        } else {
            if own.len() > 1 {
                contexts.push(vec![roles.combined_local_media]);
            }
            if !page.is_empty() {
                contexts.push(page);
            }
            contexts.extend(own.into_iter().map(|domain| vec![domain]));
        }
        let mut seen = BTreeSet::new();
        contexts
            .into_iter()
            .filter_map(|mut domains| {
                domains.sort();
                domains.dedup();
                if !seen.insert(domains.clone()) {
                    return None;
                }
                let combined = domains == [roles.combined_local_media];
                let count = memberships
                    .iter()
                    .filter(|current| {
                        if combined {
                            !current.is_empty()
                        } else {
                            domains.iter().any(|domain| current.contains(domain))
                        }
                    })
                    .count();
                if count == 0 {
                    return None;
                }
                let captured: Vec<(ServiceId, ServiceKey)> = domains
                    .iter()
                    .filter_map(|id| {
                        snapshot
                            .services
                            .get(*id)
                            .ok()
                            .map(|service| (*id, service.key.clone()))
                    })
                    .collect();
                let mut names: Vec<String> = domains
                    .iter()
                    .filter_map(|id| {
                        snapshot
                            .services
                            .get(*id)
                            .ok()
                            .map(|service| service.name.clone())
                    })
                    .collect();
                names.sort();
                let location = if names.len() > 2 {
                    format!(
                        "{} services",
                        hydrus_core::numbers::human_int(names.len() as u64)
                    )
                } else {
                    names.join(", ")
                };
                let number = hydrus_core::numbers::human_int(count as u64);
                let label = if combined {
                    format!(
                        "delete {} from {location}, sending directly to trash",
                        if count == 1 {
                            number
                        } else {
                            format!("all {number}")
                        }
                    )
                } else {
                    format!("delete {number} from {location}")
                };
                Some(DeletionChoice {
                    label,
                    domains: captured,
                })
            })
            .collect()
    }
    /// The delay is applied only to a genuinely multiple-choice finish panel.
    pub fn delay_multiple_choices(&self) -> bool {
        self.deletion_choices().len() > 1
            && self
                .store
                .read(hydrus_store::archive_delete_preferences::load)
                .unwrap_or_default()
                .delay_multiple
    }
    /// The optional kept-file caption shown above multiple deletion choices.
    pub fn kept_label(&self) -> Option<String> {
        let count = self.kept().len();
        (count > 0).then(|| format!("keep {}", hydrus_core::numbers::human_int(count as u64)))
    }

    /// What committing would do, as the reference's dialog asks it:
    /// `keep 3 and delete 2 from my files?`.
    pub fn question(&self) -> String {
        let keep = self.kept_label();
        let delete = self
            .deletion_choices()
            .into_iter()
            .next()
            .map(|choice| choice.label);
        match (keep, delete) {
            (Some(k), Some(d)) => format!("{k} and {d}?"),
            (Some(k), None) => format!("{k}?"),
            (None, Some(d)) => format!("{d}?"),
            (None, None) => {
                if self.has_decisions() {
                    "ERROR: do not seem to have any actions at all!?".into()
                } else {
                    String::new()
                }
            }
        }
    }

    /// Archive the kept files and delete the deleted ones
    /// (`CommitArchiveDelete`): with the delete lock set to inbox
    /// deletees, the archived ones go back to the inbox first so they can
    /// be deleted.
    pub fn commit(&self) -> hydrus_store::Result<()> {
        self.commit_changed().map(|_| ())
    }

    /// Report only deletees whose domain membership actually changed, for content-event pruning.
    pub fn commit_changed(&self) -> hydrus_store::Result<Vec<HashId>> {
        let choice = self.deletion_choices().into_iter().next();
        self.commit_choice_changed(choice.as_ref())
    }
    /// Commit one captured finish choice, validating service identity before any write.
    pub fn commit_choice_changed(
        &self,
        choice: Option<&DeletionChoice>,
    ) -> hydrus_store::Result<Vec<HashId>> {
        let domains = choice.map_or_else(Vec::new, |choice| choice.domains.clone());
        let deleted = self.deleted();
        let kept = self.kept();
        self.store.write_content(move |w| {
            let registry = hydrus_store::services::ServiceRegistry::load(w.conn())?;
            let roles = DomainRoles::new(&registry)?;
            for (id, key) in &domains {
                if registry.by_key(key)?.id != *id
                    || (!roles.local.contains(id) && *id != roles.combined_local_media)
                {
                    return Err(hydrus_store::error::StoreError::Invalid(
                        "The deletion domain was replaced; reopen the finish choices.".into(),
                    ));
                }
            }
            let lock: hydrus_store::delete_lock::DeleteLock =
                hydrus_store::settings::get(w.conn())?;
            if lock.archived && lock.reinbox_after_archive_delete {
                w.inbox(&deleted)?;
            }
            let mut changed = std::collections::BTreeSet::new();
            for (domain, _) in domains {
                // (only those in it, as the reference deletes them)
                let current = w.filter_current(domain, &deleted)?;
                w.delete_files(domain, &current, Some(DELETE_REASON))?;
                let remaining = w.filter_current(domain, &current)?;
                changed.extend(current.into_iter().filter(|file| !remaining.contains(file)));
            }
            w.archive(&kept)?;
            Ok(changed.into_iter().collect())
        })
    }
}
