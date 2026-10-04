//! Detached tag-banner draft, ordered namespace rows and cleaned live example.
use crate::list_selection::ListSelection;
use hydrus_core::{
    Tag,
    tag_presentation::TagPresentation,
    tag_summary::{NamespaceInfo, TagSummaryGenerator},
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    ThumbnailTop,
    ThumbnailBottomRight,
    MediaViewerTop,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub info: NamespaceInfo,
    pub selected: bool,
}
impl Row {
    pub fn label(&self) -> String {
        format!(
            "{} | prefix: \"{}\" | separator: \"{}\"",
            if self.info.namespace.is_empty() {
                "unnamespaced"
            } else {
                &self.info.namespace
            },
            self.info.prefix,
            self.info.separator
        )
    }
}
#[derive(Debug)]
pub struct Editor {
    pub show: bool,
    pub background: [u8; 4],
    pub text: [u8; 4],
    pub separator: String,
    pub examples: String,
    rows: Vec<Row>,
    selection: ListSelection<u64>,
    next: u64,
    presentation: TagPresentation,
}
impl Editor {
    pub fn new(value: &TagSummaryGenerator, presentation: TagPresentation) -> Self {
        Self {
            show: value.show,
            background: value.background,
            text: value.text,
            separator: value.separator.clone(),
            examples: value.example_tags.join("\n"),
            rows: value
                .namespace_info
                .iter()
                .enumerate()
                .map(|(i, info)| Row {
                    id: i as u64,
                    info: info.clone(),
                    selected: false,
                })
                .collect(),
            selection: ListSelection::default(),
            next: value.namespace_info.len() as u64,
            presentation,
        }
    }
    fn order(&self) -> Vec<u64> {
        self.rows.iter().map(|row| row.id).collect()
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
    pub fn click(&mut self, row: usize, ctrl: bool, shift: bool) {
        self.selection.click(&self.order(), row, ctrl, shift);
    }
    pub fn selected(&self) -> Vec<u64> {
        self.selection.in_order(&self.order())
    }
    pub fn first_selected(&self) -> Option<(u64, NamespaceInfo)> {
        self.selected()
            .first()
            .and_then(|id| self.rows.iter().find(|row| row.id == *id))
            .map(|row| (row.id, row.info.clone()))
    }
    pub fn put(&mut self, target: Option<u64>, info: NamespaceInfo) {
        if let Some(id) = target {
            if let Some(row) = self.rows.iter_mut().find(|row| row.id == id) {
                row.info = info;
            }
        } else {
            self.rows.push(Row {
                id: self.next,
                info,
                selected: false,
            });
            self.next += 1;
        }
    }
    pub fn move_selected(&mut self, down: bool) {
        let end = self.rows.len().saturating_sub(1);
        if down {
            for i in (0..end).rev() {
                if self.selection.is_selected(self.rows[i].id)
                    && !self.selection.is_selected(self.rows[i + 1].id)
                {
                    self.rows.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..self.rows.len() {
                if self.selection.is_selected(self.rows[i].id)
                    && !self.selection.is_selected(self.rows[i - 1].id)
                {
                    self.rows.swap(i, i - 1);
                }
            }
        }
    }
    pub fn delete(&mut self, ids: &[u64]) {
        self.rows.retain(|row| !ids.contains(&row.id));
        for id in ids {
            self.selection.forget(*id);
        }
    }
    pub fn value(&self) -> TagSummaryGenerator {
        let example_tags: BTreeSet<_> = self
            .examples
            .lines()
            .filter_map(Tag::new)
            .map(|tag| tag.as_str().to_owned())
            .collect();
        TagSummaryGenerator {
            background: self.background,
            text: self.text,
            namespace_info: self.rows.iter().map(|row| row.info.clone()).collect(),
            separator: self.separator.clone(),
            example_tags: example_tags.into_iter().collect(),
            show: self.show,
        }
    }
    pub fn preview(&self) -> String {
        if !self.show {
            return "not showing".into();
        }
        let value = self.value();
        value.summary(value.example_tags.iter().map(String::as_str), |tag| {
            self.presentation.render(tag)
        })
    }
}
