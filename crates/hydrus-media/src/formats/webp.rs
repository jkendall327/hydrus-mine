//! WebP RIFF chunks (`HydrusAnimationHandling.GetWebPChunks`), animation
//! frame durations, and the first frame of an animation as libwebp's
//! `WebPAnimDecoder` (which Pillow uses) renders it.

use std::{
    fs::File,
    io::{self, Cursor, Read, Seek, SeekFrom},
};

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

/// Work around image-webp 0.2.4 rounding opaque blended pixels down by one.
/// For a conforming animation declared opaque in VP8X, replacement and alpha
/// blending are identical. Present its ANMF no-blend bits to the decoder while
/// leaving the file, timing/disposal flags and alpha-bearing streams unchanged.
/// Retain only byte offsets, not a second compressed-animation buffer.
#[derive(Debug)]
pub(crate) struct AnimationReader {
    file: File,
    position: u64,
    no_blend_offsets: Vec<u64>,
}

impl AnimationReader {
    pub(crate) fn new(file: File, data: &[u8]) -> Self {
        Self {
            file,
            position: 0,
            no_blend_offsets: opaque_frame_flags(data).unwrap_or_default(),
        }
    }
}

impl Read for AnimationReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.file.read(buffer)?;
        let end = self.position.saturating_add(count as u64);
        let first = self
            .no_blend_offsets
            .partition_point(|offset| *offset < self.position);
        for offset in &self.no_blend_offsets[first..] {
            if *offset >= end {
                break;
            }
            let index = usize::try_from(*offset - self.position)
                .expect("a flag within the read fits the buffer index");
            buffer[index] |= 0b10;
        }
        self.position = end;
        Ok(count)
    }
}

impl Seek for AnimationReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = self.file.seek(position)?;
        Ok(self.position)
    }
}

// Reject incomplete/ambiguous containers for rewriting; the ordinary decoder
// still receives those original bytes and remains responsible for validation.
fn opaque_frame_flags(data: &[u8]) -> Option<Vec<u64>> {
    if data.len() < 12 || &data[..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return None;
    }
    let limit = usize::try_from(u32::from_le_bytes(data[4..8].try_into().ok()?))
        .ok()?
        .checked_add(8)?;
    if limit < 12 || limit > data.len() {
        return None;
    }
    let mut offset = 12usize;
    let mut opaque = None;
    let mut flags = Vec::new();
    let mut animation = false;
    while offset < limit {
        let start = offset.checked_add(8)?;
        if start > limit {
            return None;
        }
        let size =
            usize::try_from(u32::from_le_bytes(data[offset + 4..start].try_into().ok()?)).ok()?;
        let end = start.checked_add(size)?;
        let next = end.checked_add(size % 2)?;
        if next > limit {
            return None;
        }
        match &data[offset..offset + 4] {
            b"VP8X" => {
                if offset != 12 || size != 10 || opaque.is_some() {
                    return None;
                }
                // VP8X A means at least one frame has transparency; animation
                // must be set and A must be clear for this narrow workaround.
                opaque = Some(data[start] & 0b0001_0010 == 0b10);
            }
            b"ANIM" => {
                if opaque.is_none() || animation || size != 6 {
                    return None;
                }
                animation = true;
            }
            b"ANMF" => {
                if !animation
                    || size < 16
                    || data[start + 15] & !0b11 != 0
                    || !opaque_frame_payload(&data[start + 16..end])
                {
                    return None;
                }
                flags.push(u64::try_from(start + 15).ok()?);
            }
            _ => {}
        }
        offset = next;
    }
    (opaque == Some(true)).then_some(flags)
}

