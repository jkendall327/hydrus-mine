//! Decoding images exactly as the reference does.
//!
//! The reference loads every image through Pillow (`GenerateNumPyImage` with
//! `FORCE_PIL_ALWAYS`), applies EXIF rotation, ICC/PNG-gamma colour
//! normalisation to sRGB, converts to RGB/RGBA and drops "useless" alpha. The
//! resulting array feeds the pixel hash (sha256 of the raw bytes), the
//! perceptual hash and the thumbnail, so this module reproduces that pipeline
//! byte for byte:
//!
//! - [`pil`] models Pillow's modes, `convert` and `transpose`.
//! - [`decode`] reads each format into that model with the same decoders
//!   (libjpeg-turbo, LittleCMS) or bit-exact equivalents (PNG, WebP, GIF LZW).
//! - [`cv`] ports the OpenCV routines the reference calls (`resize` with
//!   `INTER_AREA`/`INTER_LINEAR`, `cvtColor`), including their fixed-point
//!   rounding, so perceptual hashes are bit-exact.

pub(crate) mod cv;
pub(crate) mod cvx;
pub(crate) mod decode;
pub(crate) mod exif;
pub(crate) mod icc;
pub(crate) mod pil;
pub(crate) mod resample;

use crate::error::{MediaError, Result};

/// A decoded image: `height` rows of `width` pixels of `channels` bytes
/// (1 grey, 2 grey+alpha, 3 RGB, 4 RGBA), i.e. a numpy `uint8` array of
/// shape `(height, width, channels)`.
#[derive(Clone, PartialEq, Eq)]
pub struct Raster {
    width: u32,
    height: u32,
    channels: u8,
    data: Vec<u8>,
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Raster({}x{}x{})",
            self.width, self.height, self.channels
        )
    }
}

impl Raster {
    /// Wrap pixel data, checking its length.
    pub fn new(width: u32, height: u32, channels: u8, data: Vec<u8>) -> Result<Self> {
        let expected = width as usize * height as usize * usize::from(channels);
        if !(1..=4).contains(&channels) || data.len() != expected {
            return Err(MediaError::damaged(format!(
                "pixel buffer is {} bytes, expected {expected} for {width}x{height}x{channels}",
                data.len()
            )));
        }
        Ok(Self {
            width,
            height,
            channels,
            data,
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Bytes per pixel.
    pub fn channels(&self) -> u8 {
        self.channels
    }

    /// Raw pixel bytes, row-major.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// The pixel bytes, without copying.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// Whether there is an alpha channel at all (LA or RGBA).
    pub fn has_alpha_channel(&self) -> bool {
        matches!(self.channels, 2 | 4)
    }

    fn alpha_values(&self) -> impl Iterator<Item = u8> + '_ {
        let c = usize::from(self.channels);
        self.data.iter().skip(c - 1).step_by(c).copied()
    }

    /// `NumPyImageHasHumanPerceptibleAlphaChannel`: whether transparency is
    /// visible, ignoring alpha channels that are all (or nearly all) opaque or
    /// all transparent.
    pub fn has_useful_alpha(&self) -> bool {
        self.has_alpha_channel() && alpha_is_useful(self.alpha_values(), self.width, self.height)
    }

    /// `StripOutAnyUselessAlphaChannel`.
    pub(crate) fn strip_useless_alpha(self) -> Self {
        if self.has_alpha_channel() && !self.has_useful_alpha() {
            self.drop_alpha()
        } else {
            self
        }
    }

    /// Remove the alpha channel (keeping grey or RGB).
    pub(crate) fn drop_alpha(self) -> Self {
        if !self.has_alpha_channel() {
            return self;
        }
        let c = usize::from(self.channels);
        let keep = c - 1;
        let data: Vec<u8> = self
            .data
            .chunks_exact(c)
            .flat_map(|px| px[..keep].iter().copied())
            .collect();
        Self {
            width: self.width,
            height: self.height,
            channels: self.channels - 1,
            data,
        }
    }
}

/// The alpha-plane test behind [`Raster::has_useful_alpha`]: enough pixels
/// (a circumference's worth, or 0.5% of the image) are neither nearly opaque
/// nor nearly clear.
pub(crate) fn alpha_is_useful<'a>(
    alpha: impl IntoIterator<Item = impl std::borrow::Borrow<u8> + 'a>,
    width: u32,
    height: u32,
) -> bool {
    let (w, h) = (u64::from(width), u64::from(height));
    let circumference = 2 * (w + h);
    let weight = ((w * h) / 200).max(1);
    let needed = circumference.min(weight);
    let mut not_opaque = 0u64;
    let mut not_clear = 0u64;
    for a in alpha {
        let a = *a.borrow();
        if a < 251 {
            not_opaque += 1;
        }
        if a > 4 {
            not_clear += 1;
        }
    }
    not_opaque >= needed && not_clear >= needed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(w: u32, h: u32, alpha: impl Fn(u32, u32) -> u8) -> Raster {
        let mut data = Vec::new();
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[1, 2, 3, alpha(x, y)]);
            }
        }
        Raster::new(w, h, 4, data).unwrap()
    }

    #[test]
    fn useful_alpha_needs_enough_interesting_pixels() {
        assert!(!rgba(100, 100, |_, _| 255).has_useful_alpha());
        assert!(!rgba(100, 100, |_, _| 0).has_useful_alpha());
        // 50 pixels of transparency: below the 100*100/200 = 50 threshold? exactly at it
        assert!(rgba(100, 100, |x, y| if y == 0 && x < 50 { 0 } else { 255 }).has_useful_alpha());
        assert!(!rgba(100, 100, |x, y| if y == 0 && x < 49 { 0 } else { 255 }).has_useful_alpha());
        let stripped = rgba(10, 10, |_, _| 254).strip_useless_alpha();
        assert_eq!(stripped.channels(), 3);
        assert_eq!(&stripped.data()[..3], &[1, 2, 3]);
    }
}
