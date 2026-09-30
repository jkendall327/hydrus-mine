//! Which of a session's cookies a request sends, and what a response's
//! `Set-Cookie` headers do to the session: Python's `http.cookiejar` with
//! its default policy, as the reference's `requests` sessions use it.

use hydrus_store::network::Cookie;

/// The parts of a request URL cookies care about.
#[derive(Debug, Clone)]
pub struct CookieUrl<'a> {
    pub scheme: &'a str,
    /// Without the port, lowercase.
    pub host: String,
    /// The path (with `;params`), starting with `/`.
    pub path: String,
}

impl<'a> CookieUrl<'a> {
    pub fn new(scheme: &'a str, netloc: &str, path: &str, params: &str) -> Self {
        let host = netloc
            .rsplit_once('@')
            .map_or(netloc, |(_, host)| host)
            .to_lowercase();
        // an IPv6 literal keeps its colons
        let host = match host.strip_prefix('[') {
            Some(v6) => v6.split(']').next().unwrap_or_default().to_owned(),
            None => host.split(':').next().unwrap_or_default().to_owned(),
        };
        let mut path = if params.is_empty() {
            path.to_owned()
        } else {
            format!("{path};{params}")
        };
        if !path.starts_with('/') {
            path.insert(0, '/');
        }
        Self { scheme, host, path }
    }

    /// `eff_request_host`: a host without dots gets `.local`.
    fn effective_host(&self) -> String {
        if self.host.contains('.') {
            self.host.clone()
        } else {
            format!("{}.local", self.host)
        }
    }
}

fn dotted(domain: &str) -> String {
    if domain.is_empty() || domain.starts_with('.') {
        domain.to_owned()
    } else {
        format!(".{domain}")
    }
}

fn path_ok(cookie_path: &str, request_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/') || request_path[cookie_path.len()..].starts_with('/')))
}

fn domain_ok(cookie_domain: &str, url: &CookieUrl<'_>) -> bool {
    let erhn = url.effective_host();
    let dotdomain = dotted(cookie_domain);
    let dotted_host = dotted(&url.host);
    let dotted_erhn = dotted(&erhn);
    (dotted_host.ends_with(&dotdomain) || dotted_erhn.ends_with(&dotdomain))
        && format!(".{erhn}").ends_with(&dotdomain)
}

/// The `Cookie` header for a request: the session's cookies for its
/// domain and path that have not expired (secure ones only over https),
/// longest path first.
pub fn cookie_header(cookies: &[Cookie], url: &CookieUrl<'_>, now: i64) -> Option<String> {
    let mut sending: Vec<&Cookie> = cookies
        .iter()
        .filter(|c| !(c.secure && url.scheme != "https" && url.scheme != "wss"))
        .filter(|c| c.expires.is_none_or(|e| e > now))
        .filter(|c| domain_ok(&c.domain, url))
        .filter(|c| path_ok(&c.path, &url.path))
        .collect();
    sending.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
    let parts: Vec<String> = sending
        .iter()
        .map(|c| match &c.value {
            None => c.name.clone(),
            Some(value) => format!("{}={value}", c.name),
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join("; "))
}

/// What a `Set-Cookie` header does to the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CookieChange {
    Set(Cookie),
    /// An expiry in the past: remove the cookie.
    Clear {
        domain: String,
        path: String,
        name: String,
    },
}

/// What a response's `Set-Cookie` headers do, in order. Cookies the policy
/// refuses (for another domain, malformed) are left out.
pub fn set_cookie_changes(headers: &[&str], url: &CookieUrl<'_>, now: i64) -> Vec<CookieChange> {
    headers
        .iter()
        .filter_map(|header| change(header, url, now))
        .collect()
}

const KNOWN_ATTRS: [&str; 7] = [
    "expires", "domain", "path", "secure", "version", "port", "max-age",
];
const VALUE_ATTRS: [&str; 8] = [
    "version",
    "expires",
    "max-age",
    "domain",
    "path",
    "port",
    "comment",
    "commenturl",
];
const BOOLEAN_ATTRS: [&str; 2] = ["discard", "secure"];

/// A parsed attribute value: absent (`secure`), present, or (for
/// `expires`) a time.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    None,
    Text(String),
    True,
    Time(i64),
}

