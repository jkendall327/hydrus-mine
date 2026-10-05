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
    let files = files.to_vec();
    store.write_content(move |w| w.archive(&files))
}

/// Return files to the inbox.
pub fn inbox(store: &Store, files: &[HashId]) -> hydrus_store::Result<()> {
    let files = files.to_vec();
    store.write_content(move |w| w.inbox(&files))
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
