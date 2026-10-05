//! The scheduled-work tab, keyed by job type rather than transient row indexes.
use std::collections::BTreeMap;

use hydrus_core::numbers::human_int;
use hydrus_store::file_maintenance::JobType;

use crate::list_selection::ListSelection;

pub const CLEAR_QUESTION: &str = "Clear all the selected scheduled work?";
pub const EXPLANATION: &str = "Here is the outstanding file maintenance work. This will be slowly completed in the background, usually a file every few seconds (you can edit this under _options->maintenance and processing_). Although you can rush work if you want to, it is best to generally leave it alone.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub job: JobType,
    pub name: String,
    pub due: u64,
    pub future: u64,
    pub selected: bool,
}
impl Row {
    pub fn count_text(&self) -> String {
        if self.future == 0 {
            human_int(self.due)
        } else {
            format!(
                "{} ({} is not yet due)",
                human_int(self.due),
                human_int(self.future)
            )
        }
    }
}

#[derive(Debug, Default)]
pub struct Queue {
    counts: BTreeMap<JobType, (u64, u64)>,
    selection: ListSelection<JobType>,
    pub numeric: bool,
    pub ascending: bool,
}
impl Queue {
    pub fn new() -> Self {
        Self {
            ascending: true,
            ..Self::default()
        }
    }
    pub fn replace(&mut self, counts: BTreeMap<JobType, (u64, u64)>) {
        for job in self.selection.selected_order().to_vec() {
            if !counts.contains_key(&job) {
                self.selection.forget(job);
            }
        }
        self.counts = counts;
    }
    pub fn rows(&self) -> Vec<Row> {
        let mut rows: Vec<_> = self
            .counts
            .iter()
            .map(|(&job, &(due, future))| Row {
                job,
                name: job.description().into(),
                due,
                future,
                selected: self.selection.selected_order().contains(&job),
            })
            .collect();
        rows.sort_by(|a, b| {
            let order = if self.numeric {
                a.due.cmp(&b.due).then_with(|| a.name.cmp(&b.name))
            } else {
                a.name.cmp(&b.name).then_with(|| a.due.cmp(&b.due))
            };
            if self.ascending {
                order
            } else {
                order.reverse()
            }
        });
        rows
    }
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        let order = self.rows().iter().map(|row| row.job).collect::<Vec<_>>();
        self.selection.click(&order, row, ctrl, shift);
    }
    pub fn selected(&self) -> Vec<JobType> {
        self.selection.selected_order().to_vec()
    }
    pub fn can_work(&self, selected: bool) -> bool {
        self.counts.iter().any(|(job, &(due, _))| {
            due > 0 && (!selected || self.selection.selected_order().contains(job))
        })
    }
}
