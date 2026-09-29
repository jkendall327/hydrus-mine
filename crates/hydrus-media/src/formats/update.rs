//! Hydrus repository update files, which `GetMime` recognises when asked to
//! (`ok_to_look_for_hydrus_updates`): `HydrusSerialisable.CreateFromNetworkBytes`
//! succeeds and yields a `ContentUpdate` or `DefinitionsUpdate`.
//!
//! The payload is zlib-compressed JSON (LZ4 block with a size prefix as a
//! fallback) holding a serialisable tuple `[type, version, info]`. The
//! checks the reference's deserialiser makes on `info` for these two types are
//! repeated here, with Python's unpacking and hashing rules, so malformed
//! payloads are rejected the same way.

use std::io::Read;

use hydrus_core::Mime;
use serde_json::Value;

const CONTENT_UPDATE: u64 = 34;
const DEFINITIONS_UPDATE: u64 = 36;
const DEFINITIONS_TYPE_HASHES: i64 = 0;
const DEFINITIONS_TYPE_TAGS: i64 = 1;

/// `HydrusCompression.DecompressBytesToBytes`.
fn decompress(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    if flate2::read::ZlibDecoder::new(data)
        .read_to_end(&mut out)
        .is_ok()
    {
        return Some(out);
    }
    // lz4.block.decompress: a little-endian i32 size, then the block
    let size = i32::from_le_bytes(data.get(..4)?.try_into().ok()?);
    let size = usize::try_from(size).ok()?;
    // an LZ4 block expands at most ~255x; refuse to allocate more than that
    if size > data.len().saturating_mul(255) {
        return None;
    }
    lz4_flex::block::decompress(&data[4..], size)
        .ok()
        .filter(|v| v.len() == size)
}

/// Python iteration over a JSON value: list items, string characters, dict keys.
fn py_iter(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(items) => Some(items.clone()),
        Value::String(s) => Some(s.chars().map(|c| Value::String(c.to_string())).collect()),
        Value::Object(map) => Some(map.keys().map(|k| Value::String(k.clone())).collect()),
        _ => None,
    }
}

/// `( a, b ) = value`.
fn py_unpack2(value: &Value) -> Option<(Value, Value)> {
    match <[Value; 2]>::try_from(py_iter(value)?) {
        Ok([a, b]) => Some((a, b)),
        Err(_) => None,
    }
}

/// Usable as a Python dict key.
fn py_hashable(value: &Value) -> bool {
    !matches!(value, Value::Array(_) | Value::Object(_))
}

/// `dict( pairs )`.
fn py_dict_ok(pairs: &Value) -> bool {
    py_iter(pairs).is_some_and(|items| {
        items
            .iter()
            .all(|item| py_unpack2(item).is_some_and(|(k, _)| py_hashable(&k)))
    })
}

/// `bytes.fromhex( s )`.
fn py_fromhex_ok(value: &Value) -> bool {
    let Value::String(s) = value else {
        return false;
    };
    let mut digits = 0usize;
    for c in s.chars() {
        if c.is_ascii_hexdigit() {
            digits += 1;
        } else if !(c.is_ascii_whitespace() && digits.is_multiple_of(2)) {
            // whitespace is allowed only between whole bytes
            return false;
        }
    }
    digits.is_multiple_of(2)
}

/// `ContentUpdate._InitialiseFromSerialisableInfo`.
fn content_info_ok(info: &Value) -> bool {
    py_iter(info).is_some_and(|rows| {
        rows.iter().all(|row| {
            py_unpack2(row).is_some_and(|(content_type, actions)| {
                py_hashable(&content_type) && py_dict_ok(&actions)
            })
        })
    })
}

/// `DefinitionsUpdate._InitialiseFromSerialisableInfo`.
fn definitions_info_ok(info: &Value) -> bool {
    py_iter(info).is_some_and(|rows| {
        rows.iter().all(|row| {
            let Some((kind, definitions)) = py_unpack2(row) else {
                return false;
            };
            // Python equality: 0 == 0.0 == False
            let kind = match kind {
                Value::Number(n) => n.as_f64(),
                Value::Bool(b) => Some(f64::from(u8::from(b))),
                _ => None,
            };
            let check = |f: &dyn Fn(&Value, &Value) -> bool| {
                py_iter(&definitions).is_some_and(|items| {
                    items.iter().all(|item| {
                        py_unpack2(item)
                            .is_some_and(|(key, value)| py_hashable(&key) && f(&key, &value))
                    })
                })
            };
            // exact, as Python's `==` is between an int and a float
            #[allow(clippy::float_cmp)]
            match kind {
                Some(k) if k == DEFINITIONS_TYPE_HASHES as f64 => check(&|_, v| py_fromhex_ok(v)),
                Some(k) if k == DEFINITIONS_TYPE_TAGS as f64 => check(&|_, _| true),
                _ => true,
            }
        })
    })
}

/// The update mime of these bytes, if they are a repository update.
pub(crate) fn mime(data: &[u8]) -> Option<Mime> {
    let json = decompress(data)?;
    let text = std::str::from_utf8(&json).ok()?;
    let value: Value = serde_json::from_str(text).ok()?;
    // only the three-item form works: these types take no name argument
    let items = py_iter(&value)?;
    let [kind, version, info] = <[Value; 3]>::try_from(items).ok()?;
    let version = match version {
        Value::Number(n) => n.as_f64()?,
        Value::Bool(b) => f64::from(u8::from(b)),
        _ => return None,
    };
    // older versions go through an update step these types do not have
    if version < 1.0 {
        return None;
    }
    match kind.as_u64()? {
        CONTENT_UPDATE if content_info_ok(&info) => Some(Mime::ApplicationHydrusUpdateContent),
        DEFINITIONS_UPDATE if definitions_info_ok(&info) => {
            Some(Mime::ApplicationHydrusUpdateDefinitions)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zlib(text: &str) -> Vec<u8> {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        e.write_all(text.as_bytes()).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn recognises_updates() {
        assert_eq!(
            mime(&zlib(r#"[34, 1, [[2, [[0, [["abcd", 5]]]]]]]"#)),
            Some(Mime::ApplicationHydrusUpdateContent)
        );
        assert_eq!(
            mime(&zlib(r#"[36, 1, [[0, [[1, "00ff"]]], [1, [[2, "tag"]]]]]"#)),
            Some(Mime::ApplicationHydrusUpdateDefinitions)
        );
        // bad hex, a name argument, unhashable keys, an old version
        assert_eq!(mime(&zlib(r#"[36, 1, [[0, [[1, "0g"]]]]]"#)), None);
        assert_eq!(mime(&zlib(r#"[36, "name", 1, []]"#)), None);
        assert_eq!(mime(&zlib("[34, 1, [[[1], []]]]")), None);
        assert_eq!(mime(&zlib("[34, 0, []]")), None);
        assert_eq!(mime(&zlib("[35, 1, []]")), None);
    }

    #[test]
    fn lz4_fallback() {
        let text = b"[34, 1, []]";
        let mut data = (text.len() as i32).to_le_bytes().to_vec();
        data.extend(lz4_flex::block::compress(text));
        assert_eq!(mime(&data), Some(Mime::ApplicationHydrusUpdateContent));
    }
}
