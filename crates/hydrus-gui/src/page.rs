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

use crate::autocomplete::Autocomplete;
use crate::selection::{Move, Selection};
use crate::status::Facts;

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
            PageContent::Downloader { kind, queues, .. } => PageContent::Downloader {
                kind: *kind,
                queues: queues.clone(),
                sort,
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
