//! Small helpers for picking typed values out of serialised objects.
//!
//! Every helper takes the type being decoded so errors say where the bad
//! value was.

use hydrus_core::ServiceKey;

use crate::pyjson::PyJson;
use crate::serialisable::{Body, Meta, SerialisableError, SerialisableObject, SerialisableType};

pub(crate) type DecodeResult<T> = Result<T, SerialisableError>;

pub(crate) fn malformed(kind: SerialisableType, detail: impl Into<String>) -> SerialisableError {
    SerialisableError::malformed(kind, detail)
}

fn wrong(kind: SerialisableType, what: &str, expected: &str, found: &PyJson) -> SerialisableError {
    malformed(
        kind,
        format!("{what} should be {expected}, found {}", found.kind()),
    )
}

pub(crate) fn list<'a>(
    kind: SerialisableType,
    value: &'a PyJson,
    what: &str,
) -> DecodeResult<&'a [PyJson]> {
    value
        .as_list()
        .ok_or_else(|| wrong(kind, what, "a list", value))
}

/// A list of exactly `N` items (a Python tuple).
pub(crate) fn tuple<'a, const N: usize>(
    kind: SerialisableType,
    value: &'a PyJson,
    what: &str,
) -> DecodeResult<&'a [PyJson; N]> {
    let items = list(kind, value, what)?;
    <&[PyJson; N]>::try_from(items).map_err(|_| {
        malformed(
            kind,
            format!("{what} should have {N} items, found {}", items.len()),
        )
    })
}

pub(crate) fn string(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| wrong(kind, what, "a string", value))
}

pub(crate) fn opt_string(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Option<String>> {
    match value {
        PyJson::Null => Ok(None),
        other => string(kind, other, what).map(Some),
    }
}

pub(crate) fn int(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<i64> {
    value
        .as_i64()
        .ok_or_else(|| wrong(kind, what, "an integer", value))
}

pub(crate) fn opt_int(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Option<i64>> {
    match value {
        PyJson::Null => Ok(None),
        other => int(kind, other, what).map(Some),
    }
}

pub(crate) fn float(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<f64> {
    value
        .as_f64()
        .ok_or_else(|| wrong(kind, what, "a number", value))
}

/// A Python bool. Python stores `True`/`False`, but some old values were
/// written as 0/1 integers, which Python treats the same way.
pub(crate) fn boolean(kind: SerialisableType, value: &PyJson, what: &str) -> DecodeResult<bool> {
    match value {
        PyJson::Bool(b) => Ok(*b),
        PyJson::Int(0) => Ok(false),
        PyJson::Int(1) => Ok(true),
        other => Err(wrong(kind, what, "a bool", other)),
    }
}

pub(crate) fn hex_bytes(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Vec<u8>> {
    let text = value
        .as_str()
        .ok_or_else(|| wrong(kind, what, "a hex string", value))?;
    hex::decode(text).map_err(|e| malformed(kind, format!("{what} is not hex: {e}")))
}

pub(crate) fn service_key(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<ServiceKey> {
    hex_bytes(kind, value, what).map(ServiceKey::new)
}

pub(crate) fn service_keys(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Vec<ServiceKey>> {
    list(kind, value, what)?
        .iter()
        .map(|v| service_key(kind, v, what))
        .collect()
}

pub(crate) fn strings(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<Vec<String>> {
    list(kind, value, what)?
        .iter()
        .map(|v| string(kind, v, what))
        .collect()
}

/// Parse a nested serialised tuple stored as plain JSON.
pub(crate) fn nested(
    kind: SerialisableType,
    value: &PyJson,
    what: &str,
) -> DecodeResult<SerialisableObject> {
    SerialisableObject::from_tuple(value).map_err(|e| malformed(kind, format!("{what}: {e}")))
}

/// The pairs of a dictionary object, upgrading an old-format dictionary.
pub(crate) fn dictionary_pairs(
    object: &SerialisableObject,
) -> DecodeResult<std::borrow::Cow<'_, [(Meta, Meta)]>> {
    object.expect_kind(SerialisableType::DICTIONARY)?;
    if let Body::Dictionary(pairs) = &object.body {
        return Ok(std::borrow::Cow::Borrowed(pairs));
    }
    match object.upgraded()?.body {
        Body::Dictionary(pairs) => Ok(std::borrow::Cow::Owned(pairs)),
        _ => Err(malformed(
            SerialisableType::DICTIONARY,
            "contents are not meta-encoded",
        )),
    }
}

/// The items of a list object, upgrading an old-format list.
pub(crate) fn list_items(
    object: &SerialisableObject,
) -> DecodeResult<std::borrow::Cow<'_, [Meta]>> {
    object.expect_kind(SerialisableType::LIST)?;
    if let Body::List(items) = &object.body {
        return Ok(std::borrow::Cow::Borrowed(items));
    }
    match object.upgraded()?.body {
        Body::List(items) => Ok(std::borrow::Cow::Owned(items)),
        _ => Err(malformed(
            SerialisableType::LIST,
            "contents are not meta-encoded",
        )),
    }
}

/// A value inside a dictionary that Python treats as a plain value: a
/// `[0, json]` meta value, or a list object whose items are all plain
/// (Python turns lists into `SerialisableList`s when saving dictionaries,
/// but older saves kept them as JSON).
pub(crate) fn plain(kind: SerialisableType, meta: &Meta, what: &str) -> DecodeResult<PyJson> {
    match meta {
        Meta::Json(value) => Ok(value.clone()),
        Meta::Object(object) if object.kind == SerialisableType::LIST => list_items(object)?
            .iter()
            .map(|item| plain(kind, item, what))
            .collect::<DecodeResult<Vec<_>>>()
            .map(PyJson::List),
        Meta::Object(object) => Err(malformed(
            kind,
            format!(
                "{what} should be a plain value, found a {} object",
                object.kind
            ),
        )),
        Meta::Bytes(_) => Err(malformed(
            kind,
            format!("{what} should be a plain value, found bytes"),
        )),
    }
}

/// A string-keyed view of a dictionary object, for objects the reference
/// keeps as `SerialisableDictionary`s of named settings.
#[derive(Debug)]
pub(crate) struct Settings<'a> {
    kind: SerialisableType,
    pairs: std::borrow::Cow<'a, [(Meta, Meta)]>,
}

impl<'a> Settings<'a> {
    pub(crate) fn new(
        kind: SerialisableType,
        object: &'a SerialisableObject,
    ) -> DecodeResult<Self> {
        Ok(Settings {
            kind,
            pairs: dictionary_pairs(object)?,
        })
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Meta> {
        self.pairs
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    /// A plain value, or `None` if the setting is absent.
    pub(crate) fn plain(&self, key: &str) -> DecodeResult<Option<PyJson>> {
        self.get(key)
            .map(|meta| plain(self.kind, meta, key))
            .transpose()
    }
}
