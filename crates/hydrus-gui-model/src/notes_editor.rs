//! The "manage notes" dialog (the reference's `EditFileNotesPanel`, opened
//! by a thumbnail's "manage > notes"): a tab per note of the focused file,
//! named and edited, added, renamed and deleted, then written back as the
//! notes to set and the names to delete.

use std::collections::{BTreeMap, BTreeSet};

use hydrus_core::notes::{NoteConflict, NoteMerge};
use hydrus_parse::text::clean_note_text;

use crate::favourites::non_dupe_name;

pub const TITLE: &str = "manage notes";
pub const NAME_PROMPT: &str = "Enter the name for the note.";
pub const DELETE_QUESTION: &str = "Delete this note?";
pub const CANCEL_QUESTION: &str =
    "It looks like you have made changes--are you sure you want to cancel?";

/// The dialog's tabs: each note's name and text, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesEditor {
    pub tabs: Vec<(String, String)>,
    pub current: usize,
    original: BTreeMap<String, String>,
}

impl NotesEditor {
    /// A tab per note, by name; a lone empty "notes" tab when there are
    /// none. `start_on` names the tab shown first.
    pub fn new(names_to_notes: &BTreeMap<String, String>, start_on: Option<&str>) -> Self {
        let tabs: Vec<(String, String)> = if names_to_notes.is_empty() {
            vec![("notes".to_owned(), String::new())]
        } else {
            names_to_notes
                .iter()
                .map(|(n, t)| (n.clone(), t.clone()))
                .collect()
        };
        let current = start_on
            .and_then(|s| tabs.iter().position(|(n, _)| n == s))
            .unwrap_or(0);
        Self {
            tabs,
            current,
            original: names_to_notes.clone(),
        }
    }

    /// "edit current name" and "delete current note" need a tab.
    pub fn can_edit(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// The notes to set (each tab's text cleaned, the empty ones left out)
    /// and the original names no longer among them.
    pub fn value(&self) -> (BTreeMap<String, String>, BTreeSet<String>) {
        let notes: BTreeMap<String, String> = self
            .tabs
            .iter()
            .map(|(n, t)| (n.clone(), clean_note_text(t)))
            .filter(|(_, t)| !t.is_empty())
            .collect();
        let deletees = self
            .original
            .keys()
            .filter(|n| !notes.contains_key(*n))
            .cloned()
            .collect();
        (notes, deletees)
    }

    /// Cancelling asks first when this is true.
    pub fn changed(&self) -> bool {
        self.value().0 != self.original
    }

    /// "add": a new empty tab under `name`, numbered if a note has it.
    pub fn add(&mut self, name: &str) {
        let (notes, _) = self.value();
        let name = non_dupe_name(name, &|n| notes.contains_key(n));
        self.tabs.push((name, String::new()));
        self.current = self.tabs.len() - 1;
    }

    /// "edit current name" (or double-clicking a tab): `index` renamed,
    /// numbered if another note has the name.
    pub fn rename(&mut self, index: usize, name: &str) {
        let Some(old) = self.tabs.get(index).map(|(n, _)| n.clone()) else {
            return;
        };
        let (notes, _) = self.value();
        let name = non_dupe_name(name, &|n| n != old && notes.contains_key(n));
        self.tabs[index].0 = name;
    }

    /// "delete current note", once answered yes.
    pub fn delete_current(&mut self) {
        if self.current < self.tabs.len() {
            self.tabs.remove(self.current);
            self.current = self.current.min(self.tabs.len().saturating_sub(1));
        }
    }

    /// Each tab's name and cleaned text, in tab order, the empty ones
    /// left out (a name on two tabs keeps its first place and last text).
    fn ordered_value(&self) -> Vec<(String, String)> {
        let mut notes: Vec<(String, String)> = Vec::new();
        for (name, text) in &self.tabs {
            let text = clean_note_text(text);
            match notes.iter_mut().find(|(n, _)| n == name) {
                Some(note) => note.1 = text,
                None => notes.push((name.clone(), text)),
            }
        }
        notes.retain(|(_, t)| !t.is_empty());
        notes
    }

    /// The copy button: every note as JSON, in tab order (the reference's
    /// default; the paste button reads it back), and what the button
    /// says. Nothing when there are no notes.
    pub fn copy(&self) -> Option<(String, String)> {
        let notes = self.ordered_value();
        if notes.is_empty() {
            return None;
        }
        let body: Vec<String> = notes
            .iter()
            .map(|(n, t)| format!("{}: {}", python_json_string(n), python_json_string(t)))
            .collect();
        let text = format!("{{{}}}", body.join(", "));
        Some((
            text,
            format!("Copied {} encoded notes!", human_int(notes.len())),
        ))
    }

    /// The paste button: `text` is JSON notes, an object or a list of
    /// name/note pairs, merged in as the reference merges them (extending
    /// a note it extends, renaming on a conflict). Returns what the button
    /// says, or the message for text it couldn't read.
    pub fn paste(&mut self, text: &str) -> Result<String, String> {
        let incoming = parse_pasted(text).map_err(|e| clipboard_error(text, &e))?;
        let (existing, _) = self.value();
        let merge = NoteMerge {
            extend_existing: true,
            conflict: NoteConflict::Rename,
        };
        let updates = merge.merge(&existing, &incoming);
        for (name, note) in &updates {
            if let Some(tab) = self.tabs.iter_mut().find(|(n, _)| n == name) {
                tab.1.clone_from(note);
            } else {
                self.tabs.push((name.clone(), note.clone()));
                self.current = self.tabs.len() - 1;
            }
        }
        Ok(format!("Pasted {} new notes!", human_int(updates.len())))
    }

    /// The copy-URLs button: the URLs in the tab in view, a line each
    /// (nothing copied when there are none), and what the button says.
    pub fn copy_urls(&self) -> Option<(Option<String>, String)> {
        let (_, text) = self.tabs.get(self.current)?;
        let urls = urls_in(&clean_note_text(text));
        let said = format!("Copied {} URLs!", human_int(urls.len()));
        Some(((!urls.is_empty()).then(|| urls.join("\n")), said))
    }
}

/// Pasted notes: a JSON object, or a list of `[name, note]` pairs (other
/// items are skipped), as the reference reads them.
pub fn parse_pasted(text: &str) -> Result<Vec<(String, String)>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let items: Vec<serde_json::Value> = match value {
        serde_json::Value::Object(map) => map
            .into_iter()
            .map(|(k, v)| serde_json::Value::Array(vec![serde_json::Value::String(k), v]))
            .collect(),
        serde_json::Value::Array(items) => items,
        _ => return Err("Not a list or an object!".to_owned()),
    };
    let mut pairs = Vec::new();
    for item in items {
        let serde_json::Value::Array(pair) = item else {
            continue;
        };
        let [name, note] = pair.as_slice() else {
            return Err("Not a two-tuple!".to_owned());
        };
        let Some(name) = name.as_str() else {
            return Err("Key not a string!".to_owned());
        };
        let Some(note) = note.as_str() else {
            return Err("Value not a string!".to_owned());
        };
        pairs.push((name.to_owned(), note.to_owned()));
    }
    Ok(pairs)
}

