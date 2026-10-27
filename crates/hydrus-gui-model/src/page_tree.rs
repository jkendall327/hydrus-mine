//! Stable notebook-tree cursor and expansion, independent from the shown page.
use hydrus_core::pages::{Page, PageContent, PageKey};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub key: PageKey,
    pub parent: Option<PageKey>,
    pub name: String,
    pub depth: i32,
    pub branch: bool,
    /// A page of pages (with pages or not), which a drop onto takes.
    pub notebook: bool,
}
#[derive(Debug, Default)]
pub struct Tree {
    nodes: Vec<Node>,
    expanded: HashSet<PageKey>,
    current: Option<PageKey>,
    shown: Option<PageKey>,
}
fn flatten(pages: &[Page], parent: Option<PageKey>, depth: i32, nodes: &mut Vec<Node>) {
    for page in pages {
        let children = if let PageContent::Pages(children) = &page.content {
            children.as_slice()
        } else {
            &[]
        };
        nodes.push(Node {
            key: page.key,
            parent,
            name: page.name.clone(),
            depth,
            branch: !children.is_empty(),
            notebook: matches!(page.content, PageContent::Pages(_)),
        });
        flatten(children, Some(page.key), depth + 1, nodes);
    }
}
impl Tree {
    pub fn sync(&mut self, pages: &[Page], shown: PageKey) {
        let mut nodes = Vec::new();
        flatten(pages, None, 0, &mut nodes);
        let changed = nodes != self.nodes;
        self.nodes = nodes;
        self.expanded
            .retain(|key| self.nodes.iter().any(|node| node.key == *key));
        if self.shown != Some(shown)
            || !self.nodes.iter().any(|node| Some(node.key) == self.current)
        {
            self.current = Some(shown);
            self.shown = Some(shown);
            self.reveal();
        } else if changed {
            self.reveal();
        }
    }
    fn reveal(&mut self) {
        let mut key = self.current;
        while let Some(parent) = self
            .nodes
            .iter()
            .find(|node| Some(node.key) == key)
            .and_then(|node| node.parent)
        {
            self.expanded.insert(parent);
            key = Some(parent);
        }
    }
    pub fn current(&self) -> Option<PageKey> {
        self.current
    }
    pub fn expanded(&self, key: PageKey) -> bool {
        self.expanded.contains(&key)
    }
    pub fn visible(&self) -> Vec<&Node> {
        self.nodes
            .iter()
            .filter(|node| {
                let mut parent = node.parent;
                while let Some(key) = parent {
                    if !self.expanded.contains(&key) {
                        return false;
                    }
                    parent = self
                        .nodes
                        .iter()
                        .find(|candidate| candidate.key == key)
                        .and_then(|candidate| candidate.parent);
                }
                true
            })
            .collect()
    }
    pub fn select(&mut self, key: PageKey) {
        if self.visible().iter().any(|node| node.key == key) {
            self.current = Some(key);
        }
    }
    pub fn toggle(&mut self, key: PageKey) {
        if self.nodes.iter().any(|node| node.key == key && node.branch)
            && !self.expanded.remove(&key)
        {
            self.expanded.insert(key);
        }
    }
    /// Up, Down, Left, Right, Home, End, activate, collapse all, expand all.
    pub fn navigate(&mut self, action: i32) -> Option<PageKey> {
        if action == 6 {
            return self.current;
        }
        if action == 7 {
            self.expanded.clear();
            return None;
        }
        if action == 8 {
            self.expanded.extend(self.nodes.iter().map(|node| node.key));
            return None;
        }
        let visible: Vec<PageKey> = self.visible().iter().map(|node| node.key).collect();
        let position = visible
            .iter()
            .position(|key| Some(*key) == self.current)
            .unwrap_or(0);
        let node = self
            .nodes
            .iter()
            .find(|node| Some(node.key) == self.current)
            .cloned();
        match action {
            0 => self.current = visible.get(position.saturating_sub(1)).copied(),
            1 => {
                self.current = visible
                    .get((position + 1).min(visible.len().saturating_sub(1)))
                    .copied();
            }
            2 => {
                if let Some(node) = node
                    && !self.expanded.remove(&node.key)
                {
                    self.current = node.parent.or(self.current);
                }
            }
            3 => {
                if let Some(node) = node
                    && node.branch
                {
                    if self.expanded.contains(&node.key) {
                        self.current = self
                            .nodes
                            .iter()
                            .find(|child| child.parent == Some(node.key))
                            .map(|child| child.key);
                    } else {
                        self.expanded.insert(node.key);
                    }
                }
            }
            4 => self.current = visible.first().copied(),
            5 => self.current = visible.last().copied(),
            _ => {}
        }
        None
    }
}

/// Where a page dragged in the tree is let go, as Qt's tree view decides
/// (`QAbstractItemViewPrivate::position`): near a row's top edge it goes
/// above that row, near its bottom edge below it, elsewhere on it; past the
/// last row, on the empty space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drop {
    Above(PageKey),
    On(PageKey),
    Below(PageKey),
    Space,
}

/// Where a drop `y` down the tree's rows (from the first row's top, each
/// `row_height` high) lands among the `visible` rows.
pub fn drop_at(visible: &[&Node], y: f32, row_height: f32) -> Drop {
    if y < 0.0 {
        return Drop::Space;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let row = (y / row_height).floor() as usize;
    let Some(node) = visible.get(row) else {
        return Drop::Space;
    };
    // (Qt's margin: a fifth and a bit of the row, 2 to 12 pixels; its
    // rectangles end a pixel short of the next row)
    let margin = (row_height / 5.5).round().clamp(2.0, 12.0);
    #[allow(clippy::cast_precision_loss)]
    let offset = y - row as f32 * row_height;
    if offset < margin {
        Drop::Above(node.key)
    } else if row_height - 1.0 - offset < margin {
        Drop::Below(node.key)
    } else {
        Drop::On(node.key)
    }
}

impl Tree {
    /// Where a drop puts the page `source`: the notebook it goes into
    /// (`None`: the top one) and the place among that notebook's pages
    /// (`None`: at the end), as the reference's tree model's `dropMimeData`
    /// puts it; `None` when Qt refuses the drop, it being on the dragged
    /// row or one inside it.
    pub fn drop_destination(
        &self,
        source: PageKey,
        drop: Drop,
    ) -> Option<(Option<PageKey>, Option<usize>)> {
        let node = |key: PageKey| self.nodes.iter().find(|node| node.key == key);
        let target = match drop {
            Drop::Above(key) | Drop::On(key) | Drop::Below(key) => key,
            Drop::Space => return Some((None, None)),
        };
        // (Qt's `droppingOnItself`: the row under the pointer, or one it is
        // inside, being dragged)
        let mut inside = Some(target);
        while let Some(key) = inside {
            if key == source {
                return None;
            }
            inside = node(key)?.parent;
        }
        let target = node(target)?;
        let siblings = || {
            self.nodes
                .iter()
                .filter(|node| node.parent == target.parent)
                .position(|node| node.key == target.key)
        };
        Some(match drop {
            Drop::Above(_) => (target.parent, Some(siblings()?)),
            Drop::Below(_) => (target.parent, Some(siblings()? + 1)),
            Drop::On(_) if target.notebook => (Some(target.key), None),
            _ => (target.parent, None),
        })
    }
}
