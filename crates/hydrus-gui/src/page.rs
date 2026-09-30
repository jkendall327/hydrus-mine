//! A search page: its predicates, the files they find, and which file is
//! selected. Plain Rust, driven by the window and by tests alike.

use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_search::{
    Clock, FileSearchContext, FileSort, SortBy, SortOrder, parse_api_search, search_files,
};
use hydrus_store::Store;

use crate::autocomplete::Autocomplete;

pub struct SearchPage {
    store: Arc<Store>,
    autocomplete: Autocomplete,
    /// As typed: tags, and system predicates such as `system:inbox`.
    predicates: Vec<String>,
    /// Newest import first.
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
        Self {
            autocomplete: Autocomplete::new(store.clone()),
            store,
            predicates: Vec::new(),
            results: Vec::new(),
            selected: None,
            error: None,
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn predicates(&self) -> &[String] {
        &self.predicates
    }

    pub fn results(&self) -> &[HashId] {
        &self.results
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

    /// Add a predicate as typed and search again; whether it was taken. One
    /// that doesn't parse is refused with the reason; one already there is
    /// not added twice.
    pub fn add_predicate(&mut self, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        if let Err(e) = parse_api_search(&serde_json::json!([text])) {
            self.error = Some(e.to_string());
            return false;
        }
        if !self.predicates.iter().any(|p| p == text) {
            self.predicates.push(text.to_owned());
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
        let predicates = match parse_api_search(&serde_json::json!(self.predicates)) {
            Ok(predicates) => predicates,
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        };
        let search = FileSearchContext {
            predicates,
            ..FileSearchContext::default()
        };
        let sort = FileSort {
            by: SortBy::ImportTime,
            order: SortOrder::Descending,
        };
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
