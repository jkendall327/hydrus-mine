//! The import job's XMP, IPTC and software/source checks
//! (`ClientImageMetadata.HasXMP` / `HasIPTC`,
//! `HydrusImageMetadata.HasSoftwareSource`), over what Pillow puts in an
//! image's `info`. Checked against `oracle/dump_metadata_flags.py`.

use std::collections::BTreeMap;

use super::decode::InfoValue;

/// Python's `str.strip()`: Unicode whitespace, which includes the
/// information separators U+001C..U+001F.
pub(crate) fn py_strip(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

/// `GetXMPDict` succeeds: Pillow found an XMP packet (as bytes) and it is
/// UTF-8 once trailing NULs and spaces are cut. (Its XML is then parsed
/// leniently, which doesn't fail.)
pub(crate) fn xmp_is_readable(xmp: &[u8]) -> bool {
    let end = xmp
        .iter()
        .rposition(|&b| b != 0 && b != b' ')
        .map_or(0, |i| i + 1);
    std::str::from_utf8(&xmp[..end]).is_ok()
}

/// A JPEG's Photoshop image resources (`APP13` "Photoshop 3.0" segments),
/// as `JpegImagePlugin` collects them: by resource code, later blocks
/// replacing earlier ones.
pub(crate) fn photoshop_resources(segments: &[&[u8]]) -> BTreeMap<u16, Vec<u8>> {
    let mut out = BTreeMap::new();
    for s in segments {
        let mut offset = 14;
        while s.get(offset..offset + 4) == Some(b"8BIM") {
            offset += 4;
            let Some(code) = s.get(offset..offset + 2) else {
                break;
            };
            let code = u16::from_be_bytes([code[0], code[1]]);
            offset += 2;
            let Some(&name_len) = s.get(offset) else {
                break;
            };
            offset += 1 + usize::from(name_len);
            offset += offset & 1;
            let Some(size) = s.get(offset..offset + 4) else {
                break;
            };
            let size = u32::from_be_bytes([size[0], size[1], size[2], size[3]]) as usize;
            offset += 4;
            let end = offset.saturating_add(size).min(s.len());
            out.insert(code, s.get(offset..end).unwrap_or_default().to_vec());
            offset = offset.saturating_add(size);
            offset += offset & 1;
        }
    }
    out
}

/// The IPTC records `GetIPTCDict` shows the user (its
/// `iptc_tuple_int_enums_we_want_to_show_to_the_user`).
const SHOWN: &[(u8, u8)] = &[
    (1, 70),
    (1, 80),
    (1, 100),
    (2, 3),
    (2, 4),
    (2, 5),
    (2, 7),
    (2, 8),
    (2, 10),
    (2, 12),
    (2, 15),
    (2, 20),
    (2, 22),
    (2, 25),
    (2, 26),
    (2, 27),
    (2, 30),
    (2, 35),
    (2, 37),
    (2, 38),
    (2, 40),
    (2, 42),
    (2, 45),
    (2, 47),
    (2, 50),
    (2, 55),
    (2, 60),
    (2, 62),
    (2, 63),
    (2, 65),
    (2, 70),
    (2, 75),
    (2, 80),
    (2, 85),
    (2, 90),
    (2, 92),
    (2, 95),
    (2, 100),
    (2, 101),
    (2, 103),
    (2, 105),
    (2, 110),
    (2, 115),
    (2, 116),
    (2, 118),
    (2, 120),
    (2, 122),
    (2, 125),
    (2, 130),
    (2, 131),
    (2, 135),
    (2, 150),
    (2, 151),
    (2, 152),
    (2, 153),
    (2, 154),
    (2, 200),
    (2, 201),
    (2, 202),
];

/// One IPTC dataset's value(s), as `IptcImageFile` keeps them: a field
/// of size 0 is `None`, and a repeated dataset becomes a list.
enum Values<'a> {
    One(Option<&'a [u8]>),
    Many,
}

/// `IptcImagePlugin.getiptcinfo`'s parse of an IPTC block: the datasets
/// read before the block ends (or turns out to be cut short), or `None`
/// when it is malformed in a way Pillow raises on.
fn iptc_datasets(data: &[u8]) -> Option<Vec<((u8, u8), Values<'_>)>> {
    let mut fields: Vec<((u8, u8), Values<'_>)> = Vec::new();
    let mut pos = 0;
    loop {
        let s = &data[pos.min(data.len())..(pos + 5).min(data.len())];
        pos += s.len();
        if s.iter().all(|&b| b == 0) {
            break;
        }
        // (too short for the tag: Pillow's "expected failure")
        let (Some(&marker), Some(&record), Some(&dataset)) = (s.first(), s.get(1), s.get(2)) else {
            break;
        };
        if marker != 0x1C || !matches!(record, 1..=9 | 240) {
            return None;
        }
        let Some(&size_byte) = s.get(3) else {
            break;
        };
        let size = match size_byte {
            133.. => return None,
            128 => 0,
            129..=132 => {
                let n = usize::from(size_byte - 128);
                let bytes = &data[pos.min(data.len())..(pos + n).min(data.len())];
                pos += bytes.len();
                // `_i`: the last four bytes, zero-padded, as a big-endian int
                let mut word = [0u8; 4];
                let tail = &bytes[bytes.len().saturating_sub(4)..];
                word[4 - tail.len()..].copy_from_slice(tail);
                u32::from_be_bytes(word) as usize
            }
            _ => {
                // (a two-byte size cut short is a struct.error Pillow raises)
                let low = *s.get(4)?;
                usize::from(u16::from_be_bytes([size_byte, low]))
            }
        };
        let tag = (record, dataset);
        if tag == (8, 10) {
            break;
        }
        let value = if size > 0 {
            let bytes = &data[pos.min(data.len())..pos.saturating_add(size).min(data.len())];
            pos = pos.saturating_add(size);
            Some(bytes)
        } else {
            None
        };
        match fields.iter_mut().find(|(t, _)| *t == tag) {
            Some((_, v)) => *v = Values::Many,
            None => fields.push((tag, Values::One(value))),
        }
    }
    // Pillow then reads the image records (3, x): without them that is its
    // expected failure; with them, a block that isn't a whole IPTC image
    // fails further on
    if fields.iter().any(|((record, _), _)| *record == 3) {
        let has = |t: (u8, u8)| fields.iter().any(|(k, _)| *k == t);
        if has((3, 60)) && has((3, 20)) && has((3, 30)) {
            return None;
        }
    }
    Some(fields)
}

/// `HasIPTC`: an IPTC block with a dataset worth showing that isn't blank.
pub(crate) fn has_shown_iptc(data: &[u8]) -> bool {
    let Some(fields) = iptc_datasets(data) else {
        return false;
    };
    fields.iter().any(|(tag, values)| {
        SHOWN.contains(tag)
            && match values {
                // ("unknown IPTC value type: None", and a list's repr, are
                // never blank)
                Values::One(None) | Values::Many => true,
                Values::One(Some(bytes)) => !py_strip(&String::from_utf8_lossy(bytes)).is_empty(),
            }
    })
}

/// `GetSoftwareSourceFromCommentInfoField`: the program a "Created with
/// ..." comment names.
pub(crate) fn software_from_comment(text: &str) -> Option<&str> {
    for verb in ["Created", "Converted", "Cropped", "Compressed", "Edited"] {
        for v in [verb.to_owned(), verb.to_lowercase()] {
            if let Some(rest) = text.strip_prefix(&format!("{v} with ")) {
                let software = rest.split('\n').next().unwrap_or_default();
                if !software.is_empty() {
                    return Some(software);
                }
            }
        }
    }
    None
}

/// `HasSoftwareSource` over Pillow's `info` (its text entries): a
/// non-blank Software, Creator or Source entry, or a "Created with ..."
/// comment.
pub(crate) fn has_software_source(text_info: &[(String, InfoValue)]) -> bool {
    let mut last: BTreeMap<&str, &InfoValue> = BTreeMap::new();
    for (k, v) in text_info {
        last.insert(k.as_str(), v);
    }
    let counts = |v: &InfoValue| match v {
        InfoValue::Text(t) => !py_strip(t).is_empty(),
        // (bytes are never equal to '', so they count)
        InfoValue::Bytes => true,
    };
    let named = [
        "Software", "software", "Creator", "creator", "Source", "source",
    ]
    .iter()
    .any(|k| last.get(k).is_some_and(|v| counts(v)));
    let comment = last.get("Comment").or_else(|| last.get("comment"));
    named
        || matches!(comment, Some(InfoValue::Text(t))
            if software_from_comment(t).is_some_and(|s| !py_strip(s).is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmp_packets() {
        assert!(xmp_is_readable(b"<x/>\x00\x00  "));
        assert!(xmp_is_readable(b""));
        assert!(!xmp_is_readable(b"<x>\xff</x>"));
    }

    #[test]
    fn iptc_blocks() {
        let field = |r: u8, d: u8, v: &[u8]| {
            let mut out = vec![0x1C, r, d];
            out.extend_from_slice(&(v.len() as u16).to_be_bytes());
            out.extend_from_slice(v);
            out
        };
        assert!(has_shown_iptc(&field(2, 25, b"blue eyes")));
        assert!(!has_shown_iptc(&field(2, 0, b"\x00\x04")), "not shown");
        assert!(!has_shown_iptc(&field(2, 120, b" \t ")), "blank");
        assert!(
            has_shown_iptc(&field(2, 5, b"")),
            "an empty field shows as None"
        );
        assert!(!has_shown_iptc(b"\x1d\x02\x19\x00\x03abc"), "not IPTC");
        let mut cut = field(2, 105, b"headline");
        cut.extend_from_slice(b"\x1c\x02");
        assert!(has_shown_iptc(&cut), "cut short after a good field");
    }

    #[test]
    fn software_comments() {
        assert_eq!(software_from_comment("Created with GIMP"), Some("GIMP"));
        assert_eq!(software_from_comment("edited with x\nmore"), Some("x"));
        assert_eq!(software_from_comment("Created with "), None);
        assert_eq!(software_from_comment("a nice picture"), None);
    }
}
