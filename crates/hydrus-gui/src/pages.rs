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
            let selected = sessions::page_selected(ctx.conn(), old)?;
            if !selected.is_empty() {
                sessions::set_page_selected(ctx.conn(), new, &selected)?;
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
    pub parent: Option<PageKey>,
    pub keys: Vec<PageKey>,
    pub names: Vec<String>,
    pub selected: usize,
}

/// Frozen source keys, ordered media and warning for a tab collapse.
pub type TabHarvest = (Vec<PageKey>, Vec<HashId>, String);

pub struct Pages {
    pub(crate) page_tree: RefCell<hydrus_gui_model::page_tree::Tree>,
    predicate_history: Rc<RefCell<hydrus_gui_model::predicate_history::History>>,
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
    /// Stable popup destination, with an optional sibling insertion anchor.
    new_page_target: Option<(Option<PageKey>, Option<PageKey>)>,
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
const SIMPLE_PAGE_NAME: &str = "simple downloader";

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
    /// Boot using the configured startup session; a missing name falls back to
    /// the default-domain blank page. Ordinary reopen keeps the live session.
    pub fn open_startup(store: Arc<Store>) -> hydrus_store::Result<Self> {
        Self::open_startup_choice(store, true)
    }

    /// Recovery may choose a blank page for this boot without changing the
    /// durable configured startup session.
    pub fn open_startup_choice(
        store: Arc<Store>,
        load_default: bool,
    ) -> hydrus_store::Result<Self> {
        let settings: hydrus_store::settings::GuiSessionSettings =
            store.read(hydrus_store::settings::get)?;
        Self::open_startup_named(
            store,
            if load_default {
                settings.startup.as_deref()
            } else {
                None
            },
        )
    }

    /// Load the frozen startup name that a recovery question displayed.
    pub fn open_startup_named(store: Arc<Store>, name: Option<&str>) -> hydrus_store::Result<Self> {
        let mut pages = Self::open(store)?;
        if name == Some(LAST_SESSION) {
            return Ok(pages);
        }
        if let Some(name) = name
            && pages
                .store
                .read(|conn| sessions::load(conn, name))?
                .is_some()
        {
            pages
                .clear_and_load(name)
                .map_err(hydrus_store::StoreError::Corrupt)?;
            return Ok(pages);
        }
        for index in (0..pages.session.pages.len()).rev() {
            pages
                .close(0, index)
                .map_err(hydrus_store::StoreError::Corrupt)?;
        }
        pages.forget_closed();
        Ok(pages)
    }

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
            page_tree: RefCell::default(),
            store,
            session,
            path: Vec::new(),
            open: HashMap::new(),
            closed: Vec::new(),
            history: Vec::new(),
            predicate_history: Rc::default(),
            new_page_depth: None,
            new_page_target: None,
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

