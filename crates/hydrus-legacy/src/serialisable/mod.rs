//! The reference's versioned serialised objects, generically.
//!
//! Every configuration object in a reference database is stored as JSON in
//! one of two shapes (see `hydrus/core/HydrusSerialisable.py`):
//!
//! * `[type_id, version, info]`, or
//! * `[type_id, name, version, info]` for named types,
//!
//! where `info` is type-specific. Database tables store the three or four
//! parts in separate columns (the `dump` column holds just `info`), while
//! nested objects and the services table store whole tuples.
//!
//! [`SerialisableObject`] is a lossless representation: `info` is kept as
//! [`PyJson`], except for the three generic container types, whose
//! "meta-encoded" entries are decoded into a tree so nested objects are
//! reachable:
//!
//! * `SERIALISABLE_TYPE_DICTIONARY` (21) — `[[meta_key, meta_value], ...]`
//! * `SERIALISABLE_TYPE_LIST` (26) — `[meta_value, ...]`
//! * `SERIALISABLE_TYPE_BYTES_DICT` (33) — `[[int | hex, null | hex | [hex, ...]], ...]`
//!
//! A meta value is `[0, json]`, `[1, "hex bytes"]` or `[2, nested tuple]`
//! ([`Meta`]). If a container's contents are not exactly in that form (which
//! the reference never writes) the container is kept as [`Body::Other`] so
//! that re-serialising it is still byte-for-byte identical.
//!
//! Old versions of the containers (dictionary v1, list v1/v2) are kept as
//! stored; [`SerialisableObject::upgraded`] applies the reference's
//! container upgrades when a caller needs the current form. Typed decoders
//! for specific object types live in [`crate::objects`].

mod types;

pub use types::SerialisableType;

use crate::pyjson::{JsonError, PyJson};

/// Why a serialised object could not be decoded.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SerialisableError {
    #[error(transparent)]
    Json(#[from] JsonError),

    #[error("expected a serialisable tuple [type, (name,) version, info], found {0}")]
    NotATuple(String),

    #[error("expected a {expected} object, found {found}")]
    WrongType {
        expected: SerialisableType,
        found: SerialisableType,
    },

    #[error("{kind} version {version} is newer than v688's version {current}")]
    FutureVersion {
        kind: SerialisableType,
        version: u32,
        current: u32,
    },

    #[error("{kind} version {version} is not supported ({detail})")]
    UnsupportedVersion {
        kind: SerialisableType,
        version: u32,
        detail: &'static str,
    },

    #[error("malformed {kind}: {detail}")]
    Malformed {
        kind: SerialisableType,
        detail: String,
    },
}

impl SerialisableError {
    pub(crate) fn malformed(kind: SerialisableType, detail: impl Into<String>) -> Self {
        SerialisableError::Malformed {
            kind,
            detail: detail.into(),
        }
    }
}

/// A serialised object: its type, optional name, stored version and contents.
#[derive(Debug, Clone, PartialEq)]
pub struct SerialisableObject {
    pub kind: SerialisableType,
    /// Present for named types (the four-element tuple form).
    pub name: Option<String>,
    /// The version the object was stored at, which may be older than
    /// v688's current version for objects the reference has not re-saved.
    pub version: u32,
    pub body: Body,
}

/// The contents (`info`) of a serialised object.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// A current-version `SERIALISABLE_TYPE_DICTIONARY`: key/value pairs in stored order.
    Dictionary(Vec<(Meta, Meta)>),
    /// A current-version `SERIALISABLE_TYPE_BYTES_DICT`.
    BytesDictionary(Vec<(BytesKey, BytesValue)>),
    /// A current-version `SERIALISABLE_TYPE_LIST`.
    List(Vec<Meta>),
    /// Anything else, verbatim.
    Other(PyJson),
}

/// A meta-encoded value inside a generic container.
#[derive(Debug, Clone, PartialEq)]
pub enum Meta {
    /// `[0, value]`: plain JSON (Python tuples appear as lists).
    Json(PyJson),
    /// `[1, "hex"]`: a Python `bytes`.
    Bytes(Vec<u8>),
    /// `[2, tuple]`: a nested serialisable object.
    Object(Box<SerialisableObject>),
}

