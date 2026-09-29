//! The parts of Python's `urllib.parse` (as of Python 3.11) that hydrus's
//! URL handling is defined in terms of. Stored URLs are whatever these
//! functions produced, so normalising a URL the same way needs them exactly.

use unicode_normalization::UnicodeNormalization as _;

/// A URL split the way Python's `urlparse` splits it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlParts {
    /// Lowercased.
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    /// The `;params` of the last path segment (only for some schemes).
    pub params: String,
    pub query: String,
    pub fragment: String,
}

/// Why `urlparse` refused a URL.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlParseError {
    #[error("invalid IPv6 URL")]
    InvalidIpv6,
    #[error("netloc {0:?} contains invalid characters under NFKC normalization")]
    InvalidNetloc(String),
}

const SCHEMES_WITH_PARAMS: &[&str] = &[
    "", "ftp", "hdl", "prospero", "http", "imap", "https", "shttp", "rtsp", "rtsps", "rtspu",
    "sip", "sips", "mms", "sftp", "tel",
];

const SCHEMES_WITH_NETLOC: &[&str] = &[
    "", "ftp", "http", "gopher", "nntp", "telnet", "imap", "wais", "file", "mms", "https", "shttp",
    "snews", "prospero", "rtsp", "rtsps", "rtspu", "rsync", "svn", "svn+ssh", "sftp", "nfs", "git",
    "git+ssh", "ws", "wss",
];

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

/// Python's `urlparse(url)`.
pub fn urlparse(url: &str) -> Result<UrlParts, UrlParseError> {
    // leading C0 controls and spaces go; tabs and newlines go everywhere
    let url: String = url
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|&c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let mut rest = url.as_str();
    let mut scheme = String::new();
    if let Some(i) = rest.find(':')
        && i > 0
        && rest.starts_with(|c: char| c.is_ascii_alphabetic())
        && rest[..i].chars().all(is_scheme_char)
    {
        scheme = rest[..i].to_ascii_lowercase();
        rest = &rest[i + 1..];
    }
    let mut netloc = "";
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        netloc = &after[..end];
        rest = &after[end..];
        let (open, close) = (netloc.contains('['), netloc.contains(']'));
        if open != close {
            return Err(UrlParseError::InvalidIpv6);
        }
        if open {
            let after_open = netloc.split_once('[').map_or("", |(_, a)| a);
            let host = after_open.split_once(']').map_or(after_open, |(h, _)| h);
            check_bracketed_host(host)?;
        }
    }
    let (rest, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (mut path, query) = rest.split_once('?').unwrap_or((rest, ""));
    check_netloc(netloc)?;
    let mut params = "";
    if SCHEMES_WITH_PARAMS.contains(&scheme.as_str()) && path.contains(';') {
        let from = path.rfind('/').unwrap_or(0);
        if let Some(i) = path[from..].find(';').map(|i| i + from) {
            params = &path[i + 1..];
            path = &path[..i];
        }
    }
    Ok(UrlParts {
        scheme,
        netloc: netloc.to_owned(),
        path: path.to_owned(),
        params: params.to_owned(),
        query: query.to_owned(),
        fragment: fragment.to_owned(),
    })
}

/// A bracketed host must be an IPv6 address (optionally with a `%zone`) or
/// an `IPvFuture` literal.
fn check_bracketed_host(host: &str) -> Result<(), UrlParseError> {
    let ok = if let Some(future) = host.strip_prefix('v') {
        future.split_once('.').is_some_and(|(version, rest)| {
            !version.is_empty()
                && version.chars().all(|c| c.is_ascii_hexdigit())
                && !rest.is_empty()
        })
    } else {
        let address = host.split_once('%').map_or(host, |(a, _)| a);
        address.parse::<std::net::Ipv6Addr>().is_ok()
    };
    if ok {
        Ok(())
    } else {
        Err(UrlParseError::InvalidIpv6)
    }
}

