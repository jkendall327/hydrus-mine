//! The command child's ordered, extended-selection parameter draft.
use crate::{edit_subscription::insertable_summary, list_selection::ListSelection};
use hydrus_core::{external_calls::clean_arguments, numbers::human_int};

/// Raw templates remain visible until acceptance; the example uses cleaned values.
#[derive(Debug, Default)]
pub struct Queue {
    pub arguments: Vec<String>,
    pub selection: ListSelection<usize>,
    pub current: Option<usize>,
}
impl Queue {
    #[must_use]
    pub fn new(arguments: Vec<String>) -> Self {
        Self {
            arguments,
            ..Self::default()
        }
    }
    pub fn selected(&self) -> Vec<usize> {
        self.selection
            .in_order(&(0..self.arguments.len()).collect::<Vec<_>>())
    }
    pub fn click(&mut self, row: usize, control: bool, shift: bool) {
        if row < self.arguments.len() {
            self.current = Some(row);
            self.selection.click(
                &(0..self.arguments.len()).collect::<Vec<_>>(),
                row,
                control,
                shift,
            );
        }
    }
    /// Ctrl navigation changes the current row without changing the selection.
    pub fn navigate(&mut self, destination: &str, control: bool, shift: bool) {
        let Some(last) = self.arguments.len().checked_sub(1) else {
            return;
        };
        let row = match destination {
            "home" => 0,
            "end" => last,
            "previous" => self.current.unwrap_or(0).saturating_sub(1),
            "next" => self.current.map_or(0, |i| (i + 1).min(last)),
            _ => return,
        };
        self.current = Some(row);
        if !control || shift {
            self.selection
                .click(&(0..=last).collect::<Vec<_>>(), row, control, shift);
        }
    }
    pub fn toggle_current(&mut self) {
        if let Some(row) = self.current {
            self.click(row, true, false);
        }
    }
    pub fn select_all(&mut self) {
        self.selection
            .select_many(&(0..self.arguments.len()).collect::<Vec<_>>());
    }
    pub fn copy_selected(&self) -> Option<String> {
        let selected = self.selection.selected_order();
        (!selected.is_empty()).then(|| {
            selected
                .iter()
                .map(|i| self.arguments[*i].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
    }
    /// Remove the captured selection, preserving the nearest surviving current row.
    pub fn delete(&mut self, selected: &[usize]) {
        if let Some(current) = self.current {
            let before = selected.iter().filter(|i| **i < current).count();
            let remaining = self.arguments.len() - selected.len();
            self.current = remaining
                .checked_sub(1)
                .map(|last| (current - before).min(last));
        }
        for i in selected.iter().rev() {
            self.arguments.remove(*i);
        }
        self.selection = ListSelection::default();
    }
    /// Qt moves original selected indices, even across a selected neighbour.
    pub fn reorder(&mut self, down: bool) {
        let len = self.arguments.len();
        let mut selected = self.selection.selected_order().to_vec();
        let mut order = self.selected();
        if down {
            order.reverse();
        }
        for i in order {
            let neighbour = if down {
                (i + 1 < len).then_some(i + 1)
            } else {
                i.checked_sub(1)
            };
            if let Some(j) = neighbour {
                self.arguments.swap(i, j);
                selected.retain(|index| *index != i);
                for index in &mut selected {
                    if *index == j {
                        *index = i;
                    }
                }
                selected.push(j);
                // QList takes the item, chooses its previous neighbour as
                // current, then inserts it without making it current again.
                self.current = self.current.map(|c| {
                    if c == i {
                        if down { i.saturating_sub(1) } else { i }
                    } else if c == j {
                        i
                    } else {
                        c
                    }
                });
            }
        }
        self.selection.select_many(&selected);
        self.selection.set_anchor(self.current);
    }
    pub fn full_template(&self, executable: &str) -> String {
        format!(
            "{executable} {}",
            clean_arguments(&self.arguments).join(" ")
        )
    }
}

/// Clipboard parsing deliberately splits literal spaces, without shell quoting.
#[derive(Debug, PartialEq, Eq)]
pub struct Paste {
    pub executable: String,
    pub arguments: Vec<String>,
}
impl Paste {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut items = text
            .trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
            .split(' ');
        Self {
            executable: items.next().unwrap_or_default().into(),
            arguments: items.map(str::to_owned).collect(),
        }
    }
    pub fn question(&self) -> String {
        let summary = if self.arguments.is_empty() {
            ".".into()
        } else {
            format!(":{}", insertable_summary(&self.arguments))
        };
        format!(
            "I took your paste and got a command \"{}\" and {} parameters{summary}\n\nLook good?",
            self.executable,
            human_int(self.arguments.len() as u64)
        )
    }
}
