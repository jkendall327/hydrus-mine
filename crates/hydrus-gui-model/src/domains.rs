//! A search page's file and tag domain buttons (the reference's
//! `LocationSearchContextButton` and `TagContextButton`): their labels,
//! the menus they open, the "multiple/deleted locations" list, and what
//! choosing a domain does to the other (a search of every tag service
//! doesn't search all known files).

use std::collections::BTreeSet;

use hydrus_core::ServiceKey;
use hydrus_core::numbers::human_int;
use hydrus_core::service::{ServiceType, builtin_keys};
use hydrus_search::{LocationContext, TagContext};
use hydrus_store::services::ServiceRegistry as Services;

/// What a domain menu's entry chooses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Search these file domains.
    Location(LocationContext),
    /// Open the "multiple/deleted locations" list.
    Multiple,
    /// Search this tag service.
    Tags(ServiceKey),
}

/// A domain menu's entry: its label, whether it is ticked, and what it
/// chooses (none: a separator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub checked: bool,
    pub choice: Choice,
}

/// Services of these types, each type's by name (`GetServices`, which
/// sorts by name without case).
pub fn in_order(
    services: &Services,
    types: &[ServiceType],
) -> Vec<(ServiceKey, String, ServiceType)> {
    let mut out = Vec::new();
    for &service_type in types {
        let mut found: Vec<(ServiceKey, String, ServiceType)> = services
            .of_type(service_type)
            .map(|s| (s.key.clone(), s.name.clone(), service_type))
            .collect();
        found.sort_by_key(|(_, name, _)| name.to_lowercase());
        out.extend(found);
    }
    out
}

/// What a caller lets the file domain button offer (the reference's
/// `SetOnlyImportableDomainsAllowed`, `SetOnlyLocalFileDomainsAllowed`,
/// `SetOnlyCombinedLocalFileDomainsAllowed`, `SetAllKnownFilesAllowed`
/// and `SetMultipleFileDomainsAllowed`), and whether advanced mode is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    pub advanced: bool,
    pub all_known_files_allowed: bool,
    pub only_importable: bool,
    pub only_local: bool,
    pub only_combined_local: bool,
    pub multiple_allowed: bool,
    /// Paired with a tag domain button, so all known files read "all known
    /// files with tags" in the menu.
    pub paired_with_tag_domain: bool,
}

impl Flags {
    /// A search page's button: everything, all known files in advanced mode.
    pub fn search(advanced: bool) -> Self {
        Self {
            advanced,
            all_known_files_allowed: advanced,
            only_importable: false,
            only_local: false,
            only_combined_local: false,
            multiple_allowed: true,
            paired_with_tag_domain: true,
        }
    }

    /// The import destination button: the domains files can be imported to.
    pub fn importable(advanced: bool) -> Self {
        Self {
            only_importable: true,
            paired_with_tag_domain: false,
            ..Self::search(advanced)
        }
    }

    /// The presentation location button: the button with no restrictions
    /// (all known files allowed).
    pub fn unrestricted(advanced: bool) -> Self {
        Self {
            all_known_files_allowed: true,
            paired_with_tag_domain: false,
            ..Self::search(advanced)
        }
    }

    /// Whether the multiple list offers "deleted from" boxes (advanced
    /// mode, with no `only_` restriction).
    fn offers_deleted(self) -> bool {
        self.advanced && !(self.only_local || self.only_importable || self.only_combined_local)
    }
}

/// The file domains a search page's button offers, in order
/// (`GetPossibleFileDomainServicesInOrder`): the local file domains, all of
/// them together when there are several (or in advanced mode), the trash,
/// in advanced mode the repository updates, everything stored here and
/// everything deleted, the file repositories, and in advanced mode all
/// known files.
pub fn file_domains(services: &Services, advanced: bool) -> Vec<(ServiceKey, String, ServiceType)> {
    file_domains_for(services, Flags::search(advanced))
}

/// The file domains a button with these `flags` offers.
pub fn file_domains_for(
    services: &Services,
    flags: Flags,
) -> Vec<(ServiceKey, String, ServiceType)> {
    let mut types = vec![ServiceType::LocalFileDomain];
    if !flags.only_importable {
        if services.of_type(ServiceType::LocalFileDomain).count() > 1 || flags.advanced {
            types.push(ServiceType::CombinedLocalFileDomains);
        }
        if !flags.only_combined_local {
            types.push(ServiceType::LocalFileTrashDomain);
            if flags.advanced {
                types.extend([
                    ServiceType::LocalFileUpdateDomain,
                    ServiceType::HydrusLocalFileStorage,
                ]);
            }
            if !flags.only_local {
                if flags.advanced {
                    types.push(ServiceType::CombinedDeletedFile);
                }
                types.extend([ServiceType::FileRepository, ServiceType::Ipfs]);
                if flags.all_known_files_allowed {
                    types.push(ServiceType::CombinedFile);
                }
            }
        }
    }
    in_order(services, &types)
}

fn name(services: &Services, key: &ServiceKey) -> String {
    services
        .by_key(key)
        .map_or_else(|_| "unknown service".to_owned(), |s| s.name.clone())
}

