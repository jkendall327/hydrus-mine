//! The thumbnails' right-click menu, as the reference builds it (v688's
//! default grid's `GetMenu`), so far with the entries hydrus-rs can act on:
//! refresh; select and remove, by inbox and archive, file domain, client
//! and selection, with counts; the archive/delete filter; archive and
//! re-inbox; deleting from each local file domain, deleting physically and
//! undeleting; manage → tags; open → in a new page; and first, the
//! selection's info (its files' types and size, the focused file's info
//! lines, and how often they were viewed), whose lines copy themselves
//! when chosen. Plain Rust, tested against the reference.

use std::collections::{BTreeSet, HashSet};

use hydrus_core::numbers::human_int;
use hydrus_core::{HashId, ServiceId, ServiceType};
use hydrus_store::Store;
use hydrus_store::content::DomainRoles;
use hydrus_store::services::ServiceRegistry;

/// An entry: an item, a label (which copies itself when chosen), a
/// submenu, or a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Item(String, Action),
    Label(String),
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
    /// Copy the entry's text (a label's).
    Copy,
    /// Copy the selected local files' paths, hashes of a kind, or ids.
    CopyPaths,
    CopyHashes(HashKind),
    CopyFileIds,
    /// Copy the focused file's path, hash of a kind, or id.
    CopyPath,
    CopyHash(HashKind),
    CopyFileId,
}

/// A kind of hash the share menu copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashKind {
    Sha256,
    Md5,
    Sha1,
    Sha512,
    Blurhash,
    PixelHash,
}

impl HashKind {
    const DIGESTS: [HashKind; 3] = [HashKind::Md5, HashKind::Sha1, HashKind::Sha512];

    fn name(self) -> &'static str {
        match self {
            HashKind::Sha256 => "sha256",
            HashKind::Md5 => "md5",
            HashKind::Sha1 => "sha1",
            HashKind::Sha512 => "sha512",
            HashKind::Blurhash => "blurhash",
            HashKind::PixelHash => "pixel hash",
        }
    }
}

/// `files`' hashes of `kind` (in their order, those unknown left out).
pub fn hashes(store: &Store, files: &[HashId], kind: HashKind) -> Vec<String> {
    let hex = |b: &[u8]| {
        use std::fmt::Write as _;
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
    };
    let read = store.read(|c| {
        Ok((
            hydrus_store::media::load_basic(c, files)?,
            hydrus_store::media::digests(c, files)?,
        ))
    });
    let Ok((basic, digests)) = read else {
        return Vec::new();
    };
    files
        .iter()
        .filter_map(|f| {
            let media = basic.iter().find(|m| m.hash_id == *f)?;
            let info = media.info.as_ref();
            let digest = |i: usize| digests.get(f).and_then(|d| d[i].as_deref()).map(hex);
            match kind {
                HashKind::Sha256 => Some(media.hash.to_hex()),
                HashKind::Md5 => digest(0),
                HashKind::Sha1 => digest(1),
                HashKind::Sha512 => digest(2),
                HashKind::Blurhash => info.and_then(|i| i.blurhash.clone()),
                HashKind::PixelHash => info.and_then(|i| i.pixel_hash).map(|h| h.to_hex()),
            }
        })
        .collect()
}

/// The paths of those of `files` the client has.
pub fn paths(store: &Store, files: &[HashId]) -> Vec<String> {
    let snapshot = store.snapshot();
    let Ok(basic) = store.read(|c| hydrus_store::media::load_basic(c, files)) else {
        return Vec::new();
    };
    files
        .iter()
        .filter_map(|f| {
            let media = basic.iter().find(|m| m.hash_id == *f)?;
            let path = snapshot
                .storage
                .file_path(&media.hash, media.info.as_ref()?.mime)?;
            path.exists().then(|| path.display().to_string())
        })
        .collect()
}

