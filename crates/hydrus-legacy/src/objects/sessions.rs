//! Network sessions (`ClientNetworkingSessions.NetworkSessionManagerSessionContainer`,
//! type 96): the cookies the client holds for a network context.
//!
//! A session's cookies are stored as a pickled `requests` cookie jar (from
//! version 2; version 1 pickled the whole `requests.Session`), read here with
//! [`crate::pickle`].

use super::util::{DecodeResult, int, malformed, opt_string, tuple};
use crate::pickle::{self, Pickled};
use crate::serialisable::{SerialisableError, SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType(96);
const NETWORK_CONTEXT: SerialisableType = SerialisableType(47);

/// A stored cookie (the fields of Python's `http.cookiejar.Cookie` that
/// matter for sending it back).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredCookie {
    pub name: String,
    /// `None` for a cookie set without a value.
    pub value: Option<String>,
    pub domain: String,
    pub path: String,
    /// Seconds since the epoch; `None` for a session cookie.
    pub expires: Option<i64>,
    pub secure: bool,
    /// Other attributes (`HttpOnly`, `SameSite`, ...), with their values.
    pub rest: Vec<(String, Option<String>)>,
}

/// A network session: its context and cookies.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkSession {
    /// `CC.NETWORK_CONTEXT_*`: 0 global, 2 domain, ...
    pub context_type: i64,
    pub context_data: Option<String>,
    /// In the jar's order: domains, then paths within them, then names, each
    /// in the order they were first added.
    pub cookies: Vec<StoredCookie>,
}

fn bad(detail: impl Into<String>) -> SerialisableError {
    malformed(KIND, detail)
}

impl NetworkSession {
    pub fn from_object(object: &SerialisableObject) -> DecodeResult<Self> {
        object.expect_kind(KIND)?;
        object.check_not_future()?;
        if !matches!(object.version, 1 | 2) {
            return Err(SerialisableError::UnsupportedVersion {
                kind: KIND,
                version: object.version,
                detail: "session containers are version 1 or 2",
            });
        }
        let info = object.info();
        let [context, pickled] = tuple::<2>(KIND, &info, "session container")?;
        let context = SerialisableObject::from_tuple(context)
            .map_err(|e| bad(format!("network context: {e}")))?;
        context.expect_kind(NETWORK_CONTEXT)?;
        let context_info = context.info();
        let [context_type, context_data] = tuple::<2>(NETWORK_CONTEXT, &context_info, "context")?;
        let hex_text = pickled
            .as_str()
            .ok_or_else(|| bad("pickled cookies are not a string"))?;
        let bytes = hex::decode(hex_text).map_err(|_| bad("pickled cookies are not hex"))?;
        let value = pickle::read(&bytes).map_err(|e| bad(e.to_string()))?;
        // version 1 pickled the whole session; its jar is its `cookies`
        let jar = if object.version == 1 {
            value
                .state()
                .and_then(|s| s.get("cookies"))
                .cloned()
                .ok_or_else(|| bad("a pickled session without cookies"))?
        } else {
            value
        };
        Ok(NetworkSession {
            context_type: int(NETWORK_CONTEXT, context_type, "context type")?,
            context_data: opt_string(NETWORK_CONTEXT, context_data, "context data")?,
            cookies: jar_cookies(&jar)?,
        })
    }
}

/// The cookies of a pickled cookie jar, in the order Python iterates them.
pub fn jar_cookies(jar: &Pickled) -> DecodeResult<Vec<StoredCookie>> {
    let state = jar
        .state()
        .ok_or_else(|| bad("a cookie jar has no state"))?;
    let Some(Pickled::Dict(domains)) = state.get("_cookies") else {
        return Err(bad("a cookie jar has no cookies dict"));
    };
    let mut out = Vec::new();
    for paths in values(domains)? {
        let Pickled::Dict(paths) = paths else {
            return Err(bad("a domain's cookies are not a dict"));
        };
        for names in values(paths)? {
            let Pickled::Dict(names) = names else {
                return Err(bad("a path's cookies are not a dict"));
            };
            for cookie in values(names)? {
                out.push(cookie_of(cookie)?);
            }
        }
    }
    Ok(out)
}

/// A dict's values, in order (Python dicts keep insertion order, and the
/// cookie jar iterates them so).
fn values(pairs: &[(Pickled, Pickled)]) -> DecodeResult<Vec<&Pickled>> {
    pairs
        .iter()
        .map(|(k, v)| {
            k.as_str()
                .map(|_| v)
                .ok_or_else(|| bad("a cookie jar key is not a string"))
        })
        .collect()
}

fn cookie_of(cookie: &Pickled) -> DecodeResult<StoredCookie> {
    let state = cookie.state().ok_or_else(|| bad("a cookie has no state"))?;
    let text = |key: &str| -> DecodeResult<String> {
        state
            .get(key)
            .and_then(Pickled::as_str)
            .map(str::to_owned)
            .ok_or_else(|| bad(format!("a cookie's {key} is not a string")))
    };
    let value = match state.get("value") {
        None | Some(Pickled::None) => None,
        Some(Pickled::Str(s)) => Some(s.clone()),
        Some(_) => return Err(bad("a cookie's value is not a string")),
    };
    let expires = match state.get("expires") {
        None | Some(Pickled::None) => None,
        Some(Pickled::Int(i)) => Some(*i),
        Some(Pickled::Float(f)) => Some(*f as i64),
        Some(_) => return Err(bad("a cookie's expiry is not a number")),
    };
    let rest = match state.get("_rest") {
        None | Some(Pickled::None) => Vec::new(),
        Some(Pickled::Dict(pairs)) => pairs
            .iter()
            .map(|(k, v)| {
                let key = k
                    .as_str()
                    .ok_or_else(|| bad("a cookie attribute is not a string"))?;
                let value = match v {
                    Pickled::None => None,
                    Pickled::Str(s) => Some(s.clone()),
                    Pickled::Int(i) => Some(i.to_string()),
                    Pickled::Bool(b) => Some(b.to_string()),
                    _ => return Err(bad("a cookie attribute's value is not simple")),
                };
                Ok((key.to_owned(), value))
            })
            .collect::<DecodeResult<_>>()?,
        Some(_) => return Err(bad("a cookie's attributes are not a dict")),
    };
    Ok(StoredCookie {
        name: text("name")?,
        value,
        domain: text("domain")?,
        path: text("path")?,
        expires,
        secure: state
            .get("secure")
            .and_then(Pickled::as_bool)
            .unwrap_or(false),
        rest,
    })
}
