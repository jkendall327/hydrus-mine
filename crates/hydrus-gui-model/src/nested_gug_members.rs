//! The nested gallery URL generator editor's members list
//! (`EditNGUGPanel`): the generators it runs with an "available?" column,
//! an `add` button that offers the single generators not yet in it as a
//! multi-select chooser (or warns that none is left), and a `delete`
//! button that asks first. Recorded by `oracle/record_nested_gug_members.py`
//! (`oracle/fixtures/nested_gug_members.json`).

use hydrus_core::url::AnyGug;

use crate::downloader_definitions::{DefinitionEditor, EditValue};

/// The warning when every generator is in the list already.
pub const NONE_LEFT: &str = "No remaining gugs available!";
/// The chooser's title.
pub const CHOOSER_TITLE: &str = "choose gugs";
/// The list's headings (`COLUMN_LIST_NGUG_GUGS`).
pub const COLUMNS: [&str; 2] = ["gug name", "available?"];

/// The chooser while it is open: the generators offered, in the chooser's
/// order, and whether each is ticked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chooser {
    /// `(key, name, ticked)`.
    pub choices: Vec<(String, String, bool)>,
}

/// The members list's own state in the editor: its sort, and the chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Members {
    /// Heading sorted by (0 name, 1 available), and ascending.
    pub sort: (usize, bool),
    pub chooser: Option<Chooser>,
    /// The question "Remove all selected?" is waiting for an answer.
    pub deleting: bool,
}

impl Default for Members {
    fn default() -> Self {
        Self {
            sort: (0, true),
            chooser: None,
            deleting: false,
        }
    }
}

impl DefinitionEditor {
    fn members_available(&self) -> Vec<(String, String)> {
        self.downloaders
            .gugs
            .gugs
            .iter()
            .filter_map(|g| match g {
                AnyGug::Single(s) => Some((s.key.clone(), s.name.clone())),
                AnyGug::Nested(_) => None,
            })
            .collect()
    }

    /// Whether a member is one of the available generators, by key or name
    /// (`_ConvertGUGDataToDisplayTuple`).
    fn member_available(&self, key: &str, name: &str) -> bool {
        self.members_available()
            .iter()
            .any(|(k, n)| k == key || n == name)
    }

    /// Put the members in the order the list shows them, which is also the
    /// order the panel hands them back in.
    pub(crate) fn members_sorted(&mut self) {
        let (column, ascending) = self.members.sort;
        let available: Vec<(String, String)> = self.members_available();
        let EditValue::Gug(AnyGug::Nested(n)) = &mut self.value else {
            return;
        };
        n.gugs.sort_by(|a, b| {
            let by = |m: &(String, String)| {
                let here = available.iter().any(|(k, name)| *k == m.0 || *name == m.1);
                (
                    hydrus_core::casefold::casefold(&m.1),
                    here,
                    m.1.clone(),
                    m.0.clone(),
                )
            };
            let (a, b) = (by(a), by(b));
            let order = if column == 1 {
                (a.1, &a.0, &a.2, &a.3).cmp(&(b.1, &b.0, &b.2, &b.3))
            } else {
                (&a.0, a.1, &a.2, &a.3).cmp(&(&b.0, b.1, &b.2, &b.3))
            };
            if ascending { order } else { order.reverse() }
        });
    }

    /// The rows as shown: `(name, available)`.
    pub fn member_rows(&self) -> Vec<(String, bool)> {
        let EditValue::Gug(AnyGug::Nested(n)) = &self.value else {
            return Vec::new();
        };
        n.gugs
            .iter()
            .map(|(k, name)| (name.clone(), self.member_available(k, name)))
            .collect()
    }

    /// The rows selected, in list order.
    pub fn member_selected(&self) -> Vec<usize> {
        let order: Vec<usize> = (0..self.member_rows().len()).collect();
        self.member_selection.in_order(&order)
    }

    /// A row clicked.
    pub fn member_click(&mut self, row: usize, ctrl: bool, shift: bool) {
        let order: Vec<usize> = (0..self.member_rows().len()).collect();
        self.member_selection.click(&order, row, ctrl, shift);
    }

