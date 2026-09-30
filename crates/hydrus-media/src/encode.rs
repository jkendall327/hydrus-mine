//! Encoding rendered images, as the reference's `/get_files/render` does
//! (`GenerateFileBytesForRenderAPI`, through OpenCV's `imencode`).

use crate::error::{MediaError, Result};
use crate::imaging::Raster;

/// What to encode a render as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderFormat {
    Png,
    Jpeg,
    Webp,
}

/// Encode `image`. `quality` means what OpenCV takes it to: the PNG
/// compression level (0-9), the JPEG quality (0-100, progressive), or the
/// WebP quality (over 100 is lossless).
///
/// Grey images are encoded as RGB, and JPEG drops an alpha channel, as
/// OpenCV does. WebP is always lossless here: only a lossless encoder is
/// available.
pub fn encode_render(image: &Raster, format: RenderFormat, quality: i64) -> Result<Vec<u8>> {
    let (width, height) = (image.width(), image.height());
    let rgb_or_rgba = match image.channels() {
        1 | 2 => expand_grey(image),
        _ => image.clone(),
    };
    let alpha = rgb_or_rgba.channels() == 4;
    match format {
        RenderFormat::Png => {
            let mut out = Vec::new();
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(if alpha {
                png::ColorType::Rgba
            } else {
                png::ColorType::Rgb
            });
            encoder.set_depth(png::BitDepth::Eight);
            let level = quality.clamp(0, 9) as u8;
            encoder.set_deflate_compression(if level == 0 {
                png::DeflateCompression::NoCompression
            } else {
                png::DeflateCompression::Level(level)
            });
            let mut writer = encoder.write_header().map_err(encode_error)?;
            writer
                .write_image_data(rgb_or_rgba.data())
                .map_err(encode_error)?;
            writer.finish().map_err(encode_error)?;
            Ok(out)
        }
        RenderFormat::Jpeg => {
            let rgb = if alpha {
                drop_alpha(&rgb_or_rgba)
            } else {
                rgb_or_rgba
            };
            let mut compressor = turbojpeg::Compressor::new().map_err(encode_error)?;
            // libjpeg treats 0 as 1
            compressor
                .set_quality(quality.clamp(1, 100) as i32)
                .map_err(encode_error)?;
            compressor
                .set_subsamp(turbojpeg::Subsamp::Sub2x2)
                .map_err(encode_error)?;
            compressor.set_progressive(true).map_err(encode_error)?;
            let image = turbojpeg::Image {
                pixels: rgb.data(),
                width: width as usize,
                pitch: width as usize * 3,
                height: height as usize,
                format: turbojpeg::PixelFormat::RGB,
            };
            compressor.compress_to_vec(image).map_err(encode_error)
        }
        RenderFormat::Webp => {
            let mut out = Vec::new();
            let colour = if alpha {
                image_webp::ColorType::Rgba8
            } else {
                image_webp::ColorType::Rgb8
            };
            image_webp::WebPEncoder::new(&mut out)
                .encode(rgb_or_rgba.data(), width, height, colour)
                .map_err(encode_error)?;
            Ok(out)
        }
    }
}

fn encode_error(e: impl std::fmt::Display) -> MediaError {
    MediaError::damaged(format!("Image failed to encode! {e}"))
}

/// Grey (with or without alpha) to RGB, as `cv2.COLOR_GRAY2RGB` (which
/// ignores alpha in a two-channel image, as the reference never has one).
fn expand_grey(image: &Raster) -> Raster {
    let step = usize::from(image.channels());
    let data: Vec<u8> = image
        .data()
        .chunks_exact(step)
        .flat_map(|p| [p[0], p[0], p[0]])
        .collect();
    Raster::new(image.width(), image.height(), 3, data).unwrap_or_else(|_| image.clone())
}

fn drop_alpha(image: &Raster) -> Raster {
    let data: Vec<u8> = image
        .data()
        .chunks_exact(4)
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    Raster::new(image.width(), image.height(), 3, data).unwrap_or_else(|_| image.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(channels: u8) -> Raster {
        let (w, h) = (9u32, 7u32);
        let data = (0..w * h * u32::from(channels))
            .map(|i| (i * 7 % 256) as u8)
            .collect();
        Raster::new(w, h, channels, data).unwrap()
    }

    #[test]
    fn png_round_trips_exactly() {
        for channels in [1, 3, 4] {
            let image = gradient(channels);
            let png = encode_render(&image, RenderFormat::Png, 1).unwrap();
            let decoded = crate::decode_image(&png).unwrap();
            let expected = if channels == 1 {
                expand_grey(&image)
            } else {
                image
            };
            assert_eq!(decoded.data(), expected.data(), "{channels} channels");
        }
    }

    #[test]
    fn webp_is_lossless() {
        let image = gradient(4);
        let webp = encode_render(&image, RenderFormat::Webp, 80).unwrap();
        assert_eq!(crate::decode_image(&webp).unwrap().data(), image.data());
    }

    #[test]
    fn jpeg_drops_alpha() {
        let jpeg = encode_render(&gradient(4), RenderFormat::Jpeg, 80).unwrap();
        let header = turbojpeg::read_header(&jpeg).unwrap();
        assert!(header.is_progressive);
        assert_eq!((header.width, header.height), (9, 7));
    }
}