/// A key of a `SERIALISABLE_TYPE_BYTES_DICT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BytesKey {
    Int(i64),
    Bytes(Vec<u8>),
}

/// A value of a `SERIALISABLE_TYPE_BYTES_DICT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BytesValue {
    None,
    Bytes(Vec<u8>),
    List(Vec<Vec<u8>>),
}

const META_JSON: i64 = 0;
const META_BYTES: i64 = 1;
const META_OBJECT: i64 = 2;

impl SerialisableObject {
    /// Parse a whole serialised tuple from its JSON text (as stored in
    /// `services.dictionary_string`).
    pub fn from_tuple_str(text: &str) -> Result<Self, SerialisableError> {
        Self::from_tuple(&PyJson::parse(text)?)
    }

    /// Parse a serialised tuple.
    pub fn from_tuple(value: &PyJson) -> Result<Self, SerialisableError> {
        let not_a_tuple = || SerialisableError::NotATuple(truncated(value));
        let items = value.as_list().ok_or_else(not_a_tuple)?;
        let (kind, name, version, info) = match items {
            [kind, version, info] => (kind, None, version, info),
            [kind, PyJson::Str(name), version, info] => (kind, Some(name.clone()), version, info),
            _ => return Err(not_a_tuple()),
        };
        let kind = kind
            .as_i64()
            .and_then(|k| u16::try_from(k).ok())
            .ok_or_else(not_a_tuple)?;
        let version = version
            .as_i64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(not_a_tuple)?;
        Ok(Self::from_parts(
            SerialisableType(kind),
            name,
            version,
            info.clone(),
        ))
    }

    /// Build an object from separately stored parts, as the `json_dumps*`
    /// tables hold them.
    pub fn from_parts(
        kind: SerialisableType,
        name: Option<String>,
        version: u32,
        info: PyJson,
    ) -> Self {
        let body = parse_body(kind, version, &info).unwrap_or(Body::Other(info));
        SerialisableObject {
            kind,
            name,
            version,
            body,
        }
    }

    /// Parse an object from separately stored parts where `info` is JSON text.
    pub fn from_stored(
        kind: SerialisableType,
        name: Option<String>,
        version: u32,
        info: &str,
    ) -> Result<Self, SerialisableError> {
        Ok(Self::from_parts(kind, name, version, PyJson::parse(info)?))
    }

    /// The `info` part as JSON.
    pub fn info(&self) -> PyJson {
        match &self.body {
            Body::Other(info) => info.clone(),
            Body::Dictionary(pairs) => PyJson::List(
                pairs
                    .iter()
                    .map(|(k, v)| PyJson::List(vec![k.to_json(), v.to_json()]))
                    .collect(),
            ),
            Body::List(items) => PyJson::List(items.iter().map(Meta::to_json).collect()),
            Body::BytesDictionary(pairs) => PyJson::List(
                pairs
                    .iter()
                    .map(|(k, v)| {
                        let key = match k {
                            BytesKey::Int(i) => PyJson::Int(*i),
                            BytesKey::Bytes(b) => PyJson::Str(hex::encode(b)),
                        };
                        let value = match v {
                            BytesValue::None => PyJson::Null,
                            BytesValue::Bytes(b) => PyJson::Str(hex::encode(b)),
                            BytesValue::List(items) => PyJson::List(
                                items.iter().map(|b| PyJson::Str(hex::encode(b))).collect(),
                            ),
                        };
                        PyJson::List(vec![key, value])
                    })
                    .collect(),
            ),
        }
    }

    /// The whole tuple as JSON.
    pub fn to_tuple(&self) -> PyJson {
        let mut items = vec![PyJson::Int(i64::from(self.kind.code()))];
        if let Some(name) = &self.name {
            items.push(PyJson::Str(name.clone()));
        }
        items.push(PyJson::Int(i64::from(self.version)));
        items.push(self.info());
        PyJson::List(items)
    }

    /// The `info` part exactly as the reference would store it.
    pub fn info_string(&self) -> String {
        self.info().to_python_string()
    }