fn strip_quotes(s: &str) -> &str {
    let s = s.strip_prefix('"').unwrap_or(s);
    s.strip_suffix('"').unwrap_or(s)
}

/// `http2time`, for the formats servers use.
fn http_time(s: &str) -> Option<i64> {
    let t = httpdate::parse_http_date(s.trim()).ok().or_else(|| {
        // the common "Wdy, DD-Mon-YYYY HH:MM:SS GMT" variant
        httpdate::parse_http_date(&s.trim().replacen('-', " ", 2)).ok()
    })?;
    let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

/// `parse_ns_headers`: the name and value, then the attributes, with known
/// attribute names lowercased and `expires` read as a time.
fn parse_header(header: &str) -> Vec<(String, Value)> {
    let mut pairs = Vec::new();
    let mut version_set = false;
    for (i, param) in header.split(';').enumerate() {
        let param = param.trim();
        let (key, value) = match param.split_once('=') {
            Some((k, v)) => (k.trim(), Value::Text(v.trim().to_owned())),
            None => (param, Value::None),
        };
        if key.is_empty() {
            if i == 0 {
                break;
            }
            continue;
        }
        let mut key = key.to_owned();
        let mut value = value;
        if i != 0 {
            let lower = key.to_lowercase();
            if KNOWN_ATTRS.contains(&lower.as_str()) {
                key = lower;
            }
            if key == "version" {
                if let Value::Text(v) = &value {
                    value = Value::Text(strip_quotes(v).to_owned());
                }
                version_set = true;
            } else if key == "expires"
                && let Value::Text(v) = &value
            {
                value = http_time(strip_quotes(v)).map_or(Value::None, Value::Time);
            }
        }
        pairs.push((key, value));
    }
    if !pairs.is_empty() && !version_set {
        pairs.push(("version".into(), Value::Text("0".into())));
    }
    pairs
}

fn change(header: &str, url: &CookieUrl<'_>, now: i64) -> Option<CookieChange> {
    let pairs = parse_header(header);
    let ((name, value), attrs) = pairs.split_first()?;
    let value = match value {
        Value::Text(v) => Some(v.clone()),
        _ => None,
    };
    // `_normalized_cookie_tuples`
    let mut standard: Vec<(String, Value)> = Vec::new();
    let mut rest: Vec<(String, Option<String>)> = Vec::new();
    let mut max_age_set = false;
    for (k, v) in attrs {
        let lower = k.to_lowercase();
        let mut k =
            if VALUE_ATTRS.contains(&lower.as_str()) || BOOLEAN_ATTRS.contains(&lower.as_str()) {
                lower
            } else {
                k.clone()
            };
        let mut v = v.clone();
        if BOOLEAN_ATTRS.contains(&k.as_str()) && v == Value::None {
            v = Value::True;
        }
        if standard.iter().any(|(key, _)| *key == k) {
            continue;
        }
        match k.as_str() {
            "domain" => match &v {
                Value::Text(d) => v = Value::Text(d.to_lowercase()),
                _ => return None,
            },
            "expires" => {
                if max_age_set {
                    continue;
                }
                if v == Value::None {
                    return None;
                }
            }
            "max-age" => {
                max_age_set = true;
                let Value::Text(t) = &v else { return None };
                let age: i64 = hydrus_core::numbers::py_int(t)?;
                k = "expires".into();
                v = Value::Time(now + age);
            }
            _ => {}
        }
        if VALUE_ATTRS.contains(&k.as_str()) || BOOLEAN_ATTRS.contains(&k.as_str()) {
            if v == Value::None && !matches!(k.as_str(), "port" | "comment" | "commenturl") {
                return None;
            }
            // a later max-age replaces an earlier expires
            standard.retain(|(key, _)| *key != k);
            standard.push((k, v));
        } else {
            rest.push((
                k,
                match v {
                    Value::Text(t) => Some(t),
                    _ => None,
                },
            ));
        }
    }
    let get = |key: &str| standard.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    // `_cookie_from_cookie_tuple`
    let version = match get("version") {
        Some(Value::Text(v)) => hydrus_core::numbers::py_int(v)?,
        _ => 0,
    };
    if version > 0 {
        // RFC 2965 cookies are refused by the default policy
        return None;
    }
    let secure = matches!(get("secure"), Some(Value::True));
    let path = match get("path") {
        Some(Value::Text(p)) if !p.is_empty() => p.clone(),
        _ => match url.path.rfind('/') {
            Some(i) if i > 0 => url.path[..i].to_owned(),
            _ => "/".to_owned(),
        },
    };
    let (domain, specified) = match get("domain") {
        Some(Value::Text(d)) => (dotted(d), true),
        _ => (url.effective_host(), false),
    };
    if specified && !domain_may_set(&domain, url) {
        return None;
    }
    let expires = match get("expires") {
        Some(Value::Time(t)) => Some(*t),
        _ => None,
    };
    if let Some(expires) = expires
        && expires <= now
    {
        return Some(CookieChange::Clear {
            domain,
            path,
            name: name.clone(),
        });
    }
    // requests strips escaped quotes from a quoted value
    let value = value.map(|v| {
        if v.starts_with('"') && v.ends_with('"') {
            v.replace("\\\"", "")
        } else {
            v
        }
    });
    Some(CookieChange::Set(Cookie {
        name: name.clone(),
        value,
        domain,
        path,
        expires,
        secure,
        rest,
    }))
}

/// `set_ok_domain` for a cookie that names its domain.
fn domain_may_set(domain: &str, url: &CookieUrl<'_>) -> bool {
    let erhn = url.effective_host();
    let undotted = domain.strip_prefix('.').unwrap_or(domain);
    // (hosts are lowercase here)
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    let local = erhn.ends_with(".local");
    if !undotted.contains('.') && !local {
        return false;
    }
    erhn.ends_with(domain) || (!erhn.starts_with('.') && format!(".{erhn}").ends_with(domain))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(u: &str) -> CookieUrl<'static> {
        let (scheme, rest) = u.split_once("://").unwrap();
        let scheme: &'static str = if scheme == "https" { "https" } else { "http" };
        let (netloc, path) = rest.split_once('/').map_or((rest, ""), |(n, p)| (n, p));
        CookieUrl::new(scheme, netloc, &format!("/{path}"), "")
    }

    fn cookie(name: &str, domain: &str, path: &str) -> Cookie {
        Cookie {
            name: name.into(),
            value: Some("v".into()),
            domain: domain.into(),
            path: path.into(),
            expires: None,
            secure: false,
            rest: Vec::new(),
        }
    }

    #[test]
    fn sends_matching_cookies_longest_path_first() {
        let jar = vec![
            cookie("a", ".example.com", "/"),
            cookie("b", "www.example.com", "/posts"),
            cookie("c", ".other.com", "/"),
            Cookie {
                secure: true,
                ..cookie("d", ".example.com", "/")
            },
            Cookie {
                expires: Some(10),
                ..cookie("e", ".example.com", "/")
            },
        ];
        assert_eq!(
            cookie_header(&jar, &url("http://www.example.com/posts/1"), 100).as_deref(),
            Some("b=v; a=v")
        );
        assert_eq!(
            cookie_header(&jar, &url("https://example.com/postsx"), 100).as_deref(),
            Some("a=v; d=v")
        );
    }

    #[test]
    fn reads_set_cookie_like_cookiejar() {
        let u = url("https://www.example.com/a/b");
        let changes = set_cookie_changes(
            &[
                "sid=abc; Path=/; Domain=example.com; HttpOnly; Max-Age=60",
                "pref=1",
                "gone=x; Expires=Thu, 01 Jan 1970 00:00:00 GMT",
                "evil=1; Domain=other.com",
            ],
            &u,
            1000,
        );
        assert_eq!(
            changes,
            vec![
                CookieChange::Set(Cookie {
                    name: "sid".into(),
                    value: Some("abc".into()),
                    domain: ".example.com".into(),
                    path: "/".into(),
                    expires: Some(1060),
                    secure: false,
                    rest: vec![("HttpOnly".into(), None)],
                }),
                CookieChange::Set(Cookie {
                    name: "pref".into(),
                    value: Some("1".into()),
                    domain: "www.example.com".into(),
                    path: "/a".into(),
                    expires: None,
                    secure: false,
                    rest: Vec::new(),
                }),
                CookieChange::Clear {
                    domain: "www.example.com".into(),
                    path: "/a".into(),
                    name: "gone".into(),
                },
            ]
        );
    }
}
