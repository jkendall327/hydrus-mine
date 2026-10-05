//! The thumbnails' right-click menu, as the reference builds it (v688's
//! default grid's `GetMenu`), so far with the entries hydrus-rs can act on:
//! refresh; select and remove, by inbox and archive, file domain, client
//! and selection, with counts; the archive/delete filter; archive and
//! re-inbox; deleting from each local file domain, deleting physically and
//! undeleting; manage → tags; open → in a new page, in a new duplicate
//! filter page, similar files in a new page, and the focused file outside
//! hydrus-rs; share's copying; and
//! first, the
//! selection's info (its files' types and size, the focused file's info
//! lines, and how often they were viewed), whose lines copy themselves
//! when chosen. Plain Rust, tested against the reference.

use std::collections::{BTreeSet, HashMap, HashSet};

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
    /// An item that is checked or not.
    Check(String, Action, bool),
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
    /// The selected files' ratings (the file shown's, in the viewer).
    ManageRatings,
    /// The focused file's notes.
    ManageNotes,
    /// The selected files' times (the file shown's, in the viewer).
    ManageTimes,
    /// Force the selected files' filetype (the file shown's).
    ForceFiletype,
    /// The focused file's embedded metadata window.
    EmbeddedMetadata,
    /// The selected files' URLs (the focused file's, in the viewer).
    ManageUrls,
    OpenInNewPage,
    /// Open a duplicates page searching for pairs among the selected files.
    OpenInDuplicateFilterPage,
    /// Open a page searching for files that look like the selected ones,
    /// within this hamming distance.
    OpenSimilar(u64),
    /// Open the focused file as the OS opens it, or in a web browser, or
    /// show it in the OS's file browser.
    OpenExternally,
    OpenInWebBrowser,
    OpenInFileBrowser,
    /// Copy the entry's text (a label's).
    Copy,
    /// Export the selected local files to a chosen folder.
    ExportFiles,
    /// Copy the selected local files themselves (as files a file manager
    /// pastes), or their paths, hashes of a kind, or ids.
    CopyFiles,
    CopyPaths,
    CopyHashes(HashKind),
    CopyFileIds,
    /// Copy the focused file itself, or its path, hash of a kind, or id.
    CopyFile,
    CopyPath,
    CopyHash(HashKind),
    CopyFileId,
    /// Open some URLs in the web browser, copy them, or open a page of
    /// the files that have them.
    OpenUrls(Urls),
    CopyUrls(Urls),
    UrlPage(Urls),
    /// Move the selected thumbnails (`SIMPLE_REARRANGE_THUMBNAILS`).
    Rearrange(Rearrange),
    /// One of the media viewer's own entries.
    Viewer(crate::viewer_menu::ViewerAction),
}

/// Where rearranging moves the selected thumbnails (`MOVE_HOME`,
/// `MOVE_LEFT`, `MOVE_TO_FOCUS`, `MOVE_RIGHT`, `MOVE_END`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rearrange {
    Start,
    Back,
    ToFocus,
    Forward,
    End,
}

/// The rearrange menu (`AddRearrangeMenu`), for a selection of some but
/// not all of the page's `items` (files, and collections by their first):
/// to the start and back one unless the selection starts the page; to the
/// focused thumbnail if it isn't the selection's first, or the selection
/// has gaps; and forward one and to the end unless it ends the page.
pub fn rearrange_menu(
    items: &[HashId],
    selected: &HashSet<HashId>,
    focused: Option<HashId>,
) -> Option<Entry> {
    let at: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| selected.contains(item))
        .map(|(i, _)| i)
        .collect();
    let (&earliest, &latest) = (at.first()?, at.last()?);
    if at.len() == items.len() {
        return None;
    }
    let contiguous = latest - earliest == at.len() - 1;
    let item = |label: &str, to: Rearrange| Entry::Item(label.into(), Action::Rearrange(to));
    let mut entries = Vec::new();
    if earliest > 0 {
        entries.push(item("to start", Rearrange::Start));
        entries.push(item("back one", Rearrange::Back));
    }
    if let Some(focused) = focused.and_then(|f| items.iter().position(|&i| i == f))
        && (focused != earliest || !contiguous)
    {
        entries.push(item("to here", Rearrange::ToFocus));
    }
    if earliest + at.len() < items.len() {
        entries.push(item("forward one", Rearrange::Forward));
        entries.push(item("to end", Rearrange::End));
    }
    Some(Entry::Menu("rearrange".into(), entries))
}

