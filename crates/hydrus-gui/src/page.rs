//! A page of files: a search page (its predicates and the files they find),
//! or a page that shows files without a search, and which file is selected.
//! Plain Rust, driven by the window and by tests alike.

use std::collections::HashMap;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::pages::{
    DuplicatesPage, HashLock, PageCollect, PageContent, PageMedia, PageSort, PageSortBy,
};
use hydrus_core::search::predicate::{FileHashes, SystemPredicate};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, Predicate, SortBy, SortOrder, TextContext,
    collect_page_files, parse_api_search, predicate_text, search_files, sort_page_files,
};
use hydrus_store::Store;
use hydrus_store::live::{self, JobKind, JobLine, JobLive, QueueLive};
use hydrus_store::queues::{self, StatusCounts};

use crate::autocomplete::Autocomplete;
use crate::gallery::{Column, GalleryView};
use crate::selection::{Move, Selection};
use crate::status::Facts;
use crate::watcher::WatcherView;

pub struct SearchPage {
    store: Arc<Store>,
    autocomplete: Autocomplete,
    /// The page's file and tag domains (its predicates are `predicates`).
    context: FileSearchContext,
    predicates: Vec<Predicate>,
    /// Whether the page searches as its predicates change.
    synchronised: bool,
    /// Whether the search is locked to a `system:hash` of the page's
    /// files, and what that hash follows (kept while unlocked, as the
    /// reference's page keeps it).
    locked: bool,
    lock_syncs: HashLock,
    /// Why the page shows files without a search, if it does.
    note: Option<String>,
    sort: PageSort,
    /// Whether the sort was changed since the page was opened.
    sort_changed: bool,
    /// The sort applied first, which orders the sort's ties (the options'
    /// `fallback_sort`).
    fallback: PageSort,
    /// How the page collects its files, and its collections: each by its
    /// first file (the item `results` shows), with its files in order.
    collect: PageCollect,
    collections: HashMap<HashId, Vec<HashId>>,
    /// The page's files in the order they came to it (searched, or as a
    /// session kept them), which collecting takes them in: so a
    /// collection, and what it takes from its first file, stays as it is
    /// while the page sorts (the reference collects once, and sorts its
    /// collections).
    came: Vec<HashId>,
    /// In the sort's order.
    results: Vec<HashId>,
    selection: Selection,
    /// The selection's tags (or, with nothing selected, the page's): each
    /// tag, and its row as the list shows it.
    tags: Vec<(String, String)>,
    error: Option<String>,
    /// A duplicates page's filtering, which the page can launch.
    duplicates: Option<DuplicatesPage>,
    /// What the status bar says while the page is empty (the reference's
    /// `_empty_page_status_override`, forgotten once it shows files).
    empty_status: std::cell::Cell<Option<&'static str>>,
    /// The status bar's facts of the files shown (and some no longer).
    facts: HashMap<HashId, Facts>,
    /// A downloader page's importer, as last read.
    importer: Option<Importer>,
    /// The files its importer has brought so far (shown, or taken off the
    /// page since), so each is added once.
    presented: std::collections::HashSet<HashId>,
    /// A gallery page's searches (the importer is the one it shows).
    gallery: Option<GalleryView>,
    /// A watcher page's watchers (the importer is the one it shows).
    watchers: Option<WatcherView>,
}

/// What reading a downloader page's importer again changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportRefresh {
    Nothing,
    /// Its counts, pause or live state, not its files.
    Status,
    /// Files came (and maybe more).
    Files,
}

/// A downloader page's importer: the queue it shows (which the daemon
/// works), as last read from the store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Importer {
    pub queue: i64,
    /// Its file log's seeds by status.
    pub files: StatusCounts,
    /// Its search log's.
    pub searches: StatusCounts,
    pub paused: bool,
    /// What the daemon is doing with it now (nothing, if it isn't running).
    pub live: QueueLive,
    /// Whether it is a local import's (the reference's "import" page), not
    /// a URL list's.
    pub local: bool,
}

impl Importer {
    /// Its file log's status in full ("5 successful (2 already in db), 1
    /// failed").
    pub fn files_status(&self) -> String {
        queues::file_log_status(&self.files)
    }

    /// Its files done and in all.
    pub fn progress(&self) -> (usize, usize) {
        queues::file_log_value_range(&self.files)
    }

    /// Its files done of all, "6/10" (nothing with none).
    pub fn progress_text(&self) -> String {
        match self.progress() {
            (_, 0) => String::new(),
            (done, total) => queues::value_range_text(done, total),
        }
    }

    /// Its search log's status ("1 successful, 2 pending").
    pub fn search_status(&self) -> String {
        queues::search_log_status(&self.searches).0
    }

    /// The line for the file it is downloading, under its file log.
    pub fn file_job_line(&self) -> JobLine {
        self.live
            .file_job
            .as_ref()
            .map(JobLive::line)
            .unwrap_or_default()
    }

    /// The line for the gallery page it is downloading, under its search
    /// log.
    pub fn gallery_job_line(&self) -> JobLine {
        self.live
            .gallery_job
            .as_ref()
            .map(JobLive::line)
            .unwrap_or_default()
    }

    /// Whether it has file work left and isn't paused (`CurrentlyWorking`).
    pub fn working(&self) -> bool {
        !self.paused
            && self
                .files
                .get(&queues::SeedStatus::Unknown)
                .is_some_and(|&n| n > 0)
    }

    /// Why closing its page needs asking about, as the reference says it
    /// (`CheckAbleToClose`): still importing, or (if `confirm_non_empty`,
    /// the option) holding imports.
    pub fn close_veto(&self, confirm_non_empty: bool) -> Option<String> {
        if self.working() {
            return Some("This page is still importing.".into());
        }
        let held: usize = self.files.values().sum();
        let kind = if self.local { "local" } else { "urls" };
        (confirm_non_empty && held > 0).then(|| {
            format!(
                "This is a {kind} import page holding {} import objects.",
                hydrus_core::numbers::human_int(held as u64)
            )
        })
    }
}

