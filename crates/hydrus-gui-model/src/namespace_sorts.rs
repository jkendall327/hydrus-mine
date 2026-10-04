//! Staged Options namespace-sort queue and the reference's escaped text editor.
use crate::list_selection::ListSelection;
use hydrus_core::pages::{PageSort, PageSortBy};

pub const TEXT_MESSAGE: &str = "Write the namespaces you would like to sort by here, separated by hyphens. Any namespace in any of your sort definitions will be added to the collect-by menu.\n\nIf the namespace you want to add has a hyphen, like 'creator-id', instead type it with a backslash escape, like 'creator\\-id-page'.";
pub const VIEW_TITLE: &str = "select tag view to sort on";
pub const VIEW_MESSAGE: &str = "If you filter your different tag views (e.g. hiding the PTR's title tags), sorting on those views may give a different order. If you are not sure on this, set 'display tags'.";
pub const VIEWS: [(&str, i64); 3] = [
    ("display tags", 1),
    ("multiple media view tags", 3),
    ("single media view tags", 2),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: u64,
    pub sort: PageSort,
    pub selected: bool,
}
impl Row {
    pub fn label(&self) -> String {
        match &self.sort.by {
            PageSortBy::Namespaces { namespaces, .. } => namespaces.join("-"),
            _ => String::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    Text(String),
    View,
    Done,
}
#[derive(Debug)]
struct Pending {
    target: Option<u64>,
    advanced: bool,
    namespaces: Vec<String>,
    view: bool,
}
#[derive(Debug)]
pub struct Editor {
    rows: Vec<Row>,
    selection: ListSelection<u64>,
    next: u64,
    pending: Option<Pending>,
}
impl Editor {
    pub fn new(sorts: &[PageSort]) -> Self {
        let rows: Vec<_> = sorts
            .iter()
            .filter(|sort| matches!(sort.by, PageSortBy::Namespaces { .. }))
            .enumerate()
            .map(|(i, sort)| Row {
                id: u64::try_from(i).unwrap_or(u64::MAX),
                sort: PageSort {
                    tag_context: hydrus_core::search::context::TagContext::default(),
                    ascending: true,
                    ..sort.clone()
                },
                selected: false,
            })
            .collect();
        Self {
            next: u64::try_from(rows.len()).unwrap_or(u64::MAX),
            rows,
            selection: ListSelection::default(),
            pending: None,
        }
    }
    pub fn rows(&self) -> Vec<Row> {
        self.rows
            .iter()
            .map(|row| Row {
                selected: self.selection.is_selected(row.id),
                ..row.clone()
            })
            .collect()
    }
    pub fn value(&self) -> Vec<PageSort> {
        self.rows.iter().map(|row| row.sort.clone()).collect()
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(&self.order(), index, ctrl, shift);
    }
    pub fn select(&mut self, indices: &[usize]) {
        self.selection.select_many(
            &indices
                .iter()
                .filter_map(|&i| self.rows.get(i).map(|row| row.id))
                .collect::<Vec<_>>(),
        );
    }
    pub fn order(&self) -> Vec<u64> {
        self.rows.iter().map(|row| row.id).collect()
    }
    pub fn begin(&mut self, edit: bool, advanced: bool) -> Prompt {
        let target = if edit {
            self.selection.in_order(&self.order()).first().copied()
        } else {
            None
        };
        if edit && target.is_none() {
            return Prompt::Done;
        }
        let namespaces = target
            .and_then(|id| self.rows.iter().find(|row| row.id == id))
            .and_then(|row| match &row.sort.by {
                PageSortBy::Namespaces { namespaces, .. } => Some(namespaces.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec!["creator".into(), "series".into(), "page".into()]);
        let text = namespaces
            .iter()
            .map(|namespace| namespace.replace('-', "\\-"))
            .collect::<Vec<_>>()
            .join("-");
        self.pending = Some(Pending {
            target,
            advanced,
            namespaces: vec![],
            view: false,
        });
        Prompt::Text(text)
    }
    pub fn text(&mut self, text: Option<&str>) -> Prompt {
        let Some(pending) = &mut self.pending else {
            return Prompt::Done;
        };
        if pending.view {
            return Prompt::View;
        }
        let Some(text) = text else {
            self.pending = None;
            return Prompt::Done;
        };
        let namespaces = parse(text);
        if namespaces.is_empty() {
            self.pending = None;
            return Prompt::Done;
        }
        pending.namespaces = namespaces;
        if pending.advanced {
            pending.view = true;
            Prompt::View
        } else {
            self.finish(1);
            Prompt::Done
        }
    }
    pub fn view(&mut self, index: Option<usize>) -> Prompt {
        if !self.pending.as_ref().is_some_and(|pending| pending.view) {
            return Prompt::Done;
        }
        if let Some((_, view)) = index.and_then(|i| VIEWS.get(i)) {
            self.finish(*view);
        } else {
            self.pending = None;
        }
        Prompt::Done
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }
    fn finish(&mut self, view: i64) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let sort = PageSort {
            tag_context: hydrus_core::search::context::TagContext::default(),
            ascending: true,
            by: PageSortBy::Namespaces {
                namespaces: pending.namespaces,
                tag_display_type: view,
            },
        };
        if let Some(id) = pending.target {
            if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
                row.sort = sort;
            }
        } else {
            self.rows.push(Row {
                id: self.next,
                sort,
                selected: false,
            });
            self.next = self.next.saturating_add(1);
        }
    }
    pub fn move_selected(&mut self, down: bool) {
        let mut indices: Vec<_> = self
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| self.selection.is_selected(row.id).then_some(i))
            .collect();
        if down {
            indices.reverse();
        }
        for i in indices {
            let next = if down {
                (i + 1).min(self.rows.len() - 1)
            } else {
                i.saturating_sub(1)
            };
            let row = self.rows.remove(i);
            self.rows.insert(next, row);
        }
    }
    pub fn delete_request(&self) -> Option<(Vec<u64>, String)> {
        let ids = self.selection.in_order(&self.order());
        (!ids.is_empty()).then(|| {
            let question = format!(
                "Remove {} selected?",
                hydrus_core::numbers::human_int(u64::try_from(ids.len()).unwrap_or(u64::MAX))
            );
            (ids, question)
        })
    }
    pub fn delete(&mut self, ids: &[u64]) {
        self.rows.retain(|row| !ids.contains(&row.id));
        self.selection
            .select_many(&self.selection.in_order(&self.order()));
    }
}

/// Split only hyphens whose immediate predecessor is not a backslash, then
/// unescape and clean namespaces as the reference's regular expression does.
pub fn parse(text: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut previous = None;
    for character in text.chars() {
        if character == '-' && previous != Some('\\') {
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(character);
        }
        previous = Some(character);
    }
    parts
        .into_iter()
        .filter_map(|part| hydrus_core::tag::clean_tag_checked(&part.replace("\\-", "-")))
        .collect()
}
