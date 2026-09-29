//! WebP RIFF chunks (`HydrusAnimationHandling.GetWebPChunks`), animation
//! frame durations, and the first frame of an animation as libwebp's
//! `WebPAnimDecoder` (which Pillow uses) renders it.

use std::io::Cursor;

use crate::error::{MediaError, Result};

/// `GetWebPChunks`: (fourcc, data) pairs after the RIFF header. Data may be
/// shorter than declared at the end of a truncated file, as in Python slicing.
pub(crate) fn chunks(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut offset = 12;
    while offset + 8 <= data.len() {
        let kind = [
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ];
        let size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        let start = offset + 8;
        let end = start.saturating_add(size).min(data.len());
        out.push((kind, &data[start..end]));
        let mut total = 8 + size;
        if total % 2 == 1 {
            total += 1;
        }
        offset = offset.saturating_add(total);
    }
    out
}

/// `GetWebPFrameDurationsMS`: one entry per ANMF chunk (0 or unreadable -> 83ms).
pub(crate) fn frame_durations_ms(data: &[u8]) -> Vec<u64> {
    chunks(data)
        .into_iter()
        .filter(|(k, _)| k == b"ANMF")
        .map(|(_, c)| {
            if c.len() < 16 {
                return 83;
            }
            let d = u64::from(c[12]) | u64::from(c[13]) << 8 | u64::from(c[14]) << 16;
            if d == 0 { 83 } else { d }
        })
        .collect()
}

fn riff(payload: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((payload.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(payload);
    out
}

fn u24(b: &[u8]) -> u32 {
    u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16
}

/// Frame 0 of an animated WebP on a zeroed RGBA canvas, as libwebp's
/// `WebPAnimDecoder` produces it (the first frame is never blended).
pub(crate) fn first_frame_rgba(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let bad = || MediaError::damaged("could not read the first frame of this animated webp");
    let (_, anmf) = chunks(data)
        .into_iter()
        .find(|(k, _)| k == b"ANMF")
        .ok_or_else(bad)?;
    if anmf.len() < 16 {
        return Err(bad());
    }
    let (fx, fy) = (u24(&anmf[0..3]) * 2, u24(&anmf[3..6]) * 2);
    let (fw, fh) = (u24(&anmf[6..9]) + 1, u24(&anmf[9..12]) + 1);
    let payload = &anmf[16..];
    // re-wrap the frame bitstream as a still image; ALPH needs a VP8X header
    let still = if payload.starts_with(b"ALPH") {
        let mut vp8x = b"VP8X".to_vec();
        vp8x.extend_from_slice(&10u32.to_le_bytes());
        vp8x.push(0x10);
        vp8x.extend_from_slice(&[0, 0, 0]);
        vp8x.extend_from_slice(&(fw - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(&(fh - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(payload);
        riff(&vp8x)
    } else {
        riff(payload)
    };
    let mut dec = image_webp::WebPDecoder::new(Cursor::new(&still)).map_err(|_| bad())?;
    if dec.dimensions() != (fw, fh) {
        return Err(bad());
    }
    let alpha = dec.has_alpha();
    let mut frame = vec![0u8; dec.output_buffer_size().ok_or_else(bad)?];
    dec.read_image(&mut frame).map_err(|_| bad())?;
    let mut canvas = vec![0u8; width as usize * height as usize * 4];
    let src_c = if alpha { 4 } else { 3 };
    for y in 0..fh as usize {
        let cy = fy as usize + y;
        if cy >= height as usize {
            break;
        }
        for x in 0..fw as usize {
            let cx = fx as usize + x;
            if cx >= width as usize {
                break;
            }
            let s = &frame[(y * fw as usize + x) * src_c..][..src_c];
            let d = &mut canvas[(cy * width as usize + cx) * 4..][..4];
            d[..3].copy_from_slice(&s[..3]);
            d[3] = if alpha { s[3] } else { 255 };
        }
    }
    Ok(canvas)
}

/// Whether any frame of an animated WebP (as Pillow renders them) has
/// visible transparency. Files without an alpha flag decode as RGB and never do.
pub(crate) fn any_frame_has_useful_alpha(data: &[u8], num_frames: u64) -> Option<bool> {
    let mut dec = image_webp::WebPDecoder::new(Cursor::new(data)).ok()?;
    if !dec.has_alpha() {
        return Some(false);
    }
    let (w, h) = dec.dimensions();
    let useful =
        |rgba: &[u8]| crate::imaging::alpha_is_useful(rgba.iter().skip(3).step_by(4), w, h);
    if useful(&first_frame_rgba(data, w, h).ok()?) {
        return Some(true);
    }
    if !dec.is_animated() {
        return Some(false);
    }
    let mut buf = vec![0u8; dec.output_buffer_size()?];
    for i in 0..num_frames {
        if dec.read_frame(&mut buf).is_err() {
            break;
        }
        if i > 0 && useful(&buf) {
            return Some(true);
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_walk_handles_padding_and_truncation() {
        let mut data = b"RIFF\0\0\0\0WEBP".to_vec();
        data.extend_from_slice(b"ANMF\x03\0\0\0abc\0");
        data.extend_from_slice(b"EXIF\x10\0\0\0xy");
        let c = chunks(&data);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], (*b"ANMF", &b"abc"[..]));
        assert_eq!(c[1], (*b"EXIF", &b"xy"[..]));
        assert_eq!(frame_durations_ms(&data), vec![83]);
    }
}
