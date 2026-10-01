//! A page of files: a search page (its predicates and the files they find),
//! or a page that shows files without a search, and which file is selected.
//! Plain Rust, driven by the window and by tests alike.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_core::pages::{DuplicatesPage, PageContent, PageSort, PageSortBy};
use hydrus_search::{
    Clock, FileSearchContext, FileSort, Predicate, SortBy, SortOrder, TextContext,
    parse_api_search, predicate_text, search_files, sort_files,
};
use hydrus_store::Store;

use crate::autocomplete::Autocomplete;

pub struct SearchPage {
    store: Arc<Store>,
    autocomplete: Autocomplete,
    /// The page's file and tag domains (its predicates are `predicates`).
    context: FileSearchContext,
    predicates: Vec<Predicate>,
    /// Whether the page searches as its predicates change.
    synchronised: bool,
    /// Why the page shows files without a search, if it does.
    note: Option<String>,
    sort: FileSort,
    /// Whether the sort was changed since the page was opened (a sort we
    /// can't use yet is kept until then).
    sort_changed: bool,
    /// In the sort's order.
    results: Vec<HashId>,
    selected: Option<usize>,
    /// The selection's tags (or, with nothing selected, the page's): each
    /// tag, and its row as the list shows it.
    tags: Vec<(String, String)>,
    error: Option<String>,
    /// A duplicates page's filtering, which the page can launch.
    duplicates: Option<DuplicatesPage>,
}

