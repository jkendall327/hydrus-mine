//! Paint.NET documents (`HydrusPaintNETHandling`): an XML header after the
//! `PDN3` magic carries the size and a base64 PNG thumbnail.

use crate::error::{MediaError, Result};
use crate::formats::archive::with_xml;

fn xml_header(data: &[u8]) -> Option<&str> {
    let len_bytes = data.get(4..7)?;
    let len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], 0]) as usize;
    std::str::from_utf8(data.get(7..7 + len)?).ok()
}

/// `GetPaintNETResolution`.
pub(crate) fn resolution(data: &[u8]) -> Result<(u32, u32)> {
    let header = xml_header(data).ok_or_else(|| {
        MediaError::damaged("Could not read resolution bytes from this Paint.NET!")
    })?;
    with_xml(header.as_bytes(), |doc| {
        let root = doc.root_element();
        let get = |k: &str| root.attribute(k)?.trim().parse::<u32>().ok();
        Some((get("width")?, get("height")?))
    })
    .ok_or_else(|| MediaError::damaged("Could not read resolution bytes from this Paint.NET!"))
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        if c.is_ascii_whitespace() {
            continue;
        }
        let v = ALPHABET.iter().position(|&a| a == c)? as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The PNG thumbnail in `custom/thumb@png`.
pub(crate) fn thumbnail_png(data: &[u8]) -> Option<Vec<u8>> {
    let header = xml_header(data)?;
    let b64 = with_xml(header.as_bytes(), |doc| {
        let custom = doc
            .root_element()
            .children()
            .find(|n| n.has_tag_name("custom"))?;
        let thumb = custom.children().find(|n| n.has_tag_name("thumb"))?;
        thumb.attribute("png").map(str::to_owned)
    })?;
    base64_decode(&b64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64() {
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64_decode("aGk=").unwrap(), b"hi");
    }
}
