//! The GUI's pages, as a session keeps them: a tree of notebooks and
//! pages, each page with what it shows.
//!
//! The reference saves the open pages as the session "last session" (and
//! any the user saves by name). Here a session is its tree; the files each
//! page shows are kept beside it, by page key, since they change more often
//! and are much larger.

use std::fmt;

use crate::ServiceKey;
use crate::duplicates::{DuplicatesSearch, PairOrder};
use crate::search::context::FileSearchContext;

/// Which pages' tabs show how many files they have
/// (`page_file_count_display`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum FileCountDisplay {
    All,
    None,
    OnlyImporters,
    /// Every page with any (a new client's).
    #[default]
    AllIfAny,
}

impl FileCountDisplay {
    /// From the reference's `CC.PAGE_FILE_COUNT_DISPLAY_*`.
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::All,
            1 => Self::None,
            2 => Self::OnlyImporters,
            3 => Self::AllIfAny,
            _ => return None,
        })
    }
}

/// How tabs are named (`max_page_name_chars`, `page_file_count_display`,
/// `import_page_progress_display`, `decorate_page_of_pages_tab_names`,
/// `page_of_pages_decorator`), and what importers' short summaries count
/// (`show_new_on_file_seed_short_summary`,
/// `show_deleted_on_file_seed_short_summary`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PageNameSettings {
    pub max_chars: usize,
    pub file_counts: FileCountDisplay,
    pub import_progress: bool,
    pub decorate_notebooks: bool,
    pub notebook_decorator: String,
    pub short_summary_new: bool,
    pub short_summary_deleted: bool,
}

impl Default for PageNameSettings {
    /// A new client's.
    fn default() -> Self {
        Self {
            max_chars: 20,
            file_counts: FileCountDisplay::AllIfAny,
            import_progress: true,
            decorate_notebooks: true,
            notebook_decorator: " \u{2193}".into(),
            short_summary_new: false,
            short_summary_deleted: false,
        }
    }
}

/// Downloader pages' options.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DownloaderPageSettings {
    /// Ask before closing one that holds anything, idle or not
    /// (`confirm_non_empty_downloader_page_close`).
    pub confirm_non_empty_close: bool,
    /// Show a gallery page's first new query when it shows none
    /// (`highlight_new_query`), and a watcher page's first new watcher
    /// (`highlight_new_watcher`).
    pub highlight_new_query: bool,
    pub highlight_new_watcher: bool,
    /// What a list shows for paused work, and for finished work
    /// (`pause_character`, `stop_character`).
    pub pause_character: String,
    pub stop_character: String,
}

impl Default for DownloaderPageSettings {
    /// A new client's.
    fn default() -> Self {
        Self {
            confirm_non_empty_close: true,
            highlight_new_query: true,
            highlight_new_watcher: true,
            pause_character: "\u{23F8}".into(),
            stop_character: "\u{23F9}".into(),
        }
    }
}

/// What a tab is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    Page,
    /// A downloader or other importing page.
    Importer,
    Notebook,
}

