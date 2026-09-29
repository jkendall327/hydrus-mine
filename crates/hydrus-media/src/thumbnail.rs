//! Thumbnail geometry, fallback icons and encoding.
//!
//! The dimensions follow `HydrusImageHandling.GetThumbnailResolution`
//! exactly (thumbnail sizes are visible in the UI and API). Pixels come from
//! the same decode path as everything else and a faithful port of the
//! reference's resampling choices, but are not guaranteed to be identical.

use hydrus_core::Mime;

use crate::error::{MediaError, Result};
use crate::imaging::Raster;
use crate::imaging::cv::{self, Interpolation};
use crate::imaging::decode;
use crate::mimes;

/// How thumbnails fit their bounding box (`THUMBNAIL_SCALE_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailScale {
    /// Shrink to fit, never enlarge (the default).
    DownOnly = 0,
    /// Shrink or enlarge to fit.
    ToFit = 1,
    /// Fill the box, letting one side overflow (up to 5x).
    ToFill = 2,
}

impl ThumbnailScale {
    /// From the options value the reference stores.
    pub fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => ThumbnailScale::DownOnly,
            1 => ThumbnailScale::ToFit,
            2 => ThumbnailScale::ToFill,
            _ => return None,
        })
    }
}

/// The client's thumbnail options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailSpec {
    /// `options['thumbnail_dimensions']`, default 150x125.
    pub bounding: (u32, u32),
    /// `thumbnail_scale_type`.
    pub scale: ThumbnailScale,
    /// `thumbnail_dpr_percent` (device pixel ratio), default 100.
    pub dpr_percent: u32,
    /// `video_thumbnail_percentage_in`: how far into a video to grab the frame.
    pub video_percentage_in: u32,
}

impl Default for ThumbnailSpec {
    fn default() -> Self {
        Self {
            bounding: (150, 125),
            scale: ThumbnailScale::DownOnly,
            dpr_percent: 100,
            video_percentage_in: 35,
        }
    }
}

/// How the thumbnail bytes are encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailFormat {
    /// Quality 92, for opaque thumbnails.
    Jpeg,
    /// For thumbnails with (useful) transparency.
    Png,
}

/// A generated thumbnail.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    /// The pixels before encoding (what the blurhash is computed from).
    pub pixels: Raster,
    /// The encoded file.
    pub bytes: Vec<u8>,
    pub format: ThumbnailFormat,
    /// Whether this is a generic file-type icon rather than a render of the file.
    pub is_default: bool,
}

/// `GetThumbnailResolution`. `None` only for degenerate bounding boxes that
/// make the reference divide by zero.
pub fn thumbnail_resolution(
    width: Option<u32>,
    height: Option<u32>,
    spec: &ThumbnailSpec,
) -> Option<(u32, u32)> {
    let (mut bw, mut bh) = (f64::from(spec.bounding.0), f64::from(spec.bounding.1));
    if spec.dpr_percent != 100 {
        let dpr = f64::from(spec.dpr_percent) / 100.0;
        bh = (bh * dpr).trunc();
        bw = (bw * dpr).trunc();
    }
    let (im_w, im_h) = match (width, height) {
        (Some(w), Some(h)) if w != 0 && h != 0 => (f64::from(w), f64::from(h)),
        _ => (bw, bw),
    };
    if spec.scale == ThumbnailScale::DownOnly && bw >= im_w && bh >= im_h {
        return Some((im_w as u32, im_h as u32));
    }
    if bw == 0.0 || bh == 0.0 || im_h == 0.0 {
        return None;
    }
    let image_ratio = im_w / im_h;
    let width_ratio = im_w / bw;
    let height_ratio = im_h / bh;
    let wider = width_ratio > height_ratio;
    let taller = height_ratio > width_ratio;
    let (mut tw, mut th) = (bw, bh);
    match spec.scale {
        ThumbnailScale::DownOnly | ThumbnailScale::ToFit => {
            if taller {
                tw = im_w / height_ratio;
            } else if wider {
                th = im_h / width_ratio;
            }
        }
        ThumbnailScale::ToFill => {
            if taller {
                th = bw * 5.0f64.min(1.0 / image_ratio);
            } else if wider {
                tw = bh * 5.0f64.min(image_ratio);
            }
        }
    }
    let clamp = |v: f64| (v.trunc() as i64).max(1) as u32;
    Some((clamp(tw), clamp(th)))
}

/// `ResizeNumPyImage`, including its interpolation choice. (That choice
/// compares width against height; it only affects pixels, never sizes.)
pub(crate) fn resize(image: &Raster, target: (u32, u32), forced: Option<Interpolation>) -> Raster {
    let (tw, th) = target;
    let (iw, ih) = (image.width(), image.height());
    if tw == iw && th == tw {
        return image.clone();
    }
    let interpolation = forced.unwrap_or(if tw > ih || th > iw {
        Interpolation::Lanczos4
    } else {
        Interpolation::Area
    });
    cv::resize(image, tw, th, interpolation)
}

macro_rules! icon {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../static/",
            $name,
            ".png"
        ))
    };
}

