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

/// A value as Pillow reads it from an EXIF field, as far as the embedded
/// metadata window shows it (`str()` of it).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PyValue {
    Int(i64),
    Float(f64),
    /// An `IFDRational`: numerator and denominator.
    Rational(i64, i64),
    /// An ASCII field, decoded as Latin-1.
    Str(String),
    Bytes(Vec<u8>),
    Tuple(Vec<PyValue>),
    /// A sub-IFD (GPS) read as a dict.
    Dict(Vec<(u16, PyValue)>),
    None,
}

impl PyValue {
    /// Python's `str()` of the value.
    pub(crate) fn py_str(&self) -> String {
        let mut out = String::new();
        match self {
            PyValue::Int(i) => out = i.to_string(),
            PyValue::Float(f) => hydrus_core::pyjson::write_python_float(*f, &mut out),
            PyValue::Rational(n, d) => {
                if *d == 0 {
                    out = "nan".into();
                } else {
                    hydrus_core::pyjson::write_python_float(*n as f64 / *d as f64, &mut out);
                }
            }
            PyValue::Str(s) => out.clone_from(s),
            PyValue::Bytes(_) | PyValue::Tuple(_) | PyValue::Dict(_) | PyValue::None => {
                out = self.py_repr();
            }
        }
        out
    }

    /// Python's `repr()` of the value.
    pub(crate) fn py_repr(&self) -> String {
        match self {
            PyValue::Str(s) => py_str_repr(s),
            PyValue::Bytes(b) => py_bytes_repr(b),
            PyValue::Tuple(items) => {
                let inner: Vec<String> = items.iter().map(PyValue::py_repr).collect();
                if inner.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            }
            PyValue::Dict(items) => {
                let inner: Vec<String> = items
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.py_repr()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            PyValue::None => "None".into(),
            other => other.py_str(),
        }
    }
}

/// Python's `repr()` of a string.
pub(crate) fn py_str_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f || ((0x80..0xa0).contains(&(c as u32))) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python's `repr()` of bytes.
fn py_bytes_repr(b: &[u8]) -> String {
    let quote = if b.contains(&b'\'') && !b.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut out = String::from("b");
    out.push(char::from(quote));
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(char::from(c));
            }
            0x20..=0x7e => out.push(char::from(c)),
            c => out.push_str(&format!("\\x{c:02x}")),
        }
    }
    out.push(char::from(quote));
    out
}

/// An IFD's fields as Pillow's `ImageFileDirectory_v2` gives them
/// (`dict(ifd)`, each fixed up as `Exif._fixup` does): a later field with
/// the same tag replaces an earlier one; `single` are the tags its
/// `TiffTags` reads as one value.
fn ifd_values(reader: &Reader<'_>, offset: usize, single: &[u16]) -> Vec<(u16, PyValue)> {
    let mut out: Vec<(u16, PyValue)> = Vec::new();
    let Some(count) = reader.u16(offset) else {
        return out;
    };
    for i in 0..usize::from(count) {
        let entry = offset + 2 + i * 12;
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
        let data = if size > 4 {
            let Some(at) = reader.u32(entry + 8) else {
                continue;
            };
            reader.bytes(at as usize, size)
        } else {
            Some(&inline[..size])
        };
        let Some(data) = data.filter(|d| !d.is_empty()) else {
            continue;
        };
        let value = field_value(reader.big_endian, field_type, data, single.contains(&tag));
        match out.iter_mut().find(|(t, _)| *t == tag) {
            Some((_, v)) => *v = value,
            None => out.push((tag, value)),
        }
    }
    out
}

