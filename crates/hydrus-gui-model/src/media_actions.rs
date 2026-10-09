//! What the media shortcuts do to files, as the reference's default `media`
//! shortcut set has them: F7 archives, shift+F7 inboxes, delete deletes
//! (asking first) and shift+delete undeletes.

use std::collections::BTreeSet;

use hydrus_core::{HashId, ServiceId};
use hydrus_search::LocationContext;
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;

/// The reason a deletion records, as the reference's viewer gives it.
pub const DELETE_REASON: &str = "Deleted from Preview or Media Viewer.";

/// What deleting files does: the first choice the reference's delete
/// dialog would offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deletion {
    /// Out of the page's local file domain (to the trash, if in no other).
    FromDomain { domain: ServiceId, name: String },
    /// Out of every local file domain, to the trash.
    ToTrash,
    /// Out of the trash, for good.
    Physically,
}

impl Deletion {
    /// The question asked before deleting `n` files.
    pub fn question(&self, n: usize) -> String {
        let files = if n == 1 {
            "this file".to_owned()
        } else {
            format!("these {} files", hydrus_core::numbers::human_int(n as u64))
        };
        match self {
            Deletion::FromDomain { name, .. } => format!("Delete {files} from {name}?"),
            Deletion::ToTrash => format!("Send {files} to the trash?"),
            Deletion::Physically => format!("Permanently delete {files}?"),
        }
    }
}

/// The local domains each of `files` is in.
fn domains_of(store: &Store, files: &[HashId]) -> Option<BTreeSet<ServiceId>> {
    let snapshot = store.snapshot();
    let batch = store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))
        .ok()?;
    Some(
        batch
            .results
            .iter()
            .flat_map(|m| m.current.iter().map(|c| c.service))
            .collect(),
    )
}

/// What deleting `files`, shown on a page searching `location`, would do;
/// `None` if they are in no local domain and not in the trash.
pub fn deletion(store: &Store, location: &LocationContext, files: &[HashId]) -> Option<Deletion> {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).ok()?;
    let current = domains_of(store, files)?;
    // a page on one local file domain deletes from it
    let page_domain = match location.current().iter().collect::<Vec<_>>()[..] {
        [key] => snapshot
            .services
            .by_key(key)
            .ok()
            .map(|s| (s.id, s.name.clone())),
        _ => None,
    };
    if let Some((domain, name)) = page_domain
        && roles.local.contains(&domain)
        && current.contains(&domain)
    {
        return Some(Deletion::FromDomain { domain, name });
    }
    if roles.local.iter().any(|d| current.contains(d)) {
        return Some(Deletion::ToTrash);
    }
    current
        .contains(&roles.trash)
        .then_some(Deletion::Physically)
}

/// Whether the reference asks before archiving or inboxing this actionable selection.
pub fn confirm_archive(store: &Store, count: usize) -> bool {
    count > 1
        && store
            .read(hydrus_store::settings::get::<hydrus_store::settings::DeletionPreferences>)
            .unwrap_or_default()
            .confirm_archive
}

/// Simple local deletion bypasses its question only when exactly one local domain
/// is actionable. Physical deletion and ambiguous domain choices always ask.
pub fn confirm_deletion(store: &Store, files: &[HashId], deletion: &Deletion) -> bool {
    if matches!(deletion, Deletion::Physically) {
        return true;
    }
    let preferences = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::DeletionPreferences>)
        .unwrap_or_default();
    if preferences.confirm_trash {
        return true;
    }
    let snapshot = store.snapshot();
    let Ok(roles) = DomainRoles::new(&snapshot.services) else {
        return true;
    };
    let Some(current) = domains_of(store, files) else {
        return true;
    };
    roles
        .local
        .iter()
        .filter(|domain| current.contains(domain))
        .count()
        != 1
}

