//! Photoshop documents (`HydrusPSDHandling`): resolution from the header,
//! ICC detection from image resources. Pixels come from ffmpeg's PSD decoder.

/// `GetPSDResolution`: big-endian height then width at offset 14.
pub(crate) fn resolution(data: &[u8]) -> (u32, u32) {
    // Python's int.from_bytes on a short read just uses the bytes it got
    let read = |at: usize| -> u32 {
        data.get(at..(at + 4).min(data.len()))
            .unwrap_or(&[])
            .iter()
            .fold(0u32, |acc, &b| (acc << 8) | u32::from(b))
    };
    (read(18), read(14))
}

/// `PSDHasICCProfile`: whether image resource 1039 exists.
pub(crate) fn has_icc_profile(data: &[u8]) -> Option<bool> {
    let be32 = |at: usize| -> Option<usize> {
        Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    let mut pos = 26;
    let colour_mode_len = be32(pos)?;
    pos += 4 + colour_mode_len;
    let resources_len = be32(pos)?;
    pos += 4;
    let end = pos + resources_len;
    while pos < end {
        if data.get(pos..pos + 4)? != b"8BIM" {
            return None;
        }
        let id = u16::from_be_bytes(data.get(pos + 4..pos + 6)?.try_into().ok()?);
        let name_len = usize::from(*data.get(pos + 6)?);
        pos += 7 + name_len;
        if (name_len + 1) % 2 == 1 {
            pos += 1;
        }
        let size = be32(pos)?;
        pos += 4 + size + size % 2;
        if id == 1039 {
            return Some(true);
        }
    }
    Some(false)
}
