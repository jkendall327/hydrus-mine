//! The thumbnails' right-click menu, as the reference builds it (v688's
//! default grid's `GetMenu`), so far with the entries hydrus-rs can act on:
//! refresh; select and remove, by inbox and archive, file domain, client
//! and selection, with counts; the archive/delete filter; archive and
//! re-inbox; deleting from each local file domain, deleting physically and
//! undeleting; manage → tags; and open → in a new page. Plain Rust, tested
//! against the reference.

use std::collections::{BTreeSet, HashSet};

use hydrus_core::numbers::human_int;
use hydrus_core::{HashId, ServiceId, ServiceType};
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use hydrus_store::services::ServiceRegistry;

/// An entry: an item, a submenu, or a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Item(String, Action),
    Menu(String, Vec<Entry>),
    Separator,
}

/// Which files a select or remove takes (the reference's `FileFilter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Filter {
    All,
    Selected,
    NotSelected,
    None,
    Inbox,
    Archive,
    Domain(ServiceId),
    Local,
    Remote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Refresh,
    Select(Filter),
    Remove(Filter),
    ArchiveDeleteFilter,
    Archive,
    Inbox,
    DeleteFrom(ServiceId),
    /// Delete the selected files in the trash for good.
    DeleteTrashPhysically,
    DeletePhysically,
    Undelete,
    ManageTags,
    OpenInNewPage,
}

/// What the menu knows of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    pub file: HashId,
    pub inbox: bool,
    /// The file domains it is current in.
    pub current: Vec<ServiceId>,
}

/// The facts of `files`, in their order.
pub fn facts(store: &Store, files: &[HashId]) -> Vec<FileFacts> {
    let read = store.read(|conn| {
        Ok((
            hydrus_store::media::current_domains(conn, files)?,
            hydrus_store::media::inboxed(conn, files)?,
        ))
    });
    let Ok((mut current, inbox)) = read else {
        return Vec::new();
    };
    files
        .iter()
        .map(|&file| FileFacts {
            file,
            inbox: inbox.contains(&file),
            current: current.remove(&file).unwrap_or_default(),
        })
        .collect()
}

/// Add a separator, unless the menu is empty or ends with one
/// (`ClientGUIMenus.AppendSeparator`).
fn separate(entries: &mut Vec<Entry>) {
    if entries.last().is_some_and(|e| *e != Entry::Separator) {
        entries.push(Entry::Separator);
    }
}

