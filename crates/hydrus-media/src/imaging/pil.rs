//! A model of the parts of Pillow the reference relies on: image modes,
//! `convert()` to RGB/RGBA, `transpose()` and the `GeneratePILImage` /
//! `DequantizePILImage` normalisation pipeline.

use super::exif::ExifSummary;
use super::{Raster, icc};
use crate::error::{MediaError, Result};

/// Pillow image modes we meet in practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Bilevel, stored as 0/255 bytes.
    One,
    L,
    LA,
    /// Palette indices (see [`PilImage::palette`]).
    P,
    Rgb,
    Rgba,
    /// Stored as Pillow holds it (already inverted from Adobe JPEG conventions).
    Cmyk,
    /// 16-bit greyscale (`I;16`).
    I16,
    /// 32-bit signed greyscale (`I`).
    I32,
    /// 32-bit float greyscale (`F`).
    F32,
}

impl Mode {
    pub(crate) fn channels(self) -> usize {
        match self {
            Mode::One | Mode::L | Mode::P | Mode::I16 | Mode::I32 | Mode::F32 => 1,
            Mode::LA => 2,
            Mode::Rgb => 3,
            Mode::Rgba | Mode::Cmyk => 4,
        }
    }
}

/// Pixel storage by bit depth.
#[derive(Debug, Clone)]
pub(crate) enum Pixels {
    U8(Vec<u8>),
    U16(Vec<u16>),
    I32(Vec<i32>),
    F32(Vec<f32>),
}

impl Pixels {
    fn len(&self) -> usize {
        match self {
            Pixels::U8(v) => v.len(),
            Pixels::U16(v) => v.len(),
            Pixels::I32(v) => v.len(),
            Pixels::F32(v) => v.len(),
        }
    }
}

/// Pillow's `info['transparency']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Transparency {
    /// P mode: this palette index is fully transparent.
    Index(u8),
    /// P mode: alpha per palette index (missing entries are opaque).
    Alphas(Vec<u8>),
    /// L/1/I;16 or RGB colour key; Pillow's RGB conversion ignores these.
    ColourKey,
}

/// Which Pillow plugin opened the image (`pil_image.format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Jpeg,
    Png,
    Gif,
    Webp,
    Bmp,
    Ico,
    Tiff,
    Qoi,
}

/// PNG colour metadata Pillow exposes in `info`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PngColour {
    pub srgb: bool,
    pub gamma: Option<f64>,
    pub chromaticity: Option<[f64; 8]>,
}

/// A decoded image as Pillow would hold it after `Image.open` + `load`.
#[derive(Debug, Clone)]
pub(crate) struct PilImage {
    pub mode: Mode,
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
    /// 256 RGB entries for P mode (missing entries are black, as in Pillow).
    pub palette: Vec<[u8; 3]>,
    pub transparency: Option<Transparency>,
    /// `None` for images built from arrays (no format-specific handling).
    pub format: Option<Format>,
    pub icc_profile: Option<Vec<u8>>,
    pub exif: ExifSummary,
    pub png: PngColour,
}

/// PIL `Image.Transpose` operations used by `RotateEXIFPILImage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransposeOp {
    FlipLeftRight,
    FlipTopBottom,
    /// Counter-clockwise.
    Rotate90,
    Rotate180,
    /// Counter-clockwise (i.e. 90 clockwise).
    Rotate270,
    /// Mirror across the main diagonal.
    Transpose,
    /// Mirror across the anti-diagonal.
    Transverse,
}

#[allow(
    clippy::many_single_char_names,
    reason = "image geometry: w, h, x, y, c"
)]
fn transpose_vec<T: Copy>(
    data: &[T],
    w: usize,
    h: usize,
    c: usize,
    op: TransposeOp,
) -> (Vec<T>, usize, usize) {
    let (nw, nh) = match op {
        TransposeOp::Rotate90
        | TransposeOp::Rotate270
        | TransposeOp::Transpose
        | TransposeOp::Transverse => (h, w),
        _ => (w, h),
    };
    let mut out = Vec::with_capacity(data.len());
    for ny in 0..nh {
        for nx in 0..nw {
            let (x, y) = match op {
                TransposeOp::FlipLeftRight => (w - 1 - nx, ny),
                TransposeOp::FlipTopBottom => (nx, h - 1 - ny),
                TransposeOp::Rotate180 => (w - 1 - nx, h - 1 - ny),
                TransposeOp::Rotate90 => (w - 1 - ny, nx),
                TransposeOp::Rotate270 => (ny, h - 1 - nx),
                TransposeOp::Transpose => (ny, nx),
                TransposeOp::Transverse => (w - 1 - ny, h - 1 - nx),
            };
            let i = (y * w + x) * c;
            out.extend_from_slice(&data[i..i + c]);
        }
    }
    (out, nw, nh)
}