/// The page's items with the selected ones moved as `to` says
/// (`SIMPLE_REARRANGE_THUMBNAILS`, then `move_items`: they are taken out,
/// in order, and put back at the place worked out before, or at the end
/// if that is past it); unmoved if nothing is selected, or they are
/// already first and asked back one.
pub fn rearranged(
    items: &[HashId],
    selected: &HashSet<HashId>,
    focused: Option<HashId>,
    to: Rearrange,
) -> Vec<HashId> {
    let moving: Vec<HashId> = items
        .iter()
        .copied()
        .filter(|i| selected.contains(i))
        .collect();
    let Some(earliest) = items.iter().position(|i| selected.contains(i)) else {
        return items.to_vec();
    };
    let insertion = match to {
        Rearrange::Start => Some(0),
        Rearrange::End => Some(items.len()),
        Rearrange::Back => earliest.checked_sub(1),
        Rearrange::Forward => Some(earliest + 1),
        Rearrange::ToFocus => focused.and_then(|f| items.iter().position(|&i| i == f)),
    };
    let Some(insertion) = insertion else {
        return items.to_vec();
    };
    let mut out: Vec<HashId> = items
        .iter()
        .copied()
        .filter(|i| !selected.contains(i))
        .collect();
    let at = insertion.min(out.len());
    out.splice(at..at, moving);
    out
}

