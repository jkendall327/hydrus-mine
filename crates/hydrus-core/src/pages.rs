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
    },
    /// A downloader page, showing these import queues (`queues` ids).
    Downloader {
        kind: DownloaderKind,
        queues: Vec<i64>,
        sort: Option<PageSort>,
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

/// A saved search (the reference's favourite searches), which a search
/// page can load: its domains, tag service and predicates, whether it
/// searches as they change, and its sort.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FavouriteSearch {
    /// The menu folder, `/` separating nested ones; none for the top.
    pub folder: Option<String>,
    pub name: String,
    pub search: FileSearchContext,
    pub synchronised: bool,
    pub sort: Option<PageSort>,
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
