//! Reference MediaList view pruning for successful file-domain content changes.
use hydrus_core::{HashId, ServiceId};
use hydrus_search::LocationContext;
use hydrus_store::{
    Store,
    content::DomainRoles,
    settings::{self, FileViewRemoval},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Deleted(ServiceId),
    Moved(ServiceId),
}
/// Actual trash membership intersects possible trash updates; moving never means trash.
/// Physical deletion always prunes wholly-current local views independently of preferences.
pub fn removed(
    store: &Store,
    location: &LocationContext,
    files: &[HashId],
    change: Change,
) -> Vec<HashId> {
    let snapshot = store.snapshot();
    let Ok(roles) = DomainRoles::new(&snapshot.services) else {
        return Vec::new();
    };
    let prefs: FileViewRemoval = store.read(settings::get).unwrap_or_default();
    let current = location
        .current()
        .iter()
        .filter_map(|key| snapshot.services.by_key(key).ok().map(|s| s.id))
        .collect::<std::collections::BTreeSet<_>>();
    let wholly_local = !current.is_empty()
        && current.len() == location.current().len()
        && location.deleted().is_empty()
        && current.iter().all(|id| {
            roles.local.contains(id)
                || [
                    roles.trash,
                    roles.combined_local_media,
                    roles.local_file_storage,
                    roles.local_updates,
                ]
                .contains(id)
        });
    let looking_at_trash =
        current.len() == 1 && current.contains(&roles.trash) && location.deleted().is_empty();
    match change {
        Change::Moved(source) => {
            if prefs.moved && current.contains(&source) {
                files.to_vec()
            } else {
                Vec::new()
            }
        }
        Change::Deleted(domain) => {
            let physical = domain == roles.local_file_storage && wholly_local;
            let repository = !roles.local.contains(&domain)
                && ![
                    roles.trash,
                    roles.combined_local_media,
                    roles.local_file_storage,
                    roles.local_updates,
                ]
                .contains(&domain)
                && current.len() == 1
                && current.contains(&domain)
                && location.deleted().is_empty();
            let possible_trash = prefs.trashed
                && !looking_at_trash
                && (roles.local.contains(&domain) || domain == roles.combined_local_media);
            if !(physical || repository || possible_trash) {
                return Vec::new();
            }
            if possible_trash {
                let actual = store
                    .read(|conn| hydrus_store::media::current_domains(conn, files))
                    .unwrap_or_default();
                files
                    .iter()
                    .copied()
                    .filter(|file| {
                        actual
                            .get(file)
                            .is_some_and(|domains| domains.contains(&roles.trash))
                    })
                    .collect()
            } else {
                files.to_vec()
            }
        }
    }
}
/// Successful native deletion adapters report the same domain change to the owning page.
pub fn deleted(
    store: &Store,
    location: &LocationContext,
    files: &[HashId],
    deletion: &crate::media_actions::Deletion,
) -> Vec<HashId> {
    let Ok(roles) = DomainRoles::new(&store.snapshot().services) else {
        return Vec::new();
    };
    let domain = match deletion {
        crate::media_actions::Deletion::FromDomain { domain, .. } => *domain,
        crate::media_actions::Deletion::ToTrash => roles.combined_local_media,
        crate::media_actions::Deletion::Physically => roles.local_file_storage,
    };
    removed(store, location, files, Change::Deleted(domain))
}
/// Advanced deletion reports its actionable choice, not unrelated remote/locked selections.
pub fn advanced(
    store: &Store,
    location: &LocationContext,
    choice: &crate::delete_files::Choice,
) -> Vec<HashId> {
    use hydrus_store::settings::DeletionAction;
    let snapshot = store.snapshot();
    let Ok(roles) = DomainRoles::new(&snapshot.services) else {
        return Vec::new();
    };
    match &choice.action {
        DeletionAction::Domain(key) => snapshot
            .services
            .by_key(key)
            .ok()
            .map_or_else(Vec::new, |domain| {
                removed(store, location, &choice.files, Change::Deleted(domain.id))
            }),
        DeletionAction::Physical | DeletionAction::ClearRecord => {
            let Ok(current) =
                store.read(|conn| hydrus_store::media::current_domains(conn, &choice.files))
            else {
                return Vec::new();
            };
            let physical = choice
                .files
                .iter()
                .copied()
                .filter(|file| {
                    current
                        .get(file)
                        .is_none_or(|domains| !domains.contains(&roles.local_file_storage))
                })
                .collect::<Vec<_>>();
            removed(
                store,
                location,
                &physical,
                Change::Deleted(roles.local_file_storage),
            )
        }
    }
}
