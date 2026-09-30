//! Request parameters.
//!
//! The Client API takes parameters from the query string (GET) or a JSON or
//! CBOR body (POST). Each known parameter name has a fixed type, and values
//! are converted on the way in (hex strings to bytes, JSON-encoded query
//! values to JSON) exactly as the reference does, including its quirks:
//! query values of string and JSON parameters are percent-decoded a second
//! time, and in a query string only known parameter names are kept.
//!
//! Handlers then read typed values with [`Params::optional`] and
//! [`Params::required`].

use std::collections::HashMap;

use base64::Engine as _;
use serde_json::Value as Json;

use hydrus_core::{ServiceKey, Sha256};

use crate::error::{ApiError, ApiResult};

const INT_PARAMS: &[&str] = &[
    "duplicate_pair_sort_type",
    "file_id",
    "file_sort_type",
    "potentials_search_type",
    "pixel_duplicates",
    "max_hamming_distance",
    "max_num_pairs",
    "width",
    "height",
    "render_format",
    "render_quality",
];

const BYTE_PARAMS: &[&str] = &[
    "hash",
    "destination_page_key",
    "page_key",
    "page_of_pages_key",
    "service_key",
    "Hydrus-Client-API-Access-Key",
    "Hydrus-Client-API-Session-Key",
    "file_service_key",
    "deleted_file_service_key",
    "tag_service_key",
    "tag_service_key_1",
    "tag_service_key_2",
    "rating_service_key",
    "job_status_key",
];

const STRING_PARAMS: &[&str] = &[
    "name",
    "url",
    "domain",
    "search",
    "service_name",
    "reason",
    "tag_display_type",
    "source_hash_type",
    "desired_hash_type",
    "file_service_name",
    "tag_service_name",
];

const JSON_PARAMS: &[&str] = &[
    "basic_permissions",
    "permits_everything",
    "tags",
    "tags_1",
    "tags_2",
    "file_ids",
    "download",
    "only_return_identifiers",
    "only_return_basic_information",
    "include_blurhash",
    "create_new_file_ids",
    "detailed_url_information",
    "duplicate_pair_sort_asc",
    "hide_service_keys_tags",
    "simple",
    "file_sort_asc",
    "group_mode",
    "return_hashes",
    "return_file_ids",
    "include_thumbnail_filetype",
    "include_notes",
    "include_milliseconds",
    "include_services_object",
    "notes",
    "note_names",
    "doublecheck_file_system",
    "only_in_view",
    "include_current_tags",
    "include_pending_tags",
];

const BYTE_LIST_PARAMS: &[&str] = &["file_service_keys", "deleted_file_service_keys", "hashes"];

const BYTE_DICT_PARAMS: &[&str] = &[
    "service_keys_to_tags",
    "service_keys_to_actions_to_tags",
    "service_keys_to_additional_tags",
];

/// Byte parameters that may be written `sha256:<hex>`.
const HASH_PARAMS: &[&str] = &["hash"];

/// A parsed parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    Str(String),
    Json(Json),
    ByteList(Vec<Vec<u8>>),
    ByteDict(Vec<(Vec<u8>, Json)>),
}

/// A parameter that may be absent, explicitly `null`, or have a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nullable<T> {
    Absent,
    Null,
    Value(T),
}