/// Translate a simple menu action to the advanced dialog's suggested action.
pub fn suggested_action(
    store: &Store,
    deletion: &Deletion,
) -> Option<hydrus_store::settings::DeletionAction> {
    use hydrus_store::settings::DeletionAction;
    let snapshot = store.snapshot();
    match deletion {
        Deletion::FromDomain { domain, .. } => snapshot
            .services
            .get(*domain)
            .ok()
            .map(|s| DeletionAction::Domain(s.key.clone())),
        Deletion::ToTrash => None,
        Deletion::Physically => Some(DeletionAction::Physical),
    }
}

/// Archive files in the inbox.
pub fn archive(store: &Store, files: &[HashId]) -> hydrus_store::Result<()> {
    store.write_undoable(hydrus_store::undo::Package(vec![
        hydrus_store::undo::Change::Archive(files.to_vec()),
    ]))
}

/// Return files to the inbox.
pub fn inbox(store: &Store, files: &[HashId]) -> hydrus_store::Result<()> {
    store.write_undoable(hydrus_store::undo::Package(vec![
        hydrus_store::undo::Change::Inbox(files.to_vec()),
    ]))
}

/// Delete files as `deletion` says. Deleting for good leaves those the
/// delete lock holds.
pub fn delete(store: &Store, files: &[HashId], deletion: &Deletion) -> hydrus_store::Result<()> {
    delete_changed(store, files, deletion).map(|_| ())
}
/// Successful identities only: locked or already-absent rows are not a view removal.
pub fn delete_changed(
    store: &Store,
    files: &[HashId],
    deletion: &Deletion,
) -> hydrus_store::Result<Vec<HashId>> {
    let files = files.to_vec();
    let deletion = deletion.clone();
    store.write_content(move |w| {
        let roles = w.roles().clone();
        let domain = match deletion {
            Deletion::FromDomain { domain, .. } => domain,
            Deletion::ToTrash => roles.combined_local_media,
            Deletion::Physically => roles.local_file_storage,
        };
        let before = hydrus_store::media::current_domains(w.conn(), &files)?;
        w.delete_files(domain, &files, Some(DELETE_REASON))?;
        let after = hydrus_store::media::current_domains(w.conn(), &files)?;
        Ok(files
            .into_iter()
            .filter(|file| {
                before
                    .get(file)
                    .is_some_and(|domains| domains.contains(&domain))
                    && after
                        .get(file)
                        .is_none_or(|domains| !domains.contains(&domain))
            })
            .collect())
    })
}

/// Restore files from the trash to the local domains they were deleted
/// from.
pub fn undelete(store: &Store, files: &[HashId]) -> hydrus_store::Result<()> {
    let files = files.to_vec();
    store.write_content(move |w| {
        let trash = w.roles().trash;
        w.undelete_files(trash, &files)
    })
}

/// The title of the chooser undeleting asks with when the files were
/// deleted from more than one local domain.
pub const UNDELETE_CHOOSER_TITLE: &str = "Undelete for?";

/// Where undeleting files can restore them: the local file domains they were
/// deleted from, sorted by name as the reference sorts its services
/// (`UndeleteMedia`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undeletion {
    /// The domains, by name.
    pub domains: Vec<(ServiceId, String)>,
    /// The umbrella domain "all the above" undeletes to.
    all: ServiceId,
}

impl Undeletion {
    /// The single domain, when there is no choice to make.
    pub fn only(&self) -> Option<ServiceId> {
        match self.domains[..] {
            [(domain, _)] => Some(domain),
            _ => None,
        }
    }

    /// The chooser's buttons when there is a choice: each domain, then
    /// "all the above".
    pub fn choices(&self) -> Vec<String> {
        let mut choices: Vec<String> = self.domains.iter().map(|(_, n)| n.clone()).collect();
        choices.push("all the above".to_owned());
        choices
    }

    /// The domain the chooser's `index`th button undeletes to.
    pub fn chosen(&self, index: usize) -> Option<ServiceId> {
        match index.cmp(&self.domains.len()) {
            std::cmp::Ordering::Less => Some(self.domains[index].0),
            std::cmp::Ordering::Equal => Some(self.all),
            std::cmp::Ordering::Greater => None,
        }
    }

