//! The session's pages: a tree of notebooks and pages, which of them is
//! shown, and the pages opened so far. Plain Rust, like [`SearchPage`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DownloaderKind, DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::SearchPage;
use crate::page_chooser::NewPage;

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
    /// Pages closed in the last hour, oldest first, to reopen.
    closed: Vec<Closed>,
}

/// A closed page, as it was: where it was, and its pages as opened.
struct Closed {
    at: std::time::Instant,
    /// The notebook it was in (`None`: the top one).
    notebook: Option<PageKey>,
    index: usize,
    page: Page,
    open: Vec<(PageKey, Rc<RefCell<SearchPage>>)>,
}

/// How long a closed page can be reopened (the reference's).
const CLOSED_PAGE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

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
            closed: Vec::new(),
        };
        pages.select(0, 0);
        Ok(pages)
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
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
            closed: Vec::new(),
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
            PageContent::Duplicates { duplicates, sort } => {
                SearchPage::duplicates_page(store, duplicates, sort.as_ref(), files)
            }
            // (a duplicates page an earlier import kept as stored)
            PageContent::Other {
                page_type: 8,
                stored: Some(stored),
                sort,
            } if hydrus_store::import::stored_duplicates_page(&stored).is_some() => {
                let duplicates =
                    hydrus_store::import::stored_duplicates_page(&stored).expect("read just now");
                SearchPage::duplicates_page(store, duplicates, sort.as_ref(), files)
            }
            PageContent::Other {
                page_type, sort, ..
            } => {
                let note = match page_type {
                    // (a saved session's, kept without its queues)
                    1 | 7 | 9 => {
                        let kind = match page_type {
                            1 => "gallery",
                            9 => "watcher",
                            _ => "url",
                        };
                        format!(
                            "A {kind} downloader page from a saved session. Its downloads were \
                             kept but don't run in hydrus-rs."
                        )
                    }
                    2 => "A simple downloader page, which hydrus-gui doesn't open yet.".into(),
                    3 => "An import from disk page, which hydrus-gui doesn't open yet.".into(),
                    5 => "A petitions page, which hydrus-gui doesn't open yet.".into(),
                    8 => "A duplicates page, which hydrus-gui doesn't open yet.".into(),
                    _ => "A kind of page hydrus-gui doesn't open yet.".into(),
                };
                SearchPage::fixed(store, note, sort.as_ref(), files)
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

impl Pages {
    /// The notebook `depth` levels down the way to the page shown.
    fn notebook_mut(&mut self, depth: usize) -> &mut Vec<Page> {
        let mut pages = &mut self.session.pages;
        for &i in &self.path[..depth] {
            match &mut pages[i].content {
                PageContent::Pages(children) => pages = children,
                _ => unreachable!("the path runs through notebooks"),
            }
        }
        pages
    }

    /// Where a new page goes: into the notebook shown if it is an empty
    /// one, else beside the page shown.
    fn current_depth(&self) -> usize {
        match &self.shown().content {
            PageContent::Pages(children) if children.is_empty() => self.path.len(),
            _ => self.path.len() - 1,
        }
    }

    /// Open a new search page (the reference's page chooser's "file search"
    /// on its default domain, "my files"), at the far right of the current
    /// notebook as the reference's default puts it, and show it.
    pub fn new_search_page(&mut self) {
        self.add(new_search_page());
    }

    /// Open a page of the kind chosen, at the far right of the current
    /// notebook (as the reference's page chooser does).
    pub fn new_page(&mut self, chosen: &NewPage) -> Result<(), String> {
        let page = match chosen {
            NewPage::Search { domain, .. } => {
                let mut page = new_search_page();
                if let PageContent::Search { search, .. } = &mut page.content {
                    search.location = hydrus_search::LocationContext::single(domain.clone());
                }
                page
            }
            NewPage::Duplicates => Page {
                key: PageKey::random(),
                name: "duplicates".into(),
                content: PageContent::Duplicates {
                    duplicates: new_duplicates_page(),
                    sort: None,
                },
            },
            NewPage::Pages => Page {
                key: PageKey::random(),
                name: "pages".into(),
                content: PageContent::Pages(Vec::new()),
            },
            NewPage::Session(name) => return self.append_session(name),
            NewPage::Urls | NewPage::Watcher | NewPage::Gallery | NewPage::SimpleDownloader => {
                return Err(
                    "hydrus-gui can't open downloader pages yet (`hydrus serve` runs the \
                     downloaders, and the Client API can add to them)"
                        .into(),
                );
            }
        };
        self.add(page);
        Ok(())
    }

    /// Append the saved session `name` as a page of pages named after it,
    /// at the far right of the current notebook, and show it (the
    /// reference's "append session"). Its pages are copies, with their
    /// files, so the saved session stays as it was.
    pub fn append_session(&mut self, name: &str) -> Result<(), String> {
        fn rekey(pages: &mut [Page], copies: &mut Vec<(PageKey, PageKey)>) {
            for page in pages {
                let new = PageKey::random();
                copies.push((page.key, new));
                page.key = new;
                if let PageContent::Pages(children) = &mut page.content {
                    rekey(children, copies);
                }
            }
        }
        let saved = self
            .store
            .read(|conn| sessions::load(conn, name))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("there is no saved session \"{name}\""))?;
        let mut pages = saved.pages;
        let mut copies = Vec::new();
        rekey(&mut pages, &mut copies);
        self.store
            .write(move |ctx| {
                for (old, new) in &copies {
                    let files = sessions::page_files(ctx.conn(), old)?;
                    if !files.is_empty() {
                        sessions::set_page_files(ctx.conn(), new, &files)?;
                    }
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        self.add(Page {
            key: PageKey::random(),
            name: name.to_owned(),
            content: PageContent::Pages(pages),
        });
        Ok(())
    }

    /// Add `page` at the far right of the current notebook, and show it
    /// (a notebook of pages, on its first page).
    fn add(&mut self, page: Page) {
        let depth = self.current_depth();
        let pages = self.notebook_mut(depth);
        pages.push(page);
        let index = pages.len() - 1;
        self.path.truncate(depth);
        self.select(depth, index);
    }

    /// Close the page shown (or the empty notebook shown).
    pub fn close_shown(&mut self) -> Result<(), String> {
        let depth = self.path.len() - 1;
        self.close(depth, self.path[depth])
    }

    /// Close the `index`th tab of the notebook `depth` levels down the way
    /// to the page shown (a notebook closes with its pages). If it was
    /// shown, the one to its right is shown (or, if it was last, its left),
    /// as the reference does. Downloader pages are kept: their queues would
    /// run on without them.
    pub fn close(&mut self, depth: usize, index: usize) -> Result<(), String> {
        fn has_downloader(page: &Page) -> bool {
            match &page.content {
                PageContent::Downloader { .. } => true,
                PageContent::Pages(children) => children.iter().any(has_downloader),
                _ => false,
            }
        }
        fn keys(page: &Page, out: &mut Vec<PageKey>) {
            out.push(page.key);
            if let PageContent::Pages(children) = &page.content {
                for child in children {
                    keys(child, out);
                }
            }
        }
        if depth >= self.path.len() {
            return Ok(());
        }
        let shown = self.path[depth];
        let notebook = depth.checked_sub(1).map(|_| self.notebook_key(depth));
        let pages = self.notebook_mut(depth);
        let Some(page) = pages.get(index) else {
            return Ok(());
        };
        if has_downloader(page) {
            return Err(
                "downloader pages can't be closed yet: `hydrus serve` runs their queues".into(),
            );
        }
        let closed = pages.remove(index);
        let remaining = pages.len();
        let mut closed_keys = Vec::new();
        keys(&closed, &mut closed_keys);
        let open = closed_keys
            .into_iter()
            .filter_map(|key| self.open.remove(&key).map(|page| (key, page)))
            .collect();
        let now = std::time::Instant::now();
        self.closed
            .retain(|c| now.duration_since(c.at) < CLOSED_PAGE_TIMEOUT);
        self.closed.push(Closed {
            at: now,
            notebook: notebook.flatten(),
            index,
            page: closed,
            open,
        });
        if index < shown {
            self.path[depth] -= 1;
        } else if index == shown {
            if remaining > 0 {
                self.select(depth, index.min(remaining - 1));
            } else if depth > 0 {
                // the notebook it was in is shown, empty
                self.path.truncate(depth);
            } else {
                // as the reference, never without a page
                self.session.pages.push(new_search_page());
                self.path = vec![0];
            }
        }
        Ok(())
    }
}

impl Pages {
    /// The key of the notebook `depth` levels down the way to the page
    /// shown (`None` for the top one).
    fn notebook_key(&self, depth: usize) -> Option<PageKey> {
        let mut pages = self.session.pages.as_slice();
        let mut key = None;
        for &i in &self.path[..depth] {
            let page = &pages[i];
            key = Some(page.key);
            match &page.content {
                PageContent::Pages(children) => pages = children,
                _ => return None,
            }
        }
        key
    }

    /// How many closed pages can be reopened.
    pub fn closed_count(&self) -> usize {
        let now = std::time::Instant::now();
        self.closed
            .iter()
            .filter(|c| now.duration_since(c.at) < CLOSED_PAGE_TIMEOUT)
            .count()
    }

    /// Reopen the page closed most recently (in the last hour), where it
    /// was, as it was, and show it (the reference's "unclose page"); its
    /// notebook gone, it goes in the top one. Whether there was one.
    pub fn unclose(&mut self) -> bool {
        fn path_to(pages: &[Page], key: PageKey) -> Option<Vec<usize>> {
            for (i, page) in pages.iter().enumerate() {
                if page.key == key {
                    return Some(vec![i]);
                }
                if let PageContent::Pages(children) = &page.content
                    && let Some(mut rest) = path_to(children, key)
                {
                    rest.insert(0, i);
                    return Some(rest);
                }
            }
            None
        }
        let now = std::time::Instant::now();
        self.closed
            .retain(|c| now.duration_since(c.at) < CLOSED_PAGE_TIMEOUT);
        let Some(closed) = self.closed.pop() else {
            return false;
        };
        let notebook = closed
            .notebook
            .and_then(|key| path_to(&self.session.pages, key));
        let pages = match &notebook {
            None => &mut self.session.pages,
            Some(path) => {
                let mut pages = &mut self.session.pages;
                for &i in path {
                    match &mut pages[i].content {
                        PageContent::Pages(children) => pages = children,
                        _ => unreachable!("a closed page's notebook is a notebook"),
                    }
                }
                pages
            }
        };
        let index = closed.index.min(pages.len());
        pages.insert(index, closed.page);
        self.open.extend(closed.open);
        let depth = notebook.as_ref().map_or(0, Vec::len);
        self.path = notebook.unwrap_or_default();
        self.select(depth, index);
        true
    }
}

/// A new search page, as the reference makes one: "files", searching "my
/// files" and all known tags.
/// A new duplicates page's search (the reference's
/// `CreatePageManagerDuplicateFilter`): every file in all my files, a pair
/// matching if one of its files does, within distance 4.
fn new_duplicates_page() -> DuplicatesPage {
    let search = FileSearchContext {
        location: hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
        )),
        predicates: vec![hydrus_search::Predicate::System(
            hydrus_search::SystemPredicate::Everything,
        )],
        ..FileSearchContext::default()
    };
    DuplicatesPage::new(DuplicatesSearch {
        search_1: search.clone(),
        search_2: search,
        kind: PairSearchKind::OneFileMatchesOneSearch,
        pixel_duplicates: PixelDuplicates::Allowed,
        max_hamming_distance: 4,
    })
}

fn new_search_page() -> Page {
    Page {
        key: PageKey::random(),
        name: "files".into(),
        content: PageContent::Search {
            search: FileSearchContext {
                location: hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                    hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
                )),
                ..FileSearchContext::default()
            },
            synchronised: true,
            sort: None,
        },
    }
}
