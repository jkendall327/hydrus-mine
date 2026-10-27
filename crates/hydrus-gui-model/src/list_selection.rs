//! A list's selection, as the reference's lists select (Qt's extended
//! selection): a click selects one row, ctrl+click adds or takes away
//! one, shift+click selects from the anchor (the last row clicked without
//! shift) to it, and ctrl+shift+click does the same. Rows are kept by
//! their item, so a selection outlives the list being sorted or read
//! again.
//!
//! As Qt's `QItemSelectionModel` does, the selection is the rows committed
//! so far plus a current range: a ctrl+click commits what is selected and
//! starts a new current range (toggling its row); a shift+click replaces only
//! the current range with the rows from the anchor to it, on top of the
//! committed rows. So after click 0, ctrl+click 2, ctrl+click 0, shift+click
//! 1, rows 0, 1 and 2 are selected (`oracle/fixtures/list_selection.json`).

/// A "row" the shared list (`ui/list_table.slint`) clicks for Ctrl+A: the
/// whole list selected, the anchor kept (as Qt's `selectAll`, recorded in
/// `oracle/fixtures/list_keys.json`). No list is this long.
pub const SELECT_ALL: usize = 2_147_483_647;

/// Rows from here are the shared list's ctrl+arrow keys: the anchor moved
/// to the row `row - MOVE_ANCHOR`, nothing selected or unselected (Qt moves
/// its current row there, and a shift range then starts from it).
pub const MOVE_ANCHOR: usize = 1_073_741_824;

/// The items selected, and the anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSelection<T> {
    /// What is selected, in the order Qt hands it out (`selectedRows`).
    selected: Vec<T>,
    /// The rows committed before the current range.
    committed: Vec<T>,
    /// The current range, and whether it toggles (a ctrl+click) or selects.
    current: Vec<T>,
    toggling: bool,
    anchor: Option<T>,
    /// A range is handed out from its anchor (Qt's list widgets, recorded in
    /// `external_command.json`) rather than top to bottom (its tree views,
    /// `list_selection.json`).
    from_anchor: bool,
}

impl<T> Default for ListSelection<T> {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            committed: Vec::new(),
            current: Vec::new(),
            toggling: false,
            anchor: None,
            from_anchor: false,
        }
    }
}

impl<T> ListSelection<T> {
    /// A selection whose shift ranges are handed out from their anchor, as a
    /// Qt list widget's (the reference's queue lists) are.
    #[must_use]
    pub fn from_anchor() -> Self {
        Self {
            from_anchor: true,
            ..Self::default()
        }
    }
}

impl<T: Copy + PartialEq> ListSelection<T> {
    /// The row `row` of `order` (the list as shown) clicked, or one of the
    /// list's keys ([`SELECT_ALL`], [`MOVE_ANCHOR`]).
    pub fn click(&mut self, order: &[T], row: usize, ctrl: bool, shift: bool) {
        if row == SELECT_ALL {
            self.select_all(order);
            return;
        }
        if let Some(to) = row.checked_sub(MOVE_ANCHOR) {
            if let Some(&item) = order.get(to) {
                self.anchor = Some(item);
            }
            return;
        }
        let Some(&item) = order.get(row) else {
            return;
        };
        let from = self
            .anchor
            .and_then(|a| order.iter().position(|&o| o == a))
            .filter(|_| shift);
        match from {
            Some(from) => {
                // Qt's SelectCurrent: the current range becomes the rows from
                // the anchor to this one; the committed rows stay.
                let (a, b) = (from.min(row), from.max(row));
                self.current = order[a..=b].to_vec();
                if self.from_anchor && from > row {
                    self.current.reverse();
                }
                self.toggling = false;
            }
            None if ctrl => {
                self.committed = std::mem::take(&mut self.selected);
                self.current = vec![item];
                self.toggling = true;
                self.anchor = Some(item);
            }
            None => self.restart(vec![item], Some(item)),
        }
        self.merge();
    }

    /// Qt's merge of the current range into the committed rows: those the
    /// range covers leave their place, and the range comes after the rest
    /// (a toggling range takes away the rows it covers that were committed).
    fn merge(&mut self) {
        let mut selected: Vec<T> = self
            .committed
            .iter()
            .copied()
            .filter(|c| !self.current.contains(c))
            .collect();
        selected.extend(
            self.current
                .iter()
                .copied()
                .filter(|c| !(self.toggling && self.committed.contains(c))),
        );
        self.selected = selected;
    }

    /// Select only these, as a fresh current range from `anchor`.
    fn restart(&mut self, items: Vec<T>, anchor: Option<T>) {
        self.committed.clear();
        self.current = items;
        self.toggling = false;
        self.anchor = anchor;
        self.merge();
    }

    /// Select just these (as a list's `SelectDatas`), with no anchor.
    pub fn select_many(&mut self, items: &[T]) {
        self.restart(items.to_vec(), None);
    }

    /// Reset the next Shift range origin without changing selected rows.
    /// Queue reordering keeps Qt's numeric current row as this origin.
    pub fn set_anchor(&mut self, item: Option<T>) {
        self.anchor = item;
    }

    /// Qt's select-all replaces selected rows while keeping the current range origin.
    pub fn select_all(&mut self, order: &[T]) {
        let anchor = self.anchor;
        self.restart(order.to_vec(), anchor);
    }

