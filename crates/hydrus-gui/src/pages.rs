//! The session's pages: a tree of notebooks and pages, which of them is
//! shown, and the pages opened so far. Plain Rust, like [`SearchPage`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
use hydrus_core::pages::{DownloaderKind, DuplicatesPage, Page, PageContent, PageKey, Session};
use hydrus_search::FileSearchContext;
use hydrus_store::Store;
use hydrus_store::sessions::{self, LAST_SESSION};

use crate::SearchPage;
use crate::page_chooser::NewPage;
use hydrus_core::pages::{DownloaderPageSettings, PageNameSettings, TabKind, tab_name};

/// The downloader queues of a page and the pages in it, which wait while
/// it is closed.
fn closable_queues(page: &Page) -> Vec<i64> {
    match &page.content {
        PageContent::Downloader { queues, .. } => queues.clone(),
        PageContent::Pages(children) => children.iter().flat_map(closable_queues).collect(),
        _ => Vec::new(),
    }
}

/// The page name of a new gallery downloader page.
const GALLERY_PAGE_NAME: &str = "gallery";

/// Bring pages' content up to date from those open.
/// `pages` as copies, each with a new key and its page's files, so the
/// originals can change or go without them.
fn copied(store: &hydrus_store::Store, mut pages: Vec<Page>) -> hydrus_store::Result<Vec<Page>> {
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
    let mut copies = Vec::new();
    rekey(&mut pages, &mut copies);
    store.write(move |ctx| {
        for (old, new) in &copies {
            let files = sessions::page_files(ctx.conn(), old)?;
            if !files.is_empty() {
                sessions::set_page_files(ctx.conn(), new, &files)?;
            }
        }
        Ok(())
    })?;
    Ok(pages)
}

fn refresh_contents(pages: &mut [Page], open: &HashMap<PageKey, Rc<RefCell<SearchPage>>>) {
    for page in pages {
        if let PageContent::Pages(children) = &mut page.content {
            refresh_contents(children, open);
        } else if let Some(opened) = open.get(&page.key) {
            page.content = opened.borrow().content(&page.content);
        }
    }
}

/// What a page's tab is for: importing pages are the reference's own
/// (`IsImporter`: hard drive, simple downloader, gallery, watcher and URL
/// pages).
fn tab_kind(page: &Page) -> TabKind {
    match &page.content {
        PageContent::Pages(_) => TabKind::Notebook,
        PageContent::Downloader { .. }
        | PageContent::Other {
            page_type: 1 | 2 | 3 | 4 | 7 | 9,
            ..
        } => TabKind::Importer,
        _ => TabKind::Page,
    }
}

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
    /// The pages shown, the latest last, with their names for menus as
    /// they were then (the reference's `PagesHistory`).
    history: Vec<(PageKey, String)>,
    /// The page each notebook showed last, to show again (as a tab widget
    /// keeps its tab): by notebook.
    remembered: HashMap<PageKey, usize>,
    /// When each notebook last moved its selection by ctrl+page up or down
    /// (`None`: the top one).
    last_moved: HashMap<Option<PageKey>, std::time::Instant>,
    /// What [`Pages::sync`] last wrote to the store.
    synced: Synced,
    /// How tabs are named.
    naming: PageNameSettings,
    /// Downloader pages' options (whether closing a full one asks).
    downloader_options: DownloaderPageSettings,
    /// How many files each page not yet opened shows, as kept.
    kept_counts: HashMap<PageKey, usize>,
    /// The notebook (by depth) the next new page goes in, when one was
    /// chosen for it (its tab row's empty space double-clicked).
    new_page_depth: Option<usize>,
}