/// Pillow's `MULDIV255`.
fn muldiv255(a: i32, b: i32) -> i32 {
    let tmp = a * b + 128;
    ((tmp >> 8) + tmp) >> 8
}

impl PilImage {
    /// A plain 8-bit image with no format or metadata.
    pub(crate) fn from_u8(mode: Mode, width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        let img = Self {
            mode,
            width,
            height,
            pixels: Pixels::U8(data),
            palette: vec![[0; 3]; 256],
            transparency: None,
            format: None,
            icc_profile: None,
            exif: ExifSummary::default(),
            png: PngColour::default(),
        };
        img.check()?;
        Ok(img)
    }

    pub(crate) fn check(&self) -> Result<()> {
        let expected = self.width as usize * self.height as usize * self.mode.channels();
        if self.pixels.len() != expected {
            return Err(MediaError::damaged(
                "decoded pixel count does not match image size",
            ));
        }
        Ok(())
    }

    fn u8_data(&self) -> Result<&[u8]> {
        match &self.pixels {
            Pixels::U8(v) => Ok(v),
            _ => Err(MediaError::damaged("unexpected high bit depth image")),
        }
    }

    /// `pil_image.transpose(op)`.
    pub(crate) fn transpose(&mut self, op: TransposeOp) {
        let (w, h, c) = (
            self.width as usize,
            self.height as usize,
            self.mode.channels(),
        );
        let (nw, nh) = match &mut self.pixels {
            Pixels::U8(v) => {
                let (out, nw, nh) = transpose_vec(v, w, h, c, op);
                *v = out;
                (nw, nh)
            }
            Pixels::U16(v) => {
                let (out, nw, nh) = transpose_vec(v, w, h, c, op);
                *v = out;
                (nw, nh)
            }
            Pixels::I32(v) => {
                let (out, nw, nh) = transpose_vec(v, w, h, c, op);
                *v = out;
                (nw, nh)
            }
            Pixels::F32(v) => {
                let (out, nw, nh) = transpose_vec(v, w, h, c, op);
                *v = out;
                (nw, nh)
            }
        };
        self.width = nw as u32;
        self.height = nh as u32;
    }

    /// `HydrusImageColours.PILImageHasTransparency`.
    pub(crate) fn has_transparency(&self) -> bool {
        matches!(self.mode, Mode::LA | Mode::Rgba)
            || (self.mode == Mode::P
                && matches!(
                    self.transparency,
                    Some(Transparency::Index(_) | Transparency::Alphas(_))
                ))
    }