/// A tab's name, as the reference writes it (`_RefreshPageName`): the
/// page's name on one line, elided to the longest the settings allow, then
/// its number of files and import progress as they say ("url import (5 -
/// 6/10)"), and a notebook's decoration. `files` is the page's number of
/// files (a notebook's pages' together), `progress` its import progress
/// (the done and total of its importers; none when done).
pub fn tab_name(
    name: &str,
    kind: TabKind,
    files: usize,
    progress: (usize, usize),
    settings: &PageNameSettings,
) -> String {
    // (`splitlines`, joined; at most 256 characters)
    let line_break = |c: char| {
        matches!(
            c,
            '\n' | '\r'
                | '\u{0b}'
                | '\u{0c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        )
    };
    let full: Vec<char> = name.chars().filter(|c| !line_break(*c)).take(256).collect();
    // (`ElideText`)
    let mut tab: String = if full.len() > settings.max_chars {
        let keep = if settings.max_chars == 0 {
            full.len() - 1
        } else {
            settings.max_chars - 1
        };
        full[..keep].iter().chain(['\u{2026}'].iter()).collect()
    } else {
        full.iter().collect()
    };
    let shown = match settings.file_counts {
        FileCountDisplay::All => true,
        FileCountDisplay::None => false,
        FileCountDisplay::OnlyImporters => kind == TabKind::Importer,
        FileCountDisplay::AllIfAny => files > 0,
    };
    let mut counts = String::new();
    if shown {
        counts.push_str(&crate::numbers::human_int(files as u64));
    }
    let (value, range) = progress;
    if settings.import_progress && range > 0 && value != range {
        if !counts.is_empty() {
            counts.push_str(" - ");
        }
        counts.push_str(&crate::numbers::value_range(value as u64, range as u64));
    }
    if !counts.is_empty() {
        tab.push_str(&format!(" ({counts})"));
    }
    if kind == TabKind::Notebook && settings.decorate_notebooks {
        tab.push_str(&settings.notebook_decorator);
    }
    tab
}

/// A named tree of pages.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub name: String,
    /// The top notebook's pages, in order.
    pub pages: Vec<Page>,
}

impl Session {
    /// Every page (notebooks too), depth first, in the order they appear.
    pub fn all_pages(&self) -> Vec<&Page> {
        fn walk<'a>(pages: &'a [Page], out: &mut Vec<&'a Page>) {
            for page in pages {
                out.push(page);
                if let PageContent::Pages(children) = &page.content {
                    walk(children, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.pages, &mut out);
        out
    }
}

/// A page, or a notebook of pages.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Page {
    /// Random: how the Client API refers to the page.
    pub key: PageKey,
    pub name: String,
    pub content: PageContent,
}

/// What a page is.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum PageContent {
    /// A notebook of pages ("page of pages").
    Pages(Vec<Page>),
    /// A file search page.
    Search {
        search: FileSearchContext,
        /// Whether the page searches as its predicates change; if not, it
        /// keeps its files until searched again.
        synchronised: bool,
        sort: Option<PageSort>,
        /// Whether the search is locked to a `system:hash` of the page's
        /// files.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lock: Option<HashLock>,
        /// How the page collects its files, if it does.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        collect: Option<PageCollect>,
    },
    /// A downloader page, showing these import queues (`queues` ids).
    Downloader {
        kind: DownloaderKind,
        queues: Vec<i64>,
        sort: Option<PageSort>,
        /// Its own state (a gallery or watcher page's).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<Box<DownloaderPageState>>,
    },
    /// A duplicates page: a potential-duplicates search to filter.
    Duplicates {
        duplicates: DuplicatesPage,
        sort: Option<PageSort>,
    },
    /// A page of a kind we don't open yet (the reference's `PAGE_TYPE_*`),
    /// kept so it isn't lost.
    Other {
        page_type: i64,
        /// The page as the reference stored it (its serialised tuple).
        stored: Option<serde_json::Value>,
        sort: Option<PageSort>,
    },
}

/// A search page's search locked to a `system:hash` of its files (the
/// reference's `system_hash_locked`): the page doesn't search, and the
/// hash takes in files added to the page and lets go of those removed
/// from it, as these say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HashLock {
    pub syncs_new: bool,
    pub syncs_removes: bool,
}

impl Default for HashLock {
    /// The reference's: both.
    fn default() -> Self {
        Self {
            syncs_new: true,
            syncs_removes: true,
        }
    }
}

impl PageContent {
    /// The reference's `PAGE_TYPE_*`.
    pub fn page_type(&self) -> i64 {
        match self {
            PageContent::Pages(_) => 10,
            PageContent::Search { .. } => 6,
            PageContent::Downloader { kind, .. } => match kind {
                DownloaderKind::Gallery => 1,
                DownloaderKind::Urls => 7,
                DownloaderKind::Watchers => 9,
            },
            PageContent::Duplicates { .. } => 8,
            PageContent::Other { page_type, .. } => *page_type,
        }
    }

