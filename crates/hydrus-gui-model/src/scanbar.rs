//! The scanbar under a file mpv plays (the reference's animation bar): how
//! far through it is, as a nub and as text (`12/240 - 0.480/9.600`, frames
//! for an animation of several), and where a click on it or a seek
//! shortcut goes. Plain Rust, tested directly.

use hydrus_core::numbers::human_int;
use hydrus_core::time::scanbar_timestamps;

/// The reference's default `animated_scanbar_height`.
pub const HEIGHT: f32 = 20.0;
/// Its `animated_scanbar_hide_height`, away from the pointer.
pub const HIDDEN_HEIGHT: f32 = 5.0;
/// Its `animated_scanbar_nub_width`.
pub const NUB_WIDTH: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scanbar {
    pub duration_ms: u64,
    pub num_frames: Option<u64>,
}

impl Scanbar {
    /// The scanbar of a file of this duration and frame count; none
    /// without a duration (`ShouldHaveAnimationBar`).
    pub fn new(duration_ms: Option<u64>, num_frames: Option<u64>) -> Option<Self> {
        let duration_ms = duration_ms.filter(|&d| d > 0)?;
        Some(Self {
            duration_ms,
            num_frames,
        })
    }

    fn frames(&self) -> Option<u64> {
        self.num_frames.filter(|&n| n > 1)
    }

    /// At `position_ms` (`GetAnimationBarStatus`): the nub's place along
    /// the bar (0 to 1), and the text.
    pub fn at(&self, position_ms: f64) -> (f32, String) {
        let duration = self.duration_ms as f64;
        let position = position_ms.max(0.0);
        let index = self.frames().map(|frames| {
            // (Python's round, halves to even)
            let index = (position / duration * frames as f64).round_ties_even() as u64;
            (index.min(frames - 1), frames)
        });
        let position = position.min(duration);
        let timestamps = scanbar_timestamps(position, self.duration_ms);
        match index {
            Some((index, frames)) => (
                index as f32 / (frames - 1) as f32,
                format!(
                    "{}/{} - {timestamps}",
                    human_int((index + 1).min(frames)),
                    human_int(frames)
                ),
            ),
            None => ((position / duration) as f32, timestamps),
        }
    }

    /// At frame `index`, `at_ms` in, of an animation the client plays
    /// itself (`Animation.GetAnimationBarStatus`): as [`Scanbar::at`], but
    /// by the frame shown.
    pub fn at_frame(&self, index: usize, at_ms: u64) -> (f32, String) {
        let timestamps = scanbar_timestamps(at_ms as f64, self.duration_ms);
        match self.frames() {
            Some(frames) => {
                let index = (index as u64).min(frames - 1);
                (
                    index as f32 / (frames - 1) as f32,
                    format!(
                        "{}/{} - {timestamps}",
                        human_int(index + 1),
                        human_int(frames)
                    ),
                )
            }
            None => ((at_ms as f64 / self.duration_ms as f64) as f32, timestamps),
        }
    }

    /// The frame a click (or drag) at `x` along a bar `width` wide goes to,
    /// for an animation the client plays itself.
    pub fn frame_at(&self, x: f32, width: f32) -> usize {
        let proportion = ((x - NUB_WIDTH / 2.0) / (width - NUB_WIDTH)).clamp(0.0, 1.0);
        let last = self.num_frames.unwrap_or(1).max(1) - 1;
        (f64::from(proportion) * last as f64 + 0.5) as usize
    }

    /// Where a click (or drag) at `x` along a bar `width` wide goes, in
    /// milliseconds (`_ScanToCurrentMousePos`).
    pub fn seek_to(&self, x: f32, width: f32) -> f64 {
        let proportion = ((x - NUB_WIDTH / 2.0) / (width - NUB_WIDTH)).clamp(0.0, 1.0);
        (f64::from(proportion) * self.duration_ms as f64).trunc()
    }

    /// Where seeking `step_ms` forwards (`direction` 1) or back (-1) from
    /// `position_ms` goes (`SeekDelta`): never before the start, and past
    /// the end, round to the start.
    pub fn seek_delta(&self, position_ms: f64, direction: i32, step_ms: u64) -> f64 {
        let to = (position_ms + f64::from(direction.signum()) * step_ms as f64).max(0.0);
        if to > self.duration_ms as f64 {
            0.0
        } else {
            to
        }
    }
}