/// A field's value: its type's values, one alone (or the first, for a
/// tag read as one value) as itself.
fn field_value(big_endian: bool, field_type: u16, data: &[u8], single: bool) -> PyValue {
    let r = Reader { data, big_endian };
    let many = |unit: usize, read: &dyn Fn(usize) -> Option<PyValue>| -> Vec<PyValue> {
        (0..data.len() / unit)
            .filter_map(|i| read(i * unit))
            .collect()
    };
    let values: Vec<PyValue> = match field_type {
        1 | 7 => return PyValue::Bytes(data.to_vec()),
        2 => {
            let text = data.strip_suffix(b"\0").unwrap_or(data);
            return PyValue::Str(text.iter().map(|&b| char::from(b)).collect());
        }
        3 => many(2, &|at| r.u16(at).map(|v| PyValue::Int(i64::from(v)))),
        8 => many(2, &|at| {
            r.u16(at).map(|v| PyValue::Int(i64::from(v as i16)))
        }),
        4 | 13 => many(4, &|at| r.u32(at).map(|v| PyValue::Int(i64::from(v)))),
        9 => many(4, &|at| {
            r.u32(at).map(|v| PyValue::Int(i64::from(v as i32)))
        }),
        6 => data
            .iter()
            .map(|&b| PyValue::Int(i64::from(b as i8)))
            .collect(),
        5 => many(8, &|at| {
            Some(PyValue::Rational(
                i64::from(r.u32(at)?),
                i64::from(r.u32(at + 4)?),
            ))
        }),
        10 => many(8, &|at| {
            Some(PyValue::Rational(
                i64::from(r.u32(at)? as i32),
                i64::from(r.u32(at + 4)? as i32),
            ))
        }),
        11 => many(4, &|at| {
            r.u32(at)
                .map(|v| PyValue::Float(f64::from(f32::from_bits(v))))
        }),
        12 => many(8, &|at| {
            let hi = u64::from(r.u32(at)?);
            let lo = u64::from(r.u32(at + 4)?);
            let bits = if big_endian {
                (hi << 32) | lo
            } else {
                (lo << 32) | hi
            };
            Some(PyValue::Float(f64::from_bits(bits)))
        }),
        16 => many(8, &|at| {
            let hi = u64::from(r.u32(at)?);
            let lo = u64::from(r.u32(at + 4)?);
            let v = if big_endian {
                (hi << 32) | lo
            } else {
                (lo << 32) | hi
            };
            Some(PyValue::Int(i64::try_from(v).unwrap_or(i64::MAX)))
        }),
        _ => return PyValue::None,
    };
    if values.len() == 1 || (single && !values.is_empty()) {
        values.into_iter().next().unwrap_or(PyValue::None)
    } else {
        PyValue::Tuple(values)
    }
}

/// `getexif()._get_merged_dict()` over a TIFF structure (an EXIF blob,
/// or a TIFF file's IFD0 at `ifd_offset`): IFD0's fields, the Exif IFD's
/// merged in, and the GPS IFD's under its tag; and the orientation an XMP
/// packet gives when the EXIF has none.
pub(crate) fn merged_fields(
    data: &[u8],
    ifd_offset: Option<usize>,
    xmp: Option<&[u8]>,
) -> Vec<(u16, PyValue)> {
    use super::exif_tags::{SINGLE, SINGLE_EXIF, SINGLE_GPS};

    let big_endian = match data.get(..4) {
        Some(b"II*\x00") => Some(false),
        Some(b"MM\x00*") => Some(true),
        _ => None,
    };
    let mut merged = Vec::new();
    if let Some(big_endian) = big_endian {
        let reader = Reader { data, big_endian };
        if let Some(offset) = ifd_offset.or_else(|| reader.u32(4).map(|o| o as usize)) {
            merged = ifd_values(&reader, offset, SINGLE);
            let pointer = |tag: u16, merged: &[(u16, PyValue)]| {
                merged
                    .iter()
                    .find(|(t, _)| *t == tag)
                    .map(|(_, v)| match v {
                        PyValue::Int(o) => usize::try_from(*o).ok(),
                        _ => None,
                    })
            };
            if let Some(Some(at)) = pointer(0x8769, &merged) {
                for (tag, value) in ifd_values(&reader, at, SINGLE_EXIF) {
                    match merged.iter_mut().find(|(t, _)| *t == tag) {
                        Some((_, v)) => *v = value,
                        None => merged.push((tag, value)),
                    }
                }
            }
            if let Some(at) = pointer(0x8825, &merged) {
                let gps = match at {
                    Some(at) => PyValue::Dict(ifd_values(&reader, at, SINGLE_GPS)),
                    None => PyValue::None,
                };
                if let Some((_, v)) = merged.iter_mut().find(|(t, _)| *t == 0x8825) {
                    *v = gps;
                }
            }
        }
    }
    if !merged.iter().any(|(t, _)| *t == ORIENTATION)
        && let Some(caps) = xmp.and_then(|x| XMP_ORIENTATION.captures(x))
    {
        merged.push((ORIENTATION, PyValue::Int(i64::from(caps[2][0] - b'0'))));
    }
    merged
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