    /// How the page sorts its files, if it has files and a sort.
    pub fn sort(&self) -> Option<&PageSort> {
        match self {
            PageContent::Pages(_) => None,
            PageContent::Search { sort, .. }
            | PageContent::Downloader { sort, .. }
            | PageContent::Duplicates { sort, .. }
            | PageContent::Other { sort, .. } => sort.as_ref(),
        }
    }
}

/// A duplicates page's filtering: which potential pairs, in what order,
/// a batch or a group at a time.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DuplicatesPage {
    pub search: DuplicatesSearch,
    /// Whether the page's searches run as their predicates change.
    pub synchronised: bool,
    pub order: PairOrder,
    pub ascending: bool,
    /// Filter one group of potential duplicates at a time.
    pub group_mode: bool,
}

impl DuplicatesPage {
    /// A new duplicates page's (the reference's
    /// `CreatePageManagerDuplicateFilter`): every file in all my files,
    /// within the default distance, largest files first.
    pub fn new(search: DuplicatesSearch) -> Self {
        Self {
            search,
            synchronised: true,
            order: PairOrder::MaxFilesize,
            ascending: false,
            group_mode: false,
        }
    }
}

/// A downloader page's own state, beyond its queues: the queue it shows,
/// and what the queues it makes get (the reference's
/// `MultipleGalleryImport` and `MultipleWatcherImport`).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct DownloaderPageState {
    /// The queue it shows ("highlighted"), one of its queues.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlighted: Option<i64>,
    /// The import options its new queues get.
    #[serde(default)]
    pub options: crate::import_options::ImportOptionsSlice,
    /// A gallery page's downloader, file limit and pend options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<GalleryPageState>,
    /// A watcher page's checker options for new watchers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checker: Option<crate::subscriptions::CheckerOptions>,
}

/// What a gallery page gives the queries entered into it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GalleryPageState {
    /// The downloader (gallery URL generator) new queries use: its key
    /// (hex) and name.
    pub gug_key: String,
    pub gug_name: String,
    /// Stop each new query after this many files (`None`: no limit).
    pub file_limit: Option<u64>,
    pub start_files_paused: bool,
    pub start_gallery_paused: bool,
    /// Skip a query the page already has from the same downloader.
    pub no_new_dupes: bool,
    /// Queries entered together become one ("3 queries").
    pub merge_pends: bool,
}

/// The kinds of downloader page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DownloaderKind {
    Gallery,
    Urls,
    Watchers,
}

/// How a page sorts its files (the reference's `MediaSort`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PageSort {
    pub by: PageSortBy,
    pub ascending: bool,
}

/// What a page sorts by.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageSortBy {
    /// A built-in sort: a `CC.SORT_FILES_BY_*` code, as the Client API's
    /// `file_sort_type` numbers them.
    System(i64),
    /// By the tags in these namespaces, as the tag display type
    /// (`ClientTags.TAG_DISPLAY_*`) shows them.
    Namespaces {
        namespaces: Vec<String>,
        tag_display_type: i64,
    },
    /// By rating on this service.
    Rating(ServiceKey),
}

/// How a page collects its files (the reference's `MediaCollect`): by the
/// tags in these namespaces and the ratings on these services.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PageCollect {
    pub namespaces: Vec<String>,
    pub ratings: Vec<ServiceKey>,
    /// Whether the files that match none collect together, or stay single.
    pub collect_unmatched: bool,
}

impl Default for PageCollect {
    /// The reference's: no collecting.
    fn default() -> Self {
        Self {
            namespaces: Vec::new(),
            ratings: Vec::new(),
            collect_unmatched: true,
        }
    }
}

impl PageCollect {
    /// Whether it collects anything (`DoesACollect`).
    pub fn collects(&self) -> bool {
        !self.namespaces.is_empty() || !self.ratings.is_empty()
    }
}

/// A page's item: one file, or a collection of files (in its order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageMedia {
    File(crate::HashId),
    Collection(Vec<crate::HashId>),
}