/// The menu for a page of `files` (in its order) with `selected` selected.
#[allow(clippy::too_many_lines)]
pub fn menu(
    services: &ServiceRegistry,
    files: &[FileFacts],
    selected: &HashSet<HashId>,
) -> Vec<Entry> {
    let Ok(roles) = DomainRoles::new(services) else {
        return vec![Entry::Item("refresh".into(), Action::Refresh)];
    };
    let is_local = |f: &FileFacts| f.current.contains(&roles.local_file_storage);
    let is_trashed = |f: &FileFacts| f.current.contains(&roles.trash);
    let chosen: Vec<&FileFacts> = files
        .iter()
        .filter(|f| selected.contains(&f.file))
        .collect();
    let (num_files, num_selected) = (files.len(), chosen.len());
    let num_inbox = files.iter().filter(|f| f.inbox).count();
    let multiple = num_selected > 1;
    let phrase = |one: &str, several: &str| if multiple { several } else { one }.to_owned();

    let mut entries = Vec::new();
    entries.push(Entry::Item("refresh".into(), Action::Refresh));
    if num_files > 0 {
        separate(&mut entries);
        let count = |filter: Filter| -> usize {
            match filter {
                Filter::All => num_files,
                Filter::Selected => num_selected,
                Filter::NotSelected => num_files - num_selected,
                Filter::None => 0,
                Filter::Inbox => num_inbox,
                Filter::Archive => num_files - num_inbox,
                Filter::Domain(d) => files.iter().filter(|f| f.current.contains(&d)).count(),
                Filter::Local => files.iter().filter(|f| is_local(f)).count(),
                Filter::Remote => files.iter().filter(|f| !is_local(f)).count(),
            }
        };
        let label = |filter: Filter| -> String {
            let name = match filter {
                Filter::All => "all".to_owned(),
                Filter::Selected => "selected".to_owned(),
                Filter::NotSelected => "not selected".to_owned(),
                Filter::None => "none".to_owned(),
                Filter::Inbox => "inbox".to_owned(),
                Filter::Archive => "archive".to_owned(),
                Filter::Domain(d) => services.get(d).map(|s| s.name.clone()).unwrap_or_default(),
                Filter::Local => "local".to_owned(),
                Filter::Remote => "not local".to_owned(),
            };
            let n = count(filter);
            let mut s = format!("{name} ({})", human_int(n as u64));
            if filter == Filter::All {
                if num_inbox > 0 && num_inbox == n {
                    s += " (all in inbox)";
                } else if n - num_inbox > 0 && n - num_inbox == n {
                    s += " (all in archive)";
                }
            }
            s
        };
        // (as the reference: whether to list them is asked before the
        // redundant ones go, so one domain can be listed alone)
        let (domain_count, domains) = specific_domains(services, &roles, files);
        let has_local_and_remote =
            files.iter().any(&is_local) && files.iter().any(|f| !is_local(f));
        let both = num_inbox > 0 && num_files - num_inbox > 0;

        // select (`AddSelectMenu`)
        let mut select = Vec::new();
        let item =
            |filter: Filter, make: fn(Filter) -> Action| Entry::Item(label(filter), make(filter));
        select.push(item(Filter::All, Action::Select));
        if both {
            separate(&mut select);
            select.push(item(Filter::Inbox, Action::Select));
            select.push(item(Filter::Archive, Action::Select));
        }
        if domain_count > 1 {
            separate(&mut select);
            for &d in &domains {
                select.push(item(Filter::Domain(d), Action::Select));
            }
        }
        if has_local_and_remote {
            separate(&mut select);
            select.push(item(Filter::Local, Action::Select));
            select.push(item(Filter::Remote, Action::Select));
        }
        if num_selected > 0 {
            if num_files - num_selected > 0 {
                separate(&mut select);
                select.push(item(Filter::NotSelected, Action::Select));
            }
            separate(&mut select);
            select.push(item(Filter::None, Action::Select));
        }
        entries.push(Entry::Menu("select".into(), select));

        // remove (`AddRemoveMenu`)
        let mut remove = Vec::new();
        if 0 < num_selected && num_selected < num_files {
            remove.push(item(Filter::Selected, Action::Remove));
        }
        separate(&mut remove);
        remove.push(item(Filter::All, Action::Remove));
        if both {
            separate(&mut remove);
            remove.push(item(Filter::Inbox, Action::Remove));
            remove.push(item(Filter::Archive, Action::Remove));
        }
        if domain_count > 1 {
            separate(&mut remove);
            for &d in &domains {
                remove.push(item(Filter::Domain(d), Action::Remove));
            }
        }
        if has_local_and_remote {
            separate(&mut remove);
            remove.push(item(Filter::Local, Action::Remove));
            remove.push(item(Filter::Remote, Action::Remove));
        }
        if num_files - num_selected > 0 && num_selected > 0 {
            separate(&mut remove);
            remove.push(item(Filter::NotSelected, Action::Remove));
        }
        entries.push(Entry::Menu("remove".into(), remove));

        separate(&mut entries);
        if files.iter().any(&is_local) {
            entries.push(Entry::Item(
                "archive/delete filter".into(),
                Action::ArchiveDeleteFilter,
            ));
        }
    }
    if chosen.iter().any(|f| f.inbox) {
        entries.push(Entry::Item(
            phrase("archive", "archive selected"),
            Action::Archive,
        ));
    }
    if chosen.iter().any(|f| !f.inbox && is_local(f)) {
        entries.push(Entry::Item(
            phrase("re-inbox", "re-inbox selected"),
            Action::Inbox,
        ));
    }
    separate(&mut entries);

    // deleting
    let deletable: BTreeSet<ServiceId> = services
        .all()
        .filter(|s| {
            matches!(
                s.service_type(),
                ServiceType::LocalFileDomain | ServiceType::LocalFileUpdateDomain
            )
        })
        .map(|s| s.id)
        .collect();
    let mut we_are_in: Vec<ServiceId> = chosen
        .iter()
        .flat_map(|f| f.current.iter().copied())
        .filter(|d| deletable.contains(d))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let name = |d: ServiceId| services.get(d).map(|s| s.name.clone()).unwrap_or_default();
    we_are_in.sort_by_key(|&d| name(d));
    match &we_are_in[..] {
        [] => {}
        [d] => entries.push(Entry::Item(
            format!("delete from {}", name(*d)),
            Action::DeleteFrom(*d),
        )),
        several => entries.push(Entry::Menu(
            phrase("delete", "delete selected"),
            several
                .iter()
                .map(|&d| Entry::Item(format!("from {}", name(d)), Action::DeleteFrom(d)))
                .collect(),
        )),
    }
    if chosen.iter().any(|f| is_trashed(f)) {
        if chosen.iter().any(|f| is_local(f) && !is_trashed(f)) {
            entries.push(Entry::Item(
                "delete trash physically now".into(),
                Action::DeleteTrashPhysically,
            ));
        }
        entries.push(Entry::Item(
            phrase("delete physically now", "delete selected physically now"),
            Action::DeletePhysically,
        ));
        entries.push(Entry::Item(
            phrase("undelete", "undelete selected"),
            Action::Undelete,
        ));
    }
    separate(&mut entries);
    if num_selected > 0 {
        entries.push(Entry::Menu(
            "manage".into(),
            vec![Entry::Item("tags".into(), Action::ManageTags)],
        ));
        entries.push(Entry::Menu(
            "open".into(),
            vec![Entry::Item("in a new page".into(), Action::OpenInNewPage)],
        ));
    }
    entries
}

