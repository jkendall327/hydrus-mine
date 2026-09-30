//! The session's pages: a tree of notebooks and pages, which of them is
//! shown, and the pages opened so far. Plain Rust, like [`SearchPage`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::pages::{DownloaderKind, Page, PageContent, PageKey, Session};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::SearchPage;

/// One notebook's tabs: its pages' names, and which is shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tabs {
    pub names: Vec<String>,
    pub selected: usize,
}

pub struct Pages {
    store: Arc<Store>,
    session: Session,
    /// The shown page's index in each notebook on the way to it.
    path: Vec<usize>,
    /// Pages opened so far, by key.
    open: HashMap<PageKey, Rc<RefCell<SearchPage>>>,
}

impl std::fmt::Debug for Pages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pages")
            .field("session", &self.session.name)
            .field("path", &self.path)
            .field("open", &self.open.len())
            .finish_non_exhaustive()
    }
}

impl Pages {
    /// The last session, as the reference starts with it; with none (or an
    /// empty one), a single empty search page.
    pub fn open(store: Arc<Store>) -> hydrus_store::Result<Self> {
        let session = store
            .read(|conn| sessions::load(conn, LAST_SESSION))?
            .filter(|s| !s.pages.is_empty())
            .unwrap_or_else(|| Session {
                name: LAST_SESSION.to_owned(),
                pages: vec![new_search_page()],
            });
        let mut pages = Self {
            store,
            session,
            path: Vec::new(),
            open: HashMap::new(),
        };
        pages.select(0, 0);
        Ok(pages)
    }

    /// One page, already open.
    pub fn single(page: SearchPage) -> Self {
        let tree = new_search_page();
        let mut pages = Self {
            store: page.store().clone(),
            session: Session {
                name: LAST_SESSION.to_owned(),
                pages: vec![tree.clone()],
            },
            path: vec![0],
            open: HashMap::new(),
        };
        pages.open.insert(tree.key, Rc::new(RefCell::new(page)));
        pages
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The tabs of each notebook on the way to the shown page, from the top.
    pub fn tabs(&self) -> Vec<Tabs> {
        let mut rows = Vec::new();
        let mut pages = self.session.pages.as_slice();
        for &selected in &self.path {
            rows.push(Tabs {
                names: pages.iter().map(|p| p.name.clone()).collect(),
                selected,
            });
            match &pages[selected].content {
                PageContent::Pages(children) => pages = children,
                _ => break,
            }
        }
        rows
    }

    /// Show the `index`th page of the notebook at `level` (0 is the top):
    /// within a notebook, its first page.
    pub fn select(&mut self, level: usize, index: usize) {
        if level > self.path.len() {
            return;
        }
        let mut pages = self.session.pages.as_slice();
        for &i in &self.path[..level] {
            match &pages[i].content {
                PageContent::Pages(children) => pages = children,
                _ => return,
            }
        }
        if index >= pages.len() {
            return;
        }
        self.path.truncate(level);
        self.path.push(index);
        let mut page = &pages[index];
        while let PageContent::Pages(children) = &page.content {
            let Some(first) = children.first() else {
                break;
            };
            self.path.push(0);
            page = first;
        }
    }

    /// The page shown: a page, or a notebook with no pages.
    pub fn shown(&self) -> &Page {
        let mut pages = self.session.pages.as_slice();
        let mut page = &pages[self.path[0]];
        for &i in &self.path[1..] {
            let PageContent::Pages(children) = &page.content else {
                break;
            };
            pages = children;
            page = &pages[i];
        }
        page
    }

    /// The page shown, opened.
    pub fn current(&mut self) -> Rc<RefCell<SearchPage>> {
        let page = self.shown().clone();
        if let Some(open) = self.open.get(&page.key) {
            return open.clone();
        }
        let store = self.store.clone();
        let files = store
            .read(|conn| sessions::page_files(conn, &page.key))
            .unwrap_or_default();
        let opened = match page.content {
            PageContent::Search {
                search,
                synchronised,
                sort,
            } => SearchPage::restored(store, search, synchronised, sort.as_ref(), files),
            PageContent::Downloader { kind, queues, sort } => {
                let kind = match kind {
                    DownloaderKind::Gallery => "gallery",
                    DownloaderKind::Urls => "url",
                    DownloaderKind::Watchers => "watcher",
                };
                let queues = match queues.len() {
                    1 => "its queue".to_owned(),
                    n => format!("its {n} queues"),
                };
                SearchPage::fixed(
                    store,
                    format!(
                        "A {kind} downloader page. `hydrus serve` runs {queues}; hydrus-gui \
                         doesn't show them yet."
                    ),
                    sort.as_ref(),
                    files,
                )
            }
            PageContent::Other {
                page_type, sort, ..
            } => {
                let kind = match page_type {
                    2 => "simple downloader",
                    3 => "import from disk",
                    5 => "petitions",
                    8 => "duplicates",
                    _ => "kind of",
                };
                SearchPage::fixed(
                    store,
                    format!("A {kind} page, which hydrus-gui doesn't open yet."),
                    sort.as_ref(),
                    files,
                )
            }
            PageContent::Pages(_) => {
                SearchPage::fixed(store, "An empty page of pages.", None, files)
            }
        };
        let opened = Rc::new(RefCell::new(opened));
        self.open.insert(page.key, opened.clone());
        opened
    }
}

impl Pages {
    /// Save the pages as the last session: each page opened as it is now,
    /// with the files it shows, and the rest as they were.
    pub fn save(&mut self, now: i64) -> hydrus_store::Result<()> {
        fn update(
            pages: &mut [Page],
            open: &HashMap<PageKey, Rc<RefCell<SearchPage>>>,
            files: &mut Vec<(PageKey, Vec<hydrus_core::HashId>)>,
        ) {
            for page in pages {
                if let PageContent::Pages(children) = &mut page.content {
                    update(children, open, files);
                } else if let Some(opened) = open.get(&page.key) {
                    let opened = opened.borrow();
                    page.content = opened.content(&page.content);
                    files.push((page.key, opened.results().to_vec()));
                }
            }
        }
        let mut files = Vec::new();
        update(&mut self.session.pages, &self.open, &mut files);
        let session = self.session.clone();
        self.store.write(move |ctx| {
            let conn = ctx.conn();
            sessions::save(conn, &session, now)?;
            for (key, files) in &files {
                sessions::set_page_files(conn, key, files)?;
            }
            Ok(())
        })
    }
}

/// A new search page, as the reference makes one: "files", searching
/// everything in the default domains.
fn new_search_page() -> Page {
    Page {
        key: PageKey::random(),
        name: "files".into(),
        content: PageContent::Search {
            search: FileSearchContext::default(),
            synchronised: true,
            sort: None,
        },
    }
}
