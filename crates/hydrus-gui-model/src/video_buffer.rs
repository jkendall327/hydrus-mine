//! The animation frame buffer, as the reference's `RasterContainerVideo`
//! keeps it: sized from "Memory for video buffer", two thirds of its frames
//! behind the one shown and a third ahead, so a clip that fits is decoded
//! once and loops without being decoded again; and which frame its render
//! thread decodes next.

use std::collections::BTreeMap;

/// The reference's default, 96 MB (`video_buffer_size`).
pub const DEFAULT_BYTES: u64 = 96 * 1024 * 1024;

/// The options row's estimate, `EventVideoBufferUpdate`: how many 720p
/// frames (RGB) the memory holds.
pub fn estimate(bytes: u64) -> String {
    format!(
        "(about {} frames of 720p video)",
        hydrus_core::numbers::human_int(bytes / (1280 * 720 * 3))
    )
}

/// How many frames are kept behind the one shown and ahead of it, for a
/// buffer of `bytes` at `resolution` (the size frames are decoded at): an
/// RGB frame each, but if the whole clip doesn't fit, no more than three
/// seconds' worth (and at least 48), as `RasterContainerVideo.__init__`
/// has it. A missing or zero duration is taken as a second, a missing or
/// zero frame count as one.
pub fn frames_kept(
    bytes: u64,
    resolution: (u32, u32),
    duration_ms: Option<u64>,
    num_frames: Option<u64>,
) -> (usize, usize) {
    let (x, y) = match resolution {
        (0, _) | (_, 0) => (100, 100),
        (x, y) => (u64::from(x), u64::from(y)),
    };
    let duration_ms = duration_ms.filter(|ms| *ms != 0).unwrap_or(1000);
    let num_frames = num_frames.filter(|n| *n != 0).unwrap_or(1);
    #[allow(clippy::cast_precision_loss)] // (milliseconds and frame counts)
    let average_ms = duration_ms as f64 / num_frames as f64;
    let mut length = bytes / (x * y * 3);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // (int( 3000 / average ))
    let streaming = 48.max((3000.0 / average_ms) as u64);
    if streaming < length && length < num_frames {
        length = streaming;
    }
    let frames = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
    (frames(length * 2 / 3), frames(length / 3))
}

/// `FrameIndexOutOfRange`: whether `index` is outside `start..=end`, a
/// range that may wrap round the end (when `end` is before `start`).
pub fn out_of_range(index: i64, start: i64, end: i64) -> bool {
    let (before_start, after_end) = (index < start, end < index);
    if start < end {
        before_start || after_end
    } else {
        after_end && before_start
    }
}

/// What the render thread does next: go to a frame first (`set_position`),
/// then decode the next one, keeping it as `index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Job {
    pub seek: Option<usize>,
    pub index: i64,
}

/// The frames decoded and the window of them to keep, with where the
/// render thread is.
#[derive(Debug)]
pub struct Buffer<T> {
    count: i64,
    backwards: i64,
    forwards: i64,
    start: i64,
    end: i64,
    last_rendered: i64,
    next_render: i64,
    ideal: i64,
    frames: BTreeMap<i64, T>,
}

impl<T> Buffer<T> {
    /// A buffer for `count` frames keeping `(backwards, forwards)` of them.
    pub fn new(count: usize, (backwards, forwards): (usize, usize)) -> Self {
        let int = |n: usize| i64::try_from(n).unwrap_or(i64::MAX / 4);
        Self {
            count: int(count),
            backwards: int(backwards),
            forwards: int(forwards),
            start: -1,
            end: -1,
            last_rendered: -1,
            next_render: -1,
            ideal: 0,
            frames: BTreeMap::new(),
        }
    }

    /// The frame decoded for `index`, if it is kept.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.frames.get(&i64::try_from(index).ok()?)
    }

    /// The indexes of the frames kept.
    pub fn held(&self) -> Vec<i64> {
        self.frames.keys().copied().collect()
    }

    /// The window kept, first and last.
    pub fn bounds(&self) -> (i64, i64) {
        (self.start, self.end)
    }

    /// Move the window to keep frame `next` and those round it
    /// (`GetReadyForFrame`): all of them if they fit; otherwise, remade
    /// round it if it is outside, or else moved on (never back), the start
    /// only once the frame it would start at is decoded.
    pub fn get_ready_for(&mut self, next: usize) {
        let Ok(next) = i64::try_from(next) else {
            return;
        };
        if self.count == 0 || out_of_range(next, 0, self.count - 1) {
            return;
        }
        self.ideal = next;
        if self.count > self.backwards + 1 + self.forwards {
            let outside = self.start == -1 || out_of_range(self.ideal, self.start, self.end);
            let ideal_start = 0.max(self.ideal - self.backwards);
            let ideal_end = (self.ideal + self.forwards) % self.count;
            if outside {
                self.start = ideal_start;
                self.end = ideal_end;
            } else {
                let shunt_start = !out_of_range(ideal_start, self.start, self.ideal);
                if shunt_start && self.frames.contains_key(&ideal_start) {
                    self.start = ideal_start;
                }
                if !out_of_range(ideal_end, self.end, self.start) {
                    self.end = ideal_end;
                }
            }
        } else {
            self.start = 0;
            self.end = self.count - 1;
        }
    }

    /// What the render thread does next, if anything (`THREADRender`'s
    /// loop): back to the window's start if it is decoding outside it or
    /// won't come to the frame wanted, then on until the window's end.
    pub fn next_job(&mut self) -> Option<Job> {
        let outside =
            out_of_range(self.next_render, self.start, self.end) && self.last_rendered != self.end;
        let soon = !out_of_range(self.next_render, self.start, self.ideal);
        let missing = !self.frames.contains_key(&self.ideal);
        let mut seek = None;
        if outside || (missing && !soon) {
            seek = usize::try_from(self.start).ok();
            self.last_rendered = -1;
            self.next_render = self.start;
        }
        (self.last_rendered != self.end).then_some(Job {
            seek,
            index: self.next_render,
        })
    }

    /// Frame `index` was decoded: keep it if it isn't already, dropping
    /// those outside the window. Says whether the render thread goes back
    /// to the first frame (from the last, the window not ending there).
    pub fn rendered(&mut self, index: i64, frame: T) -> bool {
        self.last_rendered = index;
        self.next_render = (self.next_render + 1) % self.count.max(1);
        let rewind = self.next_render == 0 && self.end != self.count - 1;
        if rewind {
            self.last_rendered = -1;
        }
        if let std::collections::btree_map::Entry::Vacant(slot) = self.frames.entry(index) {
            slot.insert(frame);
            let (start, end) = (self.start, self.end);
            self.frames.retain(|&i, _| !out_of_range(i, start, end));
        }
        rewind
    }
}
