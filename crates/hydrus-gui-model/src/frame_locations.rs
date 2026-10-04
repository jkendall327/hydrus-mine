//! GUI options' editable frame table. Stable row identities preserve selection
//! across sorting, replacement and batch flips/resets; every change is detached.
use crate::list_selection::ListSelection;
use hydrus_core::windows::FrameLocation;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FlipSize,
    FlipPosition,
    ResetSize,
    ResetPosition,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub id: usize,
    pub name: String,
    pub frame: FrameLocation,
}
impl Row {
    /// The exact nine reference display/sort strings.
    pub fn cells(&self) -> Vec<String> {
        fn pair(value: Option<(i32, i32)>) -> String {
            value.map_or_else(String::new, |(a, b)| format!("({a}, {b})"))
        }
        fn yes(value: bool) -> String {
            if value { "yes".into() } else { String::new() }
        }
        let (x, y) = self.frame.default_gravity;
        let gravity = if x < 0 && y < 0 {
            "free to expand".into()
        } else {
            let mut dimensions = Vec::new();
            if x > 0 {
                dimensions.push("width");
            }
            if y > 0 {
                dimensions.push("height");
            }
            format!("{} constrained", dimensions.join(", "))
        };
        vec![
            self.name.clone(),
            yes(self.frame.remember_size),
            yes(self.frame.remember_position),
            pair(self.frame.last_size),
            pair(self.frame.last_position),
            gravity,
            self.frame.default_position.clone(),
            yes(self.frame.maximised),
            yes(self.frame.fullscreen),
        ]
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    pub rows: Vec<Row>,
    pub selection: ListSelection<usize>,
    pub sort_column: usize,
    pub ascending: bool,
}
impl Table {
    pub fn new(frames: BTreeMap<String, FrameLocation>) -> Self {
        Self {
            rows: frames
                .into_iter()
                .enumerate()
                .map(|(id, (name, frame))| Row { id, name, frame })
                .collect(),
            selection: ListSelection::default(),
            sort_column: 0,
            ascending: true,
        }
    }
    pub fn values(&self) -> BTreeMap<String, FrameLocation> {
        self.rows
            .iter()
            .map(|r| (r.name.clone(), r.frame.clone()))
            .collect()
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(
            &self.rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            index,
            ctrl,
            shift,
        );
    }
    pub fn sort(&mut self, column: usize, ascending: bool) {
        if column >= 9 {
            return;
        }
        self.sort_column = column;
        self.ascending = ascending;
        self.rows.sort_by(|a, b| {
            let a = a.cells();
            let b = b.cells();
            let order = a[column].cmp(&b[column]).then_with(|| a.cmp(&b));
            if ascending { order } else { order.reverse() }
        });
    }
    pub fn selected(&self) -> Option<&Row> {
        self.selection
            .one()
            .and_then(|id| self.rows.iter().find(|r| r.id == id))
    }
    pub fn replace(&mut self, id: usize, frame: FrameLocation) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.id == id) {
            row.frame = frame;
        }
        self.sort(self.sort_column, self.ascending);
    }
    pub fn action(&mut self, action: Action) {
        for row in &mut self.rows {
            if self.selection.is_selected(row.id) {
                match action {
                    Action::FlipSize => {
                        row.frame.remember_size = !row.frame.remember_size;
                    }
                    Action::FlipPosition => {
                        row.frame.remember_position = !row.frame.remember_position;
                    }
                    Action::ResetSize => {
                        row.frame.last_size = None;
                    }
                    Action::ResetPosition => {
                        row.frame.last_position = None;
                    }
                }
            }
        }
        self.sort(self.sort_column, self.ascending);
    }
}

/// Bounds applied by the reference's optional double-spin widgets at opening.
#[must_use]
pub fn normalised(mut frame: FrameLocation) -> FrameLocation {
    frame.last_size = frame
        .last_size
        .map(|(a, b)| (a.clamp(100, 1_000_000), b.clamp(100, 1_000_000)));
    frame.last_position = frame.last_position.map(|(a, b)| {
        (
            a.clamp(-1_000_000, 1_000_000),
            b.clamp(-1_000_000, 1_000_000),
        )
    });
    frame
}