    /// Snapshot of the frame's two independent predicate recency lists.
    pub fn predicate_history(&self) -> hydrus_gui_model::predicate_history::History {
        self.predicate_history.borrow().clone()
    }
    /// Toggle the reviewed typed history entry on the visible media page.
    pub fn undo_search_predicate(
        &mut self,
        kind: hydrus_gui_model::predicate_history::Kind,
        predicate: &hydrus_search::Predicate,
    ) {
        if matches!(self.shown().content, PageContent::Pages(_)) {
            return;
        }
        let taken = self.predicate_history.borrow_mut().take(kind, predicate);
        if taken {
            self.current().borrow_mut().undo_predicate(predicate);
        }
    }
    /// Clear only search history; pages and closed-page history remain intact.
    pub fn clear_predicate_history(&mut self) {
        self.predicate_history.borrow_mut().clear();
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// A preview belongs to an open or restorable closed live page, even if
    /// another callback retains a SearchPage after the page is forgotten.
    pub(crate) fn owns_preview(
        &self,
        key: PageKey,
        owner: &std::rc::Weak<RefCell<SearchPage>>,
    ) -> bool {
        let matches =
            |opened: &Rc<RefCell<SearchPage>>| std::rc::Weak::ptr_eq(&Rc::downgrade(opened), owner);
        self.open.get(&key).is_some_and(matches)
            || self.closed.iter().any(|closed| {
                closed
                    .open
                    .iter()
                    .any(|(closed_key, opened)| *closed_key == key && matches(opened))
            })
    }

    /// One page, already open.
    pub fn single(mut page: SearchPage) -> Self {
        let tree = new_search_page(page.store());
        let mut pages = Self {
            page_tree: RefCell::default(),
            store: page.store().clone(),
            session: Session {
                name: LAST_SESSION.to_owned(),
                pages: vec![tree.clone()],
            },
            path: vec![0],
            open: HashMap::new(),
            closed: Vec::new(),
            history: Vec::new(),
            predicate_history: Rc::default(),
            new_page_depth: None,
            new_page_target: None,
            remembered: HashMap::new(),
            last_moved: HashMap::new(),
            synced: Synced::default(),
            naming: PageNameSettings::default(),
            downloader_options: DownloaderPageSettings::default(),
            kept_counts: HashMap::new(),
        };
        page.attach_predicate_history(pages.predicate_history.clone());
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
        let mut parent = None;
        for &selected in &self.path {
            rows.push(Tabs {
                parent,
                keys: pages.iter().map(|page| page.key).collect(),
                names: pages.iter().map(|p| p.name.clone()).collect(),
                selected,
            });
            parent = Some(pages[selected].key);
            match &pages[selected].content {
                PageContent::Pages(children) => pages = children,
                _ => break,
            }
        }
        rows
    }

    /// Snapshot real page menu labels without opening unopened pages or altering history.
    pub fn command_palette_pages(&self) -> Vec<hydrus_gui_model::command_palette::OpenPage> {
        fn walk(
            owner: &Pages,
            pages: &[Page],
            parent: Option<&str>,
            out: &mut Vec<hydrus_gui_model::command_palette::OpenPage>,
        ) {
            for page in pages {
                let (files, progress) = owner.file_summary(page);
                out.push(hydrus_gui_model::command_palette::OpenPage {
                    key: page.key,
                    name: hydrus_core::pages::name_for_menu(&page.name, files, progress, false),
                    parent_name: parent.map(str::to_owned),
                    notebook: matches!(page.content, PageContent::Pages(_)),
                });
                if let PageContent::Pages(children) = &page.content {
                    walk(owner, children, Some(&page.name), out);
                }
            }
        }
        let mut out = Vec::new();
        walk(self, &self.session.pages, None, &mut out);
        out
    }

    /// Launch a palette favourite on a new named search page or the current
    /// supported search. Stable keys and existing load_favourite consumers retain
    /// the full search, synchronisation, sort and collect payload.
    pub fn command_palette_favourite(
        &mut self,
        favourite: &hydrus_core::pages::FavouriteSearch,
        new_page: bool,
    ) {
        if new_page {
            let mut page = new_search_page_on(&self.store, favourite.search.location.clone());
            page.name.clone_from(&favourite.name);
            if let PageContent::Search {
                search,
                synchronised,
                sort,
                collect,
                ..
            } = &mut page.content
            {
                *search = favourite.search.clone();
                *synchronised = favourite.synchronised;
                sort.clone_from(&favourite.sort);
                collect.clone_from(&favourite.collect);
            }
            let initial_sync = self
                .store
                .read(hydrus_store::settings::get::<hydrus_store::settings::FileSearchSettings>)
                .unwrap_or_default()
                .search_immediately;
            let mut opened = SearchPage::restored(
                self.store.clone(),
                favourite.search.clone(),
                initial_sync,
                favourite.sort.as_ref(),
                Vec::new(),
            )
            .with_collect(favourite.collect.clone());
            if initial_sync {
                opened.refresh();
            }
            opened.load_favourite(favourite);
            opened.attach_predicate_history(self.predicate_history.clone());
            self.open.insert(page.key, Rc::new(RefCell::new(opened)));
            self.add(page);
        } else if matches!(self.shown().content, PageContent::Search { .. }) {
            self.current().borrow_mut().load_favourite(favourite);
        }
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
            None => match &page.content {
                PageContent::Downloader { queues, .. } => self
                    .store
                    .read(|conn| {
                        queues.iter().try_fold((0, 0), |(done, total), &queue| {
                            let (value, range) = hydrus_store::queues::file_log_value_range(
                                &hydrus_store::queues::file_seed_counts(conn, queue)?,
                            );
                            Ok(if value == range {
                                (done, total)
                            } else {
                                (done + value, total + range)
                            })
                        })
                    })
                    .unwrap_or_default(),
                _ => (0, 0),
            },
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
                kind: DownloaderKind::Simple,
                queues,
                sort,
                ..
            } if queues.len() == 1 => {
                SearchPage::simple_downloader(store, queues[0], sort.as_ref(), files)
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
                    DownloaderKind::Simple => ("simple", "empty page"),
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
        let mut opened = opened;
        if let Ok(selected) = self
            .store
            .read(|conn| sessions::page_selected(conn, &page.key))
            && !selected.is_empty()
        {
            opened.select_files(&selected);
        }
        opened.attach_predicate_history(self.predicate_history.clone());
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
        self.new_page_target = None;
    }

    /// Freeze the popup row and insertion anchor through the modal chooser.
    pub fn new_page_at(
        &mut self,
        parent: Option<PageKey>,
        before: Option<PageKey>,
    ) -> Result<(), String> {
        self.new_page_target = None;
        self.new_page_depth = None;
        self.new_page_path(parent, before)?;
        self.new_page_depth = None;
        self.new_page_target = Some((parent, before));
        Ok(())
    }

    fn new_page_path(
        &self,
        parent: Option<PageKey>,
        before: Option<PageKey>,
    ) -> Result<(Vec<usize>, Option<usize>), String> {
        let path = match parent {
            None => Vec::new(),
            Some(key) => {
                let path =
                    page_path(&self.session.pages, key).ok_or("the notebook is no longer open")?;
                let page = self
                    .session
                    .all_pages()
                    .into_iter()
                    .find(|page| page.key == key)
                    .unwrap();
                if !matches!(page.content, PageContent::Pages(_)) {
                    return Err("the destination is not a notebook".into());
                }
                path
            }
        };
        let index = if let Some(key) = before {
            let anchor =
                page_path(&self.session.pages, key).ok_or("the insertion tab is no longer open")?;
            if anchor.len() != path.len() + 1 || anchor[..path.len()] != path {
                return Err("the insertion tab moved to another notebook".into());
            }
            anchor.last().copied()
        } else {
            None
        };
        Ok((path, index))
    }

    /// Open a new search page (the reference's page chooser's "file search"
    /// on its default domain, "my files"), at the far right of the current
    /// notebook as the reference's default puts it, and show it.
    pub fn new_search_page(&mut self) {
        self.add(new_search_page(&self.store));
    }

    /// Open a blank query in the notebook current at delivery, with a frozen
    /// file location and the current saved default tag service.
    pub fn new_query_page(&mut self, location: hydrus_search::LocationContext) {
        // A deferred query is not a retained page-chooser insertion request.
        self.new_page_target = None;
        self.new_page_depth = None;
        self.add(new_search_page_on(&self.store, location));
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
        let mut opened = SearchPage::restored(
            self.store.clone(),
            search.clone(),
            *synchronised,
            sort,
            files,
        )
        .with_lock(*lock)
        .with_collect(page_collect.clone());
        opened.attach_predicate_history(self.predicate_history.clone());
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
        self.open_search_with_context(location, None, predicates, name);
    }

    /// Preserve an explicit tag context from an owning tag-list action.
    pub fn open_search_with_context(
        &mut self,
        location: hydrus_search::LocationContext,
        tags: Option<hydrus_search::TagContext>,
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
        if let Some(tags) = tags {
            search.tags = tags;
        }
        let mut opened = SearchPage::restored(
            self.store.clone(),
            search.clone(),
            *synchronised,
            None,
            Vec::new(),
        )
        .with_collect(collect.clone());
        opened.refresh();
        opened.attach_predicate_history(self.predicate_history.clone());
        self.open.insert(page.key, Rc::new(RefCell::new(opened)));
        self.add(page);
    }

    /// Open active-list predicates with the reference duplicate page's default
    /// tag contexts, rather than inheriting the source search's tag service.
    pub fn open_duplicates_predicates(
        &mut self,
        location: hydrus_search::LocationContext,
        predicates: Vec<hydrus_search::Predicate>,
        name: &str,
    ) {
        self.add(Page {
            key: PageKey::random(),
            name: name.into(),
            content: PageContent::Duplicates {
                duplicates: new_duplicates_page(location, predicates),
                sort: None,
            },
        });
    }

    /// Open the selected predicates as the two searches of a duplicate page.
    pub fn open_duplicates_with_context(
        &mut self,
        location: hydrus_search::LocationContext,
        tags: hydrus_search::TagContext,
        predicates: Vec<hydrus_search::Predicate>,
        name: &str,
    ) {
        let mut duplicates = new_duplicates_page(location, predicates);
        duplicates.search.search_1.tags = tags.clone();
        duplicates.search.search_2.tags = tags;
        self.add(Page {
            key: PageKey::random(),
            name: name.into(),
            content: PageContent::Duplicates {
                duplicates,
                sort: None,
            },
        });
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
        self.new_page_selected(chosen, true)
    }

    /// The chooser may prompt to rename its newly created notebook, after
    /// creating its initial blank search page. Freeze that notebook's key.
    pub fn new_page_from_chooser(&mut self, chosen: &NewPage) -> Result<Option<PageKey>, String> {
        self.new_page(chosen)?;
        Ok(if matches!(chosen, NewPage::Pages) {
            self.notebook_key(self.path.len() - 1)
        } else {
            None
        })
    }

    fn new_page_selected(&mut self, chosen: &NewPage, select: bool) -> Result<(), String> {
        if let Some((parent, before)) = self.new_page_target
            && let Err(error) = self.new_page_path(parent, before)
        {
            self.new_page_target = None;
            return Err(error);
        }
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
                content: PageContent::Pages(vec![new_search_page(&self.store)]),
            },
            NewPage::Session(name) => return self.append_session(name),
            NewPage::LocalImport {
                paths,
                tags,
                routers,
                delete_after_success,
            } => {
                let (paths, tags, routers, delete_after_success) = (
                    paths.clone(),
                    tags.clone(),
                    routers.clone(),
                    *delete_after_success,
                );
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
                                routers,
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
            // (on the formula last chosen, `favourite_simple_downloader_formula`)
            NewPage::SimpleDownloader => {
                let key = PageKey::random();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
                let queue = self
                    .store
                    .write(move |ctx| {
                        let conn = ctx.conn();
                        let formulae: hydrus_store::settings::SimpleDownloaderFormulae =
                            hydrus_store::settings::get(conn)?;
                        let state = hydrus_store::queues::SimpleDownloader {
                            formula_name: formulae.favourite,
                            pending: Vec::new(),
                        };
                        let queue = hydrus_store::queues::create_simple_downloader(
                            conn,
                            Some(&key.0),
                            &hydrus_core::import_options::ImportOptionsSlice::default(),
                            &state,
                            now,
                        )?;
                        hydrus_store::queues::nudge(conn, queue)?;
                        Ok(queue)
                    })
                    .map_err(|e| format!("could not make the page's queue: {e}"))?;
                Page {
                    key,
                    name: SIMPLE_PAGE_NAME.into(),
                    content: PageContent::Downloader {
                        kind: DownloaderKind::Simple,
                        queues: vec![queue],
                        sort: None,
                        page: None,
                    },
                }
            }
        };
        if select {
            self.add(page);
        } else {
            let depth = self.current_depth();
            let previous = self.shown().key;
            let current = self.path.get(depth).copied();
            let policy: hydrus_store::settings::PageInsertion = self
                .store
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            let notebook = self.notebook_mut(depth);
            let was_empty = notebook.is_empty();
            let index = policy.index(current, notebook.len());
            notebook.insert(index, page);
            if was_empty {
                // Qt selects the first tab in a formerly empty notebook, even
                // when an automatic import does not request selecting a page.
                self.select(depth, 0);
            } else {
                self.show(&previous);
            }
        }
        Ok(())
    }