impl std::fmt::Debug for SearchPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchPage")
            .field("predicates", &self.predicates)
            .field("results", &self.results.len())
            .field("selected", &self.selected)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl SearchPage {
    pub fn new(store: Arc<Store>) -> Self {
        let mut autocomplete = Autocomplete::new(store.clone());
        autocomplete.clear();
        Self {
            autocomplete,
            store,
            context: FileSearchContext::default(),
            predicates: Vec::new(),
            synchronised: true,
            note: None,
            // the reference's default: newest import first
            sort: FileSort {
                by: SortBy::ImportTime,
                order: SortOrder::Descending,
            },
            sort_changed: false,
            results: Vec::new(),
            selected: None,
            tags: Vec::new(),
            error: None,
            duplicates: None,
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
        page.results = files;
        page.count_tags();
        page
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
        page.set_page_sort(sort);
        page.results = files;
        page.count_tags();
        page
    }

    /// A session's sort, if it is one we have; otherwise the default stays.
    fn set_page_sort(&mut self, sort: Option<&PageSort>) {
        if let Some(PageSort {
            by: PageSortBy::System(code),
            ascending,
        }) = sort
            && let Some(by) = SortBy::from_code(*code)
        {
            self.sort = FileSort {
                by,
                order: if *ascending {
                    SortOrder::Ascending
                } else {
                    SortOrder::Descending
                },
            };
        }
    }

    /// The page as a session keeps it, given what it was opened from.
    pub fn content(&self, opened_from: &PageContent) -> PageContent {
        let sort = if self.sort_changed {
            Some(PageSort {
                by: PageSortBy::System(i64::from(self.sort.by.code())),
                ascending: self.sort.order == SortOrder::Ascending,
            })
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

    pub fn results(&self) -> &[HashId] {
        &self.results
    }

    pub fn sort(&self) -> FileSort {
        self.sort
    }

    /// Sort by `by`, in its default order (as the reference's sort control
    /// does when the type changes).
    pub fn set_sort_by(&mut self, by: SortBy) {
        let order = crate::sort::choices()
            .into_iter()
            .find(|c| c.by == by)
            .map_or(SortOrder::Ascending, |c| c.default_order);
        self.sort = FileSort { by, order };
        self.sort_changed = true;
        self.resort();
    }

    pub fn set_sort_order(&mut self, order: SortOrder) {
        self.sort.order = order;
        self.sort_changed = true;
        self.resort();
    }

    /// Sort the files shown again (a new sort doesn't search again).
    fn resort(&mut self) {
        let selected = self.selected.map(|i| self.results[i]);
        let snapshot = self.store.snapshot();
        let clock = Clock::system();
        match self.store.read(|conn| {
            Ok(sort_files(
                conn,
                &snapshot,
                &self.context,
                &self.results,
                self.sort,
                &clock,
            ))
        }) {
            Ok(Ok(sorted)) => self.results = sorted,
            Ok(Err(e)) => self.error = Some(e.to_string()),
            Err(e) => self.error = Some(e.to_string()),
        }
        self.selected = selected.and_then(|id| self.results.iter().position(|&r| r == id));
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
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
        if index < self.predicates.len() {
            self.predicates.remove(index);
            if self.synchronised {
                self.search();
            }
        }
    }

    pub fn select(&mut self, index: usize) {
        self.selected = (index < self.results.len()).then_some(index);
        self.count_tags();
    }

    /// The tag list's rows: the selected file's tags, or with nothing
    /// selected every file's, with how many have each (`tag (3) (+1)`).
    pub fn tag_rows(&self) -> Vec<&str> {
        self.tags.iter().map(|(_, row)| row.as_str()).collect()
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
        use hydrus_core::tag_sort::sort_tags;
        let files: Vec<HashId> = match self.selected {
            Some(i) => vec![self.results[i]],
            None => self.results.clone(),
        };
        let snapshot = self.store.snapshot();
        let service = snapshot
            .services
            .by_key(&self.context.tags.display_service)
            .ok()
            .filter(|s| s.service_type() != hydrus_core::ServiceType::CombinedTag)
            .map(|s| s.id);
        let counted = self.store.read(|conn| {
            use hydrus_core::tag_presentation::TagPresentation;
            use hydrus_store::tag_display::{TagDisplayFilters, TagView};
            let presentation: TagPresentation = hydrus_store::settings::get(conn)?;
            let filters: TagDisplayFilters = hydrus_store::settings::get(conn)?;
            let hidden = filters.by_service(TagView::SelectionList, &snapshot.services);
            let counts = hydrus_store::media::tag_counts(
                conn,
                &snapshot.services,
                &snapshot.display,
                service,
                &files,
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
            self.tags.clear();
            return;
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
        sort_tags(
            &presentation.search_page_sort,
            &mut rows,
            |(tag, _)| tag,
            |(_, n)| n.iter().sum(),
            &presentation.user_namespaces,
        );
        self.tags = rows
            .into_iter()
            .map(|(tag, [current, pending, petitioned])| {
                let mut row = presentation.render(&tag);
                for (n, prefix) in [(current, ""), (pending, "+"), (petitioned, "-")] {
                    if n > 0 {
                        row.push_str(&format!(
                            " ({prefix}{})",
                            hydrus_core::numbers::human_int(n)
                        ));
                    }
                }
                (tag, row)
            })
            .collect();
    }

    fn search(&mut self) {
        self.error = None;
        self.selected = None;
        self.results.clear();
        // as in the reference, a page with no predicates shows nothing
        if self.predicates.is_empty() {
            self.tags.clear();
            return;
        }
        let search = FileSearchContext {
            predicates: self.predicates.clone(),
            ..self.context.clone()
        };
        let sort = self.sort;
        let snapshot = self.store.snapshot();
        let clock = Clock::system();
        match self
            .store
            .read(|conn| Ok(search_files(conn, &snapshot, &search, sort, &clock)))
        {
            Ok(Ok(found)) => self.results = found,
            Ok(Err(e)) => self.error = Some(e.to_string()),
            Err(e) => self.error = Some(e.to_string()),
        }
        self.count_tags();
    }

    /// The status bar's text.
    pub fn status(&self) -> String {
        match self.results.len() {
            0 => "no files".to_owned(),
            1 => "1 file".to_owned(),
            n => format!("{n} files"),
        }
    }

    /// A file's thumbnail, decoded; `None` if it has none on disk.
    pub fn thumbnail(&self, id: HashId) -> Option<hydrus_media::Raster> {
        crate::thumbnails::thumbnail(&self.store, id)
    }
}