/// Every `http(s)://...` in `text`, up to whitespace or `<>"'`, as the
/// reference's `https?://[^\s<>"']+` finds them.
pub fn urls_in(text: &str) -> Vec<String> {
    let stops = |c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\'');
    let mut urls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("http") {
        let after = &rest[start + 4..];
        let scheme = if after.starts_with("s://") {
            Some(4 + 4)
        } else if after.starts_with("://") {
            Some(4 + 3)
        } else {
            None
        };
        let Some(scheme) = scheme else {
            rest = after;
            continue;
        };
        let tail = &rest[start + scheme..];
        let len = tail.find(stops).unwrap_or(tail.len());
        if len == 0 {
            rest = tail;
            continue;
        }
        urls.push(rest[start..start + scheme + len].to_owned());
        rest = &tail[len..];
    }
    urls
}

/// The thumbnail menu's "manage" entry: "notes", or "notes (N)" when the
/// focused file has some.
pub fn menu_label(count: usize) -> String {
    if count == 0 {
        "notes".to_owned()
    } else {
        format!("notes ({})", human_int(count))
    }
}

fn human_int(n: usize) -> String {
    hydrus_core::numbers::human_int(n as u64)
}

/// `s` as Python's `json.dumps` writes a string: non-ASCII and control
/// characters as `\uXXXX`.
fn python_json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

/// What the reference says of clipboard text it couldn't read as notes.
pub fn clipboard_error(content: &str, error: &str) -> String {
    clipboard_parse_error(
        "JSON names and notes, either as an Object or a list of pairs",
        content,
        error,
    )
}

/// What the reference says of clipboard text it couldn't read
/// (`PresentClipboardParseError`): what it expected, the text (elided),
/// and the error.
pub fn clipboard_parse_error(expected: &str, content: &str, error: &str) -> String {
    let shown: String = if content.chars().count() > 64 {
        content
            .chars()
            .take(63)
            .chain(std::iter::once('\u{2026}'))
            .collect()
    } else {
        content.to_owned()
    };
    format!(
        "Sorry, I could not understand what was in the clipboard. I was expecting \"{expected}\" but received this text:\n\n{shown}\n\nMore details have been written to the log, but the general error was:\n\n{error}"
    )
}