/// The share menu (`AddShareMenu`), so far its copying of paths, hashes
/// and file ids: of the selection, when it is more than the focused file,
/// and of the focused file.
pub fn share_menu(
    store: &Store,
    files: &[FileFacts],
    focused: Option<HashId>,
    selected: &[HashId],
) -> Entry {
    let snapshot = store.snapshot();
    let local_storage = DomainRoles::new(&snapshot.services)
        .map(|r| r.local_file_storage)
        .ok();
    let is_local = |f: HashId| {
        files
            .iter()
            .any(|x| x.file == f && local_storage.is_some_and(|l| x.current.contains(&l)))
    };
    let local: Vec<HashId> = selected.iter().copied().filter(|&f| is_local(f)).collect();
    let more_than_focused = |of: &[HashId]| {
        !(of.is_empty() || of.len() == 1 && focused.is_some_and(|f| of.contains(&f)))
    };
    let mut entries = Vec::new();
    // (the reference's "export files" and "copy files" first, which
    // hydrus-rs doesn't have yet)
    if more_than_focused(&local) {
        entries.push(Entry::Item("copy paths".into(), Action::CopyPaths));
    }
    if more_than_focused(selected) {
        let mut copy = vec![Entry::Item(
            "sha256".into(),
            Action::CopyHashes(HashKind::Sha256),
        )];
        for kind in HashKind::DIGESTS {
            copy.push(Entry::Item(kind.name().into(), Action::CopyHashes(kind)));
        }
        let blurhashes = hashes(store, selected, HashKind::Blurhash).len();
        if blurhashes > 0 {
            copy.push(Entry::Item(
                format!("blurhash ({} hashes)", human_int(blurhashes as u64)),
                Action::CopyHashes(HashKind::Blurhash),
            ));
        }
        let pixel_hashes = hashes(store, selected, HashKind::PixelHash).len();
        if pixel_hashes > 0 {
            copy.push(Entry::Item(
                format!("pixel hashes ({} hashes)", human_int(pixel_hashes as u64)),
                Action::CopyHashes(HashKind::PixelHash),
            ));
        }
        entries.push(Entry::Menu("copy hashes".into(), copy));
        entries.push(Entry::Item("copy file ids".into(), Action::CopyFileIds));
        separate(&mut entries);
    }
    if let Some(file) = focused {
        if is_local(file) {
            entries.push(Entry::Item("copy path".into(), Action::CopyPath));
        }
        let one = |kind: HashKind| hashes(store, &[file], kind).pop();
        let mut copy = Vec::new();
        for kind in [
            HashKind::Sha256,
            HashKind::Md5,
            HashKind::Sha1,
            HashKind::Sha512,
        ] {
            let hash = one(kind).unwrap_or_else(|| "unknown".to_owned());
            copy.push(Entry::Item(
                format!("{} ({hash})", kind.name()),
                Action::CopyHash(kind),
            ));
        }
        for kind in [HashKind::Blurhash, HashKind::PixelHash] {
            if let Some(hash) = one(kind) {
                copy.push(Entry::Item(
                    format!("{} ({hash})", kind.name()),
                    Action::CopyHash(kind),
                ));
            }
        }
        entries.push(Entry::Menu("copy hash".into(), copy));
        entries.push(Entry::Item(
            format!("copy file id ({})", human_int(u64::from(file.get()))),
            Action::CopyFileId,
        ));
    }
    Entry::Menu("share".into(), entries)
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

/// The selection's info, first in the menu: a submenu of `selected`'s
/// types and size (and, for several, their duration), holding the
/// `focused` file's info lines when one is selected, and how often they
/// were viewed; or, with nothing in it, a label. `None` with nothing
/// selected.
pub fn info_menu(
    store: &Store,
    focused: Option<HashId>,
    selected: &[HashId],
    settings: &hydrus_core::media_viewer::InfoLineSettings,
    now_ms: i64,
) -> Option<Entry> {
    use crate::info_lines::InfoLine;
    fn rows(lines: Vec<InfoLine>) -> Vec<Entry> {
        lines
            .into_iter()
            .map(|line| match line.submenu {
                Some(sub) => Entry::Menu(line.text, rows(sub)),
                None => Entry::Label(line.text),
            })
            .collect()
    }
    if selected.is_empty() {
        return None;
    }
    let facts: Vec<crate::status::Facts> = crate::status::facts(store, selected)
        .into_iter()
        .map(|(_, f)| f)
        .collect();
    let mut label = format!(
        "{}, {}",
        crate::status::filetype_summary(&facts),
        crate::status::total_size(&facts)
    );
    let mut entries = Vec::new();
    let snapshot = store.snapshot();
    if selected.len() > 1 {
        if let Some(duration) = crate::status::total_duration(&facts) {
            label += &format!(", {duration}");
        }
    } else if let Some(file) = focused {
        // (the reference's "show detailed embedded file metadata" comes
        // first, which hydrus-rs doesn't have yet)
        if let Ok(mut batch) =
            store.read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
            && let Some(media) = batch.results.pop()
        {
            entries.extend(rows(crate::info_lines::info_lines(
                &media,
                &snapshot.services,
                settings,
                now_ms,
                false,
            )));
        }
    }
    separate(&mut entries);
    entries.extend(views_entries(store, selected, now_ms));
    Some(if entries.iter().all(|e| *e == Entry::Separator) {
        Entry::Label(label)
    } else {
        Entry::Menu(label, entries)
    })
}

/// How often `files` were viewed (`AddFileViewingStatsMenu`, the
/// reference's defaults: the media viewer's and the Client API's views,
/// summed in a submenu's title when both have some).
fn views_entries(store: &Store, files: &[HashId], now_ms: i64) -> Vec<Entry> {
    use hydrus_core::CanvasType;
    let Ok(stats) = store.read(|c| hydrus_store::media::viewing_stats(c, files)) else {
        return Vec::new();
    };
    let line = |canvases: &[CanvasType]| -> String {
        let of = || stats.iter().filter(|s| canvases.contains(&s.canvas));
        let views: u64 = of().map(|s| s.views).sum();
        let viewtime_ms: u64 = of().map(|s| s.viewtime_ms).sum();
        let canvas = match canvases {
            [CanvasType::MediaViewer] => " in media viewer",
            [CanvasType::ClientApi] => " in client api viewer",
            _ => "",
        };
        if views == 0 {
            return format!("no view record{canvas}");
        }
        let last = match of().filter_map(|s| s.last_viewed).max() {
            Some(t) => format!(
                "last {}",
                hydrus_core::time::timestamp_to_pretty_time_delta(
                    t.0.div_euclid(1000),
                    now_ms.div_euclid(1000),
                    " ago"
                )
            ),
            None => "no recorded last view time".to_owned(),
        };
        format!(
            "viewed {} times{canvas}, totalling {}, {last}",
            human_int(views),
            hydrus_core::time::pretty_time_delta_f64(viewtime_ms as f64 / 1000.0)
        )
    };
    let with_views: Vec<CanvasType> = [CanvasType::MediaViewer, CanvasType::ClientApi]
        .into_iter()
        .filter(|&c| stats.iter().any(|s| s.canvas == c && s.views > 0))
        .collect();
    let lines: Vec<Entry> = with_views
        .iter()
        .map(|&c| Entry::Label(line(&[c])))
        .collect();
    if with_views.len() > 1 {
        vec![Entry::Menu(line(&with_views), lines)]
    } else {
        lines
    }
}

/// The menu for a page of `files` (in its order) with `selected` selected,
/// with the selection's `info` first and its `share` menu last.
#[allow(clippy::too_many_lines)]
pub fn menu(
    services: &ServiceRegistry,
    files: &[FileFacts],
    selected: &HashSet<HashId>,
    info: Option<Entry>,
    share: Option<Entry>,
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
    if let Some(info) = info {
        entries.push(info);
        separate(&mut entries);
    }
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
        entries.extend(share);
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
    /// The selection's info.
    pub info: Option<InfoSlots>,
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
    pub share: Option<ShareSlots>,
}

/// The share menu in the template: the selection's items, its copy
/// hashes submenu and its file ids item, then (after a separator) the
/// focused file's path, copy hash submenu and file id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareSlots {
    pub a: Vec<SlotItem>,
    pub hashes: Option<(String, Vec<SlotItem>)>,
    pub b: Vec<SlotItem>,
    pub c: Vec<SlotItem>,
    pub hash: Option<(String, Vec<SlotItem>)>,
    pub d: Vec<SlotItem>,
}