/// The file domain button's label (`LocationContext.ToString`, and "all
/// known files with tags" for all known files, the only files a search of
/// it can find).
pub fn location_label(services: &Services, location: &LocationContext) -> String {
    if location.is_all_known_files() {
        return "all known files with tags".to_owned();
    }
    let (current, deleted) = (location.current(), location.deleted());
    if current.is_empty() && deleted.is_empty() {
        return "nothing".to_owned();
    }
    let prefix = match (!current.is_empty(), !deleted.is_empty()) {
        (true, true) if current == deleted => "current and deleted files of ",
        (true, true) => "a mix of current and deleted files of ",
        (false, true) => "deleted files of ",
        _ => "",
    };
    let keys: BTreeSet<&ServiceKey> = current.iter().chain(deleted).collect();
    let services_text = if keys.len() <= 2 {
        let mut names: Vec<String> = keys.iter().map(|k| name(services, k)).collect();
        names.sort();
        names.join(", ")
    } else {
        format!("{} services", human_int(keys.len() as u64))
    };
    format!("{prefix}{services_text}")
}

/// The tag domain button's label: the tag service's name.
pub fn tag_label(services: &Services, tags: &TagContext) -> String {
    name(services, &tags.service)
}

/// The file domain button's menu, from the page's domains now
/// (`_EditLocation`): each domain, ticked if it is the one searched,
/// separated by kind, "all files ever imported or deleted" after the
/// trash, and "multiple/deleted locations", ticked if none other is.
pub fn location_menu(
    services: &Services,
    advanced: bool,
    current: &LocationContext,
) -> Vec<Option<Row>> {
    location_menu_for(services, Flags::search(advanced), current)
}

/// The file domain button's menu for a caller's `flags`.
pub fn location_menu_for(
    services: &Services,
    flags: Flags,
    current: &LocationContext,
) -> Vec<Option<Row>> {
    let mut rows = Vec::new();
    let mut last_type = None;
    let mut checked_any = false;
    let mut push = |rows: &mut Vec<Option<Row>>, label: String, location: LocationContext| {
        let checked = location == *current;
        checked_any |= checked;
        rows.push(Some(Row {
            label,
            checked,
            choice: Choice::Location(location),
        }));
    };
    for (key, service_name, service_type) in file_domains_for(services, flags) {
        if last_type.is_some_and(|t| t != service_type) {
            rows.push(None);
        }
        let label = if service_type == ServiceType::CombinedFile && flags.paired_with_tag_domain {
            "all known files with tags".to_owned()
        } else {
            service_name
        };
        push(&mut rows, label, LocationContext::single(key));
        last_type = Some(service_type);
        if service_type == ServiceType::LocalFileTrashDomain {
            rows.push(None);
            let all_mine = ServiceKey::new(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec());
            push(
                &mut rows,
                "all files ever imported or deleted".to_owned(),
                LocationContext::new([all_mine.clone()], [all_mine]),
            );
        }
    }
    if flags.multiple_allowed {
        rows.push(None);
        rows.push(Some(Row {
            label: "multiple/deleted locations".to_owned(),
            checked: !checked_any,
            choice: Choice::Multiple,
        }));
    }
    rows
}

/// The tag domain button's menu (`TagContextButton._Edit`): the local tag
/// services, the tag repositories and every tag service, separated by
/// kind, the one searched ticked.
pub fn tag_menu(services: &Services, current: &TagContext) -> Vec<Option<Row>> {
    let mut rows = Vec::new();
    let mut last_type = None;
    let types = [
        ServiceType::LocalTag,
        ServiceType::TagRepository,
        ServiceType::CombinedTag,
    ];
    for (key, label, service_type) in in_order(services, &types) {
        if last_type.is_some_and(|t| t != service_type) {
            rows.push(None);
        }
        rows.push(Some(Row {
            label,
            checked: key == current.service,
            choice: Choice::Tags(key),
        }));
        last_type = Some(service_type);
    }
    rows
}

/// One of the "multiple/deleted locations" list's tick boxes: a domain's
/// current files, or (in advanced mode) its deleted ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tick {
    pub label: String,
    pub deleted: bool,
    pub service: ServiceKey,
}

/// The "multiple/deleted locations" list (`EditMultipleLocationContextPanel`):
/// each domain the menu offers, then in advanced mode "deleted from" each
/// that keeps a record of what it deleted.
pub fn multiple_ticks(services: &Services, advanced: bool) -> Vec<Tick> {
    multiple_ticks_for(services, Flags::search(advanced))
}

/// The "multiple/deleted locations" list for a caller's `flags`: no
/// "deleted from" boxes when the caller restricts the domains.
pub fn multiple_ticks_for(services: &Services, flags: Flags) -> Vec<Tick> {
    let advanced = flags.offers_deleted();
    let domains = file_domains_for(services, flags);
    let mut ticks: Vec<Tick> = domains
        .iter()
        .map(|(key, name, _)| Tick {
            label: name.clone(),
            deleted: false,
            service: key.clone(),
        })
        .collect();
    if advanced {
        ticks.extend(
            domains
                .iter()
                .filter(|(_, _, t)| {
                    !matches!(
                        t,
                        ServiceType::CombinedFile
                            | ServiceType::LocalFileTrashDomain
                            | ServiceType::CombinedDeletedFile
                    )
                })
                .map(|(key, name, _)| Tick {
                    label: format!("deleted from {name}"),
                    deleted: true,
                    service: key.clone(),
                }),
        );
    }
    ticks
}