    /// A heading clicked: sort by it, ascending unless it sorted so.
    pub fn member_sort(&mut self, column: usize, ascending: bool) {
        self.members.sort = (column, ascending);
        let selected: Vec<(String, String)> = self.member_selected_data();
        self.members_sorted();
        self.member_select_data(&selected);
    }

    fn member_selected_data(&self) -> Vec<(String, String)> {
        let EditValue::Gug(AnyGug::Nested(n)) = &self.value else {
            return Vec::new();
        };
        self.member_selected()
            .into_iter()
            .filter_map(|i| n.gugs.get(i).cloned())
            .collect()
    }

    fn member_select_data(&mut self, data: &[(String, String)]) {
        let EditValue::Gug(AnyGug::Nested(n)) = &self.value else {
            return;
        };
        let rows: Vec<usize> = data
            .iter()
            .filter_map(|d| n.gugs.iter().position(|m| m == d))
            .collect();
        let order: Vec<usize> = (0..n.gugs.len()).collect();
        self.member_selection = crate::list_selection::ListSelection::default();
        for (i, row) in rows.into_iter().enumerate() {
            self.member_selection.click(&order, row, i > 0, false);
        }
    }

    /// The `add` button (`_AddGUGButtonClick`): open the chooser with the
    /// generators not yet in the list, or say none is left.
    pub fn member_add(&mut self) -> Result<(), String> {
        let EditValue::Gug(AnyGug::Nested(n)) = &self.value else {
            return Ok(());
        };
        let mut choices: Vec<(String, String, bool)> = self
            .members_available()
            .into_iter()
            .filter(|(k, name)| !n.gugs.iter().any(|(mk, mn)| mk == k || mn == name))
            .map(|(k, name)| (k, name, false))
            .collect();
        if choices.is_empty() {
            return Err(NONE_LEFT.into());
        }
        choices.sort_by_key(|(_, name, _)| hydrus_core::sort::human_sort_key(name));
        self.members.chooser = Some(Chooser { choices });
        Ok(())
    }

    /// A chooser box ticked or unticked.
    pub fn member_tick(&mut self, index: usize) {
        if let Some(chooser) = &mut self.members.chooser
            && let Some(choice) = chooser.choices.get_mut(index)
        {
            choice.2 = !choice.2;
        }
    }

    /// The chooser answered. Ok adds what is ticked (in the chooser's
    /// order), selecting the last added; cancel changes nothing.
    pub fn member_chosen(&mut self, ok: bool) {
        let Some(chooser) = self.members.chooser.take() else {
            return;
        };
        if !ok {
            return;
        }
        let chosen: Vec<(String, String)> = chooser
            .choices
            .into_iter()
            .filter(|c| c.2)
            .map(|(k, n, _)| (k, n))
            .collect();
        let EditValue::Gug(AnyGug::Nested(n)) = &mut self.value else {
            return;
        };
        n.gugs.extend(chosen.iter().cloned());
        self.members_sorted();
        if let Some(last) = chosen.last() {
            let last = [last.clone()];
            self.member_select_data(&last);
        }
    }

    /// The `delete` button: ask first (`ShowDeleteSelectedDialog`).
    pub fn member_delete(&mut self) -> Option<&'static str> {
        if self.member_selected().is_empty() {
            return None;
        }
        self.members.deleting = true;
        Some(crate::downloader_definitions::REMOVE)
    }

    /// The question answered.
    pub fn member_deleted(&mut self, yes: bool) {
        if !std::mem::take(&mut self.members.deleting) || !yes {
            return;
        }
        let selected = self.member_selected();
        let EditValue::Gug(AnyGug::Nested(n)) = &mut self.value else {
            return;
        };
        let mut i = 0;
        n.gugs.retain(|_| {
            let keep = !selected.contains(&i);
            i += 1;
            keep
        });
        self.member_selection = crate::list_selection::ListSelection::default();
    }
}