impl std::fmt::Debug for SearchPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchPage")
            .field("predicates", &self.predicates)
            .field("results", &self.results.len())
            .field("selected", &self.selection.count())
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl SearchPage {
    pub fn new(store: Arc<Store>) -> Self {
        let mut autocomplete = Autocomplete::new(store.clone());
        autocomplete.clear();
        let sorts: hydrus_core::pages::SortSettings =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        Self {
            autocomplete,
            store,
            context: FileSearchContext::default(),
            predicates: Vec::new(),
            synchronised: true,
            locked: false,
            lock_syncs: HashLock::default(),
            note: None,
            // the options' default sort
            sort: sorts.default_sort,
            sort_changed: false,
            fallback: sorts.fallback_sort,
            collect: sorts.default_collect,
            collections: HashMap::new(),
            came: Vec::new(),
            results: Vec::new(),
            selection: Selection::default(),
            tags: Vec::new(),
            error: None,
            duplicates: None,
            empty_status: std::cell::Cell::new(Some("no search done yet")),
            facts: HashMap::new(),
            importer: None,
            presented: std::collections::HashSet::new(),
            gallery: None,
            watchers: None,
        }
    }

    /// A duplicates page: how many potential pairs its search finds, and
    /// the files it showed.
    pub fn duplicates_page(
        store: Arc<Store>,
        duplicates: DuplicatesPage,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let snapshot = store.snapshot();
        let count = store.read(|conn| {
            match hydrus_duplicates::potentials::PotentialsQuery::from_search(
                &snapshot,
                &duplicates.search,
            ) {
                Ok(query) => query.count(conn, &snapshot),
                Err(_) => Ok(None),
            }
        });
        let note = match count {
            Ok(Some(n)) => format!(
                "A duplicates page: {} potential pairs to filter.",
                hydrus_core::numbers::human_int(n as u64)
            ),
            _ => "A duplicates page, whose search can't be run.".to_owned(),
        };
        let mut page = Self::fixed(store, note, sort, files);
        page.duplicates = Some(duplicates);
        page.empty_status.set(Some("no dupes found"));
        page
    }

    /// A duplicates page's filtering.
    pub fn duplicates(&self) -> Option<&DuplicatesPage> {
        self.duplicates.as_ref()
    }

    /// A search page as a session kept it: its search and sort, and the
    /// files it showed (it searches again when its search changes).
    pub fn restored(
        store: Arc<Store>,
        search: FileSearchContext,
        synchronised: bool,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let mut page = Self::new(store);
        let FileSearchContext {
            location,
            tags,
            predicates,
        } = search;
        page.context = FileSearchContext {
            location,
            tags,
            predicates: Vec::new(),
        };
        page.autocomplete
            .set_context(&page.context.location, &page.context.tags);
        page.predicates = predicates;
        page.synchronised = synchronised;
        page.set_page_sort(sort);
        // (collecting as the session says, `with_collect`: a page that
        // doesn't say doesn't)
        page.collect = PageCollect::default();
        page.came.clone_from(&files);
        page.results = files;
        page.learn_facts();
        page.count_tags();
        page
    }

    /// Load a favourite search, as the reference's favourites menu does:
    /// its domains, tag service and predicates, whether the page searches
    /// as they change, and its sort; it searches if synchronised. A page
    /// without a search (a downloader page, say) is left as it is.
    pub fn load_favourite(&mut self, favourite: &hydrus_core::pages::FavouriteSearch) {
        // (a locked page hides its favourites, with its search)
        if self.note.is_some() || self.locked {
            return;
        }
        let FileSearchContext {
            location,
            tags,
            predicates,
        } = favourite.search.clone();
        self.context = FileSearchContext {
            location,
            tags,
            predicates: Vec::new(),
        };
        self.autocomplete
            .set_context(&self.context.location, &self.context.tags);
        self.autocomplete.clear();
        self.predicates = predicates;
        self.synchronised = favourite.synchronised;
        if favourite.sort.is_some() {
            let before = self.sort.clone();
            self.set_page_sort(favourite.sort.as_ref());
            self.sort_changed |= self.sort != before;
        }
        // (the collect control collects the files shown at once, as its
        // `SetCollect` broadcasts)
        if let Some(collect) = &favourite.collect {
            self.set_collect(collect.clone());
        }
        if self.synchronised {
            self.search();
        }
    }

    /// A page that shows files but has no search, saying why.
    pub fn fixed(
        store: Arc<Store>,
        note: impl Into<String>,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let mut page = Self::new(store);
        page.note = Some(note.into());
        page.empty_status.set(Some("empty page"));
        page.set_page_sort(sort);
        page.came.clone_from(&files);
        page.results = files;
        page.learn_facts();
        page.count_tags();
        page
    }

    /// A URL downloader page over `queue`: the files it showed (as a
    /// session kept them), and those its queue brings from now on, added at
    /// its end as they come.
    pub fn url_downloader(
        store: Arc<Store>,
        queue: i64,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let mut page = Self::fixed(store, "A URL downloader page.", sort, files);
        page.importer = Some(Importer {
            queue,
            ..Importer::default()
        });
        page.read_import(true);
        page
    }

    /// A local import page (the reference's "import" page) over `queue`:
    /// its files as they are imported, in the queue's order.
    pub fn local_import(
        store: Arc<Store>,
        queue: i64,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let mut page = Self::fixed(store, "A local import page.", sort, files);
        page.importer = Some(Importer {
            queue,
            local: true,
            ..Importer::default()
        });
        page.read_import(true);
        page
    }

    /// Its importer, for a downloader page.
    pub fn importer(&self) -> Option<&Importer> {
        self.importer.as_ref()
    }

    /// A gallery downloader page over `queues` (its searches), with its own
    /// settings and the search it shows: the files it showed (as a session
    /// kept them), and those the shown search brings from now on.
    #[allow(clippy::too_many_arguments)]
    pub fn gallery_downloader(
        store: Arc<Store>,
        page_key: hydrus_core::pages::PageKey,
        page_name: &str,
        queues: Vec<i64>,
        state: Option<hydrus_core::pages::DownloaderPageState>,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let mut page = Self::fixed(store, "A gallery downloader page.", sort, files);
        page.empty_status.set(Some("no highlighted query"));
        let state = state.unwrap_or_else(|| new_gallery_state(&page.store));
        if let Some(queue) = state.highlighted {
            page.importer = Some(Importer {
                queue,
                ..Importer::default()
            });
        }
        let (definitions, settings, naming) = page
            .store
            .read(|c| {
                Ok((
                    hydrus_store::settings::get::<hydrus_parse::Downloaders>(c)?,
                    hydrus_store::settings::get::<hydrus_core::pages::DownloaderPageSettings>(c)?,
                    hydrus_store::settings::get::<hydrus_core::pages::PageNameSettings>(c)?,
                ))
            })
            .unwrap_or_default();
        page.gallery = Some(GalleryView {
            page_key,
            page_name: page_name.to_owned(),
            queues,
            queries: Vec::new(),
            state,
            sort: (Column::Query, true),
            selected: None,
            gugs: crate::gallery::offered_gugs(&definitions.gugs),
            settings,
            short_summary: (naming.short_summary_new, naming.short_summary_deleted),
        });
        page.refresh_import();
        page.read_import(true);
        page
    }

    /// A gallery page's searches, as last read.
    pub fn gallery(&self) -> Option<&GalleryView> {
        self.gallery.as_ref()
    }

    /// A watcher downloader page over `queues` (its watchers), with its own
    /// checker and import options and the watcher it shows: the files it
    /// showed (as a session kept them), and those the shown watcher brings
    /// from now on.
    #[allow(clippy::too_many_arguments)]
    pub fn watcher_downloader(
        store: Arc<Store>,
        page_key: hydrus_core::pages::PageKey,
        page_name: &str,
        queues: Vec<i64>,
        state: Option<hydrus_core::pages::DownloaderPageState>,
        sort: Option<&PageSort>,
        files: Vec<HashId>,
    ) -> Self {
        let mut page = Self::fixed(store, "A watcher downloader page.", sort, files);
        page.empty_status.set(Some("no highlighted watcher"));
        let state = state.unwrap_or_else(|| new_watcher_state(&page.store));
        if let Some(queue) = state.highlighted {
            page.importer = Some(Importer {
                queue,
                ..Importer::default()
            });
        }
        let (settings, naming) = page
            .store
            .read(|c| {
                Ok((
                    hydrus_store::settings::get::<hydrus_core::pages::DownloaderPageSettings>(c)?,
                    hydrus_store::settings::get::<hydrus_core::pages::PageNameSettings>(c)?,
                ))
            })
            .unwrap_or_default();
        page.watchers = Some(WatcherView {
            page_key,
            page_name: page_name.to_owned(),
            queues,
            watchers: Vec::new(),
            state,
            // (the reference's default: by status)
            sort: (crate::watcher::Column::Status, true),
            selected: None,
            added: Vec::new(),
            already: Vec::new(),
            settings,
            short_summary: (naming.short_summary_new, naming.short_summary_deleted),
        });
        page.refresh_import();
        page.read_import(true);
        page
    }

    /// A watcher page's watchers, as last read.
    pub fn watchers(&self) -> Option<&WatcherView> {
        self.watchers.as_ref()
    }

    /// A gallery or watcher page's own state.
    fn multi_state(&mut self) -> Option<&mut hydrus_core::pages::DownloaderPageState> {
        match (&mut self.gallery, &mut self.watchers) {
            (Some(gallery), _) => Some(&mut gallery.state),
            (None, Some(watchers)) => Some(&mut watchers.state),
            (None, None) => None,
        }
    }

    /// Sort the watchers' list by a column.
    pub fn sort_watchers(&mut self, column: crate::watcher::Column, ascending: bool) {
        if let Some(view) = &mut self.watchers {
            view.sort = (column, ascending);
            let now = now();
            let mut watchers = std::mem::take(&mut view.watchers);
            crate::watcher::sort(&mut watchers, column, ascending, now, &|w| {
                Some(view.simple_status(w, now).0)
            });
            view.watchers = watchers;
        }
    }

    /// Watch the threads typed or pasted into the page (`_AddURLs`), one a
    /// line, with the page's checker and import options: a thread it
    /// watches already says so, and the first new watcher is shown if
    /// none is and the options say so.
    pub fn pend_watchers(&mut self, text: &str) {
        let Some(view) = &self.watchers else {
            return;
        };
        let checker = view.state.checker.clone().unwrap_or_else(|| {
            self.store
                .read(hydrus_store::settings::get::<hydrus_core::subscriptions::CheckerDefaults>)
                .unwrap_or_default()
                .watchers
        });
        let (options, name, key) = (
            view.state.options.clone(),
            view.page_name.clone(),
            view.page_key.0,
        );
        let mut queues = view.queues.clone();
        let now = now();
        let (mut added, mut already) = (Vec::new(), Vec::new());
        for url in text
            .lines()
            .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
            .filter(|line| !line.is_empty())
        {
            let page = hydrus_store::watchers::WatcherPage {
                name: &name,
                key: Some(&key),
                queues: &queues,
                checker: &checker,
                options: &options,
            };
            match hydrus_store::watchers::add_watcher(&self.store, &page, url, now) {
                Ok(hydrus_store::watchers::Watched::New(queue)) => {
                    queues.push(queue.id);
                    added.push((queue.id, now));
                }
                Ok(hydrus_store::watchers::Watched::Already(queue)) => already.push((queue, now)),
                Ok(hydrus_store::watchers::Watched::NotAUrl) => {}
                Err(e) => eprintln!("could not watch {url}: {e}"),
            }
        }
        let first = added.first().map(|&(queue, _)| queue);
        let view = self.watchers.as_mut().expect("a watcher page");
        view.queues = queues;
        view.added.extend(added);
        view.already.extend(already);
        let show = view.state.highlighted.is_none() && view.settings.highlight_new_watcher;
        self.refresh_import();
        if show && first.is_some() {
            self.highlight_query(first);
        }
    }

    /// Pause or resume a watcher's files, or its checking (which a dead
    /// watcher only resumes when checked now), nudging the daemon.
    pub fn pause_play_watcher(&mut self, queue: i64, checking: bool) {
        let Some(watcher) = self.watchers.as_ref().and_then(|v| v.watcher(queue)) else {
            return;
        };
        let done = if checking {
            hydrus_store::watchers::pause_play_checking(&self.store, queue)
        } else {
            let files = !watcher.files_paused;
            self.store.write(move |ctx| {
                queues::set_paused(ctx.conn(), queue, Some(files), None)?;
                queues::nudge(ctx.conn(), queue)
            })
        };
        if let Err(e) = done {
            eprintln!("could not pause or resume the watcher: {e}");
        }
        self.refresh_import();
    }

    /// Check a watcher's thread again now (`CheckNow`).
    pub fn check_watcher_now(&mut self, queue: i64) {
        if let Err(e) = hydrus_store::watchers::check_now(&self.store, queue, now()) {
            eprintln!("could not check the watcher now: {e}");
        }
        self.refresh_import();
    }

    /// What removing a watcher asks first (`_RemoveWatchers`).
    pub fn remove_watcher_question(&self, queue: i64) -> Option<String> {
        let view = self.watchers.as_ref()?;
        let watcher = view.watcher(queue)?;
        let mut message = "Remove the 1 selected watchers?".to_owned();
        if watcher.importing() {
            message.push_str("\n\n1 are still working.");
        }
        if !watcher.dead() {
            message.push_str("\n\n1 are not yet DEAD.");
        }
        if view.state.highlighted == Some(queue) {
            message.push_str(
                "\n\nThe currently highlighted watcher will be removed, and the media panel cleared.",
            );
        }
        Some(message)
    }

    /// How fast the shown watcher's thread was getting files at its last
    /// check, as its box says.
    pub fn watcher_velocity(&self) -> String {
        let Some(view) = &self.watchers else {
            return String::new();
        };
        let Some(watcher) = view.state.highlighted.and_then(|h| view.watcher(h)) else {
            return String::new();
        };
        self.store
            .read(|c| crate::watcher::velocity(c, watcher))
            .unwrap_or_default()
    }

    /// Show a search's files in the page, as the reference's highlight
    /// does (none, or highlighting the one shown again, clears it).
    pub fn highlight_query(&mut self, queue: Option<i64>) {
        let empty = if self.watchers.is_some() {
            "no highlighted watcher"
        } else {
            "no highlighted query"
        };
        let Some(state) = self.multi_state() else {
            return;
        };
        let queue = queue.filter(|q| state.highlighted != Some(*q));
        state.highlighted = queue;
        self.presented.clear();
        self.selection.clear();
        self.collections.clear();
        self.importer = queue.map(|queue| Importer {
            queue,
            ..Importer::default()
        });
        let files = match queue {
            Some(queue) => self
                .store
                .read(|c| queues::presented_files(c, queue))
                .unwrap_or_default(),
            None => Vec::new(),
        };
        self.came.clone_from(&files);
        self.results = files;
        if queue.is_none() {
            self.empty_status.set(Some(empty));
        }
        self.read_import(true);
        self.resort();
        self.learn_facts();
        self.count_tags();
    }

    /// Select a search, or a watcher, in the list.
    pub fn select_query(&mut self, queue: Option<i64>) {
        if let Some(gallery) = &mut self.gallery {
            gallery.selected = queue;
        }
        if let Some(watchers) = &mut self.watchers {
            watchers.selected = queue;
        }
    }

    /// Sort the list by a column.
    pub fn sort_queries(&mut self, column: Column, ascending: bool) {
        if let Some(gallery) = &mut self.gallery {
            gallery.sort = (column, ascending);
            crate::gallery::sort(&mut gallery.queries, column, ascending);
        }
    }

    /// Make searches for the queries typed or pasted into the page (one a
    /// line), with the page's downloader and settings, and show the first
    /// if none is shown and the options say so; why not, if they can't be.
    pub fn pend_queries(&mut self, text: &str) -> Result<(), String> {
        let Some(gallery) = &self.gallery else {
            return Ok(());
        };
        let queries: Vec<String> = text
            .lines()
            .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        if queries.is_empty() {
            return Ok(());
        }
        let definitions: hydrus_parse::Downloaders = self
            .store
            .read(hydrus_store::settings::get)
            .map_err(|e| e.to_string())?;
        let page = gallery.gallery();
        // (as the reference's page says, before trying)
        if page.gug_name.is_empty() {
            return Err(if definitions.gugs.gugs.is_empty() {
                "Hey, you do not have any downloaders in this client! Check out the \
                 _network->downloaders_ menu to find downloaders made by users."
            } else {
                "Hey, you do not have a downloader set here! Click the downloader selector \
                 and choose somewhere to download from."
            }
            .into());
        }
        let existing: Vec<(String, String)> = gallery
            .queries
            .iter()
            .map(|q| (q.query.clone(), q.source.clone()))
            .collect();
        let how = hydrus_store::gallery::NewSearches {
            page_name: Some(&gallery.page_name),
            page_key: Some(&gallery.page_key.0),
            gug_key: &page.gug_key,
            gug_name: &page.gug_name,
            file_limit: Some(page.file_limit),
            options: gallery.state.options.clone(),
            start_files_paused: page.start_files_paused,
            start_gallery_paused: page.start_gallery_paused,
            merge: page.merge_pends,
            no_new_dupes: page.no_new_dupes,
            existing: &existing,
        };
        let made = hydrus_store::gallery::create_gallery_searches(
            &self.store,
            &definitions,
            &how,
            &queries,
            now(),
        )
        .map_err(|e| e.to_string())?;
        let gallery = self.gallery.as_mut().expect("a gallery page");
        // (the downloader as it is now called)
        if let Some(own) = &mut gallery.state.gallery {
            own.gug_key.clone_from(&made.gug_key);
            own.gug_name.clone_from(&made.gug_name);
        }
        gallery.queues.extend(made.queues.iter().map(|q| q.id));
        let first = made.queues.first().map(|q| q.id);
        let show = gallery.state.highlighted.is_none() && gallery.settings.highlight_new_query;
        self.refresh_import();
        if show && first.is_some() {
            self.highlight_query(first);
        }
        Ok(())
    }

    /// Pause or resume a search's files, or its search, nudging the daemon.
    pub fn pause_play_query(&mut self, queue: i64, search: bool) {
        let Some(query) = self.gallery.as_ref().and_then(|g| g.query(queue)) else {
            return;
        };
        let (files, gallery) = if search {
            (None, Some(!query.gallery_paused))
        } else {
            (Some(!query.files_paused), None)
        };
        if let Err(e) = self.store.write(move |ctx| {
            queues::set_paused(ctx.conn(), queue, files, gallery)?;
            queues::nudge(ctx.conn(), queue)
        }) {
            eprintln!("could not pause or resume the search: {e}");
        }
        self.refresh_import();
    }

    /// Try a search's ignored files, or its failed ones, again.
    pub fn retry_query(&mut self, queue: i64, ignored: bool) {
        let status = if ignored {
            queues::SeedStatus::Vetoed
        } else {
            queues::SeedStatus::Error
        };
        let now = now();
        if let Err(e) = self.store.write(move |ctx| {
            queues::retry_file_seeds(ctx.conn(), queue, &[status], now)?;
            queues::nudge(ctx.conn(), queue)
        }) {
            eprintln!("could not retry the search's files: {e}");
        }
        self.refresh_import();
    }

    /// What removing a search asks first (`_RemoveGalleryImports`).
    pub fn remove_query_question(&self, queue: i64) -> Option<String> {
        let gallery = self.gallery.as_ref()?;
        let query = gallery.query(queue)?;
        let mut message = "Remove the 1 selected queries?".to_owned();
        if query.importing() {
            message.push_str("\n\n1 are still working.");
        }
        if gallery.state.highlighted == Some(queue) {
            message.push_str(
                "\n\nThe currently highlighted query will be removed, and the media panel cleared.",
            );
        }
        Some(message)
    }

    /// Remove a search, and its queue (the page shows nothing if it showed
    /// it).
    pub fn remove_query(&mut self, queue: i64) {
        let shown = match (&mut self.gallery, &mut self.watchers) {
            (Some(gallery), _) => {
                gallery.queues.retain(|q| *q != queue);
                gallery.queries.retain(|q| q.queue != queue);
                if gallery.selected == Some(queue) {
                    gallery.selected = None;
                }
                gallery.state.highlighted == Some(queue)
            }
            (None, Some(watchers)) => {
                watchers.queues.retain(|q| *q != queue);
                watchers.watchers.retain(|w| w.queue != queue);
                if watchers.selected == Some(queue) {
                    watchers.selected = None;
                }
                watchers.state.highlighted == Some(queue)
            }
            (None, None) => return,
        };
        if let Err(e) = self
            .store
            .write(move |ctx| queues::delete_queue(ctx.conn(), queue))
        {
            eprintln!("could not remove the search: {e}");
        }
        if shown {
            self.highlight_query(None);
        }
    }

    /// Set the page's downloader for new searches.
    pub fn set_gug(&mut self, key: &str, name: &str) {
        if let Some(gallery) = &mut self.gallery {
            let mut own = gallery.gallery();
            key.clone_into(&mut own.gug_key);
            name.clone_into(&mut own.gug_name);
            gallery.state.gallery = Some(own);
        }
    }

    /// Set the page's file limit for new searches (`None`: no limit).
    pub fn set_file_limit(&mut self, limit: Option<u64>) {
        if let Some(gallery) = &mut self.gallery {
            let mut own = gallery.gallery();
            own.file_limit = limit;
            gallery.state.gallery = Some(own);
        }
    }

    /// Read the importer's queue again: its counts, pause and live state,
    /// and the files it brought since, added at the page's end (as the
    /// reference presents them to its page). What changed.
    pub fn refresh_import(&mut self) -> ImportRefresh {
        let gallery_changed = match &mut self.gallery {
            Some(gallery) => self.store.read(|c| gallery.refresh(c)).unwrap_or_else(|e| {
                eprintln!("could not read the gallery page's searches: {e}");
                false
            }),
            None => false,
        };
        let gallery_changed = gallery_changed
            || match &mut self.watchers {
                Some(watchers) => {
                    let now = now();
                    let said = |&(_, at): &(i64, i64)| now <= at + crate::watcher::SAID_FOR;
                    watchers.added.retain(said);
                    watchers.already.retain(said);
                    self.store
                        .read(|c| watchers.refresh(c))
                        .unwrap_or_else(|e| {
                            eprintln!("could not read the watcher page's watchers: {e}");
                            false
                        })
                }
                None => false,
            };
        match self.read_import(false) {
            ImportRefresh::Nothing if gallery_changed => ImportRefresh::Status,
            refreshed => refreshed,
        }
    }

    fn read_import(&mut self, first: bool) -> ImportRefresh {
        let Some(queue) = self.importer.as_ref().map(|i| i.queue) else {
            return ImportRefresh::Nothing;
        };
        let read = self.store.read(|c| {
            Ok((
                queues::queue(c, queue)?,
                queues::file_seed_counts(c, queue)?,
                queues::gallery_seed_counts(c, queue)?,
                queues::presented_files(c, queue)?,
                live::live(c, &[queue])?.remove(&queue),
            ))
        });
        let Ok((row, files, searches, presented, live)) = read else {
            return ImportRefresh::Nothing;
        };
        let now = Importer {
            queue,
            files,
            searches,
            paused: row.as_ref().is_some_and(|q| q.files_paused),
            live: live.unwrap_or_default(),
            local: row.is_some_and(|q| q.kind == queues::QueueKind::LocalImport),
        };
        let status_changed = self.importer.as_ref() != Some(&now);
        self.importer = Some(now);
        let arrived: Vec<HashId> = presented
            .into_iter()
            .filter(|f| self.presented.insert(*f))
            .collect();
        if !first && !arrived.is_empty() && self.add_files(&arrived) {
            ImportRefresh::Files
        } else if status_changed {
            ImportRefresh::Status
        } else {
            ImportRefresh::Nothing
        }
    }

    /// Pause or resume the importer (the reference's one switch pauses its
    /// files and its search together).
    pub fn pause_play_files(&mut self) {
        let Some(importer) = &self.importer else {
            return;
        };
        let (queue, paused) = (importer.queue, !importer.paused);
        let done = self.store.write(move |ctx| {
            queues::set_paused(ctx.conn(), queue, Some(paused), Some(paused))?;
            queues::nudge(ctx.conn(), queue)
        });
        match done {
            Ok(()) => {
                self.refresh_import();
            }
            Err(e) => eprintln!("could not pause or resume the importer: {e}"),
        }
    }

    /// Ask the daemon to stop the importer's current download of this kind
    /// (the reference's network job control's cancel button).
    pub fn cancel_download(&self, kind: JobKind) {
        let Some(importer) = &self.importer else {
            return;
        };
        let queue = importer.queue;
        if let Err(e) = self
            .store
            .write(move |ctx| live::cancel(ctx.conn(), queue, kind))
        {
            eprintln!("could not cancel the download: {e}");
        }
    }

    /// Hand URLs typed or pasted into the page to the daemon, which adds
    /// those it can as the reference does (`PendURLs`): each line, trimmed,
    /// empty ones dropped.
    pub fn pend_urls(&self, text: &str) {
        let Some(importer) = &self.importer else {
            return;
        };
        let urls: Vec<String> = text
            .lines()
            .map(|line| line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        if urls.is_empty() {
            return;
        }
        let queue = importer.queue;
        if let Err(e) = self
            .store
            .write(move |ctx| queues::request_urls(ctx.conn(), queue, &urls))
        {
            eprintln!("could not add the URLs: {e}");
        }
    }

    /// What the status bar says while the page is empty (a downloader
    /// page's "no highlighted query", say).
    #[must_use]
    pub fn with_empty_status(self, status: &'static str) -> Self {
        self.empty_status.set(Some(status));
        self
    }

    /// A session's sort, if it has one; otherwise the default stays.
    fn set_page_sort(&mut self, sort: Option<&PageSort>) {
        if let Some(sort) = sort {
            self.sort = sort.clone();
        }
    }

    /// The page as a session keeps it, given what it was opened from.
    pub fn content(&self, opened_from: &PageContent) -> PageContent {
        let sort = if self.sort_changed {
            Some(self.sort.clone())
        } else {
            opened_from.sort().cloned()
        };
        match opened_from {
            _ if self.note.is_none() => PageContent::Search {
                search: FileSearchContext {
                    predicates: self.predicates.clone(),
                    ..self.context.clone()
                },
                synchronised: self.synchronised,
                sort,
                lock: self.lock(),
                collect: Some(self.collect.clone()),
            },
            PageContent::Downloader {
                kind, queues, page, ..
            } => match (&self.gallery, &self.watchers) {
                (Some(gallery), _) => PageContent::Downloader {
                    kind: *kind,
                    queues: gallery.queues.clone(),
                    sort,
                    page: Some(Box::new(gallery.state.clone())),
                },
                (None, Some(watchers)) => PageContent::Downloader {
                    kind: *kind,
                    queues: watchers.queues.clone(),
                    sort,
                    page: Some(Box::new(watchers.state.clone())),
                },
                (None, None) => PageContent::Downloader {
                    kind: *kind,
                    queues: queues.clone(),
                    sort,
                    page: page.clone(),
                },
            },
            PageContent::Other {
                page_type, stored, ..
            } => PageContent::Other {
                page_type: *page_type,
                stored: stored.clone(),
                sort,
            },
            other => other.clone(),
        }
    }

    /// Why the page has no search, if it hasn't.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// The predicates, written as the reference writes them.
    pub fn predicates(&self) -> Vec<String> {
        let snapshot = self.store.snapshot();
        let viewing = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        let mut context = TextContext::from_store(&snapshot.services, &viewing);
        context.presentation = self.store.read(hydrus_store::settings::get).ok();
        self.predicates
            .iter()
            .map(|p| predicate_text(p, &context))
            .collect()
    }

    /// Count the tag list's tags again (after they were changed).
    pub fn refresh_tags(&mut self) {
        self.count_tags();
    }

    /// The file domains the page searches.
    pub fn location(&self) -> &hydrus_search::LocationContext {
        &self.context.location
    }

    /// Add files at the page's end, those it doesn't have, in the order
    /// given, as files of their own, as the reference's pages take them
    /// (the Client API's `/manage_pages/add_files`: `AddMediaResults`);
    /// whether any were added.
    pub fn add_files(&mut self, files: &[HashId]) -> bool {
        let mut have: std::collections::HashSet<HashId> = self.files().into_iter().collect();
        let added: Vec<HashId> = files.iter().copied().filter(|f| have.insert(*f)).collect();
        if added.is_empty() {
            return false;
        }
        self.came.extend(&added);
        self.results.extend(&added);
        self.learn_facts();
        self.count_tags();
        true
    }

    /// Take files off the page (deleted from its domain, say), as the
    /// reference's pages drop them, out of their collections (one left
    /// empty goes); a locked search lets go of them too, if it follows
    /// removals (`NotifyFilesRemoved`).
    pub fn remove_files(&mut self, files: &[HashId]) {
        let gone: std::collections::HashSet<HashId> = files.iter().copied().collect();
        let emptied: Vec<HashId> = self
            .results
            .iter()
            .copied()
            .filter(|item| self.files_of(*item).iter().all(|f| gone.contains(f)))
            .collect();
        self.selection.remove(&self.results, &emptied);
        self.results.retain(|item| !emptied.contains(item));
        self.came.retain(|f| !gone.contains(f));
        // (a collection left with files is shown by its first now)
        let mut renamed: HashMap<HashId, HashId> = HashMap::new();
        for item in &mut self.results {
            if let Some(mut members) = self.collections.remove(item) {
                members.retain(|f| !gone.contains(f));
                renamed.insert(*item, members[0]);
                *item = members[0];
                self.collections.insert(members[0], members);
            }
        }
        let shown: std::collections::HashSet<HashId> = self.results.iter().copied().collect();
        self.collections.retain(|key, _| shown.contains(key));
        if !renamed.is_empty() {
            self.selection
                .remap(|f| Some(renamed.get(&f).copied().unwrap_or(f)));
        }
        self.count_tags();
        if self.locked && self.lock_syncs.syncs_removes {
            let removed = self.sha256s(files);
            if let Some(mut hashes) = self.lock_hashes() {
                hashes.retain(|h| !removed.contains(h));
                self.set_lock_hashes(hashes);
            }
        }
    }

    /// The page restored with its search locked as a session kept it.
    #[must_use]
    pub fn with_lock(mut self, lock: Option<HashLock>) -> Self {
        self.locked = lock.is_some();
        self.lock_syncs = lock.unwrap_or_default();
        self
    }

    /// Whether the search is locked to a `system:hash` of the page's
    /// files, and what that hash follows.
    pub fn lock(&self) -> Option<HashLock> {
        self.locked.then_some(self.lock_syncs)
    }

    /// How many files a locked search holds (its `system:hash`'s), for
    /// the reference's "Locked at N files." (0 if the search is not just
    /// such a hash).
    pub fn locked_count(&self) -> usize {
        self.lock_hashes().map_or(0, |h| h.len())
    }

    /// Unlock the search: it becomes its `system:hash`, which can be
    /// edited (`UnlockSearch`).
    pub fn unlock(&mut self) {
        self.locked = false;
    }

    /// Set what a locked search's hash follows: files added to the page,
    /// and files removed from it.
    pub fn set_lock_syncs(&mut self, syncs_new: bool, syncs_removes: bool) {
        self.lock_syncs = HashLock {
            syncs_new,
            syncs_removes,
        };
    }

    /// What to ask before locking the search to the files in view, if
    /// anything (`LockSearch`): nothing for an empty search, or for a
    /// `system:hash` of just those files.
    pub fn lock_question(&self) -> Option<&'static str> {
        if self.predicates.is_empty() {
            return None;
        }
        match self.lock_hashes() {
            Some(hashes) if hashes == self.sha256s(&self.files()) => None,
            Some(_) => Some(
                "This will lock the page, collapsing the current search to a system:hash of \
                 the current files.\n\nYour search already has a system:hash, but its files \
                 are different than what is currently in view. If you want to lock your \
                 current system:hash, not what is currently in view, click no and refresh the \
                 search to reset you back to what the existing system:hash says, and then try \
                 locking again.",
            ),
            None => Some(
                "This will lock the page, collapsing the current search to a system:hash of \
                 the current files. Is this ok?",
            ),
        }
    }

    /// Lock the search to a `system:hash` of the files in view (after
    /// `lock_question`, if it asks anything).
    pub fn lock_search(&mut self) {
        if self.note.is_some() {
            return;
        }
        self.locked = true;
        let hashes = self.sha256s(&self.files());
        self.set_lock_hashes(hashes);
    }

    /// The search's hashes, if it is just an inclusive sha256
    /// `system:hash` (`_GetExistingLockHashes`).
    fn lock_hashes(&self) -> Option<std::collections::BTreeSet<hydrus_core::Sha256>> {
        match &self.predicates[..] {
            [
                Predicate::System(SystemPredicate::Hash {
                    hashes: FileHashes::Sha256(hashes),
                    inclusive: true,
                }),
            ] => Some(hashes.clone()),
            _ => None,
        }
    }

    /// Make the search a `system:hash` of `hashes` (`_UpdateSystemLockFiles`).
    fn set_lock_hashes(&mut self, hashes: std::collections::BTreeSet<hydrus_core::Sha256>) {
        self.predicates = vec![Predicate::System(SystemPredicate::Hash {
            hashes: FileHashes::Sha256(hashes),
            inclusive: true,
        })];
    }

    fn sha256s(&self, files: &[HashId]) -> std::collections::BTreeSet<hydrus_core::Sha256> {
        self.store
            .read(|c| hydrus_store::master::hashes(c, files))
            .map(|hashes| hashes.into_values().collect())
            .unwrap_or_default()
    }

    /// The page's items, in order: its files, a collection shown by its
    /// first file.
    pub fn results(&self) -> &[HashId] {
        &self.results
    }

    /// How far the page's importing has got: its done and total imports
    /// (none for a page that doesn't import).
    pub fn import_progress(&self) -> (usize, usize) {
        if let Some(gallery) = &self.gallery {
            return crate::gallery::value_range(&gallery.queries);
        }
        if let Some(watchers) = &self.watchers {
            return crate::watcher::value_range(&watchers.watchers);
        }
        self.importer.as_ref().map_or((0, 0), Importer::progress)
    }

    /// Why closing the page needs asking about, for a downloader page.
    pub fn close_veto(&self, confirm_non_empty: bool) -> Option<String> {
        if let Some(gallery) = &self.gallery {
            return crate::gallery::close_veto(&gallery.queries, confirm_non_empty);
        }
        if let Some(watchers) = &self.watchers {
            return crate::watcher::close_veto(&watchers.watchers, confirm_non_empty);
        }
        self.importer.as_ref()?.close_veto(confirm_non_empty)
    }

    /// The page's files, in order, its collections' in theirs.
    pub fn files(&self) -> Vec<HashId> {
        self.flatten(&self.results)
    }

    /// The files of the item shown by `item`: a collection's, or the file.
    pub fn files_of(&self, item: HashId) -> Vec<HashId> {
        self.collections
            .get(&item)
            .cloned()
            .unwrap_or_else(|| vec![item])
    }

    /// The collection the item shown by `item` is, if it is one.
    pub fn collection(&self, item: HashId) -> Option<&[HashId]> {
        self.collections.get(&item).map(Vec::as_slice)
    }

    fn flatten(&self, items: &[HashId]) -> Vec<HashId> {
        items
            .iter()
            .flat_map(|&item| {
                self.collections
                    .get(&item)
                    .cloned()
                    .unwrap_or_else(|| vec![item])
            })
            .collect()
    }

    /// How the page collects its files.
    pub fn collect(&self) -> &PageCollect {
        &self.collect
    }

    /// Collect the page's files anew (the reference's collect control), and
    /// sort them; as the reference's `Collect`, it selects nothing first.
    pub fn set_collect(&mut self, collect: PageCollect) {
        self.collect = collect;
        self.selection.select_none(&self.results);
        self.resort();
        self.count_tags();
    }

    /// The page restored collecting as a session kept it: collected and
    /// sorted if it collects (as the reference's page does on loading),
    /// else in the order kept.
    #[must_use]
    pub fn with_collect(mut self, collect: Option<PageCollect>) -> Self {
        match collect {
            Some(collect) if collect.collects() => self.set_collect(collect),
            Some(collect) => self.collect = collect,
            None => {}
        }
        self
    }

    /// The page's sort.
    pub fn sort(&self) -> &PageSort {
        &self.sort
    }

    /// The page's sort as a system sort, if it is one.
    pub fn file_sort(&self) -> Option<FileSort> {
        system_sort(&self.sort)
    }

    /// Sort by the system sort `by`, in its default order (as the
    /// reference's sort control does when the type changes).
    pub fn set_sort_by(&mut self, by: SortBy) {
        self.set_sort_type(PageSortBy::System(i64::from(by.code())));
    }

    /// Sort by `by` (a system, namespace or rating sort), in its default
    /// order.
    pub fn set_sort_type(&mut self, by: PageSortBy) {
        let ascending = crate::sort::page_choices(&self.store, &by)
            .into_iter()
            .find(|c| c.by == by)
            .is_none_or(|c| c.default_ascending);
        self.sort = PageSort { by, ascending };
        self.sort_changed = true;
        self.resort();
    }

    pub fn set_sort_order(&mut self, order: SortOrder) {
        self.sort.ascending = order == SortOrder::Ascending;
        self.sort_changed = true;
        self.resort();
    }

    /// Sort the files shown again (a new sort doesn't search again), as
    /// the reference's pages sort: the fallback sort first, then the
    /// page's, from the order they are in; collecting them first if the
    /// page collects (the selection following its files).
    fn resort(&mut self) {
        let snapshot = self.store.snapshot();
        let clock = Clock::system();
        let collect = self.collect.clone();
        let arranged = self.store.read(|conn| {
            Ok(if collect.collects() {
                collect_page_files(
                    conn,
                    &snapshot,
                    &self.context,
                    &self.came,
                    &collect,
                    &self.sort,
                    Some(&self.fallback),
                    &clock,
                )
            } else {
                sort_page_files(
                    conn,
                    &snapshot,
                    &self.context,
                    &self.files(),
                    &self.sort,
                    Some(&self.fallback),
                    &clock,
                )
                .map(|sorted| sorted.into_iter().map(PageMedia::File).collect())
            })
        });
        let media = match arranged {
            Ok(Ok(media)) => media,
            Ok(Err(e)) => {
                self.error = Some(e.to_string());
                return;
            }
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let mut results = Vec::with_capacity(media.len());
        let mut collections = HashMap::new();
        let mut item_of: HashMap<HashId, HashId> = HashMap::new();
        for m in media {
            match m {
                PageMedia::File(file) => {
                    item_of.insert(file, file);
                    results.push(file);
                }
                PageMedia::Collection(members) => {
                    let key = members[0];
                    item_of.extend(members.iter().map(|&f| (f, key)));
                    results.push(key);
                    collections.insert(key, members);
                }
            }
        }
        self.results = results;
        self.collections = collections;
        self.selection.remap(|f| item_of.get(&f).copied());
    }

    /// The selected files, in the page's order, collections' as theirs.
    pub fn selected_files(&self) -> Vec<HashId> {
        self.flatten(&self.selection.files(&self.results))
    }

    /// The selected items: files, and collections by their first file.
    pub fn selected_items(&self) -> Vec<HashId> {
        self.selection.files(&self.results)
    }

    /// How many items are selected, and how many of those are collections.
    pub fn selected_counts(&self) -> crate::status::Items {
        let items = self.selected_items();
        crate::status::Items {
            collections: items
                .iter()
                .filter(|i| self.collections.contains_key(i))
                .count(),
            items: items.len(),
        }
    }

    /// The selected files' indices.
    pub fn selected_indices(&self) -> std::collections::BTreeSet<usize> {
        self.results
            .iter()
            .enumerate()
            .filter(|(_, f)| self.selection.is_selected(**f))
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether the file at `index` is selected.
    pub fn is_selected(&self, index: usize) -> bool {
        self.results
            .get(index)
            .is_some_and(|&f| self.selection.is_selected(f))
    }

    /// The focused file's index, if a file is focused.
    pub fn focused(&self) -> Option<usize> {
        let focused = self.selection.focused()?;
        self.results.iter().position(|&f| f == focused)
    }

    /// Why the last change could not be made, if it couldn't.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn autocomplete(&self) -> &Autocomplete {
        &self.autocomplete
    }

    /// The search box's text changed.
    pub fn type_text(&mut self, text: &str) {
        self.autocomplete.set_text(text);
    }

    /// Move the autocomplete's highlight.
    pub fn move_highlight(&mut self, by: isize) {
        self.autocomplete.move_highlight(by);
    }

    /// Enter in the search box: add the highlighted suggestion (or the text
    /// as typed), and empty the box if that worked.
    pub fn enter(&mut self) {
        if let Some(chosen) = self.autocomplete.chosen()
            && self.add_predicate(&chosen)
        {
            self.autocomplete.clear();
        }
    }

    /// A suggestion was clicked.
    pub fn choose(&mut self, index: usize) {
        let Some(suggestion) = self.autocomplete.suggestions().get(index) else {
            return;
        };
        let predicate = suggestion.predicate.clone();
        if self.add_predicate(&predicate) {
            self.autocomplete.clear();
        }
    }

    /// Add a predicate as typed (a tag, or a system predicate such as
    /// `system:inbox`) and search again; whether it was taken. One that
    /// doesn't parse is refused with the reason; one already there is not
    /// added twice.
    pub fn add_predicate(&mut self, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        if self.note.is_some() {
            self.error = Some("this page has no search".into());
            return false;
        }
        if self.locked {
            return false;
        }
        let parsed = match parse_api_search(&serde_json::json!([text])) {
            Ok(parsed) => parsed,
            Err(e) => {
                self.error = Some(e.to_string());
                return false;
            }
        };
        for predicate in parsed {
            if !self.predicates.contains(&predicate) {
                self.predicates.push(predicate);
            }
        }
        self.error = None;
        if self.synchronised {
            self.search();
        }
        true
    }

    pub fn remove_predicate(&mut self, index: usize) {
        if !self.locked && index < self.predicates.len() {
            self.predicates.remove(index);
            if self.synchronised {
                self.search();
            }
        }
    }

    /// A plain click on the file at `index`.
    pub fn select(&mut self, index: usize) {
        self.hit(Some(index), false, false);
    }

    /// A click on the file at `index` (or on none), with ctrl or shift
    /// held, as the reference's thumbnail grid takes it.
    pub fn hit(&mut self, index: Option<usize>, ctrl: bool, shift: bool) {
        let file = index.and_then(|i| self.results.get(i).copied());
        if index.is_some() && file.is_none() {
            return;
        }
        self.selection.hit(&self.results, file, ctrl, shift);
        self.count_tags();
    }

    /// Select just `files` (the menu's select), as the reference's
    /// `_Select` does: the items they are in.
    pub fn select_files(&mut self, files: &[HashId]) {
        let wanted: std::collections::HashSet<HashId> = files.iter().copied().collect();
        let items: Vec<HashId> = self
            .results
            .iter()
            .copied()
            .filter(|&item| self.files_of(item).iter().any(|f| wanted.contains(f)))
            .collect();
        self.selection.select_only(&self.results, &items);
        self.count_tags();
    }

    /// Search again (F5, the menu's "refresh"), if the page has a search
    /// that isn't locked.
    pub fn refresh(&mut self) {
        // (`RefreshQuery`: a locked search stays as it is, and a paused one
        // resumes, which searches)
        if self.note.is_none() && !self.locked {
            self.synchronised = true;
            self.search();
        }
    }

    /// Whether the page searches as its search changes ("searching
    /// immediately"), or waits ("search paused").
    pub fn synchronised(&self) -> bool {
        self.synchronised
    }

    /// Search as the search changes, or wait (the search box's pause/play
    /// button, ctrl+i): searching again at once once on.
    pub fn set_synchronised(&mut self, synchronised: bool) {
        let resumed = synchronised && !self.synchronised;
        self.synchronised = synchronised;
        if resumed && self.note.is_none() && !self.locked {
            self.search();
        }
    }

    /// Move the selected thumbnails as `to` says (the thumbnail menu's
    /// rearrange, alt and home, end, left or right): until the page sorts
    /// again, they stay there.
    pub fn rearrange(&mut self, to: crate::thumbnail_menu::Rearrange) {
        let selected: std::collections::HashSet<HashId> =
            self.selected_items().into_iter().collect();
        self.results = crate::thumbnail_menu::rearranged(
            &self.results,
            &selected,
            self.selection.focused(),
            to,
        );
    }

    /// Select every file (ctrl+A).
    pub fn select_all(&mut self) {
        self.selection.select_all(&self.results);
        self.count_tags();
    }

    /// Select no file (escape).
    pub fn select_none(&mut self) {
        self.selection.select_none(&self.results);
        self.count_tags();
    }

    /// Move the focus (the arrows, page up and down, home and end; with
    /// shift, selecting as they go) in a grid `columns` wide showing
    /// `page_rows` rows; the index of the file moved to.
    pub fn move_focus(
        &mut self,
        to: Move,
        shift: bool,
        columns: usize,
        page_rows: usize,
    ) -> Option<usize> {
        let moved = self
            .selection
            .move_focus(&self.results, to, shift, columns, page_rows);
        self.count_tags();
        moved
    }

    /// The tag list's rows: the selected files' tags, or with nothing
    /// selected every file's, with how many have each (`tag (3) (+1)`).
    pub fn tag_rows(&self) -> Vec<&str> {
        self.tags.iter().map(|(_, row)| row.as_str()).collect()
    }

    /// The colour of each of [`Self::tag_rows`]: its tag's namespace's.
    pub fn tag_colours(
        &self,
        colours: &hydrus_core::tag_presentation::NamespaceColours,
    ) -> Vec<[u8; 3]> {
        self.tags.iter().map(|(tag, _)| colours.tag(tag)).collect()
    }

    /// The colour of each of [`Self::predicates`] (`Predicate.GetNamespace`'s).
    pub fn predicate_colours(
        &self,
        colours: &hydrus_core::tag_presentation::NamespaceColours,
    ) -> Vec<[u8; 3]> {
        self.predicates
            .iter()
            .map(|p| colours.predicate(p))
            .collect()
    }

    /// A tag in the list was activated: search for it too.
    pub fn activate_tag(&mut self, index: usize) -> bool {
        match self.tags.get(index) {
            Some((tag, _)) => {
                let tag = tag.clone();
                self.add_predicate(&tag)
            }
            None => false,
        }
    }

    /// Count the tags for the list, as the reference's selection tags box
    /// does: display tags in the page's tag domain, current, pending and
    /// petitioned, less those the user hides from it, sorted by its default
    /// sort.
    fn count_tags(&mut self) {
        let files = match self.selected_files() {
            selected if selected.is_empty() => self.files(),
            selected => selected,
        };
        let snapshot = self.store.snapshot();
        let service = snapshot
            .services
            .by_key(&self.context.tags.display_service)
            .ok()
            .filter(|s| s.service_type() != hydrus_core::ServiceType::CombinedTag)
            .map(|s| s.id);
        self.tags = tag_rows(&self.store, &files, service, TagList::Selection);
    }

    fn search(&mut self) {
        self.error = None;
        self.selection.clear();
        self.results.clear();
        self.came.clear();
        self.collections.clear();
        // as in the reference, a page with no predicates shows nothing
        if self.predicates.is_empty() {
            self.tags.clear();
            self.empty_status.set(Some("no search"));
            return;
        }
        self.empty_status
            .set(Some("no files found for this search"));
        let search = FileSearchContext {
            predicates: self.predicates.clone(),
            ..self.context.clone()
        };
        // (the database sorts by the page's sort if it is a system one, for
        // a system:limit to take the first by it; the page then sorts the
        // files as a page does)
        let sort = system_sort(&self.sort).unwrap_or(FileSort {
            by: SortBy::ImportTime,
            order: SortOrder::Descending,
        });
        let snapshot = self.store.snapshot();
        let clock = Clock::system();
        match self
            .store
            .read(|conn| Ok(search_files(conn, &snapshot, &search, sort, &clock)))
        {
            Ok(Ok(found)) => {
                self.came.clone_from(&found);
                self.results = found;
                self.resort();
            }
            Ok(Err(e)) => self.error = Some(e.to_string()),
            Err(e) => self.error = Some(e.to_string()),
        }
        self.learn_facts();
        self.count_tags();
    }

    /// Read the status bar's facts of the files shown not yet known.
    fn learn_facts(&mut self) {
        let unknown: Vec<HashId> = self
            .files()
            .into_iter()
            .filter(|f| !self.facts.contains_key(f))
            .collect();
        if unknown.is_empty() {
            return;
        }
        self.facts
            .extend(crate::status::facts(&self.store, &unknown));
    }

    /// The status bar's text, as the reference writes it: the files shown,
    /// and those selected (their inbox read afresh, as archiving changes
    /// it); for one file selected, its info lines.
    pub fn status(&self) -> String {
        let facts = |files: &[HashId]| -> Vec<Facts> {
            files
                .iter()
                .filter_map(|f| self.facts.get(f))
                .copied()
                .collect()
        };
        let all = facts(&self.files());
        if !all.is_empty() {
            self.empty_status.set(None);
        }
        let shown = crate::status::Items {
            items: self.results.len(),
            collections: self.collections.len(),
        };
        let selected_files = self.selected_files();
        let mut selected = facts(&selected_files);
        if let Ok(inbox) = self
            .store
            .read(|conn| hydrus_store::media::inboxed(conn, &selected_files))
        {
            for (fact, file) in selected.iter_mut().zip(&selected_files) {
                fact.inbox = inbox.contains(file);
            }
        }
        let single_line = match selected_files[..] {
            [file] => self.single_file_line(file),
            _ => None,
        };
        crate::status::status(
            (&all, shown),
            (&selected, self.selected_counts()),
            self.empty_status.get(),
            single_line.as_deref(),
        )
    }

    /// One file's interesting info lines, for the status bar, if the
    /// options show them.
    fn single_file_line(&self, file: HashId) -> Option<String> {
        let settings: hydrus_core::media_viewer::InfoLineSettings =
            self.store.read(hydrus_store::settings::get).ok()?;
        if !settings.single_file_in_status_bar {
            return None;
        }
        let snapshot = self.store.snapshot();
        let media = self
            .store
            .read(|conn| hydrus_store::media::load(conn, &snapshot.services, None, &[file]))
            .ok()?
            .results
            .pop()?;
        Some(crate::info_lines::status_line(
            &media,
            &snapshot.services,
            &settings,
            hydrus_core::TimestampMs::now().0,
        ))
    }

    /// A file's thumbnail, decoded; `None` if it has none on disk.
    pub fn thumbnail(&self, id: HashId) -> Option<hydrus_media::Raster> {
        crate::thumbnails::thumbnail(&self.store, id)
    }
}

/// Which tag list: the search page's ("selection tags", with counts) or
/// the media viewer's hover frame (one file's, without).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagList {
    Selection,
    MediaViewer,
}

