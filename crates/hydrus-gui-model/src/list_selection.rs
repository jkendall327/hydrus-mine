//! A list's selection, as the reference's lists select (Qt's extended
//! selection): a click selects one row, ctrl+click adds or takes away
//! one, shift+click selects from the anchor (the last row clicked without
//! shift) to it, and ctrl+shift+click adds that range. Rows are kept by
//! their item, so a selection outlives the list being sorted or read
//! again.

/// The items selected, and the anchor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSelection<T> {
    selected: Vec<T>,
    anchor: Option<T>,
}

impl<T> Default for ListSelection<T> {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            anchor: None,
        }
    }
}

impl<T: Copy + PartialEq> ListSelection<T> {
    /// The row `row` of `order` (the list as shown) clicked.
    pub fn click(&mut self, order: &[T], row: usize, ctrl: bool, shift: bool) {
        let Some(&item) = order.get(row) else {
            return;
        };
        let from = self
            .anchor
            .and_then(|a| order.iter().position(|&o| o == a))
            .filter(|_| shift);
        match from {
            Some(from) => {
                if !ctrl {
                    self.selected.clear();
                }
                let (a, b) = (from.min(row), from.max(row));
                for &o in &order[a..=b] {
                    if !self.selected.contains(&o) {
                        self.selected.push(o);
                    }
                }
            }
            None if ctrl => {
                if let Some(i) = self.selected.iter().position(|&s| s == item) {
                    self.selected.remove(i);
                } else {
                    self.selected.push(item);
                }
                self.anchor = Some(item);
            }
            None => {
                self.selected = vec![item];
                self.anchor = Some(item);
            }
        }
    }

    /// Select only `item` (or nothing).
    pub fn select_only(&mut self, item: Option<T>) {
        self.selected = item.into_iter().collect();
        self.anchor = item;
    }

    pub fn is_selected(&self, item: T) -> bool {
        self.selected.contains(&item)
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
        // shift: from the anchor (the last ctrl-clicked), replacing
        s.click(&ORDER, 0, false, true);
        assert_eq!(s.in_order(&ORDER), [10, 20]);
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
    }
}
