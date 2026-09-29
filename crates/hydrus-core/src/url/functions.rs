//! Hydrus's URL helpers: splitting a URL into path components and query
//! parameters, re-encoding them, and the variants of a URL to look for when
//! checking whether a URL is known.

use unicode_normalization::UnicodeNormalization as _;

use super::pyurl::{self, UrlParts};

/// Why a text is not a URL hydrus can work with.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlError {
    #[error("could not parse {0:?} at all")]
    Unparseable(String),
    #[error("no scheme (did you forget http/https?) in {0:?}")]
    NoScheme(String),
    #[error("no domain in {0:?}")]
    NoDomain(String),
}

/// Characters left alone when encoding a query parameter's name or value
/// (RFC 3986 section 3.4).
pub const PARAM_EXCEPTION_CHARS: &str = "!$&'()*+,;=@:/?";
/// Characters left alone when encoding a path component (RFC 3986 section 3.3).
pub const PATH_EXCEPTION_CHARS: &str = "!$&'()*+,;=@:";

/// Percent-encode a component that may already be partly encoded, without
/// double-encoding existing `%xx` escapes (a `%` not followed by two hex
/// digits is encoded).
pub fn ensure_component_is_encoded(mixed: &str, safe: &str) -> String {
    let mut out = String::with_capacity(mixed.len());
    for (i, part) in mixed.split('%').enumerate() {
        if i > 0 {
            out.push('%');
            let escape = part.len() >= 2 && part.as_bytes()[..2].iter().all(u8::is_ascii_hexdigit);
            if !escape {
                out.push_str("25");
            }
        }
        out.push_str(&pyurl::quote(part, safe));
    }
    out
}

/// A URL's path as its `/`-separated components. The first `/` (or, with
/// `collapse_leading_slashes`, every leading `/`) is dropped first.
pub fn path_components(path: &str, collapse_leading_slashes: bool) -> Vec<String> {
    let path = if collapse_leading_slashes {
        path.trim_start_matches('/')
    } else {
        path.strip_prefix('/').unwrap_or(path)
    };
    path.split('/').map(str::to_owned).collect()
}

/// A query string as hydrus reads it: `name=value` parameters (a repeated
/// name keeps its first position and last value), bare "single value"
/// parameters, and the order everything came in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// `(name, value)` in first-seen order, names unique.
    pub params: Vec<(String, String)>,
    pub single_values: Vec<String>,
    /// Each parameter's name in the order given (repeats included), with
    /// `None` marking where each single value parameter came.
    pub order: Vec<Option<String>>,
}

impl Query {
    pub fn parse(query: &str) -> Self {
        let mut out = Query::default();
        for pair in query.split('&') {
            match pair.split_once('=') {
                None => {
                    if !pair.is_empty() {
                        out.single_values.push(pair.to_owned());
                        out.order.push(None);
                    }
                }
                Some((name, value)) => {
                    out.order.push(Some(name.to_owned()));
                    out.set(name, value.to_owned());
                }
            }
        }
        out
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    pub fn set(&mut self, name: &str, value: String) {
        match self.params.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = value,
            None => self.params.push((name.to_owned(), value)),
        }
    }

    /// The query text, parameters in `order` (or alphabetically, then the
    /// single values sorted, if `order` is `None`).
    pub fn to_text(&self, order: Option<&[Option<String>]>) -> String {
        let mut singles = self.single_values.clone();
        let alphabetical: Vec<Option<String>>;
        let order: &[Option<String>] = if let Some(order) = order {
            order
        } else {
            let mut names: Vec<Option<String>> =
                self.params.iter().map(|(n, _)| Some(n.clone())).collect();
            names.sort();
            singles.sort();
            names.extend(std::iter::repeat_n(None, singles.len()));
            alphabetical = names;
            &alphabetical
        };
        let mut next_single = singles.iter();
        let mut parts: Vec<String> = Vec::new();
        for key in order {
            match key {
                None => {
                    if let Some(single) = next_single.next() {
                        parts.push(single.clone());
                    }
                }
                Some(name) => {
                    if let Some(value) = self.get(name) {
                        parts.push(format!("{name}={value}"));
                    }
                }
            }
        }
        parts.join("&")
    }
}

