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

/// What to encode an animation (a rendered ugoira) as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationFormat {
    Apng,
    Webp,
}

/// Encode frames, each shown for its duration in ms, as an animation that
/// loops forever, as Pillow saves the reference's ugoira renders
/// (`ConvertUgoiraToBytesForAPI`). The frames take the first's size and
/// colour type (RGB, or RGBA if it has alpha), and a frame the same as the
/// one before only lengthens it, as Pillow and libwebp do. WebP is lossless
/// here, whatever the quality (only a lossless encoder is available).
pub fn encode_animation(
    frames: &[Raster],
    durations_ms: &[u32],
    format: AnimationFormat,
) -> Result<Vec<u8>> {
    let Some(first) = frames.first() else {
        return Err(MediaError::damaged("The animation has no frames!"));
    };
    let (width, height) = (first.width(), first.height());
    let channels: u8 = if first.channels() == 4 || first.channels() == 2 {
        4
    } else {
        3
    };
    let mut merged: Vec<(Vec<u8>, u32)> = Vec::with_capacity(frames.len());
    for (i, frame) in frames.iter().enumerate() {
        let data = frame_data(frame, width, height, channels);
        // (a missing duration takes the default, rather than Pillow's error)
        let duration = durations_ms
            .get(i)
            .copied()
            .unwrap_or(crate::tools::UGOIRA_DEFAULT_FRAME_DURATION_MS);
        match merged.last_mut() {
            Some((previous, d)) if *previous == data => *d = d.saturating_add(duration),
            _ => merged.push((data, duration)),
        }
    }
    match format {
        AnimationFormat::Apng => encode_apng(&merged, width, height, channels),
        AnimationFormat::Webp => encode_animated_webp(&merged, width, height, channels),
    }
}

/// A frame's pixels at `width` x `height` with `channels` (3 or 4): grey
/// expanded, alpha added (opaque) or dropped, and anything outside the
/// first frame's size cropped or left transparent black.
fn frame_data(frame: &Raster, width: u32, height: u32, channels: u8) -> Vec<u8> {
    let frame = match frame.channels() {
        1 | 2 => expand_grey(frame),
        _ => frame.clone(),
    };
    let source = usize::from(frame.channels());
    let target = usize::from(channels);
    let mut out = vec![0u8; width as usize * height as usize * target];
    for y in 0..height.min(frame.height()) as usize {
        for x in 0..width.min(frame.width()) as usize {
            let from = (y * frame.width() as usize + x) * source;
            let to = (y * width as usize + x) * target;
            out[to..to + 3].copy_from_slice(&frame.data()[from..from + 3]);
            if target == 4 {
                out[to + 3] = if source == 4 {
                    frame.data()[from + 3]
                } else {
                    255
                };
            }
        }
    }
    out
}