/// How many specific file domains the page's files are in, and those the
/// select and remove menus list (`SortFileServiceKeysNicely`, then
/// `FilterOutRedundantMetaServices`; not in advanced mode).
fn specific_domains(
    services: &ServiceRegistry,
    roles: &DomainRoles,
    files: &[FileFacts],
) -> (usize, Vec<ServiceId>) {
    let mut present: HashSet<ServiceId> = files
        .iter()
        .flat_map(|f| f.current.iter().copied())
        .collect();
    present.remove(&roles.local_file_storage);
    if let Some(all_known) = roles.all_known_files {
        present.remove(&all_known);
    }
    let local_domains = services.of_type(ServiceType::LocalFileDomain).count();
    let mut types = vec![ServiceType::LocalFileDomain];
    if local_domains > 1 {
        types.push(ServiceType::CombinedLocalFileDomains);
    }
    types.extend([
        ServiceType::LocalFileTrashDomain,
        ServiceType::FileRepository,
        ServiceType::Ipfs,
    ]);
    let mut ordered = Vec::new();
    for t in types {
        let mut of_type: Vec<_> = services.of_type(t).collect();
        of_type.sort_by_key(|s| s.name.to_lowercase());
        ordered.extend(
            of_type
                .into_iter()
                .map(|s| s.id)
                .filter(|id| present.contains(id)),
        );
    }
    // (the combined local domain says nothing with one local domain among them)
    let locals = ordered
        .iter()
        .filter(|&&d| {
            services
                .get(d)
                .is_ok_and(|s| s.service_type() == ServiceType::LocalFileDomain)
        })
        .count();
    if locals <= 1 {
        ordered.retain(|&d| d != roles.combined_local_media);
    }
    (present.len(), ordered)
}

/// The files `filter` takes, of a page of `files` with `selected` selected.
pub fn matching(
    filter: Filter,
    files: &[FileFacts],
    selected: &HashSet<HashId>,
    roles: &DomainRoles,
) -> Vec<HashId> {
    let takes = |f: &FileFacts| match filter {
        Filter::All => true,
        Filter::Selected => selected.contains(&f.file),
        Filter::NotSelected => !selected.contains(&f.file),
        Filter::None => false,
        Filter::Inbox => f.inbox,
        Filter::Archive => !f.inbox,
        Filter::Domain(d) => f.current.contains(&d),
        Filter::Local => f.current.contains(&roles.local_file_storage),
        Filter::Remote => !f.current.contains(&roles.local_file_storage),
    };
    files.iter().filter(|f| takes(f)).map(|f| f.file).collect()
}

