//! The search of the auto-resolution "test A or B using search terms"
//! comparator, as the reference's metadata conditional editor
//! (`AutoCompleteDropdownMetadataConditional`) has it: a read autocomplete
//! listing only what can be tested on one file, the predicates it builds,
//! and its explicit OR construction, with paste and empty-search buttons.
//!
//! Choosing a result goes through the OR being built (`_BroadcastChoices`):
//! shift adds to it, a plain choice commits it; the predicates entered toggle
//! (one already there is removed) and replace mutually exclusive ones
//! ([`hydrus_search::enter_predicates`]). Return and a click both choose; a
//! system predicate that needs a value asks for its editor instead.

use std::sync::Arc;

use hydrus_search::{Predicate, TextContext, parse_api_search, predicate_text};
use hydrus_store::Store;

use crate::autocomplete::{Autocomplete, Navigation, Suggestion};
use crate::predicate_editors::Blank;
use crate::search_or::Construction;

/// What choosing a result did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chosen {
    /// Nothing was listed there, so nothing happened.
    Nothing,
    /// The predicates were entered (or added to the OR being built).
    Entered,
    /// The system predicate needs a value: open its editor, and give what
    /// it makes to [`ConditionalSearch::apply_editor`] with this shift.
    Editor(Blank, bool),
}

pub struct ConditionalSearch {
    autocomplete: Autocomplete,
    predicates: Vec<Predicate>,
    or: Construction,
    text: TextContext,
}

impl std::fmt::Debug for ConditionalSearch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConditionalSearch")
            .field("predicates", &self.predicates)
            .field("or", &self.or)
            .finish_non_exhaustive()
    }
}

impl ConditionalSearch {
    /// The search of a comparator holding `predicates`.
    pub fn new(store: Arc<Store>, predicates: &[Predicate], text: TextContext) -> Self {
        let mut search = Self {
            autocomplete: Autocomplete::new(store).metadata_conditional(),
            predicates: Vec::new(),
            or: Construction::default(),
            text,
        };
        hydrus_search::enter_predicates(&mut search.predicates, predicates, &search.text);
        search
    }

    /// The predicates, as the comparator keeps them: those that can be
    /// tested on one file (`MetadataConditional.SetFileSearchContext`).
    pub fn predicates(&self) -> Vec<Predicate> {
        self.predicates
            .iter()
            .filter(|p| hydrus_search::media::can_test(p))
            .cloned()
            .collect()
    }

    /// The predicates as the list draws them, in its order.
    pub fn shown(&self) -> Vec<String> {
        self.predicates
            .iter()
            .map(|p| predicate_text(p, &self.text))
            .collect()
    }

    pub fn typed(&self) -> &str {
        self.autocomplete.text()
    }

    /// The box's text changed: list again.
    pub fn set_typed(&mut self, text: &str) {
        if self.autocomplete.text() != text {
            self.autocomplete.set_text(text);
        }
    }

    pub fn suggestions(&self) -> &[Suggestion] {
        self.autocomplete.suggestions()
    }

    pub fn highlighted(&self) -> Option<usize> {
        self.autocomplete.highlighted()
    }

    pub fn navigate(&mut self, key: Navigation) {
        self.autocomplete.navigate(key);
    }

    /// The OR being built, its terms.
    pub fn or_terms(&self) -> Option<&[Predicate]> {
        self.or.terms()
    }

    /// Choose a result (`index`, or the highlighted one: Return), shift
    /// adding it to the OR being built.
    pub fn choose(&mut self, index: Option<usize>, shift: bool) -> Chosen {
        let resolved = index.or_else(|| self.autocomplete.highlighted());
        let Some(suggestion) = resolved.and_then(|i| self.suggestions().get(i)).cloned() else {
            return Chosen::Nothing;
        };
        // (the first result is the OR being built, which enters itself)
        if resolved == Some(0)
            && let Some(own) = self.or.predicate()
            && suggestion.predicate.is_empty()
        {
            self.enter(vec![own], shift);
            return Chosen::Entered;
        }
        if let Some(blank) = suggestion.editor {
            return Chosen::Editor(blank, shift);
        }
        match parse_api_search(&serde_json::json!([suggestion.predicate])) {
            Ok(parsed) if !parsed.is_empty() => {
                self.enter(parsed, shift);
                Chosen::Entered
            }
            _ => Chosen::Nothing,
        }
    }

    /// What a system predicate's editor made, entered as its choice was.
    pub fn apply_editor(&mut self, predicates: Vec<Predicate>, shift: bool) {
        if !predicates.is_empty() {
            self.enter(predicates, shift);
        }
    }

    /// A double-click on the list's row `index`: it is removed.
    pub fn activate_row(&mut self, index: usize) {
        if let Some(p) = self.predicates.get(index).cloned() {
            hydrus_search::enter_predicates(&mut self.predicates, &[p], &self.text);
        }
    }

    /// The paste button: each line that is a predicate is chosen, together;
    /// the others are passed over. How many were.
    pub fn paste(&mut self, text: &str) -> usize {
        let predicates: Vec<Predicate> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| parse_api_search(&serde_json::json!([line])).ok())
            .flatten()
            .collect();
        let pasted = predicates.len();
        if pasted > 0 {
            self.enter(predicates, false);
        }
        pasted
    }

    /// The empty-search button: no predicates, and nothing typed.
    pub fn empty(&mut self) {
        self.predicates.clear();
        self.autocomplete.clear();
    }

    /// The OR (or advanced) editor's predicates, entered as a plain choice.
    pub fn enter_or_editor(&mut self, predicates: Vec<Predicate>) {
        if !predicates.is_empty() {
            self.enter(predicates, false);
        }
    }

    /// The "↶" button: the OR being built loses its last term.
    pub fn rewind(&mut self) {
        self.or.rewind();
        self.or_changed();
        self.autocomplete.clear();
    }

    /// The "cancel OR" button.
    pub fn cancel_or(&mut self) {
        self.or.cancel();
        self.or_changed();
        self.autocomplete.clear();
    }

    /// Escape in the box: with nothing typed, the OR being built rewinds (or
    /// is cancelled, at one term); with text, it is cleared. Whether Escape
    /// was used.
    pub fn escape(&mut self) -> bool {
        if !self.typed().is_empty() {
            self.autocomplete.clear();
            return true;
        }
        match self.or.terms().map(<[Predicate]>::len) {
            Some(1) => self.cancel_or(),
            Some(_) => self.rewind(),
            None => return false,
        }
        true
    }

    fn or_changed(&mut self) {
        let label = self.or.predicate().map(|p| predicate_text(&p, &self.text));
        self.autocomplete.set_or_draft(label);
    }

    fn enter(&mut self, predicates: Vec<Predicate>, shift: bool) {
        let plain = TextContext {
            presentation: None,
            ..self.text.clone()
        };
        let committed = self.or.broadcast(predicates, shift, &plain);
        self.or_changed();
        if !committed.is_empty() {
            hydrus_search::enter_predicates(&mut self.predicates, &committed, &self.text);
        }
        self.autocomplete.clear();
    }
}