/// What the store was last told of the pages, so only changes are written.
#[derive(Debug, Default)]
struct Synced {
    pages: Option<Vec<Page>>,
    shown: Option<PageKey>,
    /// Each opened page's files and selected files.
    media: HashMap<PageKey, (Vec<HashId>, Vec<HashId>)>,
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

/// A new URL downloader page's name (the reference's).
const URL_PAGE_NAME: &str = "url import";
/// A local import page's name, as the reference names it.
const LOCAL_IMPORT_PAGE_NAME: &str = "import";

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
        // (gallery and watcher pages an earlier import left without their
        // own state get it back, once)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
        store.write(move |ctx| {
            hydrus_store::import::fill_downloader_page_state(ctx.conn(), now).map(|_| ())
        })?;
        let session = store
            .read(|conn| sessions::load(conn, LAST_SESSION))?
            .filter(|s| !s.pages.is_empty())
            .unwrap_or_else(|| Session {
                name: LAST_SESSION.to_owned(),
                pages: vec![new_search_page(&store)],
            });
        let mut pages = Self {
            store,
            session,
            path: Vec::new(),
            open: HashMap::new(),
            closed: Vec::new(),
            history: Vec::new(),
            new_page_depth: None,
            remembered: HashMap::new(),
            last_moved: HashMap::new(),
            synced: Synced::default(),
            naming: PageNameSettings::default(),
            downloader_options: DownloaderPageSettings::default(),
            kept_counts: HashMap::new(),
        };
        // (pages closed when the client last closed, or crashed, go with
        // their downloads)
        pages
            .store
            .write(|ctx| hydrus_store::queues::delete_closed_queues(ctx.conn()))?;
        (pages.naming, pages.downloader_options, pages.kept_counts) = pages.store.read(|conn| {
            Ok((
                hydrus_store::settings::get(conn)?,
                hydrus_store::settings::get(conn)?,
                sessions::page_file_counts(conn)?,
            ))
        })?;
        pages.select(0, 0);
        // (on the page shown last, or the Client API asked for since)
        let shown = pages
            .store
            .read(|conn| sessions::shown(conn, LAST_SESSION))?;
        if let Some(shown) = shown {
            pages.show(&shown);
        }
        Ok(pages)
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// One page, already open.
    pub fn single(page: SearchPage) -> Self {
        let tree = new_search_page(page.store());
        let mut pages = Self {
            store: page.store().clone(),
            session: Session {
                name: LAST_SESSION.to_owned(),
                pages: vec![tree.clone()],
            },
            path: vec![0],
            open: HashMap::new(),
            closed: Vec::new(),
            history: Vec::new(),
            new_page_depth: None,
            remembered: HashMap::new(),
            last_moved: HashMap::new(),
            synced: Synced::default(),
            naming: PageNameSettings::default(),
            downloader_options: DownloaderPageSettings::default(),
            kept_counts: HashMap::new(),
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

    /// The tabs' names, as the reference writes them, for each notebook
    /// on the way to the page shown (as [`Pages::tabs`]): each page's name
    /// with its number of files and import progress.
    pub fn tab_labels(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        let mut pages = self.session.pages.as_slice();
        for &selected in &self.path {
            rows.push(
                pages
                    .iter()
                    .map(|page| {
                        let (files, progress) = self.file_summary(page);
                        tab_name(&page.name, tab_kind(page), files, progress, &self.naming)
                    })
                    .collect(),
            );
            match &pages[selected].content {
                PageContent::Pages(children) => pages = children,
                _ => break,
            }
        }
        rows
    }

    /// A page's number of files and import progress, a notebook's pages'
    /// together (`GetNumFileSummary`): progress that is done counts as none.
    fn file_summary(&self, page: &Page) -> (usize, (usize, usize)) {
        self.file_summary_with(page, &[])
    }

    /// As [`Pages::file_summary`], the pages opened being those open or
    /// among `also` (a closed page's).
    fn file_summary_with(
        &self,
        page: &Page,
        also: &[(PageKey, Rc<RefCell<SearchPage>>)],
    ) -> (usize, (usize, usize)) {
        if let PageContent::Pages(children) = &page.content {
            return children
                .iter()
                .map(|child| self.file_summary_with(child, also))
                .fold((0, (0, 0)), |(f, (v, r)), (cf, (cv, cr))| {
                    (f + cf, (v + cv, r + cr))
                });
        }
        let opened = self.open.get(&page.key).or_else(|| {
            also.iter()
                .find(|(key, _)| *key == page.key)
                .map(|(_, opened)| opened)
        });
        let files = match opened {
            Some(opened) => opened.borrow().files().len(),
            None => self.kept_counts.get(&page.key).copied().unwrap_or(0),
        };
        let progress = match opened {
            Some(opened) => opened.borrow().import_progress(),
            None => (0, 0),
        };
        let progress = if progress.0 == progress.1 {
            (0, 0)
        } else {
            progress
        };
        (files, progress)
    }

    /// Show the `index`th page of the notebook at `level` (0 is the top):
    /// within a notebook, the page it showed last (its first, to begin
    /// with).
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
            if children.is_empty() {
                break;
            }
            let at = self
                .remembered
                .get(&page.key)
                .map_or(0, |&i| i.min(children.len() - 1));
            self.path.push(at);
            page = &children[at];
        }
        self.remember();
    }