    /// The whole tuple exactly as the reference would store it.
    pub fn to_tuple_string(&self) -> String {
        self.to_tuple().to_python_string()
    }

    /// Check this object's type, for typed decoders.
    pub fn expect_kind(&self, expected: SerialisableType) -> Result<(), SerialisableError> {
        if self.kind == expected {
            Ok(())
        } else {
            Err(SerialisableError::WrongType {
                expected,
                found: self.kind,
            })
        }
    }

    /// Reject objects stored by a newer reference version than v688.
    pub fn check_not_future(&self) -> Result<(), SerialisableError> {
        match self.kind.current_version() {
            Some(current) if self.version > current => Err(SerialisableError::FutureVersion {
                kind: self.kind,
                version: self.version,
                current,
            }),
            _ => Ok(()),
        }
    }

    /// A copy with the generic containers (dictionary v1, list v1 and v2)
    /// upgraded to their current forms, recursively, as the reference does
    /// on load. Other types are unchanged: their upgrades are the typed
    /// decoders' business.
    pub fn upgraded(&self) -> Result<SerialisableObject, SerialisableError> {
        let info = match (self.kind, self.version, &self.body) {
            (SerialisableType::DICTIONARY, 1, Body::Other(info)) => {
                Some((2, upgrade_dictionary_v1(info)?))
            }
            (SerialisableType::LIST, 1, Body::Other(info)) => {
                let v2 = upgrade_list_v1(info)?;
                Some((3, upgrade_list_v2(&v2)?))
            }
            (SerialisableType::LIST, 2, Body::Other(info)) => Some((3, upgrade_list_v2(info)?)),
            _ => None,
        };
        let mut object = match info {
            Some((version, info)) => {
                let body = parse_body(self.kind, version, &info).ok_or_else(|| {
                    SerialisableError::malformed(self.kind, "contents are not meta-encoded")
                })?;
                SerialisableObject {
                    kind: self.kind,
                    name: self.name.clone(),
                    version,
                    body,
                }
            }
            None => self.clone(),
        };
        match &mut object.body {
            Body::Dictionary(pairs) => {
                for (k, v) in pairs {
                    k.upgrade_in_place()?;
                    v.upgrade_in_place()?;
                }
            }
            Body::List(items) => {
                for item in items {
                    item.upgrade_in_place()?;
                }
            }
            Body::BytesDictionary(_) | Body::Other(_) => {}
        }
        Ok(object)
    }

    /// Visit this object and every object nested in its containers.
    pub fn visit(&self, f: &mut impl FnMut(&SerialisableObject)) {
        f(self);
        let mut visit_meta = |m: &Meta| {
            if let Meta::Object(o) = m {
                o.visit(f);
            }
        };
        match &self.body {
            Body::Dictionary(pairs) => {
                for (k, v) in pairs {
                    visit_meta(k);
                    visit_meta(v);
                }
            }
            Body::List(items) => items.iter().for_each(visit_meta),
            Body::BytesDictionary(_) | Body::Other(_) => {}
        }
    }
}

impl Meta {
    /// Decode a `[metatype, payload]` pair.
    pub fn from_json(value: &PyJson) -> Option<Meta> {
        let [metatype, payload] = value.as_list()? else {
            return None;
        };
        match metatype.as_i64()? {
            META_JSON => Some(Meta::Json(payload.clone())),
            META_BYTES => decode_canonical_hex(payload.as_str()?).map(Meta::Bytes),
            META_OBJECT => SerialisableObject::from_tuple(payload)
                .ok()
                .map(|o| Meta::Object(Box::new(o))),
            _ => None,
        }
    }

    /// Encode back to `[metatype, payload]`.
    pub fn to_json(&self) -> PyJson {
        match self {
            Meta::Json(value) => PyJson::List(vec![PyJson::Int(META_JSON), value.clone()]),
            Meta::Bytes(bytes) => PyJson::List(vec![
                PyJson::Int(META_BYTES),
                PyJson::Str(hex::encode(bytes)),
            ]),
            Meta::Object(object) => PyJson::List(vec![PyJson::Int(META_OBJECT), object.to_tuple()]),
        }
    }

