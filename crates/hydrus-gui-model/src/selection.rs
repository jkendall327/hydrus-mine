//! Which of a page's files are selected, and which is focused, as the
//! reference's thumbnail grid has them (v688's default grid,
//! `MediaResultsPanelThumbnailsGraphicsViewTest`): a click
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
    /// The file last focused, ctrl+clicked or shift+clicked: where shift
    /// and the keyboard move from.
    last_hit: Option<HashId>,
    /// The file focused before the focus went (the reference's
    /// `_previously_focused_media_when_nothing_now`), which the keyboard
    /// then selects; moved past files leaving the page.
    ghost: Option<HashId>,
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

    /// The last hit remains independent from the preview focus.
    pub fn last_hit(&self) -> Option<HashId> {
        self.last_hit
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
        self.select_only(sorted, sorted);
    }

    /// Select none (escape, `_Select` with nothing).
    pub fn select_none(&mut self, sorted: &[HashId]) {
        self.select_only(sorted, &[]);
    }

    /// Select just `files` (`_Select`): the focus goes if its file is no
    /// longer selected (a selected file is taken to be in view, so none is
    /// focused instead).
    pub fn select_only(&mut self, sorted: &[HashId], files: &[HashId]) {
        let matching: HashSet<HashId> = files.iter().copied().collect();
        let deselected = |f: HashId| self.selected.contains(&f) && !matching.contains(&f);
        let moves_focus = self.focused.is_none_or(deselected);
        if moves_focus || self.shift_start.is_some_and(deselected) {
            self.end_shift_select();
        }
        self.selected = matching;
        if moves_focus && self.selected.is_empty() {
            self.set_focused(sorted, None);
        }
    }

    /// Move the focus (the thumbnail shortcuts' `SIMPLE_MOVE_THUMBNAIL_FOCUS`,
    /// `_MoveThumbnailFocus`, `_ScrollHome`, `_ScrollEnd`), selecting as a
    /// click would (with shift, as a shift+click), in a grid `columns`
    /// wide showing `page_rows` rows: from the focused file (with shift,
    /// the one last hit); with none, the one focused before is selected
    /// again. Says the file moved to, by index, for the grid to scroll to
    /// (with shift, it isn't focused).
    pub fn move_focus(
        &mut self,
        sorted: &[HashId],
        to: Move,
        shift: bool,
        columns: usize,
        page_rows: usize,
    ) -> Option<usize> {
        self.move_focus_with_last_hit(sorted, to, shift, columns, page_rows, false)
    }

    /// The optional last-hit origin also applies to non-Shift movement keys.
    pub fn move_focus_with_last_hit(
        &mut self,
        sorted: &[HashId],
        to: Move,
        shift: bool,
        columns: usize,
        page_rows: usize,
        use_last_hit: bool,
    ) -> Option<usize> {
        let last = sorted.len().checked_sub(1)?;
        if let Move::Home | Move::End = to {
            let index = if to == Move::Home { 0 } else { last };
            self.hit(sorted, Some(sorted[index]), false, shift);
            return Some(index);
        }
        let at = |f: HashId| sorted.iter().position(|&s| s == f);
        // (`_MediaToUseWhenMovingFocus`, the reference's defaults; the arms
        // in its order of preference, so two alike stay apart)
        #[allow(clippy::match_same_arms)]
        let from = match (
            shift || use_last_hit,
            self.last_hit,
            self.focused,
            self.ghost,
        ) {
            (true, Some(hit), _, _) => hit,
            (_, _, Some(focused), _) => focused,
            (_, _, None, Some(ghost)) => {
                // (back where it was, so the keyboard shows where it is;
                // never a file no longer on the page, which the reference
                // would select)
                let index = at(ghost)?;
                self.hit(sorted, Some(ghost), false, shift);
                return Some(index);
            }
            (false, Some(hit), None, None) => hit,
            _ => sorted[0],
        };
        let current = at(from)? as isize;
        let (columns, page_rows) = (columns.max(1) as isize, page_rows.max(1) as isize);
        let to = match to {
            Move::Left => current - 1,
            Move::Right => current + 1,
            Move::Up => current - columns,
            Move::Down => current + columns,
            Move::PageUp => current - columns * page_rows,
            Move::PageDown => current + columns * page_rows,
            Move::Home | Move::End => unreachable!("moved above"),
        };
        let to = to.clamp(0, last as isize) as usize;
        self.hit(sorted, Some(sorted[to]), false, shift);
        Some(to)
    }

    /// `files` are leaving the page (whose files are `sorted`, them
    /// still among them; `_RemoveMediaDirectly`): the focus goes, and the
    /// place it was moves on past them (or, if that runs to the end, back
    /// from the end).
    pub fn remove(&mut self, sorted: &[HashId], files: &[HashId]) {
        if self.focused.is_some_and(|f| files.contains(&f)) {
            self.set_focused(sorted, None);
        }
        if let Some(ghost) = self.ghost
            && let Some(mut index) = sorted.iter().position(|&f| f == ghost)
        {
            let last = sorted.len() - 1;
            let mut candidate = ghost;
            while index < last && files.contains(&candidate) {
                index += 1;
                candidate = sorted[index];
            }
            if index == last {
                // (as the reference does, from the end, not from the ghost)
                candidate = ghost;
                while index > 0 && files.contains(&candidate) {
                    index -= 1;
                    candidate = sorted[index];
                }
            }
            self.ghost = Some(candidate);
        }
        for f in files {
            self.selected.remove(f);
        }
        let gone = |f: &Option<HashId>| f.is_some_and(|f| files.contains(&f));
        if gone(&self.last_hit) {
            self.last_hit = None;
        }
        self.end_shift_select();
    }

    /// The page's items regrouped (collected anew): each one kept as the
    /// item `to` gives for it (none: dropped).
    pub fn remap(&mut self, to: impl Fn(HashId) -> Option<HashId>) {
        self.selected = self.selected.iter().filter_map(|&f| to(f)).collect();
        self.focused = self.focused.and_then(&to);
        self.last_hit = self.last_hit.and_then(&to);
        self.ghost = self.ghost.and_then(&to);
        self.shift_start = self.shift_start.and_then(&to);
        self.shift_added = self.shift_added.iter().filter_map(|&f| to(f)).collect();
    }

    /// A new search: nothing selected or focused.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// `_SetFocusedMedia`: and when the focus goes, where it was.
    fn set_focused(&mut self, sorted: &[HashId], file: Option<HashId>) {
        if file == self.focused {
            return;
        }
        self.ghost = match (file, self.focused) {
            (None, Some(focused)) if sorted.contains(&focused) => Some(focused),
            _ => None,
        };
        self.focused = file;
        if file.is_some() {
            self.last_hit = file;
        }
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