    /// The yes/no question asked before undeleting to the only domain, when
    /// "Confirm sending files to trash" is on (the reference words it for one
    /// file whatever the count).
    pub fn question(&self, store: &Store) -> Option<String> {
        let [(_, name)] = &self.domains[..] else {
            return None;
        };
        store
            .read(hydrus_store::settings::get::<hydrus_store::settings::DeletionPreferences>)
            .unwrap_or_default()
            .confirm_trash
            .then(|| format!("Undelete this file back to {name}?"))
    }
}

/// What undeleting `files` can do; `None` when none of them is still stored
/// locally or none was deleted from a local domain.
pub fn undeletion(store: &Store, files: &[HashId]) -> Option<Undeletion> {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services).ok()?;
    let batch = store
        .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))
        .ok()?;
    let deleted: BTreeSet<ServiceId> = batch
        .results
        .iter()
        .filter(|m| m.is_current_in(roles.local_file_storage))
        .flat_map(|m| m.deleted.iter().map(|d| d.service))
        .filter(|service| roles.local.contains(service))
        .collect();
    let mut domains: Vec<(ServiceId, String)> = deleted
        .into_iter()
        .filter_map(|id| snapshot.services.get(id).ok().map(|s| (id, s.name.clone())))
        .collect();
    domains.sort_by_key(|(_, name)| name.to_lowercase());
    (!domains.is_empty()).then_some(Undeletion {
        domains,
        all: roles.combined_local_media,
    })
}

/// Restore those of `files` deleted from `domain` (every local domain they
/// were deleted from, for the umbrella domain; as the reference filters, only
/// the files that were deleted from it, which for the umbrella is those no
/// local domain holds now).
pub fn undelete_to(store: &Store, files: &[HashId], domain: ServiceId) -> hydrus_store::Result<()> {
    let snapshot = store.snapshot();
    let roles = DomainRoles::new(&snapshot.services)?;
    let batch =
        store.read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))?;
    let wanted: Vec<HashId> = batch
        .results
        .iter()
        .filter(|m| {
            if domain == roles.combined_local_media {
                !roles.local.iter().any(|d| m.is_current_in(*d))
                    && roles.local.iter().any(|d| m.is_deleted_from(*d))
            } else {
                m.is_deleted_from(domain)
            }
        })
        .map(|m| m.hash_id)
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    store.write_content(move |w| w.undelete_files(domain, &wanted))
}

/// Whether `files` are still somewhere `location` searches (a file
/// deleted from a page's domain leaves the page, as in the reference).
pub fn still_in(store: &Store, location: &LocationContext, files: &[HashId]) -> Vec<HashId> {
    let snapshot = store.snapshot();
    let wanted: BTreeSet<ServiceId> = location
        .current()
        .iter()
        .filter_map(|key| snapshot.services.by_key(key).ok().map(|s| s.id))
        .collect();
    let Ok(batch) =
        store.read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))
    else {
        return files.to_vec();
    };
    batch
        .results
        .iter()
        .filter(|m| m.current.iter().any(|c| wanted.contains(&c.service)))
        .map(|m| m.hash_id)
        .collect()
}

/// Of `files`, those in the inbox, and those archived (in the client), as
/// F7 and shift+F7 act on them.
pub fn by_inbox(store: &Store, files: &[HashId]) -> (Vec<HashId>, Vec<HashId>) {
    let snapshot = store.snapshot();
    let Ok(roles) = DomainRoles::new(&snapshot.services) else {
        return (Vec::new(), Vec::new());
    };
    let Ok(batch) =
        store.read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, files))
    else {
        return (Vec::new(), Vec::new());
    };
    let (mut inbox, mut archived) = (Vec::new(), Vec::new());
    for media in &batch.results {
        if media.inbox {
            inbox.push(media.hash_id);
        } else if media.is_current_in(roles.local_file_storage) {
            archived.push(media.hash_id);
        }
    }
    (inbox, archived)
}
