//! Which of a page's files are selected, and which is focused, as the
//! reference's thumbnail grid has them (`MediaResultsPanel`): a click
//! selects just the file (or, on one already selected, leaves the
//! selection be), ctrl+click adds or takes away one, shift+click selects
//! from where the last click started to the file, and the arrows, home,
//! end and page up and down move the focus, with shift selecting as they
//! go. Files are kept by id, so the selection survives sorting. Plain
//! Rust, tested directly.

use std::collections::HashSet;

use hydrus_core::HashId;

#[derive(Debug, Clone, Default)]
pub struct Selection {
    selected: HashSet<HashId>,
    focused: Option<HashId>,
    /// Where the keyboard moves from: the focused file, or a file
    /// ctrl+clicked or shift+clicked since.
    last_hit: Option<HashId>,
    /// Where it moves from once the focused file is gone (the reference's
    /// `_next_best_media_if_focuses_removed`).
    next_best: Option<HashId>,
    /// Where a shift+click's range starts, and the files the range added.
    shift_start: Option<HashId>,
    shift_added: HashSet<HashId>,
}

/// A move of the focus, as the reference's thumbnail shortcuts have them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

impl Selection {
    pub fn is_selected(&self, file: HashId) -> bool {
        self.selected.contains(&file)
    }

    pub fn count(&self) -> usize {
        self.selected.len()
    }

    /// The selected files, in the page's order.
    pub fn files(&self, sorted: &[HashId]) -> Vec<HashId> {
        sorted
            .iter()
            .copied()
            .filter(|f| self.selected.contains(f))
            .collect()
    }

    pub fn focused(&self) -> Option<HashId> {
        self.focused
    }

    /// A click on `file` (or on no file), with ctrl or shift held
    /// (`_HitMedia`).
    pub fn hit(&mut self, sorted: &[HashId], file: Option<HashId>, ctrl: bool, shift: bool) {
        let Some(file) = file else {
            if !ctrl && !shift {
                self.select_none(sorted);
                self.set_focused(sorted, None);
                self.end_shift_select();
            }
            return;
        };
        if ctrl && !shift {
            if self.selected.remove(&file) {
                if self.focused == Some(file) {
                    self.set_focused(sorted, None);
                }
                self.end_shift_select();
            } else {
                self.selected.insert(file);
                // (the reference's default: ctrl+click doesn't focus)
                self.last_hit = Some(file);
                self.start_shift_select(file);
            }
        } else if let Some(start) = self.shift_start.filter(|_| shift) {
            let at = |f| sorted.iter().position(|&s| s == f);
            let (Some(start), Some(end)) = (at(start), at(file)) else {
                return;
            };
            let range: HashSet<HashId> = sorted[start.min(end)..=start.max(end)]
                .iter()
                .copied()
                .collect();
            let deselect: Vec<HashId> = self
                .shift_added
                .iter()
                .copied()
                .filter(|f| !range.contains(f))
                .collect();
            let select: Vec<HashId> = range
                .iter()
                .copied()
                .filter(|f| !self.selected.contains(f))
                .collect();
            for f in &deselect {
                self.shift_added.remove(f);
                self.selected.remove(f);
            }
            for &f in &select {
                self.shift_added.insert(f);
                self.selected.insert(f);
            }
            // (nor does shift+click)
            self.last_hit = Some(file);
        } else {
            if !self.selected.contains(&file) {
                self.selected.clear();
                self.selected.insert(file);
            }
            self.set_focused(sorted, Some(file));
            self.start_shift_select(file);
        }
    }

    /// Select every file (ctrl+A, `_Select` with all).
    pub fn select_all(&mut self, sorted: &[HashId]) {
        if self.focused.is_none() {
            self.end_shift_select();
        }
        self.selected = sorted.iter().copied().collect();
    }

    /// Select none (escape, `_Select` with nothing).
    pub fn select_none(&mut self, sorted: &[HashId]) {
        let moves_focus = self.focused.is_none_or(|f| self.selected.contains(&f));
        if moves_focus || self.shift_start.is_some_and(|f| self.selected.contains(&f)) {
            self.end_shift_select();
        }
        self.selected.clear();
        if moves_focus {
            self.set_focused(sorted, None);
        }
    }

    /// Move the focus (`_MoveThumbnailFocus`, `_ScrollHome`, `_ScrollEnd`),
    /// selecting as a click would (with shift, as a shift+click); in a grid
    /// `columns` wide showing `page_rows` rows. Says the file moved to, by
    /// index, for the grid to scroll to (with shift, it isn't focused).
    pub fn move_focus(
        &mut self,
        sorted: &[HashId],
        to: Move,
        shift: bool,
        columns: usize,
        page_rows: usize,
    ) -> Option<usize> {
        let last = sorted.len().checked_sub(1)?;
        let (rows, mut step) = match to {
            Move::Home | Move::End => {
                let index = if to == Move::Home { 0 } else { last };
                self.hit(sorted, Some(sorted[index]), false, shift);
                return Some(index);
            }
            Move::Left => (0, -1),
            Move::Right => (0, 1),
            Move::Up => (-1, 0),
            Move::Down => (1, 0),
            Move::PageUp => (-(page_rows.max(1) as isize), 0),
            Move::PageDown => (page_rows.max(1) as isize, 0),
        };
        let from = if let Some(file) = self.last_hit {
            file
        } else if let Some(file) = self.next_best {
            // (as if the focus were between it and the next)
            if step == -1 {
                step = 0;
            }
            file
        } else {
            sorted[0]
        };
        let Some(current) = sorted.iter().position(|&f| f == from) else {
            self.set_focused(sorted, None);
            return None;
        };
        let to = (current as isize + step + columns as isize * rows).clamp(0, last as isize);
        let to = to as usize;
        self.hit(sorted, Some(sorted[to]), false, shift);
        Some(to)
    }

    /// `files` are leaving the page (whose files are `sorted`, them
    /// still among them).
    pub fn remove(&mut self, sorted: &[HashId], files: &[HashId]) {
        if self.focused.is_some_and(|f| files.contains(&f)) {
            self.set_focused(sorted, None);
        }
        for f in files {
            self.selected.remove(f);
        }
        self.end_shift_select();
    }

    /// A new search: nothing selected or focused.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `_SetFocusedMedia`: and where the keyboard would move from were it
    /// gone, the file or, if selected, the first unselected one before it.
    fn set_focused(&mut self, sorted: &[HashId], file: Option<HashId>) {
        if file == self.focused {
            return;
        }
        self.next_best = None;
        for candidate in [file, self.focused].into_iter().flatten() {
            let Some(mut i) = sorted.iter().position(|&f| f == candidate) else {
                continue;
            };
            let mut next_best = Some(candidate);
            while next_best.is_some_and(|f| self.selected.contains(&f)) {
                if i == 0 {
                    next_best = None;
                    break;
                }
                i -= 1;
                next_best = Some(sorted[i]);
            }
            if next_best.is_some() {
                self.next_best = next_best;
                break;
            }
        }
        self.focused = file;
        self.last_hit = file;
    }

    fn start_shift_select(&mut self, file: HashId) {
        self.shift_start = Some(file);
        self.shift_added.clear();
    }

    fn end_shift_select(&mut self) {
        self.shift_start = None;
        self.shift_added.clear();
    }
}
