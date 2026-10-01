//! Thumbnail geometry.

use serde::{Deserialize, Serialize};

/// How thumbnails fit their bounding box. Codes match the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThumbnailScale {
    /// Shrink to fit; never enlarge.
    #[default]
    DownOnly = 0,
    /// Shrink or enlarge to fit.
    ToFit = 1,
    /// Fill the box, cropping the overflow.
    ToFill = 2,
}

/// The thumbnail settings that decide thumbnail sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThumbnailSettings {
    pub bounding_width: u32,
    pub bounding_height: u32,
    pub scale: ThumbnailScale,
    /// Device pixel ratio as a percentage (100 = no scaling).
    pub dpr_percent: u32,
    /// How far into a video its thumbnail frame is, as a percentage
    /// (`video_thumbnail_percentage_in`).
    #[serde(default = "default_video_percentage_in")]
    pub video_percentage_in: u32,
}

fn default_video_percentage_in() -> u32 {
    35
}

impl Default for ThumbnailSettings {
    fn default() -> Self {
        Self {
            bounding_width: 150,
            bounding_height: 125,
            scale: ThumbnailScale::DownOnly,
            dpr_percent: 100,
            video_percentage_in: default_video_percentage_in(),
        }
    }
}

impl ThumbnailSettings {
    /// The size of the thumbnail for an image of `width` x `height`, exactly
    /// as the reference computes it (including its truncating arithmetic).
    pub fn resolution(&self, width: Option<u32>, height: Option<u32>) -> (u32, u32) {
        let mut bw = f64::from(self.bounding_width);
        let mut bh = f64::from(self.bounding_height);
        if self.dpr_percent != 100 {
            let dpr = f64::from(self.dpr_percent) / 100.0;
            bh = (bh * dpr).trunc();
            bw = (bw * dpr).trunc();
        }
        // unknown or zero sizes (e.g. some svgs) get a square thumbnail
        let (iw, ih) = match (width, height) {
            (Some(w), Some(h)) if w > 0 && h > 0 => (f64::from(w), f64::from(h)),
            _ => (bw, bw),
        };
        if self.scale == ThumbnailScale::DownOnly && bw >= iw && bh >= ih {
            return (iw as u32, ih as u32);
        }
        let image_ratio = iw / ih;
        let width_ratio = iw / bw;
        let height_ratio = ih / bh;
        let (mut tw, mut th) = (bw, bh);
        match self.scale {
            ThumbnailScale::DownOnly | ThumbnailScale::ToFit => {
                if height_ratio > width_ratio {
                    tw = iw / height_ratio;
                } else if width_ratio > height_ratio {
                    th = ih / width_ratio;
                }
            }
            ThumbnailScale::ToFill => {
                if height_ratio > width_ratio {
                    th = bw * (1.0 / image_ratio).min(5.0);
                } else if width_ratio > height_ratio {
                    tw = bh * image_ratio.min(5.0);
                }
            }
        }
        ((tw.trunc() as u32).max(1), (th.trunc() as u32).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_recorded_thumbnail_sizes() {
        let s = ThumbnailSettings::default();
        assert_eq!(s.resolution(Some(64), Some(48)), (64, 48));
        assert_eq!(s.resolution(Some(320), Some(240)), (150, 112));
        assert_eq!(s.resolution(Some(300), Some(200)), (150, 100));
        assert_eq!(s.resolution(Some(128), Some(96)), (128, 96));
        assert_eq!(s.resolution(Some(200), Some(1000)), (25, 125));
        // unknown sizes become bounding_width squared, then shrink to fit
        assert_eq!(s.resolution(None, None), (125, 125));
    }
}