/// NFKC-normalise a URL's domain (so look-alike characters compare equal),
/// and replace characters that can't be in a domain.
pub fn unicode_normalise_url(url: &str) -> String {
    if url.starts_with("file:") {
        return url.to_owned();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let (netloc, path_and_rest) = match rest.split_once('/') {
        Some((netloc, path)) => (netloc, Some(path)),
        None => (rest, None),
    };
    let netloc: String = netloc
        .nfkc()
        .map(|c| if matches!(c, '?' | '#') { '_' } else { c })
        .collect();
    match path_and_rest {
        Some(path) => format!("{scheme}://{netloc}/{path}"),
        None => format!("{scheme}://{netloc}"),
    }
}

/// Parse a URL as hydrus does: trimmed, domain normalised, then split.
pub fn parse_url(url: &str) -> Result<UrlParts, UrlError> {
    let url = unicode_normalise_url(url.trim());
    pyurl::urlparse(&url).map_err(|_| UrlError::Unparseable(url.clone()))
}

/// Check a text is a URL with a scheme and a domain.
pub fn check_full_url(text: &str) -> Result<UrlParts, UrlError> {
    let parts = parse_url(text).map_err(|_| UrlError::Unparseable(text.to_owned()))?;
    if parts.scheme.is_empty() {
        return Err(UrlError::NoScheme(text.to_owned()));
    }
    if parts.netloc.is_empty() {
        return Err(UrlError::NoDomain(text.to_owned()));
    }
    Ok(parts)
}

/// The domain (`netloc`) of a full URL.
pub fn url_domain(url: &str) -> Result<String, UrlError> {
    check_full_url(url).map(|p| p.netloc)
}

/// A URL with its path components and query parameters percent-encoded
/// (without double-encoding), and optionally without its fragment. Texts
/// that are not full URLs come back unchanged.
pub fn ensure_url_is_encoded(
    url: &str,
    keep_fragment: bool,
    collapse_leading_slashes: bool,
) -> String {
    let Ok(parts) = check_full_url(url) else {
        return url.to_owned();
    };
    let components: Vec<String> = path_components(&parts.path, collapse_leading_slashes)
        .iter()
        .map(|c| ensure_component_is_encoded(c, PATH_EXCEPTION_CHARS))
        .collect();
    let query = Query::parse(&parts.query);
    let encode = |s: &str| ensure_component_is_encoded(s, PARAM_EXCEPTION_CHARS);
    let mut encoded = Query {
        single_values: query.single_values.iter().map(|s| encode(s)).collect(),
        ..Query::default()
    };
    for (name, value) in &query.params {
        encoded.set(&encode(name), encode(value));
    }
    let order: Vec<Option<String>> = query
        .order
        .iter()
        .map(|k| k.as_deref().map(encode))
        .collect();
    pyurl::urlunparse(&UrlParts {
        scheme: parts.scheme,
        netloc: parts.netloc,
        path: format!("/{}", components.join("/")),
        params: parts.params,
        query: encoded.to_text(Some(&order)),
        fragment: if keep_fragment {
            parts.fragment
        } else {
            String::new()
        },
    })
}

/// The domain one level up (`maps.google.com` -> `google.com`).
fn next_level_domain(domain: &str) -> &str {
    domain.split_once('.').map_or("", |(_, rest)| rest)
}

/// The registrable domain, approximated as the last two labels (the
/// reference consults the public suffix list; see `docs/rust/DIFFERENCES.md`).
fn second_level_domain(domain: &str) -> &str {
    let mut domain = domain;
    while domain.matches('.').count() > 1 {
        domain = next_level_domain(domain);
    }
    domain
}

/// `www.example.com` -> `example.com`; other domains unchanged.
pub fn remove_www(domain: &str) -> &str {
    if domain != second_level_domain(domain)
        && domain.matches('.').count() > 1
        && domain.starts_with("www")
    {
        next_level_domain(domain)
    } else {
        domain
    }
}

/// Every form a URL may have been stored under: as given, normalised (for
/// the server and not), http/https swapped, with or without `www`, with a
/// legacy leading `//` of the path collapsed, and with or without a
/// trailing `/`.
pub fn search_urls(url: &str, normalised: &[String]) -> Vec<String> {
    let mut urls: Vec<String> = vec![url.to_owned()];
    let add = |urls: &mut Vec<String>, u: String| {
        if !urls.contains(&u) {
            urls.push(u);
        }
    };
    for n in normalised {
        add(&mut urls, n.clone());
    }
    for u in urls.clone() {
        if let Some(rest) = u.strip_prefix("http://") {
            add(&mut urls, format!("https://{rest}"));
        } else if let Some(rest) = u.strip_prefix("https://") {
            add(&mut urls, format!("http://{rest}"));
        }
    }
    for u in urls.clone() {
        let Ok(parts) = parse_url(&u) else {
            continue;
        };
        let rebuilt = |netloc: &str, path: &str| {
            pyurl::urlunparse(&UrlParts {
                scheme: parts.scheme.clone(),
                netloc: netloc.to_owned(),
                path: path.to_owned(),
                params: String::new(),
                query: parts.query.clone(),
                fragment: parts.fragment.clone(),
            })
        };
        if parts.path.starts_with("//") {
            let collapsed = format!("/{}", parts.path.trim_start_matches('/'));
            add(&mut urls, rebuilt(&parts.netloc, &collapsed));
        }
        // (like the reference, this also yields the URL without `;params`)
        let alternative = if parts.netloc.starts_with("www") {
            remove_www(&parts.netloc).to_owned()
        } else {
            format!("www.{}", parts.netloc)
        };
        add(&mut urls, rebuilt(&alternative, &parts.path));
    }
    for u in urls.clone() {
        match u.strip_suffix('/') {
            Some(without) => add(&mut urls, without.to_owned()),
            None => add(&mut urls, format!("{u}/")),
        }
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_encode_without_double_encoding() {
        assert_eq!(
            ensure_component_is_encoded("a b%20c%zz", PATH_EXCEPTION_CHARS),
            "a%20b%20c%25zz"
        );
        assert_eq!(ensure_component_is_encoded("6+girls", ""), "6%2Bgirls");
    }

    #[test]
    fn queries_round_trip_in_order() {
        let q = Query::parse("page=post&s=view&id=2000&&flag&id=3");
        assert_eq!(q.get("id"), Some("3"));
        assert_eq!(q.single_values, ["flag"]);
        assert_eq!(q.to_text(Some(&q.order)), "page=post&s=view&id=3&flag&id=3");
        assert_eq!(q.to_text(None), "id=3&page=post&s=view&flag");
    }

    #[test]
    fn urls_encode_like_the_reference() {
        assert_eq!(
            ensure_url_is_encoded("https://site.com/a b/é?q=x y#frag", false, false),
            "https://site.com/a%20b/%C3%A9?q=x%20y"
        );
        assert_eq!(
            ensure_url_is_encoded("not a url", false, false),
            "not a url"
        );
        assert_eq!(
            unicode_normalise_url("https://ｅｘａｍｐｌｅ.com/Ｘ"),
            "https://example.com/Ｘ"
        );
    }

    #[test]
    fn www_is_removed_from_subdomains_only() {
        assert_eq!(remove_www("www.example.com"), "example.com");
        assert_eq!(remove_www("example.com"), "example.com");
        assert_eq!(remove_www("www.com"), "www.com");
    }
}