    /// `pil_image.convert('RGB')` or `convert('RGBA')`.
    pub(crate) fn convert_to_rgb(&self, with_alpha: bool) -> Result<PilImage> {
        let src = self.u8_data()?;
        let n = self.width as usize * self.height as usize;
        let oc = if with_alpha { 4 } else { 3 };
        let mut out = Vec::with_capacity(n * oc);
        let mut push = |r: u8, g: u8, b: u8, a: u8| {
            out.extend_from_slice(&[r, g, b]);
            if with_alpha {
                out.push(a);
            }
        };
        match self.mode {
            Mode::One | Mode::L => src.iter().for_each(|&v| push(v, v, v, 255)),
            Mode::LA => src
                .chunks_exact(2)
                .for_each(|p| push(p[0], p[0], p[0], p[1])),
            Mode::Rgb => src
                .chunks_exact(3)
                .for_each(|p| push(p[0], p[1], p[2], 255)),
            Mode::Rgba => src
                .chunks_exact(4)
                .for_each(|p| push(p[0], p[1], p[2], p[3])),
            Mode::Cmyk => src.chunks_exact(4).for_each(|p| {
                let nk = 255 - i32::from(p[3]);
                let ch = |v: u8| (nk - muldiv255(i32::from(v), nk)).clamp(0, 255) as u8;
                push(ch(p[0]), ch(p[1]), ch(p[2]), 255);
            }),
            Mode::P => {
                let mut alphas = [255u8; 256];
                if with_alpha {
                    match &self.transparency {
                        Some(Transparency::Index(i)) => alphas[usize::from(*i)] = 0,
                        Some(Transparency::Alphas(a)) => {
                            for (slot, v) in alphas.iter_mut().zip(a) {
                                *slot = *v;
                            }
                        }
                        _ => {}
                    }
                }
                for &i in src {
                    let [r, g, b] = self.palette[usize::from(i)];
                    push(r, g, b, alphas[usize::from(i)]);
                }
            }
            Mode::I16 | Mode::I32 | Mode::F32 => {
                return Err(MediaError::damaged(
                    "cannot convert high bit depth directly",
                ));
            }
        }
        let mut img = self.clone();
        img.mode = if with_alpha { Mode::Rgba } else { Mode::Rgb };
        img.pixels = Pixels::U8(out);
        img.transparency = None;
        Ok(img)
    }

    /// `RotateEXIFPILImage`: apply the EXIF orientation (never for PNGs).
    ///
    /// Pillow's TIFF plugin already applies `ImageOps.exif_transpose` when
    /// the pixels load, which the transpose here triggers, so oriented TIFFs
    /// end up transformed twice. We reproduce that.
    pub(crate) fn rotate_exif(&mut self) {
        if self.format == Some(Format::Png) {
            return;
        }
        if self.format == Some(Format::Tiff) {
            let op = match self.exif.orientation {
                Some(2) => Some(TransposeOp::FlipLeftRight),
                Some(3) => Some(TransposeOp::Rotate180),
                Some(4) => Some(TransposeOp::FlipTopBottom),
                Some(5) => Some(TransposeOp::Transpose),
                Some(6) => Some(TransposeOp::Rotate270),
                Some(7) => Some(TransposeOp::Transverse),
                Some(8) => Some(TransposeOp::Rotate90),
                _ => None,
            };
            if let Some(op) = op {
                self.transpose(op);
            }
        }
        let ops: &[TransposeOp] = match self.exif.orientation {
            Some(2) => &[TransposeOp::FlipLeftRight],
            Some(3) => &[TransposeOp::Rotate180],
            Some(4) => &[TransposeOp::FlipTopBottom],
            Some(5) => &[TransposeOp::FlipLeftRight, TransposeOp::Rotate90],
            Some(6) => &[TransposeOp::Rotate270],
            Some(7) => &[TransposeOp::FlipLeftRight, TransposeOp::Rotate270],
            Some(8) => &[TransposeOp::Rotate90],
            _ => &[],
        };
        for op in ops {
            self.transpose(*op);
        }
    }

    /// The `I`/`I;16`/`F` branch of `GeneratePILImage`: squash to 8-bit
    /// greyscale via numpy (`NormaliseNumPyImageToUInt8`), dropping all metadata.
    fn normalise_high_bit_depth(&self) -> Result<Option<PilImage>> {
        let data: Vec<u8> = match &self.pixels {
            Pixels::U8(_) => return Ok(None),
            Pixels::U16(v) => v.iter().map(|x| (x >> 8) as u8).collect(),
            Pixels::I32(v) => minmax_to_u8(v.iter().map(|&x| f64::from(x))),
            Pixels::F32(v) => minmax_to_u8(v.iter().map(|&x| f64::from(x))),
        };
        Ok(Some(PilImage::from_u8(
            Mode::L,
            self.width,
            self.height,
            data,
        )?))
    }

    fn is_png_with_gamma_and_chromaticity(&self) -> bool {
        if self.format != Some(Format::Png) || !matches!(self.mode, Mode::Rgb | Mode::Rgba) {
            return false;
        }
        let Some(chroma) = self.png.chromaticity else {
            return false;
        };
        if self.png.gamma == Some(0.0) {
            return false;
        }
        !chroma.contains(&0.0)
    }