/// Python refuses netlocs that NFKC-normalise to contain URL delimiters.
fn check_netloc(netloc: &str) -> Result<(), UrlParseError> {
    if netloc.is_ascii() {
        return Ok(());
    }
    let n: String = netloc
        .chars()
        .filter(|c| !matches!(c, '@' | ':' | '#' | '?'))
        .collect();
    let normalised: String = n.nfkc().collect();
    if n != normalised && normalised.contains(['/', '?', '#', '@', ':']) {
        return Err(UrlParseError::InvalidNetloc(netloc.to_owned()));
    }
    Ok(())
}

/// Python's `urlunparse`.
pub fn urlunparse(parts: &UrlParts) -> String {
    let mut url = parts.path.clone();
    if !parts.params.is_empty() {
        url = format!("{url};{}", parts.params);
    }
    if !parts.netloc.is_empty()
        || (!parts.scheme.is_empty() && SCHEMES_WITH_NETLOC.contains(&parts.scheme.as_str()))
        || url.starts_with("//")
    {
        if !url.is_empty() && !url.starts_with('/') {
            url.insert(0, '/');
        }
        url = format!("//{}{url}", parts.netloc);
    }
    if !parts.scheme.is_empty() {
        url = format!("{}:{url}", parts.scheme);
    }
    if !parts.query.is_empty() {
        url.push('?');
        url.push_str(&parts.query);
    }
    if !parts.fragment.is_empty() {
        url.push('#');
        url.push_str(&parts.fragment);
    }
    url
}

fn always_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'~')
}

/// Python's `quote(text, safe=safe)`: percent-encode the UTF-8 bytes of
/// everything but ASCII letters, digits, `_.-~` and the `safe` characters.
pub fn quote(text: &str, safe: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for &b in text.as_bytes() {
        if always_safe(b) || (b.is_ascii() && safe.as_bytes().contains(&b)) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn hex_value(b: u8) -> Option<u8> {
    char::from(b)
        .to_digit(16)
        .and_then(|d| u8::try_from(d).ok())
}

/// Python's `unquote(text)`: decode `%xx` escapes as UTF-8, replacing
/// invalid sequences; a `%` without two hex digits stays as it is.
pub fn unquote(text: &str) -> String {
    if !text.contains('%') {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
        {
            out.push(hi * 16 + lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(url: &str) -> [String; 6] {
        let p = urlparse(url).unwrap();
        [p.scheme, p.netloc, p.path, p.params, p.query, p.fragment]
    }

    #[test]
    fn splits_like_python() {
        assert_eq!(
            parse("HTTPS://A.com/x;p?q#f"),
            ["https", "A.com", "/x", "p", "q", "f"]
        );
        assert_eq!(parse("not a url"), ["", "", "not a url", "", "", ""]);
        assert_eq!(
            parse("  https://a.com/x\ty "),
            ["https", "a.com", "/xy ", "", "", ""]
        );
        assert_eq!(parse("mailto:x@y"), ["mailto", "", "x@y", "", "", ""]);
        assert_eq!(
            parse("https://a.com//x"),
            ["https", "a.com", "//x", "", "", ""]
        );
        assert!(urlparse("https://[::1/x").is_err());
    }

    #[test]
    fn joins_like_python() {
        for url in [
            "https://example.com/post/3",
            "https://a.com/x;p?q#f",
            "not a url",
            "mailto:x@y",
            "https://a.com//x",
        ] {
            assert_eq!(urlunparse(&urlparse(url).unwrap()), url);
        }
        assert_eq!(
            urlunparse(&urlparse("https://a.com/x?#").unwrap()),
            "https://a.com/x"
        );
    }

    #[test]
    fn quotes_like_python() {
        assert_eq!(
            quote("a b/c%?é~", "!$&'()*+,;=@:"),
            "a%20b%2Fc%25%3F%C3%A9~"
        );
        assert_eq!(unquote("a%20b%zz%C3%A9%"), "a b%zzé%");
    }
}
