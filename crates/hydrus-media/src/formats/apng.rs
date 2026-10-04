//! APNG chunk handling (`HydrusAnimationHandling.GetAPNG*`).
//!
//! Detection deliberately looks only at the first 256 bytes, as the
//! reference does: an `acTL` chunk that starts later (after a large text or
//! ICC chunk) means the file is treated as a still PNG.

/// `GetAPNGChunks`: complete chunks only, stopping at the first short read.
fn chunks(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut i = 8;
    while i + 8 <= data.len() {
        let n = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        let kind = [data[i + 4], data[i + 5], data[i + 6], data[i + 7]];
        let start = i + 8;
        let Some(body) = start.checked_add(n).and_then(|end| data.get(start..end)) else {
            break;
        };
        out.push((kind, body));
        i = start + n + 4;
    }
    out
}

/// The acTL chunk data among complete chunks in `header` (the last one wins,
/// as with Python's `dict(chunks)`).
fn actl(header: &[u8]) -> Option<&[u8]> {
    chunks(header)
        .into_iter()
        .rfind(|(k, _)| k == b"acTL")
        .map(|(_, d)| d)
}

/// `GetAPNGNumFrames` on the acTL found in the first 256 bytes.
pub(crate) fn num_frames(first_256: &[u8]) -> Option<u32> {
    let d = actl(first_256)?;
    Some(u32::from_be_bytes(d.get(..4)?.try_into().ok()?))
}

pub(crate) fn times_to_play(first_256: &[u8]) -> u32 {
    actl(first_256)
        .and_then(|data| data.get(4..8))
        .and_then(|bytes| bytes.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

/// `IsPNGAnimated`: an acTL in the first 256 bytes declaring more than one frame.
pub(crate) fn is_animated(first_256: &[u8]) -> bool {
    num_frames(first_256).is_some_and(|n| n > 1)
}

/// `GetAPNGDurationMS`: summed fcTL delays over the whole file.
pub(crate) fn duration_ms(data: &[u8]) -> u64 {
    let mut total_s = 0.0f64;
    for (kind, d) in chunks(data) {
        if &kind == b"fcTL" && d.len() >= 24 {
            let num = f64::from(u16::from_be_bytes([d[20], d[21]]));
            let den = f64::from(u16::from_be_bytes([d[22], d[23]]));
            total_s += if den == 0.0 {
                0.1
            } else {
                (num / den).max(0.001)
            };
        }
    }
    let ms_float = total_s * 1000.0;
    let ms = crate::ffmpeg::video::py_int(total_s * 1000.0);
    if ms == 0 && ms_float > 0.0 { 1 } else { ms }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(kind: [u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = (data.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(&kind);
        v.extend_from_slice(data);
        v.extend_from_slice(&[0; 4]);
        v
    }

    #[test]
    fn detects_actl_near_the_top_only() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(chunk(*b"IHDR", &[0; 13]));
        png.extend(chunk(*b"acTL", &[0, 0, 0, 3, 0, 0, 0, 0]));
        assert!(is_animated(&png[..png.len().min(256)]));
        let mut late = b"\x89PNG\r\n\x1a\n".to_vec();
        late.extend(chunk(*b"IHDR", &[0; 13]));
        late.extend(chunk(*b"tEXt", &[b'x'; 300]));
        late.extend(chunk(*b"acTL", &[0, 0, 0, 3, 0, 0, 0, 0]));
        assert!(!is_animated(&late[..256]));
    }

    #[test]
    fn durations() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut fctl = vec![0u8; 26];
        fctl[20..24].copy_from_slice(&[0, 1, 0, 3]);
        png.extend(chunk(*b"fcTL", &fctl));
        fctl[20..24].copy_from_slice(&[0, 5, 0, 0]);
        png.extend(chunk(*b"fcTL", &fctl));
        assert_eq!(duration_ms(&png), 433);
    }
}