impl<T> Nullable<T> {
    /// The value, if there is one.
    pub fn value(self) -> Option<T> {
        match self {
            Nullable::Value(v) => Some(v),
            Nullable::Absent | Nullable::Null => None,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Nullable<U> {
        match self {
            Nullable::Absent => Nullable::Absent,
            Nullable::Null => Nullable::Null,
            Nullable::Value(v) => Nullable::Value(f(v)),
        }
    }
}

/// The parameters of one request.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params {
    values: HashMap<String, Value>,
    /// Whether the client asked for CBOR (`cbor` query parameter).
    pub cbor_requested: bool,
}

fn unquote(s: &str) -> String {
    percent_encoding::percent_decode_str(s)
        .decode_utf8_lossy()
        .into_owned()
}

fn decode_hex(name: &str, value: &str) -> ApiResult<Vec<u8>> {
    hex::decode(value).map_err(|_| {
        ApiError::bad_request(format!(
            "I was expecting to parse '{name}' as a hex string, but it failed."
        ))
    })
}

fn cbor_to_json(bytes: &[u8]) -> Option<Json> {
    let value: ciborium::Value = ciborium::from_reader(bytes).ok()?;
    cbor_value_to_json(value)
}

fn cbor_value_to_json(value: ciborium::Value) -> Option<Json> {
    use ciborium::Value as C;
    Some(match value {
        C::Null => Json::Null,
        C::Bool(b) => Json::Bool(b),
        C::Integer(i) => Json::from(i64::try_from(i128::from(i)).ok()?),
        C::Float(f) => serde_json::Number::from_f64(f).map_or(Json::Null, Json::Number),
        C::Text(s) => Json::String(s),
        C::Bytes(b) => Json::String(hex::encode(b)),
        C::Array(items) => Json::Array(
            items
                .into_iter()
                .map(cbor_value_to_json)
                .collect::<Option<_>>()?,
        ),
        C::Map(pairs) => {
            let mut map = serde_json::Map::new();
            for (k, v) in pairs {
                let key = match k {
                    C::Text(s) => s,
                    other => cbor_value_to_json(other)?.to_string(),
                };
                map.insert(key, cbor_value_to_json(v)?);
            }
            Json::Object(map)
        }
        C::Tag(_, inner) => cbor_value_to_json(*inner)?,
        _ => return None,
    })
}

impl Params {
    /// Parse a raw (still percent-encoded) query string.
    pub fn from_query(raw_query: &str) -> ApiResult<Self> {
        let pairs: Vec<(String, String)> = form_urlencoded::parse(raw_query.as_bytes())
            .into_owned()
            .collect();
        let cbor_requested = pairs.iter().any(|(k, _)| k == "cbor");
        let mut params = Params {
            values: HashMap::new(),
            cbor_requested,
        };
        let mut seen = std::collections::HashSet::new();
        for (name, value) in pairs {
            // the reference only looks at the first value of a repeated name
            if !seen.insert(name.clone()) {
                continue;
            }
            let decode_structured = |value: &str| -> Option<Json> {
                if cbor_requested {
                    let bytes = base64::engine::general_purpose::URL_SAFE
                        .decode(value)
                        .ok()?;
                    cbor_to_json(&bytes)
                } else {
                    serde_json::from_str(&unquote(value)).ok()
                }
            };
            let parsed = if INT_PARAMS.contains(&name.as_str()) {
                let i = value.trim().parse::<i64>().map_err(|_| {
                    ApiError::bad_request(format!(
                        "I was expecting to parse '{name}' as an integer, but it failed."
                    ))
                })?;
                Value::Int(i)
            } else if BYTE_PARAMS.contains(&name.as_str()) {
                let hex_text = match value.split_once(':') {
                    Some((_, rest)) if HASH_PARAMS.contains(&name.as_str()) => rest,
                    _ => value.as_str(),
                };
                Value::Bytes(decode_hex(&name, hex_text)?)
            } else if STRING_PARAMS.contains(&name.as_str()) {
                Value::Str(unquote(&value))
            } else if JSON_PARAMS.contains(&name.as_str()) {
                let json = decode_structured(&value).ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "I was expecting to parse '{name}' as a json-encoded string, but it failed."
                    ))
                })?;
                Value::Json(json)
            } else if BYTE_LIST_PARAMS.contains(&name.as_str()) {
                let error = || {
                    ApiError::bad_request(format!(
                        "I was expecting to parse '{name}' as a json-encoded hex strings, but it failed."
                    ))
                };
                let json = decode_structured(&value).ok_or_else(error)?;
                let items = json.as_array().ok_or_else(error)?;
                let mut list = Vec::with_capacity(items.len());
                for item in items {
                    list.push(hex::decode(item.as_str().ok_or_else(error)?).map_err(|_| error())?);
                }
                Value::ByteList(list)
            } else {
                continue;
            };
            params.values.insert(name, parsed);
        }
        Ok(params)
    }

    /// Parse a JSON request body.
    pub fn from_json_body(body: &[u8]) -> ApiResult<Self> {
        let text = std::str::from_utf8(body).map_err(|e| {
            ApiError::bad_request(format!(
                "Sorry, did not understand the JSON you gave me: {e}"
            ))
        })?;
        let json: Json = serde_json::from_str(text).map_err(|e| {
            ApiError::bad_request(format!(
                "Sorry, did not understand the JSON you gave me: {e}"
            ))
        })?;
        Self::from_structured_body(json)
    }

    /// Parse a CBOR request body.
    pub fn from_cbor_body(body: &[u8]) -> ApiResult<Self> {
        let json = cbor_to_json(body).ok_or_else(|| {
            ApiError::bad_request("Sorry, did not understand the CBOR you gave me.")
        })?;
        Self::from_structured_body(json)
    }

    fn from_structured_body(json: Json) -> ApiResult<Self> {
        let Json::Object(map) = json else {
            return Err(ApiError::bad_request(
                "The given parameter did not seem to be a JSON Object!",
            ));
        };
        let mut values = HashMap::new();
        for (name, value) in map {
            // `null` for an optional value means "not given"
            if BYTE_PARAMS.contains(&name.as_str()) {
                if value.is_null() {
                    continue;
                }
                let bytes = value
                    .as_str()
                    .and_then(|s| hex::decode(s).ok())
                    .ok_or_else(|| {
                        ApiError::bad_request(format!(
                            "I was expecting to parse '{name}' as a hex string, but it failed."
                        ))
                    })?;
                if !bytes.is_empty() {
                    values.insert(name, Value::Bytes(bytes));
                }
            } else if BYTE_LIST_PARAMS.contains(&name.as_str()) {
                if value.is_null() {
                    continue;
                }
                let error = || {
                    ApiError::bad_request(format!(
                        "I was expecting to parse '{name}' as a list of hex strings, but it failed."
                    ))
                };
                let items = value.as_array().ok_or_else(error)?;
                let mut list = Vec::with_capacity(items.len());
                for item in items {
                    let bytes = item
                        .as_str()
                        .and_then(|s| hex::decode(s).ok())
                        .ok_or_else(error)?;
                    if !bytes.is_empty() {
                        list.push(bytes);
                    }
                }
                if !list.is_empty() {
                    values.insert(name, Value::ByteList(list));
                }
            } else if BYTE_DICT_PARAMS.contains(&name.as_str()) {
                if value.is_null() {
                    continue;
                }
                let error = || {
                    ApiError::bad_request(format!(
                        "I was expecting to parse '{name}' as a dictionary of hex strings to other data, but it failed."
                    ))
                };
                let Json::Object(dict) = value else {
                    return Err(error());
                };
                let mut out = Vec::with_capacity(dict.len());
                for (key, inner) in dict {
                    if key.is_empty() {
                        continue;
                    }
                    out.push((hex::decode(&key).map_err(|_| error())?, inner));
                }
                if !out.is_empty() {
                    values.insert(name, Value::ByteDict(out));
                }
            } else {
                values.insert(name, Value::Json(value));
            }
        }
        Ok(Params {
            values,
            cbor_requested: false,
        })
    }

    pub fn contains(&self, name: &str) -> bool {
        self.values
            .get(name)
            .is_some_and(|v| !matches!(v, Value::Json(Json::Null)))
    }

    /// A parameter's value; `null` counts as absent.
    pub fn raw(&self, name: &str) -> Option<&Value> {
        self.values
            .get(name)
            .filter(|v| !matches!(v, Value::Json(Json::Null)))
    }

    /// A parameter's value, including an explicit `null`.
    pub fn raw_or_null(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, value: Value) {
        self.values.insert(name.into(), value);
    }

    pub fn remove(&mut self, name: &str) -> Option<Value> {
        self.values.remove(name)
    }

    /// A parameter converted to `T`, or `None` if absent (or JSON `null`).
    pub fn optional<T: FromParam>(&self, name: &str) -> ApiResult<Option<T>> {
        match self.raw(name) {
            None => Ok(None),
            Some(value) => T::from_param(value).map(Some).ok_or_else(|| {
                ApiError::bad_request(format!(
                    "The parameter \"{name}\", with value \"{}\", was not the expected type: {}!",
                    display_value(value),
                    T::TYPE_NAME
                ))
            }),
        }
    }

    /// A parameter converted to `T`; a 400 if absent.
    pub fn required<T: FromParam>(&self, name: &str) -> ApiResult<T> {
        self.optional(name)?.ok_or_else(|| {
            ApiError::bad_request(format!("The required parameter \"{name}\" was missing!"))
        })
    }

    /// A parameter that may be given as `null`.
    pub fn nullable<T: FromParam>(&self, name: &str) -> ApiResult<Nullable<T>> {
        Ok(match self.raw_or_null(name) {
            None => Nullable::Absent,
            Some(Value::Json(Json::Null)) => Nullable::Null,
            Some(_) => self
                .optional(name)?
                .map_or(Nullable::Absent, Nullable::Value),
        })
    }

    /// A parameter converted to `T`, or `default` if absent.
    pub fn or<T: FromParam>(&self, name: &str, default: T) -> ApiResult<T> {
        Ok(self.optional(name)?.unwrap_or(default))
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::Int(i) => i.to_string(),
        Value::Bytes(b) => hex::encode(b),
        Value::Str(s) => s.clone(),
        Value::Json(j) => j.to_string(),
        Value::ByteList(l) => format!("{} items", l.len()),
        Value::ByteDict(d) => format!("{} entries", d.len()),
    }
}