/// A tag list's rows for `files`, as `ListBoxTagsMedia` lists them: display
/// tags in `service`'s domain (all known tags with none), less those the
/// list's tag display filters hide, sorted by its tag sort, each as (tag,
/// row). The selection list counts them (`tag (3) (+1)`); the media
/// viewer's marks pending and petitioned ones (`tag (+)`).
pub(crate) fn tag_rows(
    store: &Store,
    files: &[HashId],
    service: Option<hydrus_core::ServiceId>,
    list: TagList,
) -> Vec<(String, String)> {
    use hydrus_core::tag_sort::sort_tags;
    let snapshot = store.snapshot();
    let counted = store.read(|conn| {
        use hydrus_core::tag_presentation::TagPresentation;
        use hydrus_store::tag_display::{TagDisplayFilters, TagView};
        let presentation: TagPresentation = hydrus_store::settings::get(conn)?;
        let filters: TagDisplayFilters = hydrus_store::settings::get(conn)?;
        let view = match list {
            TagList::Selection => TagView::SelectionList,
            TagList::MediaViewer => TagView::SingleMedia,
        };
        let hidden = filters.by_service(view, &snapshot.services);
        let counts = hydrus_store::media::tag_counts(
            conn,
            &snapshot.services,
            &snapshot.display,
            service,
            files,
            &hidden,
        )?;
        let ids: Vec<_> = counts
            .current
            .keys()
            .chain(counts.pending.keys())
            .chain(counts.petitioned.keys())
            .copied()
            .collect();
        Ok((
            counts,
            hydrus_store::master::tags(conn, &ids)?,
            presentation,
        ))
    });
    let Ok((counts, names, presentation)) = counted else {
        return Vec::new();
    };
    let mut rows: Vec<(String, [u64; 3])> = names
        .iter()
        .map(|(id, tag)| {
            let n = |m: &std::collections::HashMap<_, u64>| m.get(id).copied().unwrap_or(0);
            (
                tag.as_str().to_owned(),
                [
                    n(&counts.current),
                    n(&counts.pending),
                    n(&counts.petitioned),
                ],
            )
        })
        .collect();
    let sort = match list {
        TagList::Selection => &presentation.search_page_sort,
        TagList::MediaViewer => &presentation.media_viewer_sort,
    };
    sort_tags(
        sort,
        &mut rows,
        |(tag, _)| tag,
        |(_, n)| n.iter().sum(),
        &presentation.user_namespaces,
    );
    rows.into_iter()
        .map(|(tag, [current, pending, petitioned])| {
            let mut row = presentation.render(&tag);
            for (n, prefix) in [(current, ""), (pending, "+"), (petitioned, "-")] {
                if n == 0 {
                    continue;
                }
                match list {
                    TagList::Selection => {
                        row.push_str(&format!(
                            " ({prefix}{})",
                            hydrus_core::numbers::human_int(n)
                        ));
                    }
                    // (`ListBoxItemTextTagWithCounts` without counts)
                    TagList::MediaViewer if !prefix.is_empty() => {
                        row.push_str(&format!(" ({prefix})"));
                    }
                    TagList::MediaViewer => {}
                }
            }
            (tag, row)
        })
        .collect()
}

