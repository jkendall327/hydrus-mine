//! Live page-local splitter state; global defaults are persisted separately.
use hydrus_store::page_layout::PageLayout;

/// A finite Pages > sidebar command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Toggle,
    SaveOnExit,
    SaveNow,
    RestoreAll,
}
/// Geometry retained by one live page, including closed pages awaiting reopening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub hpos: i64,
    pub vpos: i64,
    pub sidebar_hidden: bool,
    pub preview_hidden: bool,
}
impl Layout {
    /// Construct/reset from the current global defaults, as SetSplitterPositions.
    pub fn saved(value: &PageLayout) -> Self {
        Self {
            hpos: value.hpos,
            vpos: value.vpos,
            sidebar_hidden: false,
            preview_hidden: value.hide_preview,
        }
    }
    /// Resolve signed positions, retaining Qt's horizontal-total quirk for positive vpos.
    pub fn dimensions(self, width: i32, height: i32) -> (i32, i32) {
        let width = i64::from(width.max(0));
        let height = i64::from(height.max(0));
        let sidebar = if self.sidebar_hidden {
            0
        } else if self.hpos < 0 {
            width.saturating_add(self.hpos)
        } else {
            self.hpos
        };
        let preview = if self.preview_hidden {
            0
        } else if self.vpos < 0 {
            self.vpos.saturating_neg()
        } else {
            width.saturating_sub(self.vpos)
        };
        (
            sidebar.clamp(0, width.saturating_sub(80).max(0)) as i32,
            preview.clamp(0, height.saturating_sub(80).max(0)) as i32,
        )
    }
    /// Save hidden sidebar zero and keep the prior preview default while hidden.
    pub fn positions(self, actual: (i32, i32), saved: &PageLayout) -> (i64, i64) {
        (
            i64::from(actual.0),
            if actual.1 == 0 {
                if self.sidebar_hidden && !self.preview_hidden && self.vpos < 0 {
                    self.vpos
                } else {
                    saved.vpos
                }
            } else {
                -i64::from(actual.1)
            },
        )
    }
    /// Hiding clears focus; revealing resets to globally saved sizes.
    pub fn toggle(&mut self, saved: &PageLayout) {
        if self.sidebar_hidden {
            *self = Self::saved(saved);
        } else {
            self.sidebar_hidden = true;
        }
    }
    /// Pointer resizing affects only this page; zero collapses the selected pane.
    pub fn resize(&mut self, preview: bool, pixels: i32) {
        let pixels = i64::from(pixels.max(0));
        if preview {
            self.vpos = -pixels;
            self.preview_hidden = pixels == 0;
        } else {
            self.hpos = pixels;
            self.sidebar_hidden = pixels == 0;
        }
    }
}