impl ShareSlots {
    fn new(inner: &[Entry]) -> Self {
        let mut share = Self::default();
        let mut past_separator = false;
        for e in inner {
            match (e, past_separator) {
                (Entry::Separator, _) => past_separator = true,
                (Entry::Item(label, action), false) if share.hashes.is_none() => {
                    share.a.push((label.clone(), *action));
                }
                (Entry::Item(label, action), false) => share.b.push((label.clone(), *action)),
                (Entry::Menu(title, sub), false) => {
                    share.hashes = Some((title.clone(), items(sub)));
                }
                (Entry::Item(label, action), true) if share.hash.is_none() => {
                    share.c.push((label.clone(), *action));
                }
                (Entry::Item(label, action), true) => share.d.push((label.clone(), *action)),
                (Entry::Menu(title, sub), true) => share.hash = Some((title.clone(), items(sub))),
                (Entry::Label(_), _) => {}
            }
        }
        share
    }

    fn entry(&self) -> Entry {
        let item = |(label, action): &SlotItem| Entry::Item(label.clone(), *action);
        let menu = |(title, items): &(String, Vec<SlotItem>)| {
            Entry::Menu(title.clone(), items.iter().map(item).collect())
        };
        let mut inner: Vec<Entry> = self.a.iter().map(item).collect();
        inner.extend(self.hashes.iter().map(menu));
        inner.extend(self.b.iter().map(item));
        separate(&mut inner);
        inner.extend(self.c.iter().map(item));
        inner.extend(self.hash.iter().map(menu));
        inner.extend(self.d.iter().map(item));
        Entry::Menu("share".into(), inner)
    }
}