/// The static icon the reference uses when it cannot render a file
/// (`mimes_to_default_thumbnail_paths`).
fn default_icon(mime: Mime) -> &'static [u8] {
    match mime {
        Mime::ApplicationUnknown => icon!("hydrus"),
        Mime::ApplicationPdf => icon!("pdf"),
        Mime::ApplicationDocx => icon!("docx"),
        Mime::ApplicationXlsx => icon!("xlsx"),
        Mime::ApplicationPptx => icon!("pptx"),
        Mime::ApplicationDoc => icon!("doc"),
        Mime::ApplicationXls => icon!("xls"),
        Mime::ApplicationPpt => icon!("ppt"),
        Mime::ApplicationEpub => icon!("epub"),
        Mime::ApplicationDjvu => icon!("djvu"),
        Mime::ApplicationPsd => icon!("psd"),
        Mime::ApplicationClip => icon!("clip"),
        Mime::ApplicationSai2 => icon!("sai"),
        Mime::ApplicationKrita => icon!("krita"),
        Mime::ImageOpenraster => icon!("image"),
        Mime::ApplicationPaintDotNet => icon!("paintnet"),
        Mime::ApplicationFlash => icon!("flash"),
        Mime::ApplicationXcf => icon!("xcf"),
        Mime::ApplicationProcreate => icon!("procreate"),
        Mime::ApplicationRtf => icon!("rtf"),
        Mime::ImageSvg => icon!("svg"),
        m if mimes::is_image(m) => icon!("image"),
        m if mimes::ARCHIVES.contains(&m) => icon!("zip"),
        m if mimes::is_audio(m) => icon!("audio"),
        m if mimes::is_video(m) || mimes::is_animation(m) => icon!("video"),
        _ => icon!("hydrus"),
    }
}

/// `GenerateDefaultThumbnail`: the type's icon padded into the target box
/// (`ImageOps.pad`), keeping its alpha channel.
pub(crate) fn default_thumbnail(mime: Mime, target: (u32, u32)) -> Result<Raster> {
    let icon = decode::open(default_icon(mime))?
        .image
        .normalise()?
        .into_raster(false)?;
    let (tw, th) = target;
    let (iw, ih) = (icon.width(), icon.height());
    // ImageOps.contain
    let (mut cw, mut ch) = (tw, th);
    let im_ratio = f64::from(iw) / f64::from(ih);
    let dest_ratio = f64::from(tw) / f64::from(th);
    #[allow(clippy::float_cmp)]
    if im_ratio != dest_ratio {
        if im_ratio > dest_ratio {
            ch = (f64::from(ih) / f64::from(iw) * f64::from(tw)).round_ties_even() as u32;
        } else {
            cw = (f64::from(iw) / f64::from(ih) * f64::from(th)).round_ties_even() as u32;
        }
    }
    let (cw, ch) = (cw.max(1), ch.max(1));
    let resized = cv::resize(&icon, cw, ch, Interpolation::Lanczos4);
    if (cw, ch) == (tw, th) {
        return Ok(resized);
    }
    let c = usize::from(icon.channels());
    let mut out = vec![0u8; tw as usize * th as usize * c];
    let (x0, y0) = if cw == tw {
        (
            0,
            (f64::from(th - ch.min(th)) * 0.5).round_ties_even() as usize,
        )
    } else {
        (
            (f64::from(tw - cw.min(tw)) * 0.5).round_ties_even() as usize,
            0,
        )
    };
    for y in 0..ch.min(th) as usize {
        for x in 0..cw.min(tw) as usize {
            let (dx, dy) = (x + x0, y + y0);
            if dx >= tw as usize || dy >= th as usize {
                continue;
            }
            let s = (y * cw as usize + x) * c;
            let d = (dy * tw as usize + dx) * c;
            out[d..d + c].copy_from_slice(&resized.data()[s..s + c]);
        }
    }
    Raster::new(tw, th, icon.channels(), out)
}

/// `GenerateThumbnailBytesFromNumPy`: PNG if there is an alpha channel, else JPEG q92.
pub(crate) fn encode(pixels: &Raster) -> Result<(ThumbnailFormat, Vec<u8>)> {
    let rgb;
    let pixels = if pixels.channels() < 3 {
        let data = pixels
            .data()
            .chunks_exact(usize::from(pixels.channels()))
            .flat_map(|p| [p[0], p[0], p[0]])
            .collect();
        rgb = Raster::new(pixels.width(), pixels.height(), 3, data)?;
        &rgb
    } else {
        pixels
    };
    if pixels.channels() == 4 {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, pixels.width(), pixels.height());
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_compression(png::Compression::High);
            let mut writer = enc
                .write_header()
                .map_err(|e| MediaError::damaged(format!("thumbnail encoding failed: {e}")))?;
            writer
                .write_image_data(pixels.data())
                .map_err(|e| MediaError::damaged(format!("thumbnail encoding failed: {e}")))?;
        }
        Ok((ThumbnailFormat::Png, out))
    } else {
        let image = turbojpeg::Image {
            pixels: pixels.data(),
            width: pixels.width() as usize,
            pitch: pixels.width() as usize * 3,
            height: pixels.height() as usize,
            format: turbojpeg::PixelFormat::RGB,
        };
        let jpeg = turbojpeg::compress(image, 92, turbojpeg::Subsamp::Sub2x2)
            .map_err(|e| MediaError::damaged(format!("thumbnail encoding failed: {e}")))?;
        Ok((ThumbnailFormat::Jpeg, jpeg.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_examples() {
        let spec = ThumbnailSpec::default();
        assert_eq!(
            thumbnail_resolution(Some(311), Some(237), &spec),
            Some((150, 114))
        );
        assert_eq!(
            thumbnail_resolution(Some(40), Some(900), &spec),
            Some((5, 125))
        );
        assert_eq!(
            thumbnail_resolution(Some(100), Some(80), &spec),
            Some((100, 80))
        );
        // unknown sizes are treated as a bounding-width square
        assert_eq!(thumbnail_resolution(None, None, &spec), Some((125, 125)));
    }

    #[test]
    fn default_thumbnails_have_the_target_size() {
        for target in [(150, 125), (125, 125), (1, 40), (300, 2)] {
            let t = default_thumbnail(Mime::AudioMp3, target).unwrap();
            assert_eq!((t.width(), t.height()), target);
        }
    }
}
