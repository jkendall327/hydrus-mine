//! Ordered JSON object names, with the reference QueueListBox's row identity.
use crate::list_selection::ListSelection;

pub const PROMPT: &str = "Enter the JSON Object name.";
pub const BLANK: &str = "Cannot enter blank text here!";

#[derive(Debug, Clone)]
pub struct ObjectNames {
    rows: Vec<(usize, String)>,
    pub selection: ListSelection<usize>,
    next_id: usize,
}
impl ObjectNames {
    pub fn new(names: &[String]) -> Self {
        Self {
            rows: names.iter().cloned().enumerate().collect(),
            selection: ListSelection::default(),
            next_id: names.len(),
        }
    }
    pub fn names(&self) -> Vec<String> {
        self.rows.iter().map(|(_, name)| name.clone()).collect()
    }
    pub fn selected(&self, index: usize) -> bool {
        self.rows
            .get(index)
            .is_some_and(|(id, _)| self.selection.is_selected(*id))
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(
            &self.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            index,
            ctrl,
            shift,
        );
    }
    /// Edit only the first selected row, even when several are selected.
    pub fn first_selected(&self) -> Option<(usize, String)> {
        self.rows
            .iter()
            .find(|(id, _)| self.selection.is_selected(*id))
            .cloned()
    }
    /// Empty input is vetoed; spaces and duplicate names remain literal keys.
    pub fn accept(&mut self, id: Option<usize>, name: String) -> Result<(), &'static str> {
        if name.is_empty() {
            return Err(BLANK);
        }
        if let Some(id) = id {
            if let Some((_, value)) = self.rows.iter_mut().find(|(row, _)| *row == id) {
                *value = name;
            }
        } else {
            self.rows.push((self.next_id, name));
            self.next_id += 1;
        }
        Ok(())
    }
    pub fn delete_question(&self) -> Option<String> {
        let count = self
            .rows
            .iter()
            .filter(|(id, _)| self.selection.is_selected(*id))
            .count();
        (count > 0).then(|| {
            format!(
                "Remove {} selected?",
                hydrus_core::numbers::human_int(count as u64)
            )
        })
    }
    pub fn delete(&mut self) {
        self.rows.retain(|(id, _)| !self.selection.is_selected(*id));
        self.selection = ListSelection::default();
    }
    /// Process upward selections from the start and downward ones from the end.
    pub fn move_selected(&mut self, distance: isize) {
        if !matches!(distance, -1 | 1) {
            return;
        }
        let mut indices: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, (id, _))| self.selection.is_selected(*id).then_some(i))
            .collect();
        if distance > 0 {
            indices.reverse();
        }
        for i in indices {
            let target = i
                .saturating_add_signed(distance)
                .min(self.rows.len().saturating_sub(1));
            let row = self.rows.remove(i);
            self.rows.insert(target, row);
        }
    }
}
