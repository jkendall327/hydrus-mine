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