/// How pages sort by default: a new page's sort, the sort applied before
/// any other (which orders its ties), and the namespace sorts offered.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SortSettings {
    /// `default_sort`.
    pub default_sort: PageSort,
    /// `fallback_sort`.
    pub fallback_sort: PageSort,
    /// `default_namespace_sorts`.
    pub namespace_sorts: Vec<PageSort>,
    /// How a new page collects (`default_collect`).
    pub default_collect: PageCollect,
}

impl Default for SortSettings {
    /// The reference's: file size, smallest first; import time, oldest
    /// first; and series-creator-title-volume-chapter-page and
    /// creator-series-title-volume-chapter-page, as displayed.
    fn default() -> Self {
        let namespaces = |names: [&str; 6]| PageSort {
            by: PageSortBy::Namespaces {
                namespaces: names.iter().map(|&n| n.to_owned()).collect(),
                tag_display_type: 1,
            },
            ascending: true,
        };
        Self {
            default_sort: PageSort {
                by: PageSortBy::System(0),
                ascending: true,
            },
            fallback_sort: PageSort {
                by: PageSortBy::System(2),
                ascending: true,
            },
            namespace_sorts: vec![
                namespaces(["series", "creator", "title", "volume", "chapter", "page"]),
                namespaces(["creator", "series", "title", "volume", "chapter", "page"]),
            ],
            default_collect: PageCollect::default(),
        }
    }
}

/// A saved search (the reference's favourite searches), which a search
/// page can load: its domains, tag service and predicates, whether it
/// searches as they change, and its sort and collect.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FavouriteSearch {
    /// The menu folder, `/` separating nested ones; none for the top.
    pub folder: Option<String>,
    pub name: String,
    pub search: FileSearchContext,
    pub synchronised: bool,
    pub sort: Option<PageSort>,
    /// (one that collects nothing still uncollects the page)
    #[serde(default)]
    pub collect: Option<PageCollect>,
}

/// A page's key: 32 random bytes, written as hex.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PageKey(pub [u8; 32]);

impl PageKey {
    pub fn random() -> Self {
        Self(rand::random())
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        hex::decode(hex).ok()?.try_into().ok().map(Self)
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Debug for PageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PageKey({})", self.to_hex())
    }
}

impl serde::Serialize for PageKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> serde::Deserialize<'de> for PageKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let hex = String::deserialize(d)?;
        Self::from_hex(&hex).ok_or_else(|| serde::de::Error::custom("a page key is 64 hex digits"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_round_trips_through_json() {
        let session = Session {
            name: "last session".into(),
            pages: vec![Page {
                key: PageKey::random(),
                name: "pages".into(),
                content: PageContent::Pages(vec![Page {
                    key: PageKey::random(),
                    name: "my search".into(),
                    content: PageContent::Search {
                        search: FileSearchContext::default(),
                        synchronised: true,
                        sort: Some(PageSort {
                            by: PageSortBy::System(2),
                            ascending: false,
                        }),
                        lock: Some(HashLock {
                            syncs_new: false,
                            syncs_removes: true,
                        }),
                        collect: Some(PageCollect {
                            namespaces: vec!["series".into()],
                            ratings: Vec::new(),
                            collect_unmatched: false,
                        }),
                    },
                }]),
            }],
        };
        let json = serde_json::to_string(&session).unwrap();
        assert_eq!(serde_json::from_str::<Session>(&json).unwrap(), session);
        // a session saved before pages could be locked reads as unlocked
        let mut unlocked = session.clone();
        if let PageContent::Pages(children) = &mut unlocked.pages[0].content
            && let PageContent::Search { lock, .. } = &mut children[0].content
        {
            *lock = None;
        }
        let old = json.replace(r#","lock":{"syncs_new":false,"syncs_removes":true}"#, "");
        assert_ne!(old, json);
        assert_eq!(serde_json::from_str::<Session>(&old).unwrap(), unlocked);
        let names: Vec<&str> = session
            .all_pages()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["pages", "my search"]);
    }
}