    pub fn as_json(&self) -> Option<&PyJson> {
        match self {
            Meta::Json(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        self.as_json().and_then(PyJson::as_str)
    }

    pub fn as_object(&self) -> Option<&SerialisableObject> {
        match self {
            Meta::Object(object) => Some(object),
            _ => None,
        }
    }

    fn upgrade_in_place(&mut self) -> Result<(), SerialisableError> {
        if let Meta::Object(object) = self {
            **object = object.upgraded()?;
        }
        Ok(())
    }
}

/// Decode a container's contents, or `None` if `info` is not in the exact
/// shape the reference writes (the caller then keeps it verbatim).
fn parse_body(kind: SerialisableType, version: u32, info: &PyJson) -> Option<Body> {
    let current = kind.current_version() == Some(version);
    match kind {
        SerialisableType::DICTIONARY if current => info
            .as_list()?
            .iter()
            .map(|pair| match pair.as_list()? {
                [k, v] => Some((Meta::from_json(k)?, Meta::from_json(v)?)),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(Body::Dictionary),
        SerialisableType::LIST if current => info
            .as_list()?
            .iter()
            .map(Meta::from_json)
            .collect::<Option<Vec<_>>>()
            .map(Body::List),
        SerialisableType::BYTES_DICT if current => info
            .as_list()?
            .iter()
            .map(|pair| {
                let [k, v] = pair.as_list()? else {
                    return None;
                };
                let key = match k {
                    PyJson::Int(i) => BytesKey::Int(*i),
                    PyJson::Str(s) => BytesKey::Bytes(decode_canonical_hex(s)?),
                    _ => return None,
                };
                let value = match v {
                    PyJson::Null => BytesValue::None,
                    PyJson::Str(s) => BytesValue::Bytes(decode_canonical_hex(s)?),
                    PyJson::List(items) => BytesValue::List(
                        items
                            .iter()
                            .map(|i| decode_canonical_hex(i.as_str()?))
                            .collect::<Option<Vec<_>>>()?,
                    ),
                    _ => return None,
                };
                Some((key, value))
            })
            .collect::<Option<Vec<_>>>()
            .map(Body::BytesDictionary),
        _ => None,
    }
}

/// Hex as Python's `bytes.hex()` writes it (lowercase). Anything else would
/// not round-trip, so it is not decoded.
fn decode_canonical_hex(s: &str) -> Option<Vec<u8>> {
    if s.bytes().any(|b| b.is_ascii_uppercase()) {
        return None;
    }
    hex::decode(s).ok()
}

/// `SerialisableDictionary._UpdateSerialisableInfo` v1 -> v2.
fn upgrade_dictionary_v1(info: &PyJson) -> Result<PyJson, SerialisableError> {
    let malformed = || SerialisableError::malformed(SerialisableType::DICTIONARY, "bad v1 layout");
    let [
        simple_simple,
        simple_serialisable,
        serialisable_simple,
        serialisable_serialisable,
    ] = info.as_list().ok_or_else(malformed)?
    else {
        return Err(malformed());
    };
    let mut pairs = Vec::new();
    for (group, key_meta, value_meta) in [
        (simple_simple, META_JSON, META_JSON),
        (simple_serialisable, META_JSON, META_OBJECT),
        (serialisable_simple, META_OBJECT, META_JSON),
        (serialisable_serialisable, META_OBJECT, META_OBJECT),
    ] {
        for pair in group.as_list().ok_or_else(malformed)? {
            let [k, v] = pair.as_list().ok_or_else(malformed)? else {
                return Err(malformed());
            };
            pairs.push(PyJson::List(vec![
                PyJson::List(vec![PyJson::Int(key_meta), k.clone()]),
                PyJson::List(vec![PyJson::Int(value_meta), v.clone()]),
            ]));
        }
    }
    Ok(PyJson::List(pairs))
}

/// `SerialisableList._UpdateSerialisableInfo` v1 -> v2.
fn upgrade_list_v1(info: &PyJson) -> Result<PyJson, SerialisableError> {
    let items = info
        .as_list()
        .ok_or_else(|| SerialisableError::malformed(SerialisableType::LIST, "bad v1 layout"))?;
    Ok(PyJson::List(
        items
            .iter()
            .map(|item| PyJson::List(vec![PyJson::Bool(true), item.clone()]))
            .collect(),
    ))
}

/// `SerialisableList._UpdateSerialisableInfo` v2 -> v3.
fn upgrade_list_v2(info: &PyJson) -> Result<PyJson, SerialisableError> {
    let malformed = || SerialisableError::malformed(SerialisableType::LIST, "bad v2 layout");
    let items = info.as_list().ok_or_else(malformed)?;
    items
        .iter()
        .map(|item| {
            let [is_serialised, value] = item.as_list().ok_or_else(malformed)? else {
                return Err(malformed());
            };
            let meta = if is_serialised.as_bool().ok_or_else(malformed)? {
                META_OBJECT
            } else {
                META_JSON
            };
            Ok(PyJson::List(vec![PyJson::Int(meta), value.clone()]))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(PyJson::List)
}

fn truncated(value: &PyJson) -> String {
    let mut text = value.to_python_string();
    if text.len() > 80 {
        let mut end = 80;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("...");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(tuple: &str) -> SerialisableObject {
        let object = SerialisableObject::from_tuple_str(tuple).unwrap();
        assert_eq!(object.to_tuple_string(), tuple);
        object
    }

    #[test]
    fn dictionary_meta_encoding() {
        let object = round_trip(
            r#"[21, 2, [[[0, "colours"], [2, [26, 3, [[0, [0, [[0, 0, 0], [240, 240, 65]]]]]]]], [[1, "00ff"], [0, null]]]]"#,
        );
        let Body::Dictionary(pairs) = &object.body else {
            panic!("{object:?}");
        };
        assert_eq!(pairs[0].0.as_str(), Some("colours"));
        let list = pairs[0].1.as_object().unwrap();
        assert!(matches!(&list.body, Body::List(items) if items.len() == 1));
        assert_eq!(pairs[1].0, Meta::Bytes(vec![0, 255]));
    }

    #[test]
    fn named_objects_and_bytes_dicts() {
        let object = round_trip(r#"[76, "oracle", 2, ["00", true, [], [44, 1, []]]]"#);
        assert_eq!(object.name.as_deref(), Some("oracle"));
        let object = round_trip(r#"[33, 1, [[5, "abcd"], ["0f", null], [7, ["01", "02"]]]]"#);
        assert!(matches!(object.body, Body::BytesDictionary(ref pairs) if pairs.len() == 3));
    }

    #[test]
    fn irregular_containers_are_kept_verbatim() {
        for tuple in [
            r#"[21, 2, [[[0, "a"], [3, 1]]]]"#,
            r#"[21, 2, [[[1, "ABCD"], [0, 1]]]]"#,
            r"[26, 3, [[2, [1, 2]]]]",
            r#"[21, 2, {"not": "a list"}]"#,
        ] {
            let object = round_trip(tuple);
            assert!(matches!(object.body, Body::Other(_)), "{tuple}");
        }
    }

    #[test]
    fn container_upgrades() {
        let dictionary = SerialisableObject::from_tuple_str(
            r#"[21, 1, [[["a", 1]], [["b", [26, 3, []]]], [], []]]"#,
        )
        .unwrap();
        assert!(matches!(dictionary.body, Body::Other(_)));
        let upgraded = dictionary.upgraded().unwrap();
        assert_eq!(
            upgraded.to_tuple_string(),
            r#"[21, 2, [[[0, "a"], [0, 1]], [[0, "b"], [2, [26, 3, []]]]]]"#
        );
        let list = SerialisableObject::from_tuple_str(r"[26, 1, [[44, 1, []]]]").unwrap();
        assert_eq!(
            list.upgraded().unwrap().to_tuple_string(),
            r"[26, 3, [[2, [44, 1, []]]]]"
        );
        let list = SerialisableObject::from_tuple_str(r#"[26, 2, [[false, "x"]]]"#).unwrap();
        assert_eq!(
            list.upgraded().unwrap().to_tuple_string(),
            r#"[26, 3, [[0, "x"]]]"#
        );
    }
}
