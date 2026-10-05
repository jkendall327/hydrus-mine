//! Owned whole-cell paint snapshots; no animation state lives in recycled delegates.
use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use hydrus_core::HashId;
use slint::{Brush, Color};

use crate::{Thumbnail, ThumbnailPaint};

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub fill: Brush,
    pub selected_fill: Brush,
    pub remote_fill: Brush,
    pub remote_selected_fill: Brush,
    pub border: Brush,
    pub selected_border: Brush,
    pub remote_border: Brush,
    pub remote_selected_border: Brush,
    pub window: Brush,
    pub text: Brush,
    pub grid: Brush,
    pub banners: [Color; 4],
}
impl Default for Palette {
    fn default() -> Self {
        Self {
            fill: Brush::default(),
            selected_fill: Brush::default(),
            remote_fill: Brush::default(),
            remote_selected_fill: Brush::default(),
            border: Brush::default(),
            selected_border: Brush::default(),
            remote_border: Brush::default(),
            remote_selected_border: Brush::default(),
            window: Brush::default(),
            text: Brush::default(),
            grid: Brush::default(),
            banners: [Color::default(); 4],
        }
    }
}
impl Palette {
    pub fn paint(&self, thumbnail: &Thumbnail, border: i32, local: bool) -> ThumbnailPaint {
        let (fill, edge) = match (local, thumbnail.selected) {
            (true, false) => (&self.fill, &self.border),
            (true, true) => (&self.selected_fill, &self.selected_border),
            (false, false) => (&self.remote_fill, &self.remote_border),
            (false, true) => (&self.remote_selected_fill, &self.remote_selected_border),
        };
        ThumbnailPaint {
            image: thumbnail.image.clone(),
            selected: thumbnail.selected,
            files: thumbnail.files.clone(),
            icons: thumbnail.icons.clone(),
            ratings: thumbnail.ratings.clone(),
            rating_boxes: thumbnail.rating_boxes.clone(),
            top: thumbnail.top.clone(),
            bottom: thumbnail.bottom.clone(),
            border: border as f32,
            fill: fill.clone(),
            border_brush: edge.clone(),
            window_brush: self.window.clone(),
            text_brush: self.text.clone(),
            banner_top_background: self.banners[0],
            banner_top_text: self.banners[1],
            banner_bottom_background: self.banners[2],
            banner_bottom_text: self.banners[3],
        }
    }
}

#[derive(Debug)]
struct Frame {
    current: ThumbnailPaint,
    previous: Option<ThumbnailPaint>,
    started: Duration,
    frames: u32,
    opacity: f32,
    index: usize,
    dirty: bool,
}
#[derive(Debug, Default)]
pub struct Paints {
    frames: HashMap<HashId, Frame>,
}
impl Paints {
    pub fn clear(&mut self) {
        self.frames.clear();
    }
    pub fn dirty_all(&mut self) {
        for frame in self.frames.values_mut() {
            frame.dirty = true;
        }
    }
    pub fn dirty(&mut self, id: HashId) {
        if let Some(frame) = self.frames.get_mut(&id) {
            frame.dirty = true;
        }
    }
    pub fn retain(&mut self, visible: &BTreeSet<HashId>) {
        self.frames.retain(|id, _| visible.contains(id));
    }
    pub fn decorate(
        &mut self,
        id: HashId,
        index: usize,
        paint: ThumbnailPaint,
        thumbnail: &mut Thumbnail,
        now: Duration,
        fade: bool,
        initial_fade: bool,
    ) {
        let ready = paint.image.size().width > 0;
        let frame = self.frames.entry(id).or_insert_with(|| Frame {
            current: ThumbnailPaint::default(),
            previous: None,
            started: now,
            frames: 0,
            opacity: 1.0,
            index,
            dirty: true,
        });
        frame.index = index;
        if frame.dirty && ready {
            let old_ready = frame.current.image.size().width > 0;
            let previous = if fade && (old_ready || initial_fade) {
                Some(frame.current.clone())
            } else {
                None
            };
            frame.current = paint;
            frame.previous = previous;
            frame.opacity = if frame.previous.is_some() { 0.0 } else { 1.0 };
            frame.started = now;
            frame.frames = 0;
            frame.dirty = false;
        }
        thumbnail.paint = frame.current.clone();
        thumbnail.previous = frame.previous.clone().unwrap_or_default();
        thumbnail.fade_opacity = frame.opacity;
    }
    /// Reference old painter accumulates DrawToPainter's outstanding frame
    /// count; its count is relative to start, not relative to the last draw.
    pub fn tick(&mut self, now: Duration, new_renderer: bool, fade: bool) -> BTreeSet<usize> {
        let mut changed = BTreeSet::new();
        for frame in self
            .frames
            .values_mut()
            .filter(|frame| frame.previous.is_some())
        {
            let elapsed = now.saturating_sub(frame.started).as_secs_f64();
            let opacity = if !fade {
                1.0
            } else if new_renderer {
                (elapsed / (13.0 / 60.0)).min(1.0) as f32
            } else {
                let outstanding = ((elapsed / (1.0 / 60.0)).floor() as u32).min(30 - frame.frames);
                frame.frames += outstanding;
                if frame.frames >= 30 {
                    1.0
                } else {
                    (1.0 - (1.0_f64 - 25.0 / 255.0).powi(frame.frames as i32)) as f32
                }
            };
            if opacity.to_bits() != frame.opacity.to_bits() {
                frame.opacity = opacity;
                changed.insert(frame.index);
            }
            if opacity >= 1.0 {
                frame.previous = None;
            }
        }
        changed
    }
}