/// Conversion from a parameter value to a typed value.
pub trait FromParam: Sized {
    const TYPE_NAME: &'static str;
    fn from_param(value: &Value) -> Option<Self>;
}

impl FromParam for bool {
    const TYPE_NAME: &'static str = "boolean";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Json(Json::Bool(b)) => Some(*b),
            _ => None,
        }
    }
}

impl FromParam for i64 {
    const TYPE_NAME: &'static str = "integer";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Int(i) => Some(*i),
            Value::Json(Json::Number(n)) => n.as_i64(),
            _ => None,
        }
    }
}

impl FromParam for f64 {
    const TYPE_NAME: &'static str = "float";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Int(i) => Some(*i as f64),
            Value::Json(Json::Number(n)) => n.as_f64(),
            _ => None,
        }
    }
}

impl FromParam for String {
    const TYPE_NAME: &'static str = "string";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Str(s) | Value::Json(Json::String(s)) => Some(s.clone()),
            _ => None,
        }
    }
}

impl FromParam for Json {
    const TYPE_NAME: &'static str = "json";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Json(j) => Some(j.clone()),
            Value::Str(s) => Some(Json::String(s.clone())),
            Value::Int(i) => Some(Json::from(*i)),
            _ => None,
        }
    }
}

impl FromParam for Vec<u8> {
    const TYPE_NAME: &'static str = "bytes";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Bytes(b) => Some(b.clone()),
            _ => None,
        }
    }
}