    /// Show the page with `key`, wherever it is (a notebook shows the page
    /// it showed last, its first to begin with): whether there is one.
    pub fn show(&mut self, key: &PageKey) -> bool {
        fn path_to(pages: &[Page], key: &PageKey, path: &mut Vec<usize>) -> bool {
            for (i, page) in pages.iter().enumerate() {
                path.push(i);
                if page.key == *key {
                    return true;
                }
                if let PageContent::Pages(children) = &page.content
                    && path_to(children, key, path)
                {
                    return true;
                }
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if !path_to(&self.session.pages, key, &mut path) {
            return false;
        }
        for (level, &index) in path.iter().enumerate() {
            self.select(level, index);
        }
        true
    }

    /// The pages (not notebooks) at or under the page with `key`; all of
    /// them for a key that isn't a page's (the top notebook's).
    pub fn pages_under(&self, key: &PageKey) -> Vec<PageKey> {
        fn media(pages: &[Page], out: &mut Vec<PageKey>) {
            for page in pages {
                match &page.content {
                    PageContent::Pages(children) => media(children, out),
                    _ => out.push(page.key),
                }
            }
        }
        let mut out = Vec::new();
        match self.session.all_pages().into_iter().find(|p| p.key == *key) {
            Some(page) => media(std::slice::from_ref(page), &mut out),
            None => media(&self.session.pages, &mut out),
        }
        out
    }

    /// Note the page each notebook on the way to the page shown shows.
    fn remember(&mut self) {
        let mut pages = self.session.pages.as_slice();
        for (depth, &i) in self.path.iter().enumerate() {
            let page = &pages[i];
            let PageContent::Pages(children) = &page.content else {
                break;
            };
            if let Some(&child) = self.path.get(depth + 1) {
                self.remembered.insert(page.key, child);
            }
            pages = children;
        }
    }

    /// Show the page `delta` along (ctrl+page up and down,
    /// `MoveSelection`): in the deepest notebook on the way to the page
    /// shown that can move so, unless a notebook above it moved in the
    /// last three seconds, which moves again (so a held key runs along
    /// its tabs, not into them); never round the ends.
    pub fn move_selection(&mut self, delta: isize, now: std::time::Instant) -> bool {
        self.move_at(0, delta, false, now)
    }

    fn move_at(&mut self, depth: usize, delta: isize, test: bool, now: std::time::Instant) -> bool {
        const RECENT: std::time::Duration = std::time::Duration::from_secs(3);
        let Some(&current) = self.path.get(depth) else {
            return false;
        };
        let (count, current_is_notebook) = {
            let mut pages = self.session.pages.as_slice();
            for &i in &self.path[..depth] {
                match &pages[i].content {
                    PageContent::Pages(children) => pages = children,
                    _ => return false,
                }
            }
            (
                pages.len(),
                matches!(pages[current].content, PageContent::Pages(_)),
            )
        };
        if count <= 1 {
            return false;
        }
        let key = self.notebook_key(depth);
        let recent = self
            .last_moved
            .get(&key)
            .is_some_and(|&at| now.duration_since(at) < RECENT);
        if current_is_notebook && !recent && self.move_at(depth + 1, delta, true, now) {
            return self.move_at(depth + 1, delta, test, now);
        }
        let Some(new) = current.checked_add_signed(delta).filter(|&i| i < count) else {
            return false;
        };
        if !test {
            self.select(depth, new);
            self.last_moved.insert(key, now);
        }
        true
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
        self.opened(page)
    }

    /// The page (not a notebook) with `key`, opened.
    pub fn page(&mut self, key: &PageKey) -> Option<Rc<RefCell<SearchPage>>> {
        let page = self
            .session
            .all_pages()
            .into_iter()
            .find(|p| p.key == *key && !matches!(p.content, PageContent::Pages(_)))?
            .clone();
        Some(self.opened(page))
    }

    fn opened(&mut self, page: Page) -> Rc<RefCell<SearchPage>> {
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
                lock,
                collect,
            } => SearchPage::restored(store, search, synchronised, sort.as_ref(), files)
                .with_lock(lock)
                .with_collect(collect),
            PageContent::Downloader {
                kind: DownloaderKind::Urls,
                queues,
                sort,
                ..
            } if queues.len() == 1 => {
                SearchPage::url_downloader(store, queues[0], sort.as_ref(), files)
            }
            PageContent::Downloader {
                kind: DownloaderKind::Local,
                queues,
                sort,
                ..
            } if queues.len() == 1 => {
                SearchPage::local_import(store, queues[0], sort.as_ref(), files)
            }
            PageContent::Downloader {
                kind: DownloaderKind::Gallery,
                queues,
                sort,
                page: state,
            } => SearchPage::gallery_downloader(
                store,
                page.key,
                &page.name,
                queues,
                state.map(|s| *s),
                sort.as_ref(),
                files,
            ),
            PageContent::Downloader {
                kind: DownloaderKind::Watchers,
                queues,
                sort,
                page: state,
            } => SearchPage::watcher_downloader(
                store,
                page.key,
                &page.name,
                queues,
                state.map(|s| *s),
                sort.as_ref(),
                files,
            ),
            PageContent::Downloader {
                kind, queues, sort, ..
            } => {
                // (and what the reference's says while empty)
                let (kind, empty) = match kind {
                    DownloaderKind::Gallery => ("gallery", "no highlighted query"),
                    DownloaderKind::Urls => ("url", "empty page"),
                    DownloaderKind::Watchers => ("watcher", "no highlighted watcher"),
                    DownloaderKind::Local => ("local import", "empty page"),
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
                .with_empty_status(empty)
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
    /// with the files it shows and those selected, the rest as they were,
    /// and the page shown.
    pub fn save(&mut self, now: i64) -> hydrus_store::Result<()> {
        self.synced = Synced::default();
        self.sync(now)
    }

    /// Keep the last session up to date in the store, as [`Pages::save`]
    /// does, writing only what changed since last time: the Client API's
    /// `/manage_pages` answers from it.
    pub fn sync(&mut self, now: i64) -> hydrus_store::Result<()> {
        type Media = Vec<(PageKey, Vec<HashId>, Vec<HashId>)>;
        fn update(
            pages: &mut [Page],
            open: &HashMap<PageKey, Rc<RefCell<SearchPage>>>,
            media: &mut Media,
        ) {
            for page in pages {
                if let PageContent::Pages(children) = &mut page.content {
                    update(children, open, media);
                } else if let Some(opened) = open.get(&page.key) {
                    let opened = opened.borrow();
                    page.content = opened.content(&page.content);
                    media.push((page.key, opened.files(), opened.selected_files()));
                }
            }
        }
        let mut media = Vec::new();
        update(&mut self.session.pages, &self.open, &mut media);
        media.retain(|(key, files, selected)| {
            self.synced
                .media
                .get(key)
                .is_none_or(|(f, s)| f != files || s != selected)
        });
        let session =
            (self.synced.pages.as_ref() != Some(&self.session.pages)).then(|| self.session.clone());
        let shown = self.shown().key;
        let shown_changed = self.synced.shown != Some(shown);
        if session.is_none() && !shown_changed && media.is_empty() {
            return Ok(());
        }
        let name = self.session.name.clone();
        let written = self.store.write(move |ctx| {
            let conn = ctx.conn();
            if let Some(session) = &session {
                sessions::save(conn, session, now)?;
            }
            if shown_changed {
                sessions::set_shown(conn, &name, Some(&shown))?;
            }
            for (key, files, selected) in &media {
                sessions::set_page_files(conn, key, files)?;
                sessions::set_page_selected(conn, key, selected)?;
            }
            Ok(media)
        })?;
        self.synced.pages = Some(self.session.pages.clone());
        self.synced.shown = Some(shown);
        for (key, files, selected) in written {
            self.synced.media.insert(key, (files, selected));
        }
        Ok(())
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

    /// Put the next new page in the notebook `depth` levels down the way to
    /// the page shown, as the reference's notebook does when its tab bar's
    /// empty space is double-clicked (`ChooseNewPage` on that notebook);
    /// `None` for the current notebook.
    pub fn new_page_in(&mut self, depth: Option<usize>) {
        self.new_page_depth = depth;
    }

    /// Open a new search page (the reference's page chooser's "file search"
    /// on its default domain, "my files"), at the far right of the current
    /// notebook as the reference's default puts it, and show it.
    pub fn new_search_page(&mut self) {
        self.add(new_search_page(&self.store));
    }

    /// Open `files` in a new page searching `location` (the reference's
    /// "open in a new page", `ShowFilesInNewPage`), at the far right of
    /// the current notebook, and show it. Its search is locked to a
    /// `system:hash` of them, as the reference's `NewPageQuery` locks a
    /// page opened on files.
    pub fn open_files(
        &mut self,
        location: hydrus_search::LocationContext,
        files: Vec<hydrus_core::HashId>,
        sort: Option<&hydrus_core::pages::PageSort>,
        collect: Option<&hydrus_core::pages::PageCollect>,
    ) {
        let hashes = self
            .store
            .read(|c| hydrus_store::master::hashes(c, &files))
            .unwrap_or_default();
        let mut page = new_search_page_on(&self.store, location);
        let PageContent::Search {
            search,
            synchronised,
            lock,
            sort: page_sort,
            collect: page_collect,
        } = &mut page.content
        else {
            unreachable!("a search page");
        };
        // (the files keep their order, under the page's sort, and collect
        // as the page's did)
        *page_sort = sort.cloned();
        if let Some(collect) = collect {
            *page_collect = Some(collect.clone());
        }
        search.predicates = vec![hydrus_search::Predicate::System(
            hydrus_core::search::predicate::SystemPredicate::Hash {
                hashes: hydrus_core::search::predicate::FileHashes::Sha256(
                    hashes.into_values().collect(),
                ),
                inclusive: true,
            },
        )];
        *lock = Some(hydrus_core::pages::HashLock::default());
        let opened = SearchPage::restored(
            self.store.clone(),
            search.clone(),
            *synchronised,
            sort,
            files,
        )
        .with_lock(*lock)
        .with_collect(page_collect.clone());
        self.open.insert(page.key, Rc::new(RefCell::new(opened)));
        self.add(page);
    }

    /// Open a new page named `name` searching `location` for `predicates`,
    /// at the far right of the current notebook, and show it; it searches
    /// at once (the reference's `NewPageQuery` with initial predicates).
    pub fn open_search(
        &mut self,
        location: hydrus_search::LocationContext,
        predicates: Vec<hydrus_search::Predicate>,
        name: &str,
    ) {
        let mut page = new_search_page_on(&self.store, location);
        name.clone_into(&mut page.name);
        let PageContent::Search {
            search,
            synchronised,
            collect,
            ..
        } = &mut page.content
        else {
            unreachable!("a search page");
        };
        search.predicates = predicates;
        let mut opened = SearchPage::restored(
            self.store.clone(),
            search.clone(),
            *synchronised,
            None,
            Vec::new(),
        )
        .with_collect(collect.clone());
        opened.refresh();
        self.open.insert(page.key, Rc::new(RefCell::new(opened)));
        self.add(page);
    }

    /// Open a new duplicates page searching `location` for potential pairs
    /// among `files` (`ShowFilesInNewDuplicatesFilterPage`: both its
    /// searches a `system:hash` of them), at the far right of the current
    /// notebook, and show it.
    pub fn open_duplicates(
        &mut self,
        location: hydrus_search::LocationContext,
        files: &[hydrus_core::HashId],
    ) {
        let hashes = self
            .store
            .read(|c| hydrus_store::master::hashes(c, files))
            .unwrap_or_default();
        let predicates = vec![hydrus_search::Predicate::System(
            hydrus_core::search::predicate::SystemPredicate::Hash {
                hashes: hydrus_core::search::predicate::FileHashes::Sha256(
                    hashes.into_values().collect(),
                ),
                inclusive: true,
            },
        )];
        self.add(Page {
            key: PageKey::random(),
            name: "duplicates".into(),
            content: PageContent::Duplicates {
                duplicates: new_duplicates_page(location, predicates),
                sort: None,
            },
        });
    }

    /// Open a page of the kind chosen, at the far right of the current
    /// notebook (as the reference's page chooser does).
    pub fn new_page(&mut self, chosen: &NewPage) -> Result<(), String> {
        let page = match chosen {
            NewPage::Search { domain, .. } => new_search_page_on(
                &self.store,
                hydrus_search::LocationContext::single(domain.clone()),
            ),
            NewPage::Duplicates => Page {
                key: PageKey::random(),
                name: "duplicates".into(),
                content: PageContent::Duplicates {
                    duplicates: new_duplicates_page(
                        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
                            hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS
                                .to_vec(),
                        )),
                        vec![hydrus_search::Predicate::System(
                            hydrus_search::SystemPredicate::Everything,
                        )],
                    ),
                    sort: None,
                },
            },
            NewPage::Pages => Page {
                key: PageKey::random(),
                name: "pages".into(),
                content: PageContent::Pages(Vec::new()),
            },
            NewPage::Session(name) => return self.append_session(name),
            NewPage::LocalImport {
                paths,
                tags,
                delete_after_success,
            } => {
                let (paths, tags, delete_after_success) =
                    (paths.clone(), tags.clone(), *delete_after_success);
                let key = PageKey::random();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
                let queue = self
                    .store
                    .write(move |ctx| {
                        let conn = ctx.conn();
                        let queue = hydrus_store::queues::create_local_import(
                            conn,
                            Some(&key.0),
                            &hydrus_core::import_options::ImportOptionsSlice::default(),
                            &paths,
                            &tags,
                            hydrus_store::queues::LocalImport {
                                delete_after_success,
                                routers: Vec::new(),
                            },
                            now,
                        )?;
                        hydrus_store::queues::nudge(conn, queue)?;
                        Ok(queue)
                    })
                    .map_err(|e| format!("could not make the import: {e}"))?;
                Page {
                    key,
                    name: LOCAL_IMPORT_PAGE_NAME.into(),
                    content: PageContent::Downloader {
                        kind: DownloaderKind::Local,
                        queues: vec![queue],
                        sort: None,
                        page: None,
                    },
                }
            }
            NewPage::Urls => {
                let key = PageKey::random();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
                let queue = self
                    .store
                    .write(move |ctx| {
                        let conn = ctx.conn();
                        let queue = hydrus_store::queues::create_queue(
                            conn,
                            hydrus_store::queues::QueueKind::Urls,
                            URL_PAGE_NAME,
                            Some(&key.0),
                            &hydrus_core::import_options::ImportOptionsSlice::default(),
                            now,
                        )?;
                        hydrus_store::queues::nudge(conn, queue)?;
                        Ok(queue)
                    })
                    .map_err(|e| format!("could not make the page's queue: {e}"))?;
                Page {
                    key,
                    name: URL_PAGE_NAME.into(),
                    content: PageContent::Downloader {
                        kind: DownloaderKind::Urls,
                        queues: vec![queue],
                        sort: None,
                        page: None,
                    },
                }
            }
            NewPage::Gallery => Page {
                key: PageKey::random(),
                name: GALLERY_PAGE_NAME.into(),
                content: PageContent::Downloader {
                    kind: DownloaderKind::Gallery,
                    queues: Vec::new(),
                    sort: None,
                    page: Some(Box::new(crate::page::new_gallery_state(&self.store))),
                },
            },
            NewPage::Watcher => Page {
                key: PageKey::random(),
                name: hydrus_store::watchers::DEFAULT_WATCHER_PAGE_NAME.into(),
                content: PageContent::Downloader {
                    kind: DownloaderKind::Watchers,
                    queues: Vec::new(),
                    sort: None,
                    page: Some(Box::new(crate::page::new_watcher_state(&self.store))),
                },
            },
            NewPage::SimpleDownloader => {
                return Err("hydrus-gui can't open simple downloader pages yet".into());
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
        let saved = self
            .store
            .read(|conn| sessions::load(conn, name))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("there is no saved session \"{name}\""))?;
        let pages = copied(&self.store, saved.pages).map_err(|e| e.to_string())?;
        self.add(Page {
            key: PageKey::random(),
            name: name.to_owned(),
            content: PageContent::Pages(pages),
        });
        Ok(())
    }

    /// Close every page and load the saved session `name` in their place,
    /// its pages at the top (the reference's "clear and load": the pages
    /// closed are gone, not kept to reopen, and their downloads with them).
    pub fn clear_and_load(&mut self, name: &str) -> Result<(), String> {
        let saved = self
            .store
            .read(|conn| sessions::load(conn, name))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("there is no saved session \"{name}\""))?;
        let pages = copied(&self.store, saved.pages).map_err(|e| e.to_string())?;
        let mut old = std::mem::take(&mut self.session.pages);
        refresh_contents(&mut old, &self.open);
        let queues: Vec<i64> = old.iter().flat_map(closable_queues).collect();
        self.open.clear();
        self.delete_queues(queues);
        self.session.pages = if pages.is_empty() {
            vec![new_search_page(&self.store)]
        } else {
            pages
        };
        self.path = vec![0];
        self.select(0, 0);
        Ok(())
    }

    /// What every downloader page says against closing it for a session
    /// load (`CheckAbleToClose(for_session_close = True)`): `(reason, page
    /// name)`, in the pages' order.
    pub fn session_close_vetoes(&mut self) -> Vec<(String, String)> {
        fn walk(pages: &[Page], out: &mut Vec<(PageKey, String)>) {
            for page in pages {
                match &page.content {
                    PageContent::Pages(children) => walk(children, out),
                    PageContent::Downloader { .. } => out.push((page.key, page.name.clone())),
                    _ => {}
                }
            }
        }
        let mut downloaders = Vec::new();
        walk(&self.session.pages, &mut downloaders);
        downloaders
            .into_iter()
            .filter_map(|(key, name)| {
                let opened = self.page(&key)?;
                let veto = opened.borrow().close_veto(false)?;
                Some((veto, name))
            })
            .collect()
    }

    /// Save the open pages, as they are now, as the session `name`
    /// (replacing one of that name): copies, with their files, so the
    /// saved session stays as it is while the pages change.
    pub fn save_session(&mut self, name: &str, now: i64) -> Result<(), String> {
        self.sync(now).map_err(|e| e.to_string())?;
        let pages = copied(&self.store, self.session.pages.clone()).map_err(|e| e.to_string())?;
        let session = Session {
            name: name.to_owned(),
            pages,
        };
        self.store
            .write(move |ctx| sessions::save(ctx.conn(), &session, now))
            .map_err(|e| e.to_string())
    }

    /// Add `page` at the far right of the current notebook, and show it
    /// (a notebook of pages, on its first page).
    fn add(&mut self, page: Page) {
        let depth = self
            .new_page_depth
            .take()
            .filter(|&d| d <= self.current_depth())
            .unwrap_or_else(|| self.current_depth());
        let pages = self.notebook_mut(depth);
        pages.push(page);
        let index = pages.len() - 1;
        self.path.truncate(depth);
        self.select(depth, index);
    }

    /// Close the page shown (or the empty notebook shown).
    /// Where the page shown is: its depth, and its index in its notebook.
    pub fn shown_position(&self) -> (usize, usize) {
        let depth = self.path.len() - 1;
        (depth, self.path[depth])
    }

    pub fn close_shown(&mut self) -> Result<(), String> {
        let depth = self.path.len() - 1;
        self.close(depth, self.path[depth])
    }

    /// Close the `index`th tab of the notebook `depth` levels down the way
    /// to the page shown (a notebook closes with its pages). If it was
    /// shown, the one to its right is shown (or, if it was last, its left),
    /// as the reference does. A downloader page's queues wait while it is
    /// closed.
    pub fn close(&mut self, depth: usize, index: usize) -> Result<(), String> {
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
        if index >= pages.len() {
            return Ok(());
        }
        let mut closed = pages.remove(index);
        let remaining = pages.len();
        // (as it is now: a gallery or watcher page's queues made since it
        // opened)
        refresh_contents(std::slice::from_mut(&mut closed), &self.open);
        let mut closed_keys = Vec::new();
        keys(&closed, &mut closed_keys);
        let open = closed_keys
            .into_iter()
            .filter_map(|key| self.open.remove(&key).map(|page| (key, page)))
            .collect();
        let queues = closable_queues(&closed);
        self.close_queues(&queues, true);
        self.forget_old_closed();
        let now = std::time::Instant::now();
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
                self.session.pages.push(new_search_page(&self.store));
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
    /// Mark downloader queues' pages closed (they wait), or open again.
    fn close_queues(&self, queues: &[i64], closed: bool) {
        if queues.is_empty() {
            return;
        }
        let queues = queues.to_vec();
        if let Err(e) = self.store.write(move |ctx| {
            for queue in &queues {
                hydrus_store::queues::set_page_closed(ctx.conn(), *queue, closed)?;
            }
            Ok(())
        }) {
            eprintln!("could not close or reopen a downloader page's queue: {e}");
        }
    }

    /// Forget pages closed over an hour ago, as the reference does: their
    /// downloads go with them.
    fn forget_old_closed(&mut self) {
        let now = std::time::Instant::now();
        let (old, kept): (Vec<Closed>, Vec<Closed>) = self
            .closed
            .drain(..)
            .partition(|c| now.duration_since(c.at) >= CLOSED_PAGE_TIMEOUT);
        self.closed = kept;
        self.delete_queues(old.iter().flat_map(|c| closable_queues(&c.page)).collect());
    }

    /// Forget every closed page, their downloads with them (as the client
    /// closes: the reference's closed pages don't outlive it).
    pub fn forget_closed(&mut self) {
        let closed = std::mem::take(&mut self.closed);
        self.delete_queues(
            closed
                .iter()
                .flat_map(|c| closable_queues(&c.page))
                .collect(),
        );
    }

    fn delete_queues(&self, queues: Vec<i64>) {
        if queues.is_empty() {
            return;
        }
        if let Err(e) = self.store.write(move |ctx| {
            for queue in &queues {
                hydrus_store::queues::delete_queue(ctx.conn(), *queue)?;
            }
            Ok(())
        }) {
            eprintln!("could not delete a closed page's queue: {e}");
        }
    }

    /// What closing the page at `depth`'s `index` needs asking first, as
    /// the reference asks it (`AskIfAbleToClose`): for a URL downloader
    /// page still importing or holding imports.
    pub fn close_question(&mut self, depth: usize, index: usize) -> Option<String> {
        let page = self.notebook_at(depth)?.get(index)?.clone();
        if !matches!(
            page.content,
            PageContent::Downloader {
                kind: DownloaderKind::Urls
                    | DownloaderKind::Gallery
                    | DownloaderKind::Watchers
                    | DownloaderKind::Local,
                ..
            }
        ) {
            return None;
        }
        let opened = self.page(&page.key)?;
        let veto = opened
            .borrow()
            .close_veto(self.downloader_options.confirm_non_empty_close)?;
        Some(format!("Close \"{}\"?\n\n{veto}", page.name))
    }

    /// The pages of the notebook at `depth` on the way to the page shown
    /// (0 is the top).
    fn notebook_at(&self, depth: usize) -> Option<&[Page]> {
        let mut pages = self.session.pages.as_slice();
        for &i in self.path.get(..depth)? {
            match &pages.get(i)?.content {
                PageContent::Pages(children) => pages = children,
                _ => return None,
            }
        }
        Some(pages)
    }

    /// The pages opened so far.
    pub fn open_pages(&self) -> Vec<Rc<RefCell<SearchPage>>> {
        self.open.values().cloned().collect()
    }

    pub fn unclose(&mut self) -> bool {
        self.forget_old_closed();
        match self.closed.len().checked_sub(1) {
            Some(last) => self.unclose_at(last),
            None => false,
        }
    }

    /// Reopen the `index`th page closed in the last hour (oldest first),
    /// as [`Pages::unclose`] does the latest (the reference's undo menu's
    /// closed pages). Whether there was one.
    pub fn unclose_at(&mut self, index: usize) -> bool {
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
        self.forget_old_closed();
        if index >= self.closed.len() {
            return false;
        }
        let closed = self.closed.remove(index);
        // (its downloads run again)
        let queues = closable_queues(&closed.page);
        self.close_queues(&queues, false);
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

/// The menu bar's facts about the pages.
impl Pages {
    /// Rename the page shown.
    pub fn rename_shown(&mut self, name: &str) {
        let depth = self.path.len() - 1;
        let index = self.path[depth];
        if let Some(page) = self.notebook_mut(depth).get_mut(index) {
            name.clone_into(&mut page.name);
        }
    }

    /// Read the settings the pages keep again (the options were changed).
    pub fn reload_settings(&mut self) {
        let read = self.store.read(|conn| {
            Ok((
                hydrus_store::settings::get(conn)?,
                hydrus_store::settings::get(conn)?,
            ))
        });
        match read {
            Ok((naming, downloader_options)) => {
                self.naming = naming;
                self.downloader_options = downloader_options;
            }
            Err(e) => eprintln!("could not read the pages' settings: {e}"),
        }
    }

    /// The pages open, notebooks and all (`GetNumPagesHeld`).
    pub fn page_count(&self) -> usize {
        fn count(pages: &[Page]) -> usize {
            pages
                .iter()
                .map(|page| match &page.content {
                    PageContent::Pages(children) => 1 + count(children),
                    _ => 1,
                })
                .sum()
        }
        count(&self.session.pages)
    }

    /// The session's weight, as the reference weighs it
    /// (`ConvertNumHashesAndSeedsToWeight`): each page's files, and twenty
    /// for each item and search of its downloads.
    pub fn session_weight(&self) -> u64 {
        fn walk(me: &Pages, pages: &[Page], files: &mut u64, queues: &mut Vec<i64>) {
            for page in pages {
                match &page.content {
                    PageContent::Pages(children) => walk(me, children, files, queues),
                    content => {
                        *files += me.file_summary(page).0 as u64;
                        if let PageContent::Downloader { queues: q, .. } = content {
                            queues.extend(q);
                        }
                    }
                }
            }
        }
        let mut pages = self.session.pages.clone();
        refresh_contents(&mut pages, &self.open);
        let (mut files, mut queues) = (0, Vec::new());
        walk(self, &pages, &mut files, &mut queues);
        let seeds: u64 = self
            .store
            .read(|conn| {
                let mut seeds = 0;
                for queue in &queues {
                    let file_seeds: usize = hydrus_store::queues::file_seed_counts(conn, *queue)?
                        .values()
                        .sum();
                    let gallery_seeds: usize =
                        hydrus_store::queues::gallery_seed_counts(conn, *queue)?
                            .values()
                            .sum();
                    seeds += (file_seeds + gallery_seeds) as u64;
                }
                Ok(seeds)
            })
            .unwrap_or(0);
        files + 20 * seeds
    }

    /// Note the page shown in the history (the reference's
    /// `NotifyPageJustChanged`: a notebook shown empty isn't one), and
    /// forget pages no longer open.
    pub fn note_shown(&mut self) {
        fn keys(pages: &[Page], out: &mut std::collections::HashSet<PageKey>) {
            for page in pages {
                out.insert(page.key);
                if let PageContent::Pages(children) = &page.content {
                    keys(children, out);
                }
            }
        }
        let mut open = std::collections::HashSet::new();
        keys(&self.session.pages, &mut open);
        self.history.retain(|(key, _)| open.contains(key));
        let shown = self.shown();
        if matches!(shown.content, PageContent::Pages(_)) {
            return;
        }
        let (files, progress) = self.file_summary(shown);
        let entry = (
            shown.key,
            hydrus_core::pages::name_for_menu(&shown.name, files, progress, false),
        );
        self.history.retain(|(key, _)| *key != entry.0);
        self.history.push(entry);
    }

    /// The pages shown, the latest last.
    pub fn history(&self) -> &[(PageKey, String)] {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// The pages closed in the last hour, oldest first: their names for
    /// menus.
    pub fn closed_names(&mut self) -> Vec<String> {
        self.forget_old_closed();
        self.closed
            .iter()
            .map(|closed| {
                let (files, progress) = self.file_summary_with(&closed.page, &closed.open);
                hydrus_core::pages::name_for_menu(&closed.page.name, files, progress, true)
            })
            .collect()
    }

    /// Clear every watcher page's highlighted watcher (the reference's
    /// "clear_multiwatcher_highlights").
    pub fn clear_watcher_highlights(&mut self) {
        fn clear(pages: &mut [Page]) {
            for page in pages {
                match &mut page.content {
                    PageContent::Pages(children) => clear(children),
                    PageContent::Downloader {
                        kind: DownloaderKind::Watchers,
                        page: Some(state),
                        ..
                    } => state.highlighted = None,
                    _ => {}
                }
            }
        }
        clear(&mut self.session.pages);
        for opened in self.open.values() {
            let highlighted = opened
                .borrow()
                .watchers()
                .is_some_and(|w| w.state.highlighted.is_some());
            if highlighted {
                opened.borrow_mut().highlight_query(None);
            }
        }
    }
}

/// A new duplicates page's search (the reference's
/// `CreatePageManagerDuplicateFilter`): files in `location` matching
/// `predicates` (the page chooser's: every file in all my files), a pair
/// matching if one of its files does, within distance 4.
fn new_duplicates_page(
    location: hydrus_search::LocationContext,
    predicates: Vec<hydrus_search::Predicate>,
) -> DuplicatesPage {
    let search = FileSearchContext {
        location,
        predicates,
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

/// A new search page, "files", searching "my files" (see
/// [`new_search_page_on`]).
fn new_search_page(store: &Store) -> Page {
    new_search_page_on(
        store,
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            hydrus_core::service::builtin_keys::MY_FILES.to_vec(),
        )),
    )
}

/// A new search page, "files", as the reference's `NewPageQuery` makes
/// one: searching `location` and the options' default tag service for
/// search pages (every tag service, if it is gone), sorting by default and
/// collecting by the options' default collect (as `CreatePageManager`); a
/// search of every tag service doesn't search all known files, but all the
/// files stored here.
fn new_search_page_on(store: &Store, location: hydrus_search::LocationContext) -> Page {
    use hydrus_core::service::builtin_keys;
    let sorts: hydrus_core::pages::SortSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let defaults: hydrus_store::settings::SearchDefaults =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let tag_service = if store
        .snapshot()
        .services
        .by_key(&defaults.tag_service)
        .is_ok()
    {
        defaults.tag_service
    } else {
        hydrus_core::ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec())
    };
    let tags = hydrus_search::TagContext::new(tag_service, true, true);
    let location = if location.is_all_known_files() && tags.is_all_known_tags() {
        hydrus_search::LocationContext::single(hydrus_core::ServiceKey::new(
            builtin_keys::HYDRUS_LOCAL_FILE_STORAGE.to_vec(),
        ))
    } else {
        location
    };
    Page {
        key: PageKey::random(),
        name: "files".into(),
        content: PageContent::Search {
            search: FileSearchContext {
                location,
                tags,
                ..FileSearchContext::default()
            },
            synchronised: true,
            sort: None,
            lock: None,
            collect: Some(sorts.default_collect),
        },
    }
}