    /// Select only `item` (or nothing).
    pub fn select_only(&mut self, item: Option<T>) {
        self.restart(item.into_iter().collect(), item);
    }

    pub fn is_selected(&self, item: T) -> bool {
        self.selected.contains(&item)
    }

    /// Selection insertion order, as Qt's selectedItems used for list copying.
    pub fn selected_order(&self) -> &[T] {
        &self.selected
    }

    /// Those selected, in the list's order.
    pub fn in_order(&self, order: &[T]) -> Vec<T> {
        order
            .iter()
            .copied()
            .filter(|o| self.selected.contains(o))
            .collect()
    }

    /// The one selected, if exactly one is.
    pub fn one(&self) -> Option<T> {
        match self.selected.as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Forget an item gone from the list.
    pub fn forget(&mut self, item: T) {
        self.selected.retain(|&s| s != item);
        self.committed.retain(|&s| s != item);
        self.current.retain(|&s| s != item);
        if self.anchor == Some(item) {
            self.anchor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: [u32; 5] = [10, 20, 30, 40, 50];

    #[test]
    fn clicks_select_as_qt_s_extended_selection() {
        let mut s = ListSelection::default();
        assert!(s.is_empty());
        s.click(&ORDER, 1, false, false);
        assert_eq!(s.in_order(&ORDER), [20]);
        assert_eq!(s.one(), Some(20));
        assert!(!s.is_empty());
        // ctrl adds, and takes away
        s.click(&ORDER, 3, true, false);
        assert_eq!(s.in_order(&ORDER), [20, 40]);
        assert_eq!(s.one(), None);
        s.click(&ORDER, 1, true, false);
        assert_eq!(s.in_order(&ORDER), [40]);
        // shift: from the anchor (the last ctrl-clicked), replacing the
        // ctrl+click's toggle but keeping what was committed before it
        s.click(&ORDER, 0, false, true);
        assert_eq!(s.in_order(&ORDER), [10, 20, 40]);
        // (the anchor stays where it was)
        s.click(&ORDER, 4, false, true);
        assert_eq!(s.in_order(&ORDER), [20, 30, 40, 50]);
        // a plain click selects one, and is the anchor
        s.click(&ORDER, 2, false, false);
        assert_eq!(s.in_order(&ORDER), [30]);
        // ctrl+shift adds a range
        s.click(&ORDER, 0, true, false);
        s.click(&ORDER, 4, true, true);
        assert_eq!(s.in_order(&ORDER), [10, 20, 30, 40, 50]);
        // shift with no anchor is a plain click
        let mut s = ListSelection::default();
        s.click(&ORDER, 3, false, true);
        assert_eq!(s.in_order(&ORDER), [40]);
        // past the end: nothing
        s.click(&ORDER, 9, false, false);
        assert_eq!(s.in_order(&ORDER), [40]);
    }

    #[test]
    fn the_list_s_keys_select_all_and_move_the_anchor() {
        // (`oracle/fixtures/list_keys.json`, "ctrl+a then shift arrow" and
        // "ctrl move then shift arrow")
        let mut s = ListSelection::default();
        s.click(&ORDER, 1, false, false);
        s.click(&ORDER, SELECT_ALL, false, false);
        assert_eq!(s.in_order(&ORDER), ORDER);
        // the anchor kept: a shift range replaces the whole selection
        s.click(&ORDER, 2, false, true);
        assert_eq!(s.in_order(&ORDER), [20, 30]);
        // ctrl+arrow: the anchor moves, the selection stays
        s.click(&ORDER, 0, false, false);
        s.click(&ORDER, MOVE_ANCHOR + 3, false, false);
        assert_eq!(s.in_order(&ORDER), [10]);
        s.click(&ORDER, 4, false, true);
        assert_eq!(s.in_order(&ORDER), [40, 50]);
        // (past the end of the list: nothing)
        s.click(&ORDER, MOVE_ANCHOR + 9, false, false);
        s.click(&ORDER, 2, false, true);
        assert_eq!(s.in_order(&ORDER), [30, 40]);
    }

    #[test]
    fn a_selection_follows_its_items() {
        let mut s = ListSelection::default();
        s.click(&ORDER, 1, false, false);
        s.click(&ORDER, 2, true, false);
        // (sorted otherwise: the same items)
        let sorted = [50, 40, 30, 20, 10];
        assert_eq!(s.in_order(&sorted), [30, 20]);
        assert!(s.is_selected(20) && !s.is_selected(10));
        // the anchor gone, shift is a plain click
        s.forget(30);
        assert_eq!(s.in_order(&ORDER), [20]);
        s.click(&ORDER, 4, false, true);
        assert_eq!(s.in_order(&ORDER), [50]);
        s.select_only(Some(10));
        assert_eq!(s.in_order(&ORDER), [10]);
        s.select_only(None);
        assert!(s.is_empty());
        s.select_many(&[40, 20]);
        assert_eq!(s.in_order(&ORDER), [20, 40]);
        // (no anchor: shift is a plain click)
        s.click(&ORDER, 0, false, true);
        assert_eq!(s.in_order(&ORDER), [10]);
    }
}