/// What the ticked boxes search, without the domains another ticked one
/// covers (`ClearSurplusLocalFilesServices`): everything stored here
/// covers the local file domains, all of them together and the trash and
/// repository updates; all of them together covers each.
pub fn ticked_location(services: &Services, ticked: &[(bool, ServiceKey)]) -> LocationContext {
    let service_type = |key: &ServiceKey| services.by_key(key).ok().map(|s| s.service_type());
    let clear = |keys: BTreeSet<ServiceKey>| -> BTreeSet<ServiceKey> {
        let storage = ServiceKey::new(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec());
        let all_mine = ServiceKey::new(builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec());
        let mut keys = keys;
        if keys.contains(&storage) {
            keys.retain(|k| {
                !matches!(
                    service_type(k),
                    Some(
                        ServiceType::CombinedLocalFileDomains
                            | ServiceType::LocalFileDomain
                            | ServiceType::LocalFileUpdateDomain
                            | ServiceType::LocalFileTrashDomain
                    )
                )
            });
        }
        if keys.contains(&all_mine) {
            keys.retain(|k| service_type(k) != Some(ServiceType::LocalFileDomain));
        }
        keys
    };
    let exists = |key: &&ServiceKey| services.by_key(key).is_ok();
    let current = ticked
        .iter()
        .filter(|(deleted, _)| !deleted)
        .map(|(_, k)| k)
        .filter(exists)
        .cloned()
        .collect();
    let deleted = ticked
        .iter()
        .filter(|(deleted, _)| *deleted)
        .map(|(_, k)| k)
        .filter(exists)
        .cloned()
        .collect();
    LocationContext::new(clear(current), clear(deleted))
}

/// A page's file and tag domains, changed as the reference's autocomplete
/// changes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domains {
    pub location: LocationContext,
    pub tags: TagContext,
}

impl Domains {
    /// Search `location` (`_LocationContextJustChanged`): all known files
    /// can't be searched with every tag service, so the tags move to the
    /// first local tag service.
    pub fn choose_location(&mut self, services: &Services, location: LocationContext) {
        if location.is_all_known_files()
            && self.tags.is_all_known_tags()
            && let Some((key, _, _)) = in_order(services, &[ServiceType::LocalTag])
                .into_iter()
                .next()
        {
            self.tags.service = key;
        }
        self.location = location;
    }

    /// Search the tag service `service` (`_TagContextJustChanged`): every
    /// tag service can't search all known files, so the files move to the
    /// options' default local domain.
    pub fn choose_tags(&mut self, service: ServiceKey, default_location: &LocationContext) {
        self.tags.service = service;
        if self.tags.is_all_known_tags() && self.location.is_all_known_files() {
            self.location = default_location.clone();
        }
    }
}

/// A domain menu as the menus draw it: each entry a tick box choosing its
/// domain.
pub fn entries(rows: Vec<Option<Row>>) -> Vec<crate::main_menu::Entry> {
    use crate::main_menu::{Command, Entry};
    rows.into_iter()
        .map(|row| match row {
            None => Entry::Separator,
            Some(Row {
                label,
                checked,
                choice,
            }) => Entry::Check {
                label,
                command: Some(Command::SearchDomain(choice)),
                checked,
            },
        })
        .collect()
}

/// A location context from the hex service keys import options store,
/// without the services that no longer exist (the reference's
/// `FixMissingServices`, which every location button does on `SetValue`).
pub fn context_from_hex(
    services: &Services,
    current: &[String],
    deleted: &[String],
) -> LocationContext {
    let keys = |list: &[String]| -> Vec<ServiceKey> {
        list.iter()
            .filter_map(|k| hex::decode(k).ok())
            .map(ServiceKey::new)
            .filter(|k| services.by_key(k).is_ok())
            .collect()
    };
    LocationContext::new(keys(current), keys(deleted))
}

/// A location context as the hex service keys import options store:
/// (current, deleted).
pub fn hex_of_context(location: &LocationContext) -> (Vec<String>, Vec<String>) {
    let hexed =
        |keys: &BTreeSet<ServiceKey>| keys.iter().map(|k| hex::encode(k.as_bytes())).collect();
    (hexed(location.current()), hexed(location.deleted()))
}

/// A location button as a drop-down: the menu's entries (no separators),
/// and the ticked one ("multiple/deleted locations" if no domain is).
pub fn dropdown(
    services: &Services,
    flags: Flags,
    current: &LocationContext,
) -> (Vec<Row>, Option<usize>) {
    let rows: Vec<Row> = location_menu_for(services, flags, current)
        .into_iter()
        .flatten()
        .collect();
    let at = rows.iter().position(|r| r.checked);
    (rows, at)
}