/// A stored sort as the system sort it is, if it is one we know.
fn system_sort(sort: &PageSort) -> Option<FileSort> {
    match sort.by {
        PageSortBy::System(code) => Some(FileSort {
            by: SortBy::from_code(code)?,
            order: if sort.ascending {
                SortOrder::Ascending
            } else {
                SortOrder::Descending
            },
        }),
        _ => None,
    }
}

/// Seconds since the epoch.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// A new gallery page's own state, as the reference makes one: the
/// client's default downloader and file limit, nothing paused or merged.
pub fn new_gallery_state(store: &Store) -> hydrus_core::pages::DownloaderPageState {
    let defaults: hydrus_core::subscriptions::GalleryDefaults =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let (gug_key, gug_name) = defaults.gug.unwrap_or_default();
    hydrus_core::pages::DownloaderPageState {
        highlighted: None,
        options: hydrus_core::import_options::ImportOptionsSlice::default(),
        gallery: Some(hydrus_core::pages::GalleryPageState {
            gug_key,
            gug_name,
            file_limit: defaults.file_limit,
            start_files_paused: false,
            start_gallery_paused: false,
            no_new_dupes: false,
            merge_pends: false,
        }),
        checker: None,
    }
}

/// A new watcher page's own state, as the reference makes one: the
/// client's default checker options for watchers.
pub fn new_watcher_state(store: &Store) -> hydrus_core::pages::DownloaderPageState {
    let defaults: hydrus_core::subscriptions::CheckerDefaults =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    hydrus_core::pages::DownloaderPageState {
        checker: Some(defaults.watchers),
        ..hydrus_core::pages::DownloaderPageState::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_asks_while_importing_and_when_full_if_the_option_says() {
        let mut importer = Importer {
            queue: 1,
            files: [(queues::SeedStatus::SuccessfulAndNew, 3)]
                .into_iter()
                .collect(),
            ..Importer::default()
        };
        assert_eq!(
            importer.close_veto(true).as_deref(),
            Some("This is a urls import page holding 3 import objects.")
        );
        assert_eq!(
            importer.close_veto(false),
            None,
            "full, but not to be asked"
        );
        importer.files.insert(queues::SeedStatus::Unknown, 1);
        assert_eq!(
            importer.close_veto(false).as_deref(),
            Some("This page is still importing."),
            "importing asks regardless"
        );
        importer.paused = true;
        assert_eq!(importer.close_veto(false), None);
    }
}
