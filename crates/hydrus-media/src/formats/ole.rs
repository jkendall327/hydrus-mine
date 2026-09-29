//! Legacy Office files in OLE compound documents (`HydrusOLEHandling`,
//! which uses `olefile`): the root storage CLSID or the SummaryInformation
//! "creating application" decides doc/xls/ppt, and the word count comes from
//! the same property set.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use hydrus_core::Mime;

const OLE_MAGIC: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

/// Root storage CLSIDs that identify an Office application.
const GUID_MIMES: [(&str, Mime); 11] = [
    ("00020900-0000-0000-C000-000000000046", Mime::ApplicationDoc),
    ("00020906-0000-0000-C000-000000000046", Mime::ApplicationDoc),
    ("00030003-0000-0000-C000-000000000046", Mime::ApplicationDoc),
    ("F4754C9B-64F5-4B40-8AF4-679732AC0607", Mime::ApplicationDoc),
    ("00044851-0000-0000-C000-000000000046", Mime::ApplicationPpt),
    ("64818D10-4F9B-11CF-86EA-00AA00B929E8", Mime::ApplicationPpt),
    ("EA7BAE70-FB3B-11CD-A903-00AA00510EA3", Mime::ApplicationPpt),
    ("CF4F55F4-8F87-4D47-80BB-5808164BB3F8", Mime::ApplicationPpt),
    ("00020810-0000-0000-C000-000000000046", Mime::ApplicationXls),
    ("00020820-0000-0000-C000-000000000046", Mime::ApplicationXls),
    ("00020830-0000-0000-C000-000000000046", Mime::ApplicationXls),
];

/// `olefile.isOleFile(path)`: the 8-byte signature.
pub(crate) fn is_ole_file(path: &Path) -> bool {
    let mut header = [0u8; 8];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok()
        && &header == OLE_MAGIC
}

/// A SummaryInformation property value, as far as we use them.
#[derive(Debug, Clone, PartialEq)]
enum Property {
    Int(i64),
    Bytes(Vec<u8>),
    Other,
}

fn u32_at(s: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(s.get(o..o + 4)?.try_into().ok()?))
}

fn parse_property(s: &[u8], offset: usize, vt: u32) -> Option<Property> {
    Some(match vt {
        2 => Property::Int(i64::from(i16::from_le_bytes(
            s.get(offset..offset + 2)?.try_into().ok()?,
        ))),
        18 => Property::Int(i64::from(u16::from_le_bytes(
            s.get(offset..offset + 2)?.try_into().ok()?,
        ))),
        3 | 22 | 10 | 19 | 23 => Property::Int(i64::from(u32_at(s, offset)?)),
        17 => Property::Int(i64::from(*s.get(offset)?)),
        8 | 30 => {
            let count = u32_at(s, offset)? as usize;
            let start = offset + 4;
            let end = (start + count.saturating_sub(1)).min(s.len());
            let mut v: Vec<u8> = s.get(start..end)?.to_vec();
            v.retain(|&b| b != 0);
            Property::Bytes(v)
        }
        _ => Property::Other,
    })
}

/// `OleFileIO.getproperties('\x05SummaryInformation')`, keeping ids 15 and 18.
fn summary_properties(stream: &mut impl Read) -> Vec<(u32, Property)> {
    let mut data = Vec::new();
    if stream.read_to_end(&mut data).is_err() || data.len() < 48 {
        return Vec::new();
    }
    let Some(section_offset) = u32_at(&data, 44).map(|o| o as usize) else {
        return Vec::new();
    };
    let Some(section_size) = u32_at(&data, section_offset) else {
        return Vec::new();
    };
    // olefile replaces the size field with "****" and keeps the rest
    let end = (section_offset + section_size as usize).min(data.len());
    let Some(section) = data.get(section_offset..end) else {
        return Vec::new();
    };
    let Some(num_props) = u32_at(section, 4) else {
        return Vec::new();
    };
    let num_props = (num_props as usize).min(section.len() / 8);
    let mut out = Vec::new();
    for i in 0..num_props {
        let (Some(pid), Some(offset)) = (u32_at(section, 8 + i * 8), u32_at(section, 12 + i * 8))
        else {
            continue;
        };
        let offset = offset as usize;
        let Some(vt) = u32_at(section, offset) else {
            continue;
        };
        if let Some(p) = parse_property(section, offset + 4, vt) {
            out.push((pid, p));
        }
    }
    out
}

struct OleInfo {
    root_clsid: String,
    creating_application: Option<Property>,
    num_words: Option<Property>,
}

fn read_ole(path: &Path) -> Option<OleInfo> {
    let mut cfb = cfb::open(path).ok()?;
    let clsid = cfb.root_entry().clsid().to_owned();
    let root_clsid = if clsid.is_nil() {
        String::new()
    } else {
        clsid.hyphenated().to_string().to_uppercase()
    };
    let mut creating_application = None;
    let mut num_words = None;
    if cfb.is_stream("/\u{5}SummaryInformation")
        && let Ok(mut stream) = cfb.open_stream("/\u{5}SummaryInformation")
    {
        let _ = stream.seek(SeekFrom::Start(0));
        for (pid, p) in summary_properties(&mut stream) {
            match pid {
                15 => num_words = Some(p),
                18 => creating_application = Some(p),
                _ => {}
            }
        }
    }
    Some(OleInfo {
        root_clsid,
        creating_application,
        num_words,
    })
}

/// `MimeFromOLEFile`.
pub(crate) fn mime(path: &Path) -> Mime {
    if !is_ole_file(path) {
        return Mime::ApplicationUnknown;
    }
    let Some(info) = read_ole(path) else {
        return Mime::UndeterminedOle;
    };
    if let Some((_, m)) = GUID_MIMES.iter().find(|(g, _)| *g == info.root_clsid) {
        return *m;
    }
    if let Some(Property::Bytes(app)) = &info.creating_application {
        for (prefix, m) in [
            (&b"Microsoft Word"[..], Mime::ApplicationDoc),
            (b"Microsoft PowerPoint", Mime::ApplicationPpt),
            (b"Microsoft Excel", Mime::ApplicationXls),
        ] {
            if app.starts_with(prefix) {
                return m;
            }
        }
    }
    Mime::UndeterminedOle
}

/// `OfficeOLEDocumentWordCount` (`None` when absent or not an integer).
pub(crate) fn word_count(path: &Path) -> Option<u64> {
    if !is_ole_file(path) {
        return None;
    }
    match read_ole(path)?.num_words? {
        Property::Int(n) => Some(n.unsigned_abs()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_parsing() {
        let mut s = vec![0u8; 8];
        s.extend_from_slice(&30u32.to_le_bytes());
        s.extend_from_slice(&6u32.to_le_bytes());
        s.extend_from_slice(b"Wo\0rd\0");
        assert_eq!(
            parse_property(&s, 12, 30),
            Some(Property::Bytes(b"Word".to_vec()))
        );
        assert_eq!(parse_property(&[7, 0, 0, 0], 0, 3), Some(Property::Int(7)));
    }
}