/// An item of a slot: its label and action.
pub type SlotItem = (String, Action);

/// The menu laid into the window's fixed template (a menu's markup can't
/// mix items, submenus and separators from one list): the entries of each
/// part in order, a submenu's runs between separators as groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slots {
    pub head: Vec<SlotItem>,
    pub select: Vec<Vec<SlotItem>>,
    pub remove: Vec<Vec<SlotItem>>,
    /// The archive/delete filter, archiving and re-inboxing.
    pub filter: Vec<SlotItem>,
    /// Deleting from the one local domain the files are in.
    pub delete: Vec<SlotItem>,
    /// Or from several: the submenu's title and items.
    pub delete_menu: Option<(String, Vec<SlotItem>)>,
    /// Deleting physically and undeleting.
    pub trash: Vec<SlotItem>,
    pub manage: Vec<SlotItem>,
    pub open: Vec<SlotItem>,
}

/// The most groups the template has for a submenu.
pub const GROUPS: usize = 6;

impl Slots {
    pub fn new(entries: &[Entry]) -> Self {
        fn items(entries: &[Entry]) -> Vec<SlotItem> {
            entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Item(label, action) => Some((label.clone(), *action)),
                    _ => None,
                })
                .collect()
        }
        fn groups(entries: &[Entry]) -> Vec<Vec<SlotItem>> {
            let mut out: Vec<Vec<SlotItem>> = vec![Vec::new()];
            for e in entries {
                match e {
                    Entry::Separator => out.push(Vec::new()),
                    Entry::Item(label, action) => {
                        out.last_mut().expect("one").push((label.clone(), *action));
                    }
                    Entry::Menu(..) => {}
                }
            }
            out.retain(|g| !g.is_empty());
            out
        }
        let mut slots = Self::default();
        for e in entries {
            match e {
                Entry::Item(label, action) => {
                    let slot = match action {
                        Action::Refresh => &mut slots.head,
                        Action::DeleteFrom(_) => &mut slots.delete,
                        Action::DeleteTrashPhysically
                        | Action::DeletePhysically
                        | Action::Undelete => &mut slots.trash,
                        _ => &mut slots.filter,
                    };
                    slot.push((label.clone(), *action));
                }
                Entry::Menu(title, inner) => match title.as_str() {
                    "select" => slots.select = groups(inner),
                    "remove" => slots.remove = groups(inner),
                    "manage" => slots.manage = items(inner),
                    "open" => slots.open = items(inner),
                    _ => slots.delete_menu = Some((title.clone(), items(inner))),
                },
                Entry::Separator => {}
            }
        }
        slots
    }

    /// The menu the template shows.
    pub fn entries(&self) -> Vec<Entry> {
        let item = |(label, action): &SlotItem| Entry::Item(label.clone(), *action);
        let menu = |title: &str, groups: &[Vec<SlotItem>]| {
            let mut inner = Vec::new();
            for group in groups {
                separate(&mut inner);
                inner.extend(group.iter().map(item));
            }
            Entry::Menu(title.into(), inner)
        };
        let mut out = Vec::new();
        out.extend(self.head.iter().map(item));
        separate(&mut out);
        if !self.select.is_empty() {
            out.push(menu("select", &self.select));
        }
        if !self.remove.is_empty() {
            out.push(menu("remove", &self.remove));
        }
        separate(&mut out);
        out.extend(self.filter.iter().map(item));
        separate(&mut out);
        out.extend(self.delete.iter().map(item));
        if let Some((title, items)) = &self.delete_menu {
            out.push(Entry::Menu(title.clone(), items.iter().map(item).collect()));
        }
        out.extend(self.trash.iter().map(item));
        separate(&mut out);
        if !self.manage.is_empty() {
            out.push(Entry::Menu(
                "manage".into(),
                self.manage.iter().map(item).collect(),
            ));
        }
        if !self.open.is_empty() {
            out.push(Entry::Menu(
                "open".into(),
                self.open.iter().map(item).collect(),
            ));
        }
        out
    }
}