/// A submenu's items.
fn items(entries: &[Entry]) -> Vec<SlotItem> {
    entries
        .iter()
        .filter_map(|e| match e {
            Entry::Item(label, action) => Some((label.clone(), *action)),
            _ => None,
        })
        .collect()
}

/// The selection's info in the template: a label alone, or a submenu of
/// labels with at most one submenu among them, then (after a separator)
/// the views' labels or their summed submenu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InfoSlots {
    pub title: String,
    pub is_menu: bool,
    pub before: Vec<String>,
    pub sub: Option<(String, Vec<String>)>,
    pub after: Vec<String>,
    pub views: Vec<String>,
    pub views_sub: Option<(String, Vec<String>)>,
}

impl InfoSlots {
    fn new(entry: &Entry) -> Self {
        fn labels(entries: &[Entry]) -> Vec<String> {
            entries
                .iter()
                .filter_map(|e| match e {
                    Entry::Label(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        }
        let (title, inner) = match entry {
            Entry::Menu(title, inner) => (title, &inner[..]),
            Entry::Label(title) | Entry::Item(title, _) => {
                return Self {
                    title: title.clone(),
                    ..Self::default()
                };
            }
            Entry::Separator => return Self::default(),
        };
        let mut info = Self {
            title: title.clone(),
            is_menu: true,
            ..Self::default()
        };
        let mut past_separator = false;
        for e in inner {
            match (e, past_separator) {
                (Entry::Separator, _) => past_separator = true,
                (Entry::Label(text), false) if info.sub.is_none() => info.before.push(text.clone()),
                (Entry::Label(text), false) => info.after.push(text.clone()),
                (Entry::Menu(title, sub), false) => info.sub = Some((title.clone(), labels(sub))),
                (Entry::Label(text), true) => info.views.push(text.clone()),
                (Entry::Menu(title, sub), true) => {
                    info.views_sub = Some((title.clone(), labels(sub)));
                }
                (Entry::Item(..), _) => {}
            }
        }
        info
    }

    fn entry(&self) -> Entry {
        if !self.is_menu {
            return Entry::Label(self.title.clone());
        }
        let label = |text: &String| Entry::Label(text.clone());
        let menu = |(title, lines): &(String, Vec<String>)| {
            Entry::Menu(title.clone(), lines.iter().map(label).collect())
        };
        let mut inner: Vec<Entry> = self.before.iter().map(label).collect();
        inner.extend(self.sub.iter().map(menu));
        inner.extend(self.after.iter().map(label));
        separate(&mut inner);
        inner.extend(self.views.iter().map(label));
        inner.extend(self.views_sub.iter().map(menu));
        Entry::Menu(self.title.clone(), inner)
    }
}

/// The most groups the template has for a submenu.
pub const GROUPS: usize = 6;

impl Slots {
    pub fn new(entries: &[Entry]) -> Self {
        fn groups(entries: &[Entry]) -> Vec<Vec<SlotItem>> {
            let mut out: Vec<Vec<SlotItem>> = vec![Vec::new()];
            for e in entries {
                match e {
                    Entry::Separator => out.push(Vec::new()),
                    Entry::Item(label, action) => {
                        out.last_mut().expect("one").push((label.clone(), *action));
                    }
                    Entry::Menu(..) | Entry::Label(_) => {}
                }
            }
            out.retain(|g| !g.is_empty());
            out
        }
        let mut slots = Self::default();
        // (the selection's info, if any, comes first)
        let entries = match entries.first() {
            Some(first) if !matches!(first, Entry::Item(_, Action::Refresh)) => {
                slots.info = Some(InfoSlots::new(first));
                &entries[1..]
            }
            _ => entries,
        };
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
                    "share" => slots.share = Some(ShareSlots::new(inner)),
                    _ => slots.delete_menu = Some((title.clone(), items(inner))),
                },
                Entry::Separator | Entry::Label(_) => {}
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
        if let Some(info) = &self.info {
            out.push(info.entry());
            separate(&mut out);
        }
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
        out.extend(self.share.iter().map(ShareSlots::entry));
        out
    }
}
