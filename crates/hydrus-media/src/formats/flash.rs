//! SWF headers (`hexagonitswfheader.parse` + `GetFlashProperties`).

use std::io::Read;

use crate::error::{MediaError, Result};

/// Resolution, duration and frame count from an SWF header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FlashProperties {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub num_frames: u64,
}

fn body(data: &[u8]) -> Result<Vec<u8>> {
    let bad = |m: &str| MediaError::damaged(format!("could not parse this flash file: {m}"));
    if data.len() < 8 {
        return Err(bad("too short"));
    }
    let size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let rest = &data[8..];
    let payload = &rest[..size.min(rest.len())];
    match &data[..3] {
        b"FWS" => Ok(payload.to_vec()),
        b"CWS" => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(payload)
                .read_to_end(&mut out)
                .map_err(|_| bad("bad zlib data"))?;
            Ok(out)
        }
        b"ZWS" => {
            // 4 bytes of compressed length, 5 bytes of lzma properties, then data;
            // re-assemble an .lzma stream with unknown size, as the reference does
            let props = rest.get(4..9).ok_or_else(|| bad("truncated lzma header"))?;
            let mut stream = props.to_vec();
            stream.extend_from_slice(&[0xFF; 8]);
            stream.extend_from_slice(rest.get(9..).unwrap_or(&[]));
            let mut out = Vec::new();
            lzma_rs::lzma_decompress(&mut std::io::Cursor::new(stream), &mut out)
                .map_err(|_| bad("This flash file is too weird, could not parse it!"))?;
            Ok(out)
        }
        _ => Err(bad("invalid signature")),
    }
}

/// `GetFlashProperties`.
pub(crate) fn properties(data: &[u8]) -> Result<FlashProperties> {
    let buffer = body(data)?;
    let truncated = || MediaError::damaged("this flash file header is truncated");
    let mut bytes = buffer.iter().copied();
    let first = bytes.next().ok_or_else(truncated)?;
    let nbits = first >> 3;
    let mut current = first;
    let mut cursor = 5u32;
    let mut values = [0i64; 4];
    for value in &mut values {
        for bit in (0..nbits).rev() {
            if (u32::from(current) << cursor) & 0x80 != 0 {
                *value |= 1 << bit;
            }
            cursor += 1;
            if cursor > 7 {
                current = bytes.next().ok_or_else(truncated)?;
                cursor = 0;
            }
        }
        *value /= 20;
    }
    let rest: Vec<u8> = bytes.collect();
    if rest.len() < 4 {
        return Err(truncated());
    }
    let fps = u16::from_le_bytes([rest[0], rest[1]]) >> 8;
    let frames = u16::from_le_bytes([rest[2], rest[3]]);
    let [xmin, xmax, ymin, ymax] = values;
    let fps = if fps == 0 { 1 } else { fps };
    let duration_s = f64::from(frames) / f64::from(fps);
    Ok(FlashProperties {
        width: u32::try_from((xmax - xmin).abs()).unwrap_or(u32::MAX),
        height: u32::try_from((ymax - ymin).abs()).unwrap_or(u32::MAX),
        duration_ms: crate::ffmpeg::video::py_int(duration_s * 1000.0),
        num_frames: u64::from(frames),
    })
}