impl FromParam for ServiceKey {
    const TYPE_NAME: &'static str = "bytes";
    fn from_param(value: &Value) -> Option<Self> {
        Vec::<u8>::from_param(value).map(ServiceKey::new)
    }
}

impl FromParam for Vec<Vec<u8>> {
    const TYPE_NAME: &'static str = "list";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::ByteList(l) => Some(l.clone()),
            _ => None,
        }
    }
}

impl FromParam for Vec<Json> {
    const TYPE_NAME: &'static str = "list";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Json(Json::Array(items)) => Some(items.clone()),
            _ => None,
        }
    }
}

impl FromParam for Vec<String> {
    const TYPE_NAME: &'static str = "list";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Json(Json::Array(items)) => items
                .iter()
                .map(|i| i.as_str().map(str::to_owned))
                .collect(),
            _ => None,
        }
    }
}

impl FromParam for Vec<i64> {
    const TYPE_NAME: &'static str = "list";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::Json(Json::Array(items)) => items.iter().map(Json::as_i64).collect(),
            _ => None,
        }
    }
}

impl FromParam for Vec<(Vec<u8>, Json)> {
    const TYPE_NAME: &'static str = "dict";
    fn from_param(value: &Value) -> Option<Self> {
        match value {
            Value::ByteDict(d) => Some(d.clone()),
            _ => None,
        }
    }
}