    /// `DequantizePILImage`: colour-normalise to sRGB and convert to RGB/RGBA.
    fn dequantize(self) -> Result<PilImage> {
        let mut img = self;
        if let Some(profile) = img.icc_profile.clone().filter(|p| !p.is_empty()) {
            if let Some(converted) = icc::to_srgb(&img, &profile) {
                img = converted;
            }
        } else if img.format == Some(Format::Png) && img.png.srgb {
            // already sRGB
        } else if img.is_png_with_gamma_and_chromaticity() {
            let chroma = img.png.chromaticity.unwrap_or_default();
            let gamma = img.png.gamma.unwrap_or(0.45455);
            let profile = icc::profile_from_png_gamma_chromaticity(gamma, &chroma);
            if let Some(converted) = icc::to_srgb(&img, &profile) {
                img = converted;
            }
        }
        let want_alpha = img.has_transparency();
        let desired = if want_alpha { Mode::Rgba } else { Mode::Rgb };
        if img.mode != desired {
            img = img.convert_to_rgb(want_alpha)?;
        }
        Ok(img)
    }

    /// `GeneratePILImage(path)` from an opened image: rotate, then dequantize.
    pub(crate) fn normalise(mut self) -> Result<PilImage> {
        self.rotate_exif();
        let img = match self.normalise_high_bit_depth()? {
            Some(grey) => grey,
            None => self,
        };
        img.dequantize()
    }

    /// `GenerateNumPyImageFromPILImage`: the array, optionally without useless alpha.
    pub(crate) fn into_raster(self, strip_useless_alpha: bool) -> Result<Raster> {
        let channels = self.mode.channels() as u8;
        let Pixels::U8(data) = self.pixels else {
            return Err(MediaError::damaged("unexpected high bit depth image"));
        };
        let raster = Raster::new(self.width, self.height, channels, data)?;
        Ok(if strip_useless_alpha {
            raster.strip_useless_alpha()
        } else {
            raster
        })
    }
}

/// The float branch of `NormaliseNumPyImageToUInt8` (min-max stretch).
fn minmax_to_u8(values: impl Iterator<Item = f64> + Clone) -> Vec<u8> {
    let min = values.clone().fold(f64::INFINITY, f64::min);
    let max = values.clone().fold(f64::NEG_INFINITY, f64::max);
    let offset = if min > 0.0 { min } else { 0.0 };
    let range = (max - min) + 1.0;
    values
        .map(|v| {
            let v = v - offset;
            if range > 0.0 {
                let scaled = (v * (256.0 / range)).clamp(0.0, 255.0);
                scaled as u8
            } else {
                v as u8
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(w: u32, h: u32) -> PilImage {
        let data: Vec<u8> = (0..w * h).map(|i| i as u8).collect();
        PilImage::from_u8(Mode::L, w, h, data).unwrap()
    }

    fn pixels(img: &PilImage) -> Vec<u8> {
        match &img.pixels {
            Pixels::U8(v) => v.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn transposes_match_pil() {
        // 3x2 image:  0 1 2 / 3 4 5
        let mut img = numbered(3, 2);
        img.transpose(TransposeOp::Rotate90);
        // PIL ROTATE_90 is counter-clockwise: 2 5 / 1 4 / 0 3
        assert_eq!((img.width, img.height), (2, 3));
        assert_eq!(pixels(&img), vec![2, 5, 1, 4, 0, 3]);
        let mut img = numbered(3, 2);
        img.transpose(TransposeOp::Rotate270);
        assert_eq!(pixels(&img), vec![3, 0, 4, 1, 5, 2]);
        let mut img = numbered(3, 2);
        img.transpose(TransposeOp::FlipLeftRight);
        assert_eq!(pixels(&img), vec![2, 1, 0, 5, 4, 3]);
        let mut img = numbered(3, 2);
        img.transpose(TransposeOp::Rotate180);
        assert_eq!(pixels(&img), vec![5, 4, 3, 2, 1, 0]);
    }

    #[test]
    fn cmyk_conversion_matches_pillow_formula() {
        let img = PilImage::from_u8(Mode::Cmyk, 1, 1, vec![0, 128, 255, 64]).unwrap();
        let rgb = img.convert_to_rgb(false).unwrap();
        // nk = 191; 191 - muldiv255(0|128|255, 191)
        assert_eq!(pixels(&rgb), vec![191, 95, 0]);
    }
}