/// What a urls menu entry takes: one of the focused file's URLs (by its
/// place in the menu), its URLs of a class, all its URLs, the selection's
/// URLs of a class (by its place), or all the selection's URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urls {
    One(u16),
    Recognised,
    Focused,
    Class(u16),
    Selection,
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
            hydrus_store::settings::get::<hydrus_store::settings::FileHandlingSettings>(c)?
                .prefix_hash_when_copying,
        ))
    });
    let Ok((basic, digests, prefix)) = read else {
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
        .map(|hash| {
            if prefix {
                let name = if kind == HashKind::PixelHash {
                    "pixel_hash"
                } else {
                    kind.name()
                };
                format!("{name}:{hash}")
            } else {
                hash
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
    if !local.is_empty() {
        entries.push(Entry::Item("export files".into(), Action::ExportFiles));
        entries.push(Entry::Separator);
    }
    if more_than_focused(&local) {
        entries.push(Entry::Item("copy files".into(), Action::CopyFiles));
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
            entries.push(Entry::Item("copy file".into(), Action::CopyFile));
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

/// The info menu's entry for the embedded metadata window.
pub const EMBEDDED_METADATA: &str = "show detailed embedded file metadata";

/// The selection's info, first in the menu: a submenu of `selected`'s
/// types and size (and, for several, their duration), holding the
/// `focused` file's info lines when one is selected, and how often they
/// were viewed; or, with nothing in it, a label. `None` with nothing
/// selected.
pub fn info_menu(
    store: &Store,
    focused: Option<HashId>,
    (selected, items): (&[HashId], crate::status::Items),
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
        crate::status::filetype_summary(&facts, items),
        crate::status::total_size_with_format(
            &facts,
            &hydrus_gui_model::gui_format::preferences(store)
        )
    );
    let mut entries = Vec::new();
    let snapshot = store.snapshot();
    if selected.len() > 1 {
        if let Some(duration) = crate::status::total_duration(&facts) {
            label += &format!(", {duration}");
        }
    } else if let Some(file) = focused {
        entries.push(Entry::Item(
            EMBEDDED_METADATA.into(),
            Action::EmbeddedMetadata,
        ));
        entries.push(Entry::Separator);
        if let Ok(mut batch) =
            store.read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
            && let Some(media) = batch.results.pop()
        {
            entries.extend(rows(crate::info_lines::info_lines_with_format(
                &media,
                &snapshot.services,
                settings,
                now_ms,
                false,
                &hydrus_gui_model::gui_format::preferences(store),
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
/// selected canvases, summed in a submenu or stacked according to Options).
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
            [CanvasType::Preview] => " in preview viewer",
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
    let settings: hydrus_store::settings::FileViewingStatistics =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let with_views: Vec<CanvasType> = settings
        .interesting_canvases
        .into_iter()
        .filter(|&c| stats.iter().any(|s| s.canvas == c && s.views > 0))
        .collect();
    let lines: Vec<Entry> = with_views
        .iter()
        .map(|&c| Entry::Label(line(&[c])))
        .collect();
    if with_views.len() > 1
        && settings.menu_display == hydrus_store::settings::ViewingStatsMenuDisplay::Combined
    {
        vec![Entry::Menu(line(&with_views), lines)]
    } else {
        lines
    }
}

/// The similar-files searches the open menu offers: their names and
/// hamming distances (`CC.hamming_string_lookup`).
pub const SIMILAR_DISTANCES: [(&str, u64); 4] = [
    ("exact match", 0),
    ("very similar", 2),
    ("similar", 4),
    ("speculative", 8),
];

/// The open menu (`AddOpenMenu`): in a new page, or a new duplicate filter
/// page; similar files in a new page, when the focused file is a still
/// image; and the focused file as the OS opens it (the reference's default
/// launch), in a web browser, or (when it is here) in the file browser,
/// which the reference offers in advanced mode.
pub fn open_menu(store: &Store, focused: Option<HashId>, num_selected: usize) -> Vec<Entry> {
    let mut open = vec![
        Entry::Item("in a new page".into(), Action::OpenInNewPage),
        Entry::Item(
            "in a new duplicate filter page".into(),
            Action::OpenInDuplicateFilterPage,
        ),
    ];
    if let Some(focused) = focused {
        if !perceptual_hashed(store, &[focused]).is_empty() {
            // (less the reference's "custom", which asks for the distance)
            let similar = SIMILAR_DISTANCES
                .iter()
                .map(|&(name, distance)| Entry::Item(name.into(), Action::OpenSimilar(distance)))
                .collect();
            open.push(Entry::Menu("similar files in a new page".into(), similar));
        }
        separate(&mut open);
        let prefix = if num_selected > 1 {
            "focused file "
        } else {
            ""
        };
        open.push(Entry::Item(
            format!("{prefix}using Default OS File Launch"),
            Action::OpenExternally,
        ));
        open.push(Entry::Item(
            format!("{prefix}in web browser"),
            Action::OpenInWebBrowser,
        ));
        if !paths(store, &[focused]).is_empty() {
            open.push(Entry::Item(
                format!("{prefix}in file browser"),
                Action::OpenInFileBrowser,
            ));
        }
    }
    open
}

/// The sha256 hashes of those of `files` with perceptual hashes (still
/// images), in their order.
fn perceptual_hashed(store: &Store, files: &[HashId]) -> Vec<hydrus_core::Sha256> {
    let Ok(basic) = store.read(|c| hydrus_store::media::load_basic(c, files)) else {
        return Vec::new();
    };
    files
        .iter()
        .filter_map(|f| basic.iter().find(|m| m.hash_id == *f))
        .filter(|m| {
            m.info
                .as_ref()
                .is_some_and(|i| hydrus_media::mimes::has_perceptual_hash(i.mime))
        })
        .map(|m| m.hash)
        .collect()
}

/// The search for files that look like `files` within `distance`
/// (`ShowSimilarFilesInNewPage`): `system:similar to` those of them that
/// are still images, if any are.
pub fn similar_search(
    store: &Store,
    files: &[HashId],
    distance: u64,
) -> Option<hydrus_search::Predicate> {
    use hydrus_search::{Predicate, SystemPredicate};
    let hashes = perceptual_hashed(store, files);
    (!hashes.is_empty()).then(|| {
        Predicate::System(SystemPredicate::SimilarToFiles {
            files: hashes.into_iter().collect(),
            max_distance: distance,
        })
    })
}

/// A URL as the reference shows it (`ConvertURLToHumanString`):
/// percent-decoded, as UTF-8 (`urllib.parse.unquote`).
pub fn human_url(url: &str) -> String {
    let bytes = url.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(&h), Some(&l)) = (bytes.get(i + 1), bytes.get(i + 2))
            && let (Some(h), Some(l)) = (hex(h), hex(l))
        {
            out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The URLs the urls menu offers (`AddKnownURLsViewCopyMenu`'s facts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlFacts {
    /// The focused file's URLs and their labels, as listed: those of a URL
    /// class ("class: url"), then the rest, each sorted.
    pub focus: Vec<(String, String)>,
    /// How many of those are of a class.
    pub matched: usize,
    /// The selection's URL classes' names, sorted.
    pub classes: Vec<String>,
    /// Whether the selection has URLs of no class, or of several classes.
    pub mixed: bool,
}

/// Each of `files`' URLs.
fn file_urls(store: &Store, files: &[HashId]) -> HashMap<HashId, Vec<String>> {
    let snapshot = store.snapshot();
    store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, files))
        .map(|batch| {
            batch
                .results
                .into_iter()
                .map(|m| (m.hash_id, m.urls))
                .collect()
        })
        .unwrap_or_default()
}

/// What the urls menu offers for the `focused` file and the `selected`
/// files.
pub fn url_facts(store: &Store, focused: Option<HashId>, selected: &[HashId]) -> UrlFacts {
    let classes = &store.snapshot().url_classes;
    let class_of = |url: &str| classes.class_for(url).map(|c| c.name.clone());
    let mut facts = UrlFacts::default();
    if let Some(focused) = focused {
        let urls = file_urls(store, &[focused])
            .remove(&focused)
            .unwrap_or_default();
        let mut matched: Vec<(String, String)> = Vec::new();
        let mut unmatched: Vec<String> = Vec::new();
        for url in urls {
            match class_of(&url) {
                Some(class) => matched.push((format!("{class}: {}", human_url(&url)), url)),
                None => unmatched.push(url),
            }
        }
        matched.sort();
        unmatched.sort();
        facts.matched = matched.len();
        facts.focus = matched;
        facts
            .focus
            .extend(unmatched.into_iter().map(|url| (human_url(&url), url)));
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for urls in file_urls(store, selected).values() {
        for url in urls {
            match class_of(url) {
                Some(class) => {
                    seen.insert(class);
                }
                None => facts.mixed = true,
            }
        }
    }
    facts.mixed |= seen.len() > 1;
    facts.classes = seen.into_iter().collect();
    facts
}

/// The most of a list a menu shows (`SpamItems`' `MAX_TO_SHOW`).
const MAX_TO_SHOW: usize = 15;

/// `entries` as `SpamItems` adds them: past the most shown, all but one
/// and a count of the rest.
fn spam(menu: &mut Vec<Entry>, entries: Vec<Entry>) {
    let shown = if entries.len() > MAX_TO_SHOW {
        MAX_TO_SHOW - 1
    } else {
        MAX_TO_SHOW
    };
    let more = entries.len().saturating_sub(shown);
    menu.extend(entries.into_iter().take(shown));
    if more > 0 {
        menu.push(Entry::Label(format!("{more} more...")));
    }
}

/// The manage menu: tags, ratings if there are rating services to rate
/// with, notes (counting the focused file's), times and force filetype.
pub fn manage_menu(services: &ServiceRegistry, notes: usize) -> Vec<Entry> {
    let mut entries = vec![Entry::Item("tags".into(), Action::ManageTags)];
    if services.all().any(|s| s.service_type().is_rating_service()) {
        entries.push(Entry::Item("ratings".into(), Action::ManageRatings));
    }
    entries.push(Entry::Item(
        crate::notes_editor::menu_label(notes),
        Action::ManageNotes,
    ));
    entries.push(Entry::Item("times".into(), Action::ManageTimes));
    entries.push(Entry::Item("force filetype".into(), Action::ForceFiletype));
    entries
}

/// The urls menu (`AddKnownURLsViewCopyMenu`), less forcing a metadata
/// refetch: manage, then, if there are URLs to offer, the focused file's
/// URLs and the selection's, to open in the web browser, open a page of
/// the files that have them, or copy.
pub fn urls_menu(facts: &UrlFacts) -> Entry {
    let manage = Entry::Item("manage".into(), Action::ManageUrls);
    if facts.focus.is_empty() && facts.classes.is_empty() && !facts.mixed {
        return Entry::Menu("urls".into(), vec![manage]);
    }
    let mut visit = Vec::new();
    let mut copy = Vec::new();
    let mut pages = Vec::new();
    let one = |i: usize| Urls::One(u16::try_from(i).unwrap_or(u16::MAX));
    if !facts.focus.is_empty() {
        let each = |label: &dyn Fn(&str) -> String, action: fn(Urls) -> Action| {
            facts
                .focus
                .iter()
                .enumerate()
                .map(|(i, (l, _))| Entry::Item(label(l), action(one(i))))
                .collect::<Vec<_>>()
        };
        spam(&mut visit, each(&|l| l.to_owned(), Action::OpenUrls));
        spam(&mut copy, each(&|l| l.to_owned(), Action::CopyUrls));
        spam(
            &mut pages,
            each(&|l| format!("files with {l}"), Action::UrlPage),
        );
        if facts.focus.len() > 1 {
            separate(&mut pages);
            pages.push(Entry::Item(
                "files with any of the above".into(),
                Action::UrlPage(Urls::Focused),
            ));
        }
    }
    let unmatched = facts.focus.len() - facts.matched;
    let recognised = facts.matched > 1;
    let all_focused = unmatched > 0 && facts.focus.len() > 1;
    let both = |visit: &mut Vec<Entry>, copy: &mut Vec<Entry>, label: String, urls: Urls| {
        visit.push(Entry::Item(label.clone(), Action::OpenUrls(urls)));
        copy.push(Entry::Item(label, Action::CopyUrls(urls)));
    };
    if recognised || all_focused {
        separate(&mut visit);
        separate(&mut copy);
    }
    if recognised {
        let label = format!(
            "this file's {} recognised urls",
            human_int(facts.matched as u64)
        );
        both(&mut visit, &mut copy, label, Urls::Recognised);
    }
    if all_focused {
        let label = format!("this file's {} urls", human_int(facts.focus.len() as u64));
        both(&mut visit, &mut copy, label, Urls::Focused);
    }
    if !facts.classes.is_empty() || facts.mixed {
        separate(&mut visit);
        separate(&mut copy);
    }
    for (i, class) in facts.classes.iter().enumerate() {
        let label = format!("these files' {class} urls");
        both(
            &mut visit,
            &mut copy,
            label,
            Urls::Class(u16::try_from(i).unwrap_or(u16::MAX)),
        );
    }
    if facts.mixed {
        both(
            &mut visit,
            &mut copy,
            "all these files' urls".into(),
            Urls::Selection,
        );
    }
    let mut inner = vec![manage, Entry::Menu("open in browser".into(), visit)];
    if !facts.focus.is_empty() {
        inner.push(Entry::Menu("open in a new page".into(), pages));
    }
    inner.push(Entry::Menu("copy".into(), copy));
    Entry::Menu("urls".into(), inner)
}

/// The URLs a urls menu entry takes, as the reference's actions take them:
/// the focused file's in the menu's order, the selection's sorted and
/// without repeats.
pub fn urls_for(store: &Store, facts: &UrlFacts, which: Urls, selected: &[HashId]) -> Vec<String> {
    let focus = |range: std::ops::Range<usize>| {
        facts.focus[range]
            .iter()
            .map(|(_, url)| url.clone())
            .collect::<Vec<_>>()
    };
    match which {
        Urls::One(i) => facts
            .focus
            .get(usize::from(i))
            .map(|(_, url)| vec![url.clone()])
            .unwrap_or_default(),
        Urls::Recognised => focus(0..facts.matched),
        Urls::Focused => focus(0..facts.focus.len()),
        Urls::Class(i) => {
            let Some(name) = facts.classes.get(usize::from(i)) else {
                return Vec::new();
            };
            let classes = &store.snapshot().url_classes;
            let urls: BTreeSet<String> = file_urls(store, selected)
                .into_values()
                .flatten()
                .filter(|url| classes.class_for(url).is_some_and(|c| c.name == *name))
                .collect();
            urls.into_iter().collect()
        }
        Urls::Selection => {
            let urls: BTreeSet<String> =
                file_urls(store, selected).into_values().flatten().collect();
            urls.into_iter().collect()
        }
    }
}

/// The search for the files that have a urls menu entry's URLs: one, or
/// any of the focused file's (a page of them is "url search", on all my
/// files).
pub fn url_search(facts: &UrlFacts, which: Urls) -> Vec<hydrus_search::Predicate> {
    use hydrus_core::search::predicate::UrlRule;
    use hydrus_search::{Predicate, SystemPredicate};
    let has = |url: &str| {
        Predicate::System(SystemPredicate::KnownUrl {
            rule: UrlRule::ExactMatch(url.to_owned()),
            has: true,
        })
    };
    match which {
        Urls::One(i) => facts
            .focus
            .get(usize::from(i))
            .map(|(_, url)| vec![has(url)])
            .unwrap_or_default(),
        _ => vec![Predicate::Or(
            facts.focus.iter().map(|(_, url)| has(url)).collect(),
        )],
    }
}

/// The menu for a page of `files` (in its order) with `selected` selected,
/// with the selection's `info` first and its `share` menu last.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub fn menu(
    services: &ServiceRegistry,
    files: &[FileFacts],
    selected: &HashSet<HashId>,
    info: Option<Entry>,
    urls: Option<Entry>,
    open: Vec<Entry>,
    share: Option<Entry>,
    rearrange: Option<Entry>,
    notes: Option<usize>,
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
        // rearrange (`AddRearrangeMenu`), for a selection
        if num_selected > 0 {
            entries.extend(rearrange);
        }

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
            manage_menu(services, notes.unwrap_or(0)),
        ));
        // (the reference's locations, which hydrus-rs doesn't have yet)
        entries.extend(urls);
        entries.push(Entry::Menu("open".into(), open));
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
    pub rearrange: Vec<SlotItem>,
    /// The media viewer's zoom submenu: its title and items.
    pub zoom: Option<(String, Vec<SlotItem>)>,
    /// The media viewer's slideshow submenu: its title and groups.
    pub slideshow: Option<(String, Vec<Vec<CheckSlot>>)>,
    /// The media viewer's volume submenu's groups.
    pub volume: Vec<Vec<SlotItem>>,
    /// The media viewer's "remove from view".
    pub dismiss: Vec<SlotItem>,
    /// What shows the viewer's file: the submenu's title and its line.
    pub player: Option<(String, Vec<String>)>,
    /// The archive/delete filter, archiving and re-inboxing.
    pub filter: Vec<SlotItem>,
    /// Deleting from the one local domain the files are in.
    pub delete: Vec<SlotItem>,
    /// Or from several: the submenu's title and items.
    pub delete_menu: Option<(String, Vec<SlotItem>)>,
    /// Deleting physically and undeleting.
    pub trash: Vec<SlotItem>,
    pub manage: Vec<SlotItem>,
    pub urls: Option<UrlsSlots>,
    pub open: Option<OpenSlots>,
    pub share: Option<ShareSlots>,
}

/// The urls menu in the template: manage, then, if there are URLs to
/// offer (`lists`), its open in browser, open in a new page and copy
/// submenus' groups.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlsSlots {
    pub lists: bool,
    pub visit: Vec<Vec<SlotItem>>,
    pub pages: Option<Vec<Vec<SlotItem>>>,
    pub copy: Vec<Vec<SlotItem>>,
}

impl UrlsSlots {
    fn new(inner: &[Entry]) -> Self {
        let mut urls = Self::default();
        for e in inner {
            if let Entry::Menu(title, sub) = e {
                urls.lists = true;
                match title.as_str() {
                    "open in browser" => urls.visit = groups(sub),
                    "open in a new page" => urls.pages = Some(groups(sub)),
                    _ => urls.copy = groups(sub),
                }
            }
        }
        urls
    }

    fn entry(&self) -> Entry {
        let mut inner = vec![Entry::Item("manage".into(), Action::ManageUrls)];
        if self.lists {
            inner.push(group_menu("open in browser", &self.visit));
            inner.extend(
                self.pages
                    .iter()
                    .map(|pages| group_menu("open in a new page", pages)),
            );
            inner.push(group_menu("copy", &self.copy));
        }
        Entry::Menu("urls".into(), inner)
    }
}

/// A submenu's runs between separators, its labels as items copying
/// themselves.
fn groups(entries: &[Entry]) -> Vec<Vec<SlotItem>> {
    let mut out: Vec<Vec<SlotItem>> = vec![Vec::new()];
    for e in entries {
        match e {
            Entry::Separator => out.push(Vec::new()),
            Entry::Item(label, action) => {
                out.last_mut().expect("one").push((label.clone(), *action));
            }
            Entry::Label(label) => {
                out.last_mut()
                    .expect("one")
                    .push((label.clone(), Action::Copy));
            }
            Entry::Menu(..) | Entry::Check(..) => {}
        }
    }
    out.retain(|g| !g.is_empty());
    out
}

/// An item of a submenu with checked items: checked or not, if it can be.
pub type CheckSlot = (String, Action, Option<bool>);

/// A submenu's runs between separators, checked items and all.
fn check_groups(entries: &[Entry]) -> Vec<Vec<CheckSlot>> {
    let mut out: Vec<Vec<CheckSlot>> = vec![Vec::new()];
    for e in entries {
        match e {
            Entry::Separator => out.push(Vec::new()),
            Entry::Item(label, action) => {
                out.last_mut()
                    .expect("one")
                    .push((label.clone(), *action, None));
            }
            Entry::Check(label, action, checked) => {
                out.last_mut()
                    .expect("one")
                    .push((label.clone(), *action, Some(*checked)));
            }
            Entry::Label(_) | Entry::Menu(..) => {}
        }
    }
    out.retain(|g| !g.is_empty());
    out
}

/// A submenu of `groups` with checked items, separated.
fn check_group_menu(title: &str, groups: &[Vec<CheckSlot>]) -> Entry {
    let mut inner = Vec::new();
    for group in groups {
        separate(&mut inner);
        inner.extend(group.iter().map(|(label, action, checked)| match checked {
            Some(checked) => Entry::Check(label.clone(), *action, *checked),
            None => Entry::Item(label.clone(), *action),
        }));
    }
    Entry::Menu(title.into(), inner)
}

/// A submenu of `groups`, separated.
fn group_menu(title: &str, groups: &[Vec<SlotItem>]) -> Entry {
    let mut inner = Vec::new();
    for group in groups {
        separate(&mut inner);
        inner.extend(
            group
                .iter()
                .map(|(label, action)| Entry::Item(label.clone(), *action)),
        );
    }
    Entry::Menu(title.into(), inner)
}

/// The open menu in the template: its first items and the similar files
/// submenu, then (after a separator) the focused file's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenSlots {
    pub a: Vec<SlotItem>,
    pub similar: Option<(String, Vec<SlotItem>)>,
    pub b: Vec<SlotItem>,
}

impl OpenSlots {
    fn new(inner: &[Entry]) -> Self {
        let mut open = Self::default();
        let mut past_separator = false;
        for e in inner {
            match (e, past_separator) {
                (Entry::Separator, _) => past_separator = true,
                (Entry::Item(label, action), false) => open.a.push((label.clone(), *action)),
                (Entry::Item(label, action), true) => open.b.push((label.clone(), *action)),
                (Entry::Menu(title, sub), _) => open.similar = Some((title.clone(), items(sub))),
                (Entry::Label(_) | Entry::Check(..), _) => {}
            }
        }
        open
    }

    fn entry(&self) -> Entry {
        let item = |(label, action): &SlotItem| Entry::Item(label.clone(), *action);
        let mut inner: Vec<Entry> = self.a.iter().map(item).collect();
        inner.extend(
            self.similar
                .iter()
                .map(|(title, items)| Entry::Menu(title.clone(), items.iter().map(item).collect())),
        );
        separate(&mut inner);
        inner.extend(self.b.iter().map(item));
        Entry::Menu("open".into(), inner)
    }
}

/// The share menu in the template: the selection's items, its copy
/// hashes submenu and its file ids item, then (after a separator) the
/// focused file's path, copy hash submenu and file id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShareSlots {
    /// Manual export before the copying groups.
    pub export: Option<SlotItem>,
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
        let mut export_separator = false;
        for e in inner {
            if let Entry::Item(label, Action::ExportFiles) = e {
                share.export = Some((label.clone(), Action::ExportFiles));
                export_separator = true;
                continue;
            }
            if export_separator && matches!(e, Entry::Separator) {
                export_separator = false;
                continue;
            }
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
                (Entry::Label(_) | Entry::Check(..), _) => {}
            }
        }
        share
    }

    fn entry(&self) -> Entry {
        let item = |(label, action): &SlotItem| Entry::Item(label.clone(), *action);
        let menu = |(title, items): &(String, Vec<SlotItem>)| {
            Entry::Menu(title.clone(), items.iter().map(item).collect())
        };
        let mut inner: Vec<Entry> = self.export.iter().map(item).collect();
        separate(&mut inner);
        inner.extend(self.a.iter().map(item));
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
    /// Whether it starts with the embedded metadata window's entry.
    pub metadata: bool,
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
            Entry::Label(title) | Entry::Item(title, _) | Entry::Check(title, ..) => {
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
        let mut inner = inner;
        if let [Entry::Item(_, Action::EmbeddedMetadata), rest @ ..] = inner {
            info.metadata = true;
            inner = rest.strip_prefix(&[Entry::Separator]).unwrap_or(rest);
        }
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
                (Entry::Item(..) | Entry::Check(..), _) => {}
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
        let mut inner: Vec<Entry> = Vec::new();
        if self.metadata {
            inner.push(Entry::Item(
                EMBEDDED_METADATA.into(),
                Action::EmbeddedMetadata,
            ));
            inner.push(Entry::Separator);
        }
        inner.extend(self.before.iter().map(label));
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
                        Action::Refresh
                        | Action::Viewer(crate::viewer_menu::ViewerAction::Fullscreen) => {
                            &mut slots.head
                        }
                        Action::Viewer(crate::viewer_menu::ViewerAction::RemoveFromView) => {
                            &mut slots.dismiss
                        }
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
                    "rearrange" => slots.rearrange = items(inner),
                    "manage" => slots.manage = items(inner),
                    "urls" => slots.urls = Some(UrlsSlots::new(inner)),
                    "open" => slots.open = Some(OpenSlots::new(inner)),
                    "share" => slots.share = Some(ShareSlots::new(inner)),
                    "volume" => slots.volume = groups(inner),
                    "start slideshow" | "slideshow running" => {
                        slots.slideshow = Some((title.clone(), check_groups(inner)));
                    }
                    "player" => {
                        let lines = inner
                            .iter()
                            .filter_map(|e| match e {
                                Entry::Label(line) => Some(line.clone()),
                                _ => None,
                            })
                            .collect();
                        slots.player = Some((title.clone(), lines));
                    }
                    t if t.starts_with("zoom: ") => {
                        slots.zoom = Some((title.clone(), items(inner)));
                    }
                    _ => slots.delete_menu = Some((title.clone(), items(inner))),
                },
                Entry::Separator | Entry::Label(_) | Entry::Check(..) => {}
            }
        }
        slots
    }

    /// The menu the template shows.
    pub fn entries(&self) -> Vec<Entry> {
        let item = |(label, action): &SlotItem| Entry::Item(label.clone(), *action);
        let menu = group_menu;
        let mut out = Vec::new();
        if let Some(info) = &self.info {
            out.push(info.entry());
            separate(&mut out);
        }
        if let Some((title, items)) = &self.zoom {
            out.push(Entry::Menu(title.clone(), items.iter().map(item).collect()));
        }
        out.extend(self.head.iter().map(item));
        if let Some((title, groups)) = &self.slideshow {
            out.push(check_group_menu(title, groups));
        }
        separate(&mut out);
        if !self.select.is_empty() {
            out.push(menu("select", &self.select));
        }
        if !self.remove.is_empty() {
            out.push(menu("remove", &self.remove));
        }
        if !self.rearrange.is_empty() {
            out.push(Entry::Menu(
                "rearrange".into(),
                self.rearrange.iter().map(item).collect(),
            ));
        }
        if !self.volume.is_empty() {
            out.push(menu("volume", &self.volume));
        }
        separate(&mut out);
        out.extend(self.dismiss.iter().map(item));
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
        out.extend(self.urls.iter().map(UrlsSlots::entry));
        out.extend(self.open.iter().map(OpenSlots::entry));
        out.extend(self.share.iter().map(ShareSlots::entry));
        if let Some((title, lines)) = &self.player {
            separate(&mut out);
            out.push(Entry::Menu(
                title.clone(),
                lines.iter().map(|l| Entry::Label(l.clone())).collect(),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                Entry::Item(label, _) | Entry::Label(label) | Entry::Check(label, ..) => {
                    label.clone()
                }
                Entry::Menu(title, _) => format!("{title} >"),
                Entry::Separator => "---".into(),
            })
            .collect()
    }

    #[test]
    fn urls_show_as_the_reference_shows_them() {
        assert_eq!(
            human_url("https://example.com/a%20b/%E3%83%9F?q=%2Fx&r=1"),
            "https://example.com/a b/ミ?q=/x&r=1"
        );
        // a stray % stays, and bad UTF-8 is replaced
        assert_eq!(human_url("100%zz/%FF"), "100%zz/\u{fffd}");
    }

    /// Recognised URLs and the selection's classes, which the recorded
    /// menus (over URLs of no class) don't have, as `AddKnownURLsViewCopyMenu`
    /// lays them out.
    #[test]
    fn the_urls_menu_lays_out_classes_as_the_reference_s() {
        let facts = UrlFacts {
            focus: vec![
                ("booru: https://a/1".into(), "https://a/1".into()),
                ("booru: https://a/2".into(), "https://a/2".into()),
                ("https://b/x".into(), "https://b/x".into()),
            ],
            matched: 2,
            classes: vec!["booru".into(), "gallery".into()],
            mixed: true,
        };
        let Entry::Menu(title, mut inner) = urls_menu(&facts) else {
            panic!("a menu");
        };
        assert_eq!(title, "urls");
        assert_eq!(
            titles(&inner),
            [
                "manage",
                "open in browser >",
                "open in a new page >",
                "copy >"
            ]
        );
        inner.remove(0);
        let Entry::Menu(_, visit) = &inner[0] else {
            panic!()
        };
        assert_eq!(
            titles(visit),
            [
                "booru: https://a/1",
                "booru: https://a/2",
                "https://b/x",
                "---",
                "this file's 2 recognised urls",
                "this file's 3 urls",
                "---",
                "these files' booru urls",
                "these files' gallery urls",
                "all these files' urls",
            ]
        );
        let Entry::Menu(_, copy) = &inner[2] else {
            panic!()
        };
        assert_eq!(titles(copy), titles(visit));
        let Entry::Menu(_, pages) = &inner[1] else {
            panic!()
        };
        assert_eq!(
            titles(pages),
            [
                "files with booru: https://a/1",
                "files with booru: https://a/2",
                "files with https://b/x",
                "---",
                "files with any of the above",
            ]
        );
        // only one URL, of a class, and one class selected: no more
        let one = UrlFacts {
            focus: facts.focus[..1].to_vec(),
            matched: 1,
            classes: vec!["booru".into()],
            mixed: false,
        };
        let Entry::Menu(_, inner) = urls_menu(&one) else {
            panic!()
        };
        let Entry::Menu(_, visit) = &inner[1] else {
            panic!()
        };
        assert_eq!(
            titles(visit),
            ["booru: https://a/1", "---", "these files' booru urls"]
        );
        // nothing to offer: manage alone
        assert_eq!(
            urls_menu(&UrlFacts::default()),
            Entry::Menu(
                "urls".into(),
                vec![Entry::Item("manage".into(), Action::ManageUrls)]
            )
        );
        // and past 15, 14 and a count
        let many = UrlFacts {
            focus: (0..20)
                .map(|i| (format!("https://c/{i}"), format!("https://c/{i}")))
                .collect(),
            ..UrlFacts::default()
        };
        let Entry::Menu(_, inner) = urls_menu(&many) else {
            panic!()
        };
        let Entry::Menu(_, visit) = &inner[1] else {
            panic!()
        };
        assert_eq!(visit.len(), 15 + 2);
        assert_eq!(titles(visit)[14], "6 more...");
    }

    /// Letters for ids, to read the orders below.
    fn page(letters: &str) -> Vec<HashId> {
        letters.bytes().map(|b| HashId(u32::from(b))).collect()
    }

    fn letters(items: &[HashId]) -> String {
        items
            .iter()
            .map(|i| char::from(u8::try_from(i.0).unwrap()))
            .collect()
    }

    #[test]
    fn rearranging_moves_as_the_reference_s_lists_do() {
        // (each as hydrus's FastIndexUniqueList.move_items gives it, from
        // the insertion index the thumbnail panel works out)
        let items = page("abcdef");
        for (selected, to, focus, wanted) in [
            ("c", Rearrange::Start, None, "cabdef"),
            ("c", Rearrange::End, None, "abdefc"),
            ("c", Rearrange::Back, None, "acbdef"),
            ("c", Rearrange::Forward, None, "abdcef"),
            ("bd", Rearrange::Forward, None, "acbdef"),
            ("bd", Rearrange::Back, None, "bdacef"),
            ("bd", Rearrange::Start, None, "bdacef"),
            ("bd", Rearrange::End, None, "acefbd"),
            ("ae", Rearrange::ToFocus, Some("c"), "bcaedf"),
            ("bc", Rearrange::ToFocus, Some("e"), "adefbc"),
            ("df", Rearrange::ToFocus, Some("a"), "dfabce"),
            ("a", Rearrange::Back, None, "abcdef"),
            ("ef", Rearrange::Forward, None, "abcdef"),
            ("ce", Rearrange::Forward, None, "abdcef"),
        ] {
            let selected: HashSet<HashId> = page(selected).into_iter().collect();
            let focus = focus.map(|f| page(f)[0]);
            assert_eq!(
                letters(&rearranged(&items, &selected, focus, to)),
                wanted,
                "{to:?}"
            );
        }
    }

    #[test]
    fn the_rearrange_menu_offers_what_would_move() {
        let items = page("abcdef");
        let offered = |selected: &str, focus: char| {
            let selected: HashSet<HashId> = page(selected).into_iter().collect();
            rearrange_menu(&items, &selected, Some(HashId(u32::from(focus as u8)))).map(|m| match m
            {
                Entry::Menu(_, inner) => titles(&inner),
                _ => unreachable!(),
            })
        };
        // first and contiguous, focused first: nothing back, no "to here"
        assert_eq!(offered("ab", 'a').unwrap(), ["forward one", "to end"]);
        // with a gap, "to here" even focused on the first
        assert_eq!(
            offered("ac", 'a').unwrap(),
            ["to here", "forward one", "to end"]
        );
        // focused elsewhere: "to here"; at the end: nothing forward
        assert_eq!(
            offered("ef", 'b').unwrap(),
            ["to start", "back one", "to here"]
        );
        // all of them, or none: no menu
        assert!(offered("abcdef", 'a').is_none());
        assert!(offered("", 'a').is_none());
    }
}