/// Check a list of byte strings are all sha256 hashes.
pub fn sha256_list(name: &str, raw: &[Vec<u8>]) -> ApiResult<Vec<Sha256>> {
    raw.iter()
        .map(|b| {
            Sha256::from_slice(b).map_err(|_| {
                ApiError::bad_request(format!(
                    "Sorry, one of the hashes you gave me ({}) in '{name}' was not 64 characters long (sha256)!",
                    hex::encode(b)
                ))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_values_are_typed_by_name() {
        let p = Params::from_query(
            "tags=%5B%22blue%20eyes%22%5D&file_sort_type=2&return_hashes=true&tag_service_key=6c6f63616c2074616773&search=a%2520b&unknown=1&hash=sha256:00ff",
        )
        .unwrap();
        assert_eq!(
            p.optional::<Vec<String>>("tags").unwrap(),
            Some(vec!["blue eyes".into()])
        );
        assert_eq!(p.required::<i64>("file_sort_type").unwrap(), 2);
        assert!(p.required::<bool>("return_hashes").unwrap());
        assert_eq!(
            p.required::<ServiceKey>("tag_service_key")
                .unwrap()
                .as_bytes(),
            b"local tags"
        );
        // string params are unquoted twice, like the reference
        assert_eq!(p.required::<String>("search").unwrap(), "a b");
        assert!(!p.contains("unknown"));
        assert_eq!(p.required::<Vec<u8>>("hash").unwrap(), vec![0, 255]);
    }

    #[test]
    fn bad_query_values_are_400s() {
        assert!(Params::from_query("file_id=abc").is_err());
        assert!(Params::from_query("tags=not%20json").is_err());
        assert!(Params::from_query("hash=zz").is_err());
        let p = Params::from_query("tags=%7B%7D").unwrap();
        let err = p.required::<Vec<String>>("tags").unwrap_err();
        assert_eq!(err.kind, crate::error::ErrorKind::BadRequest);
        assert!(p.required::<bool>("missing").is_err());
    }

    #[test]
    fn json_bodies_convert_byte_params() {
        let body = br#"{"hash": "00ff", "hashes": ["01", ""], "service_keys_to_tags": {"6c6f63616c2074616773": ["x"]}, "reason": null, "file_service_key": null}"#;
        let p = Params::from_json_body(body).unwrap();
        assert_eq!(p.required::<Vec<u8>>("hash").unwrap(), vec![0, 255]);
        assert_eq!(p.required::<Vec<Vec<u8>>>("hashes").unwrap(), vec![vec![1]]);
        assert!(!p.contains("reason"));
        assert!(!p.contains("file_service_key"));
        let dict = p
            .required::<Vec<(Vec<u8>, Json)>>("service_keys_to_tags")
            .unwrap();
        assert_eq!(dict[0].0, b"local tags");
        assert!(Params::from_json_body(b"[1]").is_err());
        assert!(Params::from_json_body(b"{").is_err());
    }
}