fn encode_apng(
    frames: &[(Vec<u8>, u32)],
    width: u32,
    height: u32,
    channels: u8,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(if channels == 4 {
        png::ColorType::Rgba
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    let count = u32::try_from(frames.len()).map_err(encode_error)?;
    encoder.set_animated(count, 0).map_err(encode_error)?;
    let mut writer = encoder.write_header().map_err(encode_error)?;
    for (data, duration) in frames {
        // (a delay is a fraction of a second, in u16s)
        let (numerator, denominator) = match u16::try_from(*duration) {
            Ok(ms) => (ms, 1000),
            Err(_) => (u16::try_from(duration / 100).unwrap_or(u16::MAX), 10),
        };
        writer
            .set_frame_delay(numerator, denominator)
            .map_err(encode_error)?;
        writer.write_image_data(data).map_err(encode_error)?;
    }
    writer.finish().map_err(encode_error)?;
    Ok(out)
}

/// An animated WebP: each frame a lossless image in an `ANMF` chunk.
fn encode_animated_webp(
    frames: &[(Vec<u8>, u32)],
    width: u32,
    height: u32,
    channels: u8,
) -> Result<Vec<u8>> {
    fn chunk(out: &mut Vec<u8>, fourcc: [u8; 4], payload: &[u8]) -> Result<()> {
        out.extend_from_slice(&fourcc);
        out.extend_from_slice(
            &u32::try_from(payload.len())
                .map_err(encode_error)?
                .to_le_bytes(),
        );
        out.extend_from_slice(payload);
        if payload.len() % 2 == 1 {
            out.push(0);
        }
        Ok(())
    }
    fn u24(out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&v.min(0x00ff_ffff).to_le_bytes()[..3]);
    }
    let colour = if channels == 4 {
        image_webp::ColorType::Rgba8
    } else {
        image_webp::ColorType::Rgb8
    };
    let mut body = Vec::new();
    // VP8X: animated (and with alpha), and the canvas size
    let mut vp8x = vec![if channels == 4 { 0x12 } else { 0x02 }, 0, 0, 0];
    u24(&mut vp8x, width - 1);
    u24(&mut vp8x, height - 1);
    chunk(&mut body, *b"VP8X", &vp8x)?;
    // ANIM: a transparent background, looping forever
    chunk(&mut body, *b"ANIM", &[0, 0, 0, 0, 0, 0])?;
    for (data, duration) in frames {
        let mut still = Vec::new();
        image_webp::WebPEncoder::new(&mut still)
            .encode(data, width, height, colour)
            .map_err(encode_error)?;
        // the encoder's VP8L chunk, which follows its RIFF header
        let image = still
            .get(12..)
            .filter(|c| c.starts_with(b"VP8L"))
            .ok_or_else(|| encode_error("the WebP encoder made no lossless image"))?;
        let mut anmf = Vec::with_capacity(16 + image.len());
        u24(&mut anmf, 0);
        u24(&mut anmf, 0);
        u24(&mut anmf, width - 1);
        u24(&mut anmf, height - 1);
        u24(&mut anmf, *duration);
        // (whole frames: no blending, no disposal)
        anmf.push(0x02);
        anmf.extend_from_slice(image);
        chunk(&mut body, *b"ANMF", &anmf)?;
    }
    let mut out = Vec::with_capacity(12 + body.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(
        &u32::try_from(4 + body.len())
            .map_err(encode_error)?
            .to_le_bytes(),
    );
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    Ok(out)
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

    /// (duration, pixels) of each frame of an animated WebP.
    fn webp_frames(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(decoder.is_animated());
        assert_eq!(decoder.loop_count(), image_webp::LoopCount::Forever);
        (0..decoder.num_frames())
            .map(|_| {
                let mut buf = vec![0; decoder.output_buffer_size().unwrap()];
                let ms = decoder.read_frame(&mut buf).unwrap();
                (ms, buf)
            })
            .collect()
    }

    /// (duration, pixels) of each frame of an APNG.
    fn apng_frames(bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let control = reader.info().animation_control.unwrap();
        assert_eq!(control.num_plays, 0);
        (0..control.num_frames)
            .map(|_| {
                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf).unwrap();
                buf.truncate(info.buffer_size());
                let fc = reader.info().frame_control.unwrap();
                (
                    u32::from(fc.delay_num) * 1000 / u32::from(fc.delay_den),
                    buf,
                )
            })
            .collect()
    }

    #[test]
    fn animations_keep_their_frames_and_merge_repeats() {
        let a = gradient(3);
        let b = Raster::new(9, 7, 3, a.data().iter().map(|v| v ^ 0xff).collect()).unwrap();
        // a repeated frame only lengthens the one before; a long delay
        // still fits an APNG's u16 fraction
        let frames = [a.clone(), b.clone(), b.clone(), a.clone()];
        let durations = [40, 50, 60, 70_000];
        let want = vec![
            (40, a.data().to_vec()),
            (110, b.data().to_vec()),
            (70_000, a.data().to_vec()),
        ];
        let apng = encode_animation(&frames, &durations, AnimationFormat::Apng).unwrap();
        assert_eq!(apng_frames(&apng), want);
        let webp = encode_animation(&frames, &durations, AnimationFormat::Webp).unwrap();
        assert_eq!(webp_frames(&webp), want);
        // with alpha, and a grey frame among them taking the first's type
        let rgba = gradient(4);
        let grey = gradient(1);
        let webp = encode_animation(
            &[rgba.clone(), grey.clone()],
            &[10, 20],
            AnimationFormat::Webp,
        )
        .unwrap();
        let frames = webp_frames(&webp);
        assert_eq!(frames[0], (10, rgba.data().to_vec()));
        let grey_rgba: Vec<u8> = grey.data().iter().flat_map(|&v| [v, v, v, 255]).collect();
        assert_eq!(frames[1], (20, grey_rgba));
        // missing durations take the default
        let apng = encode_animation(&[a.clone(), b.clone()], &[], AnimationFormat::Apng).unwrap();
        assert_eq!(
            apng_frames(&apng).iter().map(|f| f.0).collect::<Vec<_>>(),
            [125, 125]
        );
        assert!(encode_animation(&[], &[], AnimationFormat::Apng).is_err());
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
