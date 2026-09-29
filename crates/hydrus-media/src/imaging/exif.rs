//! EXIF, as far as the reference looks at it: whether Pillow's
//! `getexif()._get_merged_dict()` is non-empty (the `has_exif` flag) and the
//! orientation tag (EXIF rotation on load).

use std::sync::LazyLock;

use regex::bytes::Regex;

/// What the reference learns from an image's EXIF.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExifSummary {
    /// Pillow's EXIF dict has at least one entry.
    pub has_tags: bool,
    /// Orientation (tag 274), when it is a single integer.
    pub orientation: Option<u32>,
}

const ORIENTATION: u16 = 274;

/// Unit size of each TIFF field type Pillow can load (others are skipped).
fn unit_size(field_type: u16) -> Option<usize> {
    Some(match field_type {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 | 16 => 8,
        _ => return None,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    big_endian: bool,
}

impl Reader<'_> {
    fn bytes(&self, at: usize, n: usize) -> Option<&[u8]> {
        self.data.get(at..at.checked_add(n)?)
    }

    fn u16(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.bytes(at, 2)?.try_into().ok()?;
        Some(if self.big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }

    fn u32(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.bytes(at, 4)?.try_into().ok()?;
        Some(if self.big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }
}

/// One IFD as Pillow's `ImageFileDirectory_v2.load` reads it: the number of
/// tags it keeps, and the orientation value.
fn read_ifd(reader: &Reader<'_>, offset: usize) -> (usize, Option<u32>) {
    let Some(count) = reader.u16(offset) else {
        return (0, None);
    };
    let mut kept = 0;
    let mut orientation = None;
    for i in 0..usize::from(count) {
        let entry = offset + 2 + i * 12;
        // a short read raises OSError, which ends the directory
        let (Some(tag), Some(field_type), Some(n), Some(inline)) = (
            reader.u16(entry),
            reader.u16(entry + 2),
            reader.u32(entry + 4),
            reader.bytes(entry + 8, 4),
        ) else {
            break;
        };
        let Some(unit) = unit_size(field_type) else {
            continue;
        };
        let size = n as usize * unit;
        let value = if size > 4 {
            let Some(at) = reader.u32(entry + 8) else {
                continue;
            };
            reader.bytes(at as usize, size)
        } else {
            Some(&inline[..size])
        };
        let Some(value) = value.filter(|v| !v.is_empty()) else {
            continue;
        };
        kept += 1;
        if tag == ORIENTATION && n == 1 {
            orientation = match field_type {
                3 | 8 => reader_value_u16(reader, value).map(u32::from),
                4 | 9 | 13 => Reader {
                    data: value,
                    big_endian: reader.big_endian,
                }
                .u32(0),
                _ => None,
            };
        }
    }
    (kept, orientation)
}

fn reader_value_u16(reader: &Reader<'_>, value: &[u8]) -> Option<u16> {
    Reader {
        data: value,
        big_endian: reader.big_endian,
    }
    .u16(0)
}

/// Parse a TIFF structure (an EXIF blob, or a TIFF file) starting at IFD0,
/// or at `ifd_offset` when given.
pub(crate) fn summarise_tiff(data: &[u8], ifd_offset: Option<usize>) -> ExifSummary {
    let big_endian = match data.get(..4) {
        Some(b"II*\x00") => false,
        Some(b"MM\x00*") => true,
        _ => return ExifSummary::default(),
    };
    let reader = Reader { data, big_endian };
    let Some(offset) = ifd_offset.or_else(|| reader.u32(4).map(|o| o as usize)) else {
        return ExifSummary::default();
    };
    let (kept, orientation) = read_ifd(&reader, offset);
    ExifSummary {
        has_tags: kept > 0,
        orientation,
    }
}

/// `Exif.load(info['exif'])`: strips any `Exif\0\0` prefixes first.
pub(crate) fn summarise_exif_blob(mut data: &[u8]) -> ExifSummary {
    while let Some(rest) = data.strip_prefix(b"Exif\x00\x00") {
        data = rest;
    }
    summarise_tiff(data, None)
}

static XMP_ORIENTATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"tiff:Orientation(="|>)([0-9])"#).expect("valid regex"));

/// The XMP fallback in `Image.getexif()`: when EXIF has no orientation, a
/// `tiff:Orientation` in the XMP packet adds one.
pub(crate) fn with_xmp_fallback(mut summary: ExifSummary, xmp: Option<&[u8]>) -> ExifSummary {
    if summary.orientation.is_some() {
        return summary;
    }
    if let Some(caps) = xmp.and_then(|x| XMP_ORIENTATION.captures(x)) {
        let digit = caps[2][0] - b'0';
        summary.orientation = Some(u32::from(digit));
        summary.has_tags = true;
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exif_with_orientation(big_endian: bool, value: u16) -> Vec<u8> {
        let mut v = Vec::new();
        if big_endian {
            v.extend_from_slice(b"MM\x00*\x00\x00\x00\x08\x00\x01");
            v.extend_from_slice(&ORIENTATION.to_be_bytes());
            v.extend_from_slice(&3u16.to_be_bytes());
            v.extend_from_slice(&1u32.to_be_bytes());
            v.extend_from_slice(&value.to_be_bytes());
            v.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        } else {
            v.extend_from_slice(b"II*\x00\x08\x00\x00\x00\x01\x00");
            v.extend_from_slice(&ORIENTATION.to_le_bytes());
            v.extend_from_slice(&3u16.to_le_bytes());
            v.extend_from_slice(&1u32.to_le_bytes());
            v.extend_from_slice(&value.to_le_bytes());
            v.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        }
        v
    }

    #[test]
    fn reads_orientation_both_endians() {
        for be in [false, true] {
            let mut blob = b"Exif\x00\x00".to_vec();
            blob.extend(exif_with_orientation(be, 6));
            let s = summarise_exif_blob(&blob);
            assert_eq!(
                s,
                ExifSummary {
                    has_tags: true,
                    orientation: Some(6)
                }
            );
        }
    }

    #[test]
    fn garbage_is_empty() {
        assert_eq!(summarise_exif_blob(b"hello"), ExifSummary::default());
        assert_eq!(summarise_exif_blob(b""), ExifSummary::default());
    }

    #[test]
    fn xmp_orientation_fallback() {
        let s = with_xmp_fallback(ExifSummary::default(), Some(b"<x tiff:Orientation=\"8\"/>"));
        assert_eq!(s.orientation, Some(8));
        assert!(s.has_tags);
    }
}
