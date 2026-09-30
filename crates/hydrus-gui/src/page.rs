//! A search page: its predicates, the files they find, and which file is
//! selected. Plain Rust, driven by the window and by tests alike.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_search::{
    Clock, FileSearchContext, FileSort, Predicate, SortBy, SortOrder, TextContext,
    parse_api_search, predicate_text, search_files,
};
use hydrus_store::Store;

use crate::autocomplete::Autocomplete;

pub struct SearchPage {
    store: Arc<Store>,
    autocomplete: Autocomplete,
    predicates: Vec<Predicate>,
    sort: FileSort,
    /// In the sort's order.
    results: Vec<HashId>,
    selected: Option<usize>,
    error: Option<String>,
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
            predicates: Vec::new(),
            // the reference's default: newest import first
            sort: FileSort {
                by: SortBy::ImportTime,
                order: SortOrder::Descending,
            },
            results: Vec::new(),
            selected: None,
            error: None,
        }
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
        let context = TextContext::from_store(&snapshot.services, &viewing);
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
        self.search();
    }

    pub fn set_sort_order(&mut self, order: SortOrder) {
        self.sort.order = order;
        self.search();
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
        self.search();
        true
    }

    pub fn remove_predicate(&mut self, index: usize) {
        if index < self.predicates.len() {
            self.predicates.remove(index);
            self.search();
        }
    }

    pub fn select(&mut self, index: usize) {
        self.selected = (index < self.results.len()).then_some(index);
    }

    fn search(&mut self) {
        self.error = None;
        self.selected = None;
        self.results.clear();
        // as in the reference, a page with no predicates shows nothing
        if self.predicates.is_empty() {
            return;
        }
        let search = FileSearchContext {
            predicates: self.predicates.clone(),
            ..FileSearchContext::default()
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
        let hash = self
            .store
            .read(|conn| hydrus_store::master::hash(conn, id))
            .ok()??;
        let path = self.store.snapshot().storage.thumbnail_path(&hash)?;
        let bytes = std::fs::read(path).ok()?;
        hydrus_media::decode_image(&bytes).ok()
    }
}
