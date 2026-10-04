//! Staged filename extraction lists, with owned quick-namespace questions.
use crate::{filename_tagging::regex_error, list_selection::ListSelection};
use hydrus_parse::folders::FilenameTagging;

/// Instructions shown by the reference's quick namespace child.
pub const INTRO: &str = "Put the namespace (e.g. page) on the left.\nPut the regex (e.g. [1-9]+\\d*(?=.{4}$)) on the right.\nAll files will be tagged with \"namespace:regex\".";
/// The list's deletion question.
pub const DELETE_QUESTION: &str = "Remove all selected?";

/// One extraction row, with an identity that survives sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<T> {
    pub id: u64,
    pub value: T,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pending {
    Quick(Option<u64>),
    Delete(Vec<u64>),
}

/// The advanced lists for one tag service; all changes belong to its owner draft.
#[derive(Debug, Default)]
pub struct Editor {
    quick: Vec<Row<(String, String)>>,
    regexes: Vec<Row<String>>,
    quick_selection: ListSelection<u64>,
    regex_selection: ListSelection<u64>,
    next: u64,
    pending: Option<Pending>,
    /// Regex returned to the input when selected expressions are removed.
    pub input: String,
}
impl Editor {
    /// Open the saved extraction rules, sorting quick namespaces as Qt does.
    pub fn new(options: &FilenameTagging) -> Self {
        let mut editor = Self::default();
        for value in &options.quick_namespaces {
            let id = editor.next_id();
            editor.quick.push(Row {
                id,
                value: value.clone(),
                selected: false,
            });
        }
        for value in &options.regexes {
            let id = editor.next_id();
            editor.regexes.push(Row {
                id,
                value: value.clone(),
                selected: false,
            });
        }
        editor.sort_quick();
        editor
    }
    fn next_id(&mut self) -> u64 {
        let id = self.next;
        self.next = self.next.saturating_add(1);
        id
    }
    fn sort_quick(&mut self) {
        self.quick.sort_by_cached_key(|row| {
            (
                hydrus_core::casefold::casefold(&row.value.0),
                hydrus_core::casefold::casefold(&row.value.1),
            )
        });
    }
    /// Quick rules in displayed order, including their current selection.
    pub fn quick_rows(&self) -> Vec<Row<(String, String)>> {
        self.quick
            .iter()
            .map(|row| Row {
                selected: self.quick_selection.is_selected(row.id),
                ..row.clone()
            })
            .collect()
    }
    /// Regex rules preserve their insertion order and allow duplicates.
    pub fn regex_rows(&self) -> Vec<Row<String>> {
        self.regexes
            .iter()
            .map(|row| Row {
                selected: self.regex_selection.is_selected(row.id),
                ..row.clone()
            })
            .collect()
    }
    /// Join the accepted rules to the owning filename-tagging options.
    pub fn update(&self, options: &mut FilenameTagging) {
        options.quick_namespaces = self.quick.iter().map(|row| row.value.clone()).collect();
        options.regexes = self.regexes.iter().map(|row| row.value.clone()).collect();
    }
    /// Whether a child or confirmation owns the editor's input.
    pub fn asking(&self) -> bool {
        self.pending.is_some()
    }
    /// A real extended list click; pending children freeze list selection.
    pub fn click(&mut self, quick: bool, index: usize, ctrl: bool, shift: bool) {
        if self.asking() {
            return;
        }
        if quick {
            self.quick_selection.click(
                &self.quick.iter().map(|row| row.id).collect::<Vec<_>>(),
                index,
                ctrl,
                shift,
            );
        } else {
            self.regex_selection.click(
                &self.regexes.iter().map(|row| row.id).collect::<Vec<_>>(),
                index,
                false,
                false,
            );
        }
    }
    /// Start Add or Edit, freezing the first selected rule's identity.
    pub fn begin(&mut self, edit: bool) -> Option<(String, String)> {
        if self.asking() {
            return None;
        }
        let target = if edit {
            Some(
                self.quick_selection
                    .in_order(&self.quick.iter().map(|row| row.id).collect::<Vec<_>>())
                    .first()
                    .copied()?,
            )
        } else {
            None
        };
        let value = target
            .and_then(|id| self.quick.iter().find(|row| row.id == id))
            .map(|row| row.value.clone())
            .unwrap_or_default();
        self.pending = Some(Pending::Quick(target));
        Some(value)
    }
    /// Validate and accept the child, retaining literal namespace/regex text.
    /// Invalid input leaves both the child and accepted list unchanged.
    pub fn accept(&mut self, namespace: &str, regex: &str) -> Result<(), String> {
        let Some(Pending::Quick(target)) = self.pending.clone() else {
            return Ok(());
        };
        if namespace.is_empty() {
            return Err("Please enter something for the namespace.".into());
        }
        if let Some(error) = regex_error(regex) {
            return Err(error);
        }
        let value = (namespace.to_owned(), regex.to_owned());
        let id = if let Some(id) = target {
            if let Some(row) = self.quick.iter_mut().find(|row| row.id == id) {
                row.value = value;
            }
            id
        } else if let Some(row) = self.quick.iter().find(|row| row.value == value) {
            row.id
        } else {
            let id = self.next_id();
            self.quick.push(Row {
                id,
                value,
                selected: false,
            });
            id
        };
        self.pending = None;
        if target.is_none() {
            self.quick_selection.select_only(Some(id));
        }
        self.sort_quick();
        Ok(())
    }
    /// Cancel only the unfinished quick rule or deletion confirmation.
    pub fn cancel(&mut self) {
        self.pending = None;
    }
    /// Freeze the selected quick rules before asking their deletion question.
    pub fn request_delete(&mut self) -> bool {
        if self.asking() {
            return false;
        }
        let ids = self
            .quick_selection
            .in_order(&self.quick.iter().map(|row| row.id).collect::<Vec<_>>());
        if ids.is_empty() {
            return false;
        }
        self.pending = Some(Pending::Delete(ids));
        true
    }
    /// Delete only the frozen rows on Yes, retaining everything on No.
    pub fn answer(&mut self, yes: bool) {
        if !matches!(self.pending, Some(Pending::Delete(_))) {
            return;
        }
        let Some(Pending::Delete(ids)) = self.pending.take() else {
            return;
        };
        if yes {
            self.quick.retain(|row| !ids.contains(&row.id));
            for id in ids {
                self.quick_selection.forget(id);
            }
        }
    }
    /// Enter adds a compiled nonempty regex, clears the input, and allows duplicates.
    pub fn add_regex(&mut self) -> Result<(), String> {
        if self.asking() || self.input.is_empty() {
            return Ok(());
        }
        if let Some(error) = regex_error(&self.input) {
            return Err(error);
        }
        let id = self.next_id();
        let value = std::mem::take(&mut self.input);
        self.regexes.push(Row {
            id,
            value,
            selected: false,
        });
        Ok(())
    }
    /// Double-click removes all selected regexes and returns the first to the input.
    pub fn remove_regexes(&mut self) {
        if self.asking() {
            return;
        }
        let ids = self
            .regex_selection
            .in_order(&self.regexes.iter().map(|row| row.id).collect::<Vec<_>>());
        if let Some(value) = self
            .regexes
            .iter()
            .find(|row| ids.contains(&row.id))
            .map(|row| row.value.clone())
        {
            self.input = value;
        }
        self.regexes.retain(|row| !ids.contains(&row.id));
        for id in ids {
            self.regex_selection.forget(id);
        }
    }
}
