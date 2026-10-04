//! The reference regex favourites list: sorted pairs, exact duplicate guard,
//! advisory validity, and edits isolated until the owner's Apply.
use crate::list_selection::ListSelection;
use hydrus_core::url::strings::PyRegex;
pub use hydrus_store::regex_favourites::{RegexFavourites, load};

/// A sorted favourites draft with stable selection across edits.
#[derive(Debug, Clone)]
pub struct Editor {
    rows: Vec<(usize, (String, String))>,
    selection: ListSelection<usize>,
    next_id: usize,
}

impl Editor {
    pub fn new(value: &RegexFavourites) -> Self {
        let mut editor = Self {
            rows: value.0.iter().cloned().enumerate().collect(),
            selection: ListSelection::default(),
            next_id: value.0.len(),
        };
        editor.sort();
        editor
    }
    fn sort(&mut self) {
        self.rows.sort_by(|a, b| a.1.cmp(&b.1));
    }
    fn order(&self) -> Vec<usize> {
        self.rows.iter().map(|row| row.0).collect()
    }
    /// The sorted phrase/description rows shown in the list.
    pub fn rows(&self) -> Vec<(String, String)> {
        self.rows.iter().map(|row| row.1.clone()).collect()
    }
    /// Selected indices in display order.
    pub fn selected(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| self.selection.is_selected(row.0).then_some(i))
            .collect()
    }
    /// Extended Qt selection: click, ctrl and shift.
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(&self.order(), index, ctrl, shift);
    }
    /// The single selected pair for editing; multiple selections do not edit.
    pub fn editing(&self) -> Option<(usize, (String, String))> {
        let id = self.selection.one()?;
        self.rows.iter().find(|row| row.0 == id).cloned()
    }
    /// Add a pair unless that exact phrase and description already exist.
    pub fn add(&mut self, phrase: String, description: String) -> Result<(), String> {
        let row = (phrase, description);
        if self.rows.iter().any(|item| item.1 == row) {
            return Err("That regex and description are already in the list!".into());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.rows.push((id, row));
        self.sort();
        let index = self.rows.iter().position(|row| row.0 == id).unwrap_or(0);
        self.click(index, false, false);
        Ok(())
    }
    /// Replace the selected pair; edit preserves the reference's permissive
    /// behavior, including invalid expressions and duplicate descriptions.
    pub fn replace(&mut self, id: usize, phrase: String, description: String) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.0 == id) {
            row.1 = (phrase, description);
            self.sort();
        }
    }
    /// Delete selected pairs after the owner's confirmation.
    pub fn delete(&mut self) {
        self.rows.retain(|row| !self.selection.is_selected(row.0));
        self.selection.select_only(None);
    }
    /// The value committed only when this editor's owner accepts it.
    pub fn value(&self) -> RegexFavourites {
        RegexFavourites(self.rows())
    }
}

/// Validity is an advisory colour/message; the reference allows saving invalid
/// regex favourites for fragments or expressions intended for later editing.
pub fn validity(phrase: &str) -> Result<(), String> {
    PyRegex::new(phrase)
        .regex()
        .map(|_| ())
        .map_err(str::to_owned)
}