    /// Add a recognised clipboard URL to the current compatible importer, or the
    /// first open one. A new importer leaves an existing selected page in view;
    /// the first child of an empty notebook becomes its selected page.
    pub fn import_clipboard_url(
        &mut self,
        routed: &hydrus_gui_model::clipboard_urls::Routed,
    ) -> Result<(), String> {
        fn first(pages: &[Page], kind: DownloaderKind) -> Option<PageKey> {
            for page in pages {
                match &page.content {
                    PageContent::Downloader {
                        kind: page_kind, ..
                    } if *page_kind == kind => {
                        return Some(page.key);
                    }
                    PageContent::Pages(children) => {
                        if let Some(key) = first(children, kind) {
                            return Some(key);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        use hydrus_gui_model::clipboard_urls::Destination;
        let (kind, choice) = match routed.destination {
            Destination::Urls => (DownloaderKind::Urls, NewPage::Urls),
            Destination::Watchers => (DownloaderKind::Watchers, NewPage::Watcher),
        };
        let compatible = |page: &Page| {
            matches!(
                &page.content, PageContent::Downloader { kind: page_kind, .. } if *page_kind == kind
            )
        };
        let mut key = if compatible(self.shown()) {
            Some(self.shown().key)
        } else {
            first(&self.session.pages, kind)
        };
        if key.is_none() {
            self.new_page_selected(&choice, false)?;
            key = first(&self.session.pages, kind);
        }
        let importer = key
            .and_then(|key| self.page(&key))
            .ok_or("Could not find a new page to place the clipboard URL.")?;
        match routed.destination {
            Destination::Urls => importer.borrow_mut().pend_urls(&routed.url),
            Destination::Watchers => importer.borrow_mut().pend_watchers(&routed.url),
        }
        Ok(())
    }

    /// Append the saved session `name` as a page of pages named after it,
    /// at the far right of the current notebook, and show it (the
    /// reference's "append session"). Its pages are copies, with their
    /// files, so the saved session stays as it was.
    pub fn append_session(&mut self, name: &str) -> Result<(), String> {
        let pages = self.fresh_session_pages(name)?;
        self.kept_counts = self
            .store
            .read(sessions::page_file_counts)
            .map_err(|e| e.to_string())?;
        self.add(Page {
            key: PageKey::random(),
            name: name.to_owned(),
            content: PageContent::Pages(pages),
        });
        Ok(())
    }

    /// Append into a frozen parent notebook, preserving a different visible
    /// branch while remembering the new selection inside the destination.
    pub fn append_session_to_notebook(
        &mut self,
        notebook: Option<PageKey>,
        name: &str,
    ) -> Result<(), String> {
        let previous = self.shown().key;
        let (depth, visible) = if let Some(key) = notebook {
            let path = page_path(&self.session.pages, key)
                .ok_or("destination notebook is no longer open")?;
            let page = self
                .session
                .all_pages()
                .into_iter()
                .find(|page| page.key == key)
                .ok_or("destination notebook is no longer open")?;
            if !matches!(page.content, PageContent::Pages(_)) {
                return Err("session destination is not a notebook".into());
            }
            let visible = self.path.starts_with(&path);
            self.show(&key);
            (path.len(), visible)
        } else {
            (0, true)
        };
        self.new_page_depth = Some(depth);
        let result = self.append_session(name);
        if result.is_err() {
            self.new_page_depth = None;
        }
        if !visible || result.is_err() {
            self.show(&previous);
        }
        result
    }

    /// Append an immutable historical snapshot as a fresh notebook. Re-key
    /// every descendant and restore media from the snapshot, never live pages.
    pub fn append_session_backup(&mut self, name: &str, timestamp: i64) -> Result<(), String> {
        let saved_name = name.to_owned();
        let pages = self
            .store
            .write(move |ctx| {
                let name = saved_name;
                let snapshot = hydrus_store::session_backups::load(ctx.conn(), &name, timestamp)?
                    .ok_or_else(|| {
                    hydrus_store::StoreError::Corrupt(format!(
                        "there is no backup of session \"{name}\" at {timestamp}"
                    ))
                })?;
                hydrus_store::session_backups::restore_pages(ctx.conn(), snapshot)
            })
            .map_err(|e| e.to_string())?;
        self.kept_counts = self
            .store
            .read(sessions::page_file_counts)
            .map_err(|e| e.to_string())?;
        self.new_page_depth = Some(0);
        self.add(Page {
            key: PageKey::random(),
            name: name.into(),
            content: PageContent::Pages(pages),
        });
        Ok(())
    }

    fn fresh_session_pages(&self, name: &str) -> Result<Vec<Page>, String> {
        let name = name.to_owned();
        self.store
            .write(move |ctx| {
                let conn = ctx.conn();
                let saved = sessions::load(conn, &name)?.ok_or_else(|| {
                    hydrus_store::StoreError::Corrupt(format!(
                        "there is no saved session \"{name}\""
                    ))
                })?;
                let snapshot = match hydrus_store::session_backups::latest(conn, &name)? {
                    Some(snapshot) => snapshot,
                    None => hydrus_store::session_backups::capture(conn, &saved)?,
                };
                hydrus_store::session_backups::restore_pages(conn, snapshot)
            })
            .map_err(|e| e.to_string())
    }

    /// Close every page and load the saved session `name` in their place,
    /// its pages at the top (the reference's "clear and load": the pages
    /// closed are gone, not kept to reopen, and their downloads with them).
    pub fn clear_and_load(&mut self, name: &str) -> Result<(), String> {
        let pages = self.fresh_session_pages(name)?;
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
        self.kept_counts = self
            .store
            .read(sessions::page_file_counts)
            .map_err(|e| e.to_string())?;
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
        self.save_session_at_ms(name, now.saturating_mul(1000))
    }

    /// Save a named session at millisecond precision for backup timestamps.
    pub fn save_session_at_ms(&mut self, name: &str, now_ms: i64) -> Result<(), String> {
        self.sync(now_ms / 1000).map_err(|e| e.to_string())?;
        let pages = copied(&self.store, self.session.pages.clone()).map_err(|e| e.to_string())?;
        let session = Session {
            name: name.to_owned(),
            pages,
        };
        self.store
            .write(move |ctx| hydrus_store::session_backups::save(ctx.conn(), &session, now_ms))
            .map_err(|e| e.to_string())
    }

    /// Save only one frozen notebook's children, independently from other
    /// visible branches. The notebook wrapper itself is not part of the save.
    pub fn save_notebook_session_at_ms(
        &mut self,
        key: PageKey,
        name: &str,
        now_ms: i64,
    ) -> Result<(), String> {
        self.sync(now_ms / 1000).map_err(|e| e.to_string())?;
        let page = self
            .session
            .all_pages()
            .into_iter()
            .find(|page| page.key == key)
            .ok_or("the notebook to save is no longer open")?;
        let PageContent::Pages(children) = &page.content else {
            return Err("the page to save is not a notebook".into());
        };
        let pages = copied(&self.store, children.clone()).map_err(|e| e.to_string())?;
        let session = Session {
            name: name.into(),
            pages,
        };
        self.store
            .write(move |ctx| hydrus_store::session_backups::save(ctx.conn(), &session, now_ms))
            .map_err(|e| e.to_string())
    }

    /// Add `page` at the far right of the current notebook, and show it
    /// (a notebook of pages, on its first page).
    fn add(&mut self, page: Page) {
        let target = self.new_page_target.take();
        let (depth, insertion, selected) = if let Some((parent, before)) = target {
            // The chooser validates before creating queue-backed pages. No
            // event loop runs between that validation and this insertion.
            let (path, index) = self
                .new_page_path(parent, before)
                .expect("validated chooser destination");
            let selected = if self.path.starts_with(&path) {
                self.path.get(path.len()).copied()
            } else {
                parent.and_then(|key| self.remembered.get(&key).copied())
            };
            self.path = path;
            (self.path.len(), index, selected)
        } else {
            let depth = self
                .new_page_depth
                .take()
                .filter(|&d| d <= self.current_depth())
                .unwrap_or_else(|| self.current_depth());
            (depth, None, self.path.get(depth).copied())
        };
        let policy: hydrus_store::settings::PageInsertion = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let pages = self.notebook_mut(depth);
        let index = insertion.unwrap_or_else(|| policy.index(selected, pages.len()));
        pages.insert(index, page);
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
                let settings: crate::tab_context::NotebookSettings = self
                    .store
                    .read(hydrus_store::settings::get)
                    .unwrap_or_default();
                let at = if settings.close_focus_left {
                    index.saturating_sub(1)
                } else {
                    index
                };
                self.select(depth, at.min(remaining - 1));
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
        if matches!(page.content, PageContent::Pages(_)) {
            let vetoes = self.close_vetoes(std::slice::from_ref(&page));
            return crate::session_saving::close_all_question(&vetoes)
                .map(|question| question.replacen("top page notebook", &page.name, 1))
                .or_else(|| self.basic_close_question(&page));
        }
        if matches!(page.content, PageContent::Downloader { .. }) {
            let opened = self.page(&page.key)?;
            if let Some(veto) = opened
                .borrow()
                .close_veto(self.downloader_options.confirm_non_empty_close)
            {
                return Some(format!("Close \"{}\"?\n\n{veto}", page.name));
            }
        }
        self.basic_close_question(&page)
    }

    fn basic_close_question(&self, page: &Page) -> Option<String> {
        fn held(pages: &[Page]) -> usize {
            pages
                .iter()
                .map(|page| {
                    1 + match &page.content {
                        PageContent::Pages(children) => held(children),
                        _ => 0,
                    }
                })
                .sum()
        }
        let settings: hydrus_store::settings::PageNavigationSettings = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        if !settings.confirm_all_closes {
            return None;
        }
        let mut question = format!("Close \"{}\"?", page.name);
        if let PageContent::Pages(children) = &page.content {
            let count = held(children);
            if count == 0 {
                question.push_str("\n\nIt is empty.");
            } else {
                question.push_str(&format!(
                    "\n\nIt is holding {} pages.",
                    hydrus_core::numbers::human_int(u64::try_from(count).unwrap_or(u64::MAX))
                ));
            }
        }
        Some(question)
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

    /// Actions on the clicked tab, using that tab row's notebook.
    pub fn tab_menu(&self, depth: usize, index: usize) -> Vec<crate::main_menu::Entry> {
        let Some(page) = self.notebook_at(depth).and_then(|pages| pages.get(index)) else {
            return Vec::new();
        };
        let mut entries = crate::tab_context::menu(
            depth,
            index,
            self.notebook_at(depth).map_or(0, <[Page]>::len),
            self.path.get(depth).copied().unwrap_or(0),
        );
        entries.push(crate::main_menu::Entry::Separator);
        for (label, before) in [("new page", None), ("new page here", Some(page.key))] {
            entries.push(crate::main_menu::Entry::Item {
                label: label.into(),
                enabled: true,
                command: Some(crate::main_menu::Command::ChooseNotebookPage {
                    parent: self.notebook_key(depth),
                    before,
                }),
            });
        }
        let advanced: hydrus_store::settings::AdvancedMode = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        if advanced.0 {
            let label = format!(
                "page weight: {}",
                hydrus_core::numbers::human_int(self.page_weight(page.key).unwrap_or(0))
            );
            entries.insert(0, crate::main_menu::Entry::Separator);
            entries.insert(
                0,
                crate::main_menu::Entry::Item {
                    label: label.clone(),
                    enabled: true,
                    command: Some(crate::main_menu::Command::Copy(label)),
                },
            );
        }
        let refresh = match &page.content {
            PageContent::Pages(children) if children.is_empty() => None,
            PageContent::Pages(_) => Some("refresh all this page's pages"),
            _ => Some("refresh this page"),
        };
        if let Some(label) = refresh {
            entries.push(crate::main_menu::Entry::Separator);
            entries.push(crate::main_menu::Entry::Item {
                label: label.into(),
                enabled: true,
                command: Some(crate::main_menu::Command::RefreshTab(page.key)),
            });
        }
        let names: Vec<_> = self
            .store
            .read(sessions::names)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let notebook =
            matches!(page.content, PageContent::Pages(_)).then_some((page.key, page.name.as_str()));
        entries.extend(crate::tab_context::session_entries(
            self.notebook_key(depth),
            notebook,
            &names,
        ));
        entries
    }

    /// Refresh only initialized media descendants, as Qt Page::RefreshQuery
    /// skips pages whose media panels have not initialized. Preserve selection.
    pub fn refresh_tab_tree(&mut self, key: PageKey) {
        if !self.session.all_pages().iter().any(|page| page.key == key) {
            return;
        }
        for leaf in self.pages_under(&key) {
            if let Some(open) = self.open.get(&leaf) {
                open.borrow_mut().refresh_tab();
            }
        }
    }

    /// The clicked subtree's file-and-seed weight, including repeated files
    /// across separate pages, as each reference leaf contributes independently.
    pub fn page_weight(&self, key: PageKey) -> Option<u64> {
        self.session
            .all_pages()
            .into_iter()
            .find(|page| page.key == key)
            .map(|page| self.weight_for_pages(vec![page.clone()]))
    }

    /// Popup on the unused part of a tab row: choose in that row's notebook.
    pub fn tab_space_menu(&self, depth: usize) -> Vec<crate::main_menu::Entry> {
        if depth > self.current_depth() {
            return Vec::new();
        }
        vec![crate::main_menu::Entry::Item {
            label: "new page".into(),
            enabled: true,
            command: Some(crate::main_menu::Command::ChooseNotebookPage {
                parent: self.notebook_key(depth),
                before: None,
            }),
        }]
    }

    /// Navigate at a tab row's notebook, descending into its selected
    /// notebook unless this row moved recently, just as the reference does.
    pub fn navigate_tabs(
        &mut self,
        depth: usize,
        movement: crate::tab_context::Move,
        now: std::time::Instant,
    ) {
        match movement {
            crate::tab_context::Move::Left => {
                self.move_at(depth, -1, false, now);
            }
            crate::tab_context::Move::Right => {
                self.move_at(depth, 1, false, now);
            }
            crate::tab_context::Move::First => {
                self.move_end_at(depth, false, false, now);
            }
            crate::tab_context::Move::Last => {
                self.move_end_at(depth, true, false, now);
            }
        }
    }

    fn move_end_at(
        &mut self,
        depth: usize,
        last: bool,
        test: bool,
        now: std::time::Instant,
    ) -> bool {
        let Some(pages) = self.notebook_at(depth) else {
            return false;
        };
        if pages.len() <= 1 {
            return false;
        }
        let Some(&current) = self.path.get(depth) else {
            return false;
        };
        let target = if last { pages.len() - 1 } else { 0 };
        let nested = matches!(pages[current].content, PageContent::Pages(_));
        let key = self.notebook_key(depth);
        let recent = self
            .last_moved
            .get(&key)
            .is_some_and(|&at| now.duration_since(at) < std::time::Duration::from_secs(3));
        if nested && !recent && self.move_end_at(depth + 1, last, true, now) {
            return self.move_end_at(depth + 1, last, test, now);
        }
        if !test {
            self.select(depth, target);
            self.last_moved.insert(key, now);
        }
        true
    }

    /// Freeze a bulk-close's targets and aggregate its confirmation. Keys
    /// remain valid if the user selects another notebook before answering.
    pub fn close_tabs_question(
        &mut self,
        depth: usize,
        index: usize,
        side: crate::tab_context::Close,
    ) -> Option<(Vec<PageKey>, String)> {
        fn count(page: &Page) -> usize {
            1 + if let PageContent::Pages(children) = &page.content {
                children.iter().map(count).sum()
            } else {
                0
            }
        }
        let pages = self.notebook_at(depth)?;
        let indices = crate::tab_context::close_indices(index, pages.len(), side);
        let targets: Vec<Page> = indices.into_iter().map(|i| pages[i].clone()).collect();
        if targets.is_empty() {
            return None;
        }
        let held = targets.iter().map(count).sum();
        let keys = targets.iter().map(|p| p.key).collect();
        let vetoes = self.close_vetoes(&targets);
        Some((
            keys,
            crate::session_saving::close_group_question(
                held,
                crate::tab_context::close_description(side),
                &vetoes,
            ),
        ))
    }

    fn close_vetoes(&mut self, pages: &[Page]) -> Vec<(String, String)> {
        fn leaves(pages: &[Page], out: &mut Vec<PageKey>) {
            for page in pages {
                match &page.content {
                    PageContent::Pages(children) => leaves(children, out),
                    _ => out.push(page.key),
                }
            }
        }
        let mut keys = Vec::new();
        leaves(pages, &mut keys);
        let mut vetoes = Vec::new();
        for key in keys {
            let name = self
                .session
                .all_pages()
                .into_iter()
                .find(|p| p.key == key)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            if let Some(page) = self.page(&key)
                && let Some(reason) = page
                    .borrow()
                    .close_veto(self.downloader_options.confirm_non_empty_close)
            {
                vetoes.push((reason, name));
            }
        }
        vetoes
    }

    /// Close the frozen sibling targets in reverse order. Each uses the
    /// normal closed-page stack, so undo restores pages, queues and positions.
    pub fn close_tab_keys(&mut self, keys: &[PageKey]) -> Result<(), String> {
        let shown = self.shown().key;
        for key in keys.iter().rev() {
            if let Some(path) = page_path(&self.session.pages, *key) {
                // Showing a notebook descends into its selected child; close
                // the requested notebook itself, at its frozen key's depth.
                let depth = path.len() - 1;
                self.show(key);
                self.close(depth, path[depth])?;
            }
        }
        self.show(&shown);
        Ok(())
    }

    /// The clicked page's identity, frozen before opening its text entry.
    pub fn tab_identity(&self, depth: usize, index: usize) -> Option<(PageKey, String)> {
        self.notebook_at(depth)?
            .get(index)
            .map(|p| (p.key, p.name.clone()))
    }

    /// Rename an existing page by key without altering its content or selection.
    pub fn rename_key(&mut self, key: &PageKey, name: &str) {
        fn rename(pages: &mut [Page], key: &PageKey, name: &str) {
            for page in pages {
                if page.key == *key {
                    name.clone_into(&mut page.name);
                    return;
                }
                if let PageContent::Pages(children) = &mut page.content {
                    rename(children, key, name);
                }
            }
        }
        rename(&mut self.session.pages, key, name);
    }

    /// Copy the clicked subtree beside itself with fresh media and importer
    /// storage. A copied notebook starts on its first child, as session insert.
    pub fn duplicate_tab(&mut self, depth: usize, index: usize) -> Result<(), String> {
        let Some((key, _)) = self.tab_identity(depth, index) else {
            return Ok(());
        };
        self.sync(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX)),
        )
        .map_err(|e| e.to_string())?;
        let original = self
            .session
            .all_pages()
            .into_iter()
            .find(|page| page.key == key)
            .cloned()
            .ok_or("page disappeared")?;
        let session = Session {
            name: "dupe session".into(),
            pages: vec![original],
        };
        let mut copies = self
            .store
            .write(move |ctx| {
                let snapshot = hydrus_store::session_backups::capture(ctx.conn(), &session)?;
                hydrus_store::session_backups::restore_pages(ctx.conn(), snapshot)
            })
            .map_err(|e| e.to_string())?;
        let copy = copies.pop().ok_or("could not duplicate page")?;
        self.kept_counts = self
            .store
            .read(sessions::page_file_counts)
            .map_err(|e| e.to_string())?;
        self.remember();
        self.notebook_mut(depth).insert(index + 1, copy);
        self.path.truncate(depth);
        self.select(depth, index + 1);
        Ok(())
    }

    /// Freeze both target keys and their current ordered media for the harvest
    /// confirmation. Groups deduplicate their flattened notebook media.
    pub fn collapse_tabs_question(
        &mut self,
        depth: usize,
        index: usize,
        scope: crate::tab_context::Send,
    ) -> Result<Option<TabHarvest>, String> {
        let keys = self.send_tab_targets(depth, index, scope);
        if keys.is_empty() {
            return Ok(None);
        }
        let mut files = Vec::new();
        for key in &keys {
            for leaf in self.pages_under(key) {
                let media = if let Some(open) = self.open.get(&leaf) {
                    open.borrow().files()
                } else {
                    self.store
                        .read(|conn| sessions::page_files(conn, &leaf))
                        .map_err(|e| e.to_string())?
                };
                files.extend(media);
            }
        }
        // A notebook's GetHashes deduplicates its leaves; group harvest also
        // deduplicates across sibling pages, always keeping first appearance.
        let mut seen = std::collections::HashSet::new();
        files.retain(|file| seen.insert(*file));
        let question = crate::tab_context::collapse_question(
            files.len(),
            keys.len(),
            scope == crate::tab_context::Send::This,
        );
        Ok(Some((keys, files, question)))
    }

    /// Harvest the frozen media, close source tabs into undo without further
    /// importer objections, and insert a default search at the first source.
    pub fn collapse_tab_keys(&mut self, keys: &[PageKey], files: &[HashId]) -> Result<(), String> {
        let Some(&first) = keys.first() else {
            return Ok(());
        };
        let Some(path) = page_path(&self.session.pages, first) else {
            return Ok(());
        };
        // Do nothing if the confirmed siblings have since been removed/moved.
        let parent = &path[..path.len() - 1];
        if keys.iter().any(|key| {
            page_path(&self.session.pages, *key).is_none_or(|p| p[..p.len() - 1] != *parent)
        }) {
            return Ok(());
        }
        let depth = path.len() - 1;
        let insertion = path[depth];
        let replaces_all_top = depth == 0 && keys.len() == self.session.pages.len();
        let mut page = new_search_page(&self.store);
        let PageContent::Search {
            search,
            synchronised,
            lock,
            sort,
            collect,
        } = &mut page.content
        else {
            unreachable!("a search page");
        };
        if !files.is_empty() {
            let hashes = self
                .store
                .read(|conn| hydrus_store::master::hashes(conn, files))
                .map_err(|e| e.to_string())?;
            search.predicates = vec![hydrus_search::Predicate::System(
                hydrus_core::search::predicate::SystemPredicate::Hash {
                    hashes: hydrus_core::search::predicate::FileHashes::Sha256(
                        files
                            .iter()
                            .filter_map(|file| hashes.get(file).copied())
                            .collect(),
                    ),
                    inclusive: true,
                },
            )];
            *lock = Some(hydrus_core::pages::HashLock::default());
        }
        let mut opened = SearchPage::restored(
            self.store.clone(),
            search.clone(),
            *synchronised,
            sort.as_ref(),
            files.to_vec(),
        )
        .with_lock(*lock)
        .with_collect(collect.clone());
        self.show(&first);
        self.close_tab_keys(keys)?;
        // Closing all children leaves their parent notebook selected. Retain
        // its ancestry explicitly, including the top-level empty fallback.
        self.path = parent.to_vec();
        if self.path.is_empty() {
            self.path.push(0);
        }
        let row = self.notebook_mut(depth);
        if replaces_all_top {
            // Ordinary last-tab close supplies a blank fallback; this action
            // already provides its replacement and must not keep that fallback.
            row.clear();
        }
        let at = insertion.min(row.len());
        row.insert(at, page.clone());
        opened.attach_predicate_history(self.predicate_history.clone());
        self.open.insert(page.key, Rc::new(RefCell::new(opened)));
        self.path.truncate(depth);
        self.select(depth, at);
        Ok(())
    }

    /// Freeze the siblings targeted by a send-down menu action.
    pub fn send_tab_targets(
        &self,
        depth: usize,
        index: usize,
        scope: crate::tab_context::Send,
    ) -> Vec<PageKey> {
        let Some(pages) = self.notebook_at(depth) else {
            return Vec::new();
        };
        crate::tab_context::send_indices(index, pages.len(), scope)
            .into_iter()
            .map(|i| pages[i].key)
            .collect()
    }

    /// Group siblings into a fresh notebook without closing/reopening them;
    /// their keys, open search state and downloader queues all survive.
    pub fn send_tab_keys(&mut self, keys: &[PageKey], single: bool) -> Option<PageKey> {
        fn locate(pages: &[Page], key: PageKey) -> Option<Vec<usize>> {
            for (index, page) in pages.iter().enumerate() {
                if page.key == key {
                    return Some(vec![index]);
                }
                if let PageContent::Pages(children) = &page.content
                    && let Some(mut path) = locate(children, key)
                {
                    path.insert(0, index);
                    return Some(path);
                }
            }
            None
        }
        let first = *keys.first()?;
        let path = locate(&self.session.pages, first)?;
        let depth = path.len() - 1;
        let insertion = path[depth];
        // Preserve the selected child within every moved notebook before
        // replacing this row's parent path.
        self.remember();
        let previous_path = self.path.clone();
        self.path = path;
        let pages = self.notebook_mut(depth);
        let moving: Vec<_> = keys
            .iter()
            .filter_map(|key| pages.iter().find(|p| p.key == *key).cloned())
            .collect();
        if moving.len() != keys.len() {
            self.path = previous_path;
            return None;
        }
        pages.retain(|p| !keys.contains(&p.key));
        let at = if single {
            insertion.min(pages.len())
        } else {
            pages.len()
        };
        let selected = moving.len() - 1;
        let notebook = PageKey::random();
        pages.insert(
            at,
            Page {
                key: notebook,
                name: "pages".into(),
                content: PageContent::Pages(moving),
            },
        );
        self.path.truncate(depth);
        self.select(depth, at);
        // Qt inserts each reversed group member at zero, retaining the first
        // inserted widget (the rightmost original page) as the selected tab.
        self.select(depth + 1, selected);
        Some(notebook)
    }

    /// Sort only the notebook at `depth`, retaining the shown leaf and
    /// descendants' selections. Equal keys keep their original order.
    pub fn sort_tabs(
        &mut self,
        depth: usize,
        by: crate::tab_context::Sort,
        ascending: bool,
    ) -> Result<(), String> {
        let Some(pages) = self.notebook_at(depth) else {
            return Ok(());
        };
        let summaries = pages
            .iter()
            .map(|page| {
                let (files, progress) = self.file_summary(page);
                Ok(crate::tab_context::Summary {
                    name: page.name.clone(),
                    files,
                    progress,
                    size: self.total_file_size(page).map_err(|e| e.to_string())?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let order = crate::tab_context::order(&summaries, by, ascending);
        let shown = self.shown().key;
        let old = self.notebook_mut(depth).clone();
        *self.notebook_mut(depth) = order.into_iter().map(|i| old[i].clone()).collect();
        self.show(&shown);
        Ok(())
    }

    /// Parent notebook of a stable page key (`None` is the frame notebook).
    pub fn tab_parent(&self, key: PageKey) -> Option<Option<PageKey>> {
        let path = page_path(&self.session.pages, key)?;
        if path.len() == 1 {
            return Some(None);
        }
        let mut pages = self.session.pages.as_slice();
        for &index in &path[..path.len() - 2] {
            let PageContent::Pages(children) = &pages[index].content else {
                return None;
            };
            pages = children;
        }
        Some(Some(pages[path[path.len() - 2]].key))
    }
    fn children(&self, parent: Option<PageKey>) -> Option<&[Page]> {
        let Some(key) = parent else {
            return Some(&self.session.pages);
        };
        let page = self
            .session
            .all_pages()
            .into_iter()
            .find(|page| page.key == key)?;
        if let PageContent::Pages(children) = &page.content {
            Some(children)
        } else {
            None
        }
    }
    /// Move the original page object between notebooks; queues/media keep keys.
    pub fn drop_tab(
        &mut self,
        source: PageKey,
        parent: Option<PageKey>,
        target: Option<PageKey>,
        edge: hydrus_gui_model::tab_drag::Edge,
        chase: bool,
    ) -> bool {
        let Some(source_parent) = self.tab_parent(source) else {
            return false;
        };
        if let Some(parent) = parent {
            let Some(parent_path) = page_path(&self.session.pages, parent) else {
                return false;
            };
            let Some(source_path) = page_path(&self.session.pages, source) else {
                return false;
            };
            if parent_path.starts_with(&source_path) {
                return false;
            }
        }
        let Some(siblings) = self.children(source_parent) else {
            return false;
        };
        let Some(source_index) = siblings.iter().position(|page| page.key == source) else {
            return false;
        };
        let shown = self.shown().key;
        let source_path = page_path(&self.session.pages, source).expect("validated source");
        let was_shown = page_path(&self.session.pages, shown)
            .is_some_and(|path| path.starts_with(&source_path));
        let remembered_child = source_parent
            .and_then(|key| self.remembered.get(&key))
            .and_then(|index| siblings.get(*index))
            .map(|page| page.key);
        let neighbour = siblings
            .get(source_index + 1)
            .or_else(|| {
                source_index
                    .checked_sub(1)
                    .and_then(|index| siblings.get(index))
            })
            .map(|page| page.key);
        let Some(destination) = self.children(parent) else {
            return false;
        };
        let count = destination.len();
        let target_index = match target {
            Some(key) => {
                let Some(index) = destination.iter().position(|page| page.key == key) else {
                    return false;
                };
                Some(index)
            }
            None => None,
        };
        let Some(insertion) = hydrus_gui_model::tab_drag::insertion(
            (source_parent == parent).then_some(source_index),
            target_index,
            edge,
            count,
        ) else {
            return false;
        };
        self.remember();
        let moving = children_mut(&mut self.session.pages, source_parent)
            .expect("validated source")
            .remove(source_index);
        let destination = children_mut(&mut self.session.pages, parent)
            .expect("validated independent destination");
        destination.insert(insertion.min(destination.len()), moving);
        if let Some(parent) = source_parent {
            let keep = remembered_child.filter(|key| *key != source).or(neighbour);
            if let Some(index) = keep.and_then(|key| {
                self.children(source_parent)?
                    .iter()
                    .position(|page| page.key == key)
            }) {
                self.remembered.insert(parent, index);
            }
        }
        if was_shown {
            if let Some(key) = neighbour {
                self.show(&key);
            }
        } else {
            self.show(&shown);
        }
        if chase || count == 0 {
            self.show(&source);
        } else if source_index > 1 {
            if let Some(key) = self
                .children(source_parent)
                .and_then(|pages| pages.get(source_index - 1))
                .map(|page| page.key)
            {
                self.show(&key);
            }
        } else if let Some(key) = source_parent {
            self.show(&key);
        }
        true
    }

    /// Qt wheel selection clamps at the ends of the hovered notebook bar.
    pub fn wheel_tab(&mut self, depth: usize, step: i32) {
        let Some(pages) = self.notebook_at(depth) else {
            return;
        };
        let Some(&selected) = self.path.get(depth) else {
            return;
        };
        let next = if step < 0 {
            selected.saturating_sub(1)
        } else {
            (selected + 1).min(pages.len().saturating_sub(1))
        };
        self.select(depth, next);
    }

    /// Move a clicked tab within its notebook while keeping the selected
    /// leaf, including when moving a containing notebook or an unselected tab.
    pub fn move_tab(&mut self, depth: usize, index: usize, movement: crate::tab_context::Move) {
        let Some(pages) = self.notebook_at(depth) else {
            return;
        };
        let Some(target) = crate::tab_context::destination(index, pages.len(), movement) else {
            return;
        };
        let shown = self.shown().key;
        let pages = self.notebook_mut(depth);
        let page = pages.remove(index);
        pages.insert(target, page);
        self.show(&shown);
    }

    /// The reference reports zero size for a page not initialised yet;
    /// notebooks sum each child's size, including repeated files on siblings.
    fn total_file_size(&self, page: &Page) -> hydrus_store::Result<u64> {
        if let PageContent::Pages(children) = &page.content {
            return children.iter().try_fold(0_u64, |sum, child| {
                Ok(sum.saturating_add(self.total_file_size(child)?))
            });
        }
        let Some(open) = self.open.get(&page.key) else {
            return Ok(0);
        };
        let files = open.borrow().files();
        self.store.read(|conn| {
            Ok(hydrus_store::media::load_basic(conn, &files)?
                .iter()
                .filter_map(|media| media.info.as_ref())
                .map(|info| info.size)
                .sum())
        })
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
        self.weight_for_pages(self.session.pages.clone())
    }

    fn weight_for_pages(&self, mut pages: Vec<Page>) -> u64 {
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

/// A blank search page uses the configured default local file location.
fn new_search_page(store: &Store) -> Page {
    let defaults: hydrus_store::settings::SearchDefaults =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    new_search_page_on(
        store,
        defaults.resolved_local_location(&store.snapshot().services),
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
    let file_search: hydrus_store::settings::FileSearchSettings =
        store.read(hydrus_store::settings::get).unwrap_or_default();
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
            synchronised: file_search.search_immediately,
            sort: None,
            lock: None,
            collect: Some(sorts.default_collect),
        },
    }
}

/// Locate a page or notebook without descending through its selected child.
fn page_path(pages: &[Page], key: PageKey) -> Option<Vec<usize>> {
    for (index, page) in pages.iter().enumerate() {
        if page.key == key {
            return Some(vec![index]);
        }
        if let PageContent::Pages(children) = &page.content
            && let Some(mut path) = page_path(children, key)
        {
            path.insert(0, index);
            return Some(path);
        }
    }
    None
}

fn children_mut(pages: &mut Vec<Page>, parent: Option<PageKey>) -> Option<&mut Vec<Page>> {
    let Some(key) = parent else {
        return Some(pages);
    };
    for page in pages {
        if let PageContent::Pages(children) = &mut page.content {
            if page.key == key {
                return Some(children);
            }
            if let Some(found) = children_mut(children, Some(key)) {
                return Some(found);
            }
        }
    }
    None
}
