//! Frame-owned search history, separate from page state and content undo.
//!
//! The reference keeps two independent deduplicated recency lists. Both menu
//! actions toggle the chosen predicate on the currently visible media page;
//! their labels describe the historical change, not the eventual toggle.
use hydrus_core::search::predicate::Predicate;

/// Which reference search-history submenu owns an entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Addition,
    Removal,
}
/// Transient history shared explicitly by one frame's search pages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    pub added: Vec<Predicate>,
    pub removed: Vec<Predicate>,
}
impl History {
    /// Record actual set changes, ignoring reordering, refused inputs and drafts.
    pub fn record(&mut self, before: &[Predicate], after: &[Predicate]) {
        for predicate in after.iter().filter(|p| !before.contains(p)) {
            remember(&mut self.added, predicate);
        }
        for predicate in before.iter().filter(|p| !after.contains(p)) {
            remember(&mut self.removed, predicate);
        }
    }
    /// Retire the chosen historical entry before the page emits its new delta.
    /// A stale menu command does nothing; typed identity avoids index reuse.
    pub fn take(&mut self, kind: Kind, predicate: &Predicate) -> bool {
        let bucket = match kind {
            Kind::Addition => &mut self.added,
            Kind::Removal => &mut self.removed,
        };
        if let Some(index) = bucket.iter().position(|p| p == predicate) {
            bucket.remove(index);
            true
        } else {
            false
        }
    }
    /// Clear both histories after the separate confirmation is accepted.
    pub fn clear(&mut self) {
        self.added.clear();
        self.removed.clear();
    }
}
fn remember(bucket: &mut Vec<Predicate>, predicate: &Predicate) {
    bucket.retain(|p| p != predicate);
    bucket.push(predicate.clone());
}