// The VP8X transparency declaration is the semantic guarantee. Reject an
// explicit ALPH subchunk or contradictory VP8L alpha hint as well; the hint
// alone must never authorize this workaround (it does not affect decoding).
fn opaque_frame_payload(data: &[u8]) -> bool {
    let mut offset = 0usize;
    let mut bitstream = false;
    while offset < data.len() {
        let Some(start) = offset.checked_add(8).filter(|start| *start <= data.len()) else {
            return false;
        };
        let size = u32::from_le_bytes(data[offset + 4..start].try_into().unwrap()) as usize;
        let Some(end) = start.checked_add(size) else {
            return false;
        };
        let Some(next) = end.checked_add(size % 2).filter(|next| *next <= data.len()) else {
            return false;
        };
        match &data[offset..offset + 4] {
            b"ALPH" => return false,
            b"VP8 " | b"VP8L" => {
                if bitstream {
                    return false;
                }
                if &data[offset..offset + 4] == b"VP8L"
                    && (size < 5 || data[start] != 0x2f || data[start + 4] & 0xf0 != 0)
                {
                    return false;
                }
                bitstream = true;
            }
            _ => {}
        }
        offset = next;
    }
    bitstream
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

    fn read_presented(data: &[u8]) -> Vec<u8> {
        let mut temporary = tempfile::tempfile().unwrap();
        std::io::Write::write_all(&mut temporary, data).unwrap();
        temporary.rewind().unwrap();
        let mut reader = AnimationReader::new(temporary, data);
        let mut presented = Vec::new();
        reader.read_to_end(&mut presented).unwrap();
        // Re-reading each byte backwards exercises buffering and arbitrary seeks.
        for (index, expected) in presented.iter().enumerate().rev() {
            reader.seek(SeekFrom::Start(index as u64)).unwrap();
            let mut byte = [0];
            reader.read_exact(&mut byte).unwrap();
            assert_eq!(byte[0], *expected);
        }
        presented
    }

    #[test]
    fn opaque_reader_preserves_payload_duration_disposal_and_file_bytes_across_seeks() {
        let path = hydrus_testkit::fixture_path("image_decoder_policies/embedded-animation.webp");
        let original = std::fs::read(&path).unwrap();
        let mut data = original.clone();
        let offsets = opaque_frame_flags(&data).unwrap();
        assert_eq!(offsets.len(), 2);
        // Exercise disposal without changing the workaround's opaque guarantee.
        data[usize::try_from(offsets[0]).unwrap()] |= 1;
        let presented = read_presented(&data);
        let mut expected = data.clone();
        for offset in offsets {
            expected[usize::try_from(offset).unwrap()] |= 2;
        }
        assert_eq!(presented, expected);
        assert_eq!(frame_durations_ms(&presented), [100, 150]);
        assert_eq!(std::fs::read(path).unwrap(), original);
    }

    #[test]
    fn alpha_bearing_and_incomplete_or_contradictory_containers_are_not_rewritten() {
        let alpha =
            std::fs::read(hydrus_testkit::fixture_path("media/webp_anim_alpha.webp")).unwrap();
        assert_eq!(read_presented(&alpha), alpha);
        // A corrupt global declaration must not override an explicit frame alpha hint.
        let mut contradictory = alpha.clone();
        contradictory[20] &= !0x10;
        assert_eq!(read_presented(&contradictory), contradictory);
        let data = std::fs::read(hydrus_testkit::fixture_path(
            "image_decoder_policies/embedded-animation.webp",
        ))
        .unwrap();
        for length in [0, 11, 21, data.len() - 1] {
            assert_eq!(read_presented(&data[..length]), data[..length]);
        }
        let mut oversized = data.clone();
        oversized[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(read_presented(&oversized), oversized);
        // The odd payload's mandatory pad byte is inside the RIFF boundary.
        let mut padded = data.clone();
        padded.extend_from_slice(b"JUNK\x01\0\0\0x\0");
        let size = u32::try_from(padded.len() - 8).unwrap();
        padded[4..8].copy_from_slice(&size.to_le_bytes());
        assert_ne!(read_presented(&padded), padded);
        padded.pop();
        let size = u32::try_from(padded.len() - 8).unwrap();
        padded[4..8].copy_from_slice(&size.to_le_bytes());
        assert_eq!(read_presented(&padded), padded);
    }

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
