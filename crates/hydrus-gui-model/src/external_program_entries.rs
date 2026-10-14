//! The list of external program calls an import options editor makes, as the
//! reference's `EditExternalProgramsImportOptionsPanel` has it: each entry a
//! registered "send single file" call and the imports it is made for. Plain
//! Rust, tested against `oracle/fixtures/import_external_programs.json`.

use hydrus_core::external_calls::{Manager, Pipeline};
use hydrus_core::import_options::{ExternalProgramEntry, ExternalProgramsOptions};

/// The entry editor's "when to do it" choices: their words, and whether they
/// are for new files and for files already in the client.
pub const WHENS: [(&str, bool, bool); 4] = [
    ("all file imports", true, true),
    ("all new file imports", true, false),
    ("all \"already in db\" imports", false, true),
    ("no imports (will do nothing!)", false, false),
];

/// The title and text of the notice that there are no calls to choose from.
pub const NO_CALLS_TITLE: &str = "No external calls!";

pub fn no_calls_message() -> String {
    format!(
        "Hey, there do not seem to be any \"{}\" external program calls set! If you are in the options dialog and only just added some, please apply and re-open.",
        Pipeline::File.label()
    )
}

/// What an entry says when its call is not among the registered ones.
pub const NOT_IN_LIST: &str = "Hey, just so you know, I do not see the entry's external call in the currently list of available calls. If you are fixing that now, great.";

pub const EDIT_TITLE: &str = "Edit external program call";

/// `Remove N selected?`.
pub fn delete_question(count: usize) -> String {
    format!(
        "Remove {} selected?",
        hydrus_core::numbers::human_int(count as u64)
    )
}

/// A registered call an entry can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub key: String,
    pub name: String,
}

/// The registered "send single file" calls, by name
/// (`GetIdsAndNamesOfType`).
pub fn choices(manager: &Manager) -> Vec<Choice> {
    let mut choices: Vec<Choice> = manager
        .calls
        .iter()
        .filter(|call| call.pipeline == Pipeline::File)
        .map(|call| Choice {
            key: call.key_hex(),
            name: call.name.clone(),
        })
        .collect();
    choices.sort_by(|a, b| a.name.cmp(&b.name));
    choices
}

/// An entry about to be edited: the dialog's choices, which are chosen, and
/// a notice to give first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editing {
    pub program_index: usize,
    pub when_index: usize,
    /// Show this (information) before the dialog.
    pub notice: Option<&'static str>,
}

/// `_AddEntry`: a new entry, its call the first there is.
pub fn new_entry(choices: &[Choice]) -> ExternalProgramEntry {
    let mut entry = ExternalProgramEntry::default();
    if let Some(first) = choices.first() {
        entry.key.clone_from(&first.key);
        entry.name.clone_from(&first.name);
    }
    entry
}

/// `_EditEntry`'s start: no dialog when there are no calls (`None`, the
/// notice being [`NO_CALLS_TITLE`] and [`no_calls_message`]).
pub fn start_editing(entry: &ExternalProgramEntry, choices: &[Choice]) -> Option<Editing> {
    if choices.is_empty() {
        return None;
    }
    let found = choices.iter().position(|c| c.key == entry.key);
    let when_index = WHENS
        .iter()
        .position(|(_, new, already)| *new == entry.on_new && *already == entry.on_already_in)
        .unwrap_or(0);
    Some(Editing {
        program_index: found.unwrap_or(0),
        when_index,
        notice: found.is_none().then_some(NOT_IN_LIST),
    })
}

/// `GetValue` of the entry dialog.
pub fn finished(
    choices: &[Choice],
    program_index: usize,
    when_index: usize,
) -> Option<ExternalProgramEntry> {
    let choice = choices.get(program_index)?;
    let (_, on_new, on_already_in) = *WHENS.get(when_index)?;
    Some(ExternalProgramEntry {
        key: choice.key.clone(),
        name: choice.name.clone(),
        on_new,
        on_already_in,
    })
}

/// The list's rows, in the order they are kept.
pub fn rows(options: &ExternalProgramsOptions) -> Vec<String> {
    options
        .entries
        .iter()
        .map(ExternalProgramEntry::summary)
        .collect()
}

/// The list's selection, kept as the rows clicked.
#[derive(Default)]
pub struct List {
    pub selection: crate::list_selection::ListSelection<usize>,
}

impl std::fmt::Debug for List {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("List").finish_non_exhaustive()
    }
}

impl List {
    pub fn click(&mut self, count: usize, row: usize, ctrl: bool, shift: bool) {
        let order: Vec<usize> = (0..count).collect();
        self.selection.click(&order, row, ctrl, shift);
    }
    /// The selected rows, top first.
    pub fn selected(&self, count: usize) -> Vec<usize> {
        (0..count)
            .filter(|r| self.selection.is_selected(*r))
            .collect()
    }
    pub fn clear(&mut self) {
        self.selection = crate::list_selection::ListSelection::default();
    }
}
