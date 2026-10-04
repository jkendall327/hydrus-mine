//! URL classes: patterns recognising the URLs of a site ("a booru post
//! page"), which also say how to normalise such URLs so the same page is
//! always stored under the same URL.

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

use super::functions::{Query, ensure_url_is_encoded, parse_url, path_components};
use super::pyurl::{UrlParts, urlunparse};
use super::strings::{StringConverter, StringMatch, StringProcessor, translate_python_regex};
use crate::numbers::py_int;

/// What kind of page a URL is. Codes match the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum UrlType {
    Post = 0,
    Api = 1,
    File = 2,
    Gallery = 3,
    Watchable = 4,
    Unknown = 5,
    Next = 6,
    Desired = 7,
    Source = 8,
    SubGallery = 9,
}

impl UrlType {
    pub const ALL: [UrlType; 10] = [
        UrlType::Post,
        UrlType::Api,
        UrlType::File,
        UrlType::Gallery,
        UrlType::Watchable,
        UrlType::Unknown,
        UrlType::Next,
        UrlType::Desired,
        UrlType::Source,
        UrlType::SubGallery,
    ];

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        UrlType::ALL.iter().copied().find(|t| t.code() == code)
    }

    /// The name the Client API reports (the reference has none for
    /// [`UrlType::Source`]).
    pub fn name(self) -> Option<&'static str> {
        Some(match self {
            UrlType::Post => "post url",
            UrlType::Api => "api/redirect url",
            UrlType::File => "file url",
            UrlType::Gallery => "gallery url",
            UrlType::Watchable => "watchable url",
            UrlType::Unknown => "unknown url",
            UrlType::Next => "next page url",
            UrlType::Desired => "downloadable/pursuable url",
            UrlType::SubGallery => {
                "sub-gallery url (is queued even if creator found no post/file urls)"
            }
            UrlType::Source => return None,
        })
    }
}

impl From<UrlType> for u8 {
    fn from(t: UrlType) -> u8 {
        t.code()
    }
}

impl TryFrom<u8> for UrlType {
    type Error = String;
    fn try_from(code: u8) -> Result<Self, String> {
        UrlType::from_code(code).ok_or_else(|| format!("unknown url type {code}"))
    }
}

/// Why a URL doesn't fit a URL class, or can't be normalised by one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UrlClassError(pub String);

fn fail<T>(message: impl Into<String>) -> Result<T, UrlClassError> {
    Err(UrlClassError(message.into()))
}

/// Python's `re.escape`.
fn python_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if "()[]{}?*+-|^$\\.&~# \t\n\r\u{b}\u{c}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Which domains a URL class covers.
#[derive(Clone, Serialize, Deserialize)]
pub struct DomainMask {
    /// Sorted.
    pub raw_domains: Vec<String>,
    /// Python regexes, sorted.
    pub domain_regexes: Vec<String>,
    /// Also match `anything.domain`, not just `www*.domain`.
    pub match_subdomains: bool,
    /// Keep the subdomain when normalising.
    pub keep_matched_subdomains: bool,
    #[serde(skip)]
    compiled: Arc<OnceLock<Compiled>>,
}

#[derive(Default)]
struct Compiled {
    matchers: Vec<fancy_regex::Regex>,
    clippers: Vec<fancy_regex::Regex>,
}

impl std::fmt::Debug for DomainMask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DomainMask")
            .field("raw_domains", &self.raw_domains)
            .field("domain_regexes", &self.domain_regexes)
            .field("match_subdomains", &self.match_subdomains)
            .field("keep_matched_subdomains", &self.keep_matched_subdomains)
            .finish_non_exhaustive()
    }
}

impl PartialEq for DomainMask {
    fn eq(&self, other: &Self) -> bool {
        self.raw_domains == other.raw_domains
            && self.domain_regexes == other.domain_regexes
            && self.match_subdomains == other.match_subdomains
            && self.keep_matched_subdomains == other.keep_matched_subdomains
    }
}

impl DomainMask {
    pub fn new(
        mut raw_domains: Vec<String>,
        mut domain_regexes: Vec<String>,
        match_subdomains: bool,
        keep_matched_subdomains: bool,
    ) -> Self {
        raw_domains.sort();
        domain_regexes.sort();
        Self {
            raw_domains,
            domain_regexes,
            match_subdomains,
            keep_matched_subdomains,
            compiled: Arc::default(),
        }
    }

    /// What the reference groups URL classes by (it ignores
    /// `keep_matched_subdomains`).
    pub fn grouping_key(&self) -> (&[String], &[String], bool) {
        (
            &self.raw_domains,
            &self.domain_regexes,
            self.match_subdomains,
        )
    }

    fn compiled(&self) -> &Compiled {
        self.compiled.get_or_init(|| {
            let mut regexes: Vec<String> = self.domain_regexes.clone();
            regexes.extend(self.raw_domains.iter().map(|d| python_escape(d)));
            regexes.sort();
            let regexes: Vec<String> = regexes.iter().map(|r| translate_python_regex(r)).collect();
            let prefix = if self.match_subdomains {
                r"^(.*\.)?"
            } else {
                r"^(www[^\.]*\.)?"
            };
            // concatenated as the reference does, so a top-level `|` in a
            // user's regex means what it means there; `\A(?:...)` makes the
            // match start at the beginning, as Python's `re.match` does
            let compile = |pattern: String| fancy_regex::Regex::new(&pattern).ok();
            let matchers: Option<Vec<_>> = regexes
                .iter()
                .map(|r| compile(format!(r"\A(?:{prefix}{r}$)")))
                .collect();
            let clippers: Option<Vec<_>> =
                regexes.iter().map(|r| compile(format!("{r}$"))).collect();
            // like the reference, one bad regex disables the whole mask
            match (matchers, clippers) {
                (Some(matchers), Some(clippers)) => Compiled { matchers, clippers },
                _ => Compiled::default(),
            }
        })
    }

    /// Test a domain with the same mask description shown by the reference.
    pub fn test(&self, domain: &str) -> Result<(), UrlClassError> {
        fn summary(values: &[String], noun: &str) -> String {
            let mut values = values.to_vec();
            crate::sort::human_sort(&mut values);
            let full = values
                .iter()
                .map(|value| format!("\"{value}\""))
                .collect::<Vec<_>>()
                .join(", ");
            if full.chars().count() <= 48 {
                return full;
            }
            if values.len() > 1 {
                let leading = format!(
                    "\"{}\" & {} other {noun}",
                    values[0],
                    crate::numbers::human_int(u64::try_from(values.len() - 1).unwrap_or(u64::MAX))
                );
                if leading.chars().count() <= 48 {
                    return leading;
                }
            }
            format!(
                "{} {noun}",
                crate::numbers::human_int(u64::try_from(values.len()).unwrap_or(u64::MAX))
            )
        }
        if self.matches(domain) {
            return Ok(());
        }
        let mut description = if self.raw_domains.is_empty() {
            String::new()
        } else {
            summary(&self.raw_domains, "domains")
        };
        if !self.domain_regexes.is_empty() {
            description.push_str(&summary(&self.domain_regexes, "domain regexes"));
        }
        if self.raw_domains.is_empty() && self.domain_regexes.is_empty() {
            description = "no domain rules, will not match anything!".into();
        }
        let subdomains = if self.match_subdomains {
            " (potentially excluding subdomains)"
        } else {
            ""
        };
        fail(format!(
            "{domain}{subdomains} did not match URL Domain Mask: {description}"
        ))
    }

    pub fn matches(&self, domain: &str) -> bool {
        self.compiled()
            .matchers
            .iter()
            .any(|r| r.is_match(domain).unwrap_or(false))
    }

    /// The domain to store: as given if subdomains are kept, else the part
    /// the mask matched.
    pub fn normalise(&self, domain: &str) -> Result<String, UrlClassError> {
        if self.keep_matched_subdomains {
            return Ok(domain.to_owned());
        }
        for clipper in &self.compiled().clippers {
            if let Ok(Some(m)) = clipper.find(domain) {
                return Ok(m.as_str().to_owned());
            }
        }
        fail("Could not match that domain with this domain mask!")
    }

    /// Longer domain rules sort first.
    pub fn sorting_complexity(&self) -> usize {
        self.raw_domains
            .iter()
            .chain(&self.domain_regexes)
            .map(|d| d.chars().count())
            .max()
            .unwrap_or(10)
    }
}

/// A named query parameter a URL class expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlParameter {
    pub name: String,
    pub value: StringMatch,
    /// A token that is not part of the page's identity (e.g. a session
    /// token): only sent to the server.
    pub ephemeral: bool,
    /// Filled in when missing.
    pub default: Option<String>,
    pub default_processor: StringProcessor,
}

impl UrlParameter {
    fn must_be_in_original_url(&self) -> bool {
        self.default.is_none() && !self.ephemeral
    }

    /// The default value, run through its processor (falling back to the raw
    /// default if processing fails or yields nothing).
    fn processed_default(&self) -> Option<String> {
        let default = self.default.as_ref()?;
        Some(
            self.default_processor
                .process(vec![default.clone()])
                .ok()
                .and_then(|mut v| (!v.is_empty()).then(|| v.swap_remove(0)))
                .unwrap_or_else(|| default.clone()),
        )
    }
}

/// A URL class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UrlClass {
    pub name: String,
    /// Identifies the class (parsers are linked to it by this).
    #[serde(with = "hex_bytes")]
    pub key: Vec<u8>,
    pub url_type: UrlType,
    pub preferred_scheme: String,
    pub domain_mask: DomainMask,
    pub alphabetise_get_parameters: bool,
    pub no_more_path_components_than_this: bool,
    pub no_more_parameters_than_this: bool,
    pub keep_extra_parameters_for_server: bool,
    pub can_produce_multiple_files: bool,
    pub should_be_associated_with_files: bool,
    pub keep_fragment: bool,
    /// Each path component's test, and the value to fill in if it's missing.
    pub path_components: Vec<(StringMatch, Option<String>)>,
    pub parameters: Vec<UrlParameter>,
    pub has_single_value_parameters: bool,
    pub single_value_parameters_match: StringMatch,
    pub header_overrides: Vec<(String, String)>,
    /// Turns a URL into the URL to actually fetch (an API or redirect).
    pub api_lookup_converter: StringConverter,
    pub example_url: String,
    /// What requests for this class's URLs send as their referral URL.
    pub referral: Referral,
    /// Where a gallery URL keeps its page number, to make the next page's.
    pub gallery_index: Option<GalleryIndex>,
}

impl Default for UrlClass {
    /// A new class as the reference makes one: a post URL class for no
    /// domain, which alphabetises parameters, keeps extra ones for the
    /// server and is associated with files.
    fn default() -> Self {
        Self {
            name: "new url class".into(),
            key: Vec::new(),
            url_type: UrlType::Post,
            preferred_scheme: "https".into(),
            domain_mask: DomainMask::new(Vec::new(), Vec::new(), false, false),
            alphabetise_get_parameters: true,
            no_more_path_components_than_this: false,
            no_more_parameters_than_this: false,
            keep_extra_parameters_for_server: true,
            can_produce_multiple_files: false,
            should_be_associated_with_files: true,
            keep_fragment: false,
            path_components: Vec::new(),
            parameters: Vec::new(),
            has_single_value_parameters: false,
            single_value_parameters_match: StringMatch::any(),
            header_overrides: Vec::new(),
            api_lookup_converter: StringConverter::default(),
            example_url: String::new(),
            referral: Referral::default(),
            gallery_index: None,
        }
    }
}

/// What a request sends as its referral URL (`Referer`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Referral {
    pub mode: ReferralMode,
    /// Makes a referral URL from the URL being requested.
    pub converter: StringConverter,
}

/// The reference's `SEND_REFERRAL_URL_*`, in code order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferralMode {
    /// The page the URL was found on, if known.
    #[default]
    OnlyIfProvided,
    Never,
    /// The converter's, if the page the URL was found on isn't known.
    ConverterIfNoneProvided,
    /// Always the converter's.
    OnlyConverter,
}

impl ReferralMode {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::OnlyIfProvided,
            1 => Self::Never,
            2 => Self::ConverterIfNoneProvided,
            3 => Self::OnlyConverter,
            _ => return None,
        })
    }
}

/// Where a gallery URL keeps its page number, and how much the next page
/// adds to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GalleryIndex {
    pub position: GalleryIndexPosition,
    pub delta: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GalleryIndexPosition {
    /// A path component, by index (negative from the end, as in Python).
    PathComponent(i64),
    /// A query parameter, by name.
    Parameter(String),
}

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        hex::decode(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl UrlClass {
    /// Whether this class's URLs are fetched through another URL.
    pub fn uses_api_url(&self) -> bool {
        self.api_lookup_converter.makes_changes()
    }

    /// Check that a URL is one of this class's.
    pub fn test(&self, url: &str, collapse_leading_slashes: bool) -> Result<(), UrlClassError> {
        let url = ensure_url_is_encoded(url, true, collapse_leading_slashes);
        let parts = parse_url(&url).map_err(|e| UrlClassError(e.to_string()))?;
        self.domain_mask.test(&parts.netloc)?;
        self.test_path(&parts.path, collapse_leading_slashes)?;
        let query = Query::parse(&parts.query);
        if self.no_more_parameters_than_this
            && let Some((name, _)) = query
                .params
                .iter()
                .find(|(n, _)| !self.parameters.iter().any(|p| p.name == *n))
        {
            return fail(format!(
                "\"This has a \"{name}\" parameter, but I am set to not allow any unexpected parameters!"
            ));
        }
        for parameter in &self.parameters {
            let ok = match query.get(&parameter.name) {
                None => !parameter.must_be_in_original_url(),
                Some(value) => parameter.value.matches(value),
            };
            if !ok {
                return fail(format!(
                    "Problem with {} in {}",
                    parameter.name, parts.query
                ));
            }
        }
        if !query.single_values.is_empty()
            && !self.has_single_value_parameters
            && self.no_more_parameters_than_this
        {
            return fail("unexpected single-value parameters");
        }
        if self.has_single_value_parameters {
            if query.single_values.is_empty() {
                return fail(
                    "Was expecting single-value parameter(s), but this URL did not seem to have any.",
                );
            }
            if !query
                .single_values
                .iter()
                .all(|v| self.single_value_parameters_match.matches(v))
            {
                return fail("a single-value parameter did not match");
            }
        }
        Ok(())
    }

    fn test_path(&self, path: &str, collapse_leading_slashes: bool) -> Result<(), UrlClassError> {
        let components = path_components(path, collapse_leading_slashes);
        if self.no_more_path_components_than_this && components.len() > self.path_components.len() {
            return fail(format!(
                "\"{path}\" has {} path components, but I will not allow more than my defined {}!",
                components.len(),
                self.path_components.len()
            ));
        }
        for (index, (test, default)) in self.path_components.iter().enumerate() {
            let ok = match components.get(index) {
                Some(component) => test.matches(component),
                None => default.is_some(),
            };
            if !ok {
                return fail(format!(
                    "\"{path}\" does not fit path component {} (of {})",
                    index + 1,
                    self.path_components.len()
                ));
            }
        }
        Ok(())
    }

    pub fn matches(&self, url: &str, collapse_leading_slashes: bool) -> bool {
        self.test(url, collapse_leading_slashes).is_ok()
    }

    /// The URL this class stores for `url` (`for_server`: the URL to request,
    /// which may keep extra parameters and tokens).
    pub fn normalise(
        &self,
        url: &str,
        for_server: bool,
        collapse_leading_slashes: bool,
    ) -> Result<String, UrlClassError> {
        let parts = parse_url(url).map_err(|e| UrlClassError(e.to_string()))?;
        let netloc = self.domain_mask.normalise(&parts.netloc)?;
        let components = path_components(&parts.path, collapse_leading_slashes);
        let path = self.clip_path(components, for_server)?;
        let query = self.clip_query(Query::parse(&parts.query), for_server)?;
        Ok(urlunparse(&UrlParts {
            scheme: self.preferred_scheme.clone(),
            netloc,
            path,
            params: String::new(),
            query,
            fragment: if self.keep_fragment {
                parts.fragment
            } else {
                String::new()
            },
        }))
    }

    fn clip_path(
        &self,
        components: Vec<String>,
        for_server: bool,
    ) -> Result<String, UrlClassError> {
        let clip = self.uses_api_url() || !for_server;
        let flesh_out = components.len() < self.path_components.len();
        let components = if clip || flesh_out {
            self.path_components
                .iter()
                .enumerate()
                .map(
                    |(index, (_, default))| match (components.get(index), default) {
                        (Some(c), _) => Ok(c.clone()),
                        (None, Some(default)) => Ok(default.clone()),
                        (None, None) => {
                            fail("Could not clip path--given url appeared to be too short!")
                        }
                    },
                )
                .collect::<Result<Vec<_>, _>>()?
        } else {
            components
        };
        Ok(format!("/{}", components.join("/")))
    }

    fn clip_query(&self, mut query: Query, for_server: bool) -> Result<String, UrlClassError> {
        // which given parameter each of ours is, filling in defaults
        let mut known: Vec<(String, bool)> = Vec::new();
        for parameter in &self.parameters {
            if query.contains(&parameter.name) {
                known.push((parameter.name.clone(), parameter.ephemeral));
            } else if let Some(default) = parameter.processed_default() {
                query.set(&parameter.name, default);
                query.order.push(Some(parameter.name.clone()));
                known.push((parameter.name.clone(), parameter.ephemeral));
            } else if !parameter.ephemeral {
                return fail(format!(
                    "Could not flesh out query--no default for {} defined!",
                    parameter.name
                ));
            }
        }
        let keep_extras = for_server && self.keep_extra_parameters_for_server;
        let mut kept = Query {
            single_values: if self.has_single_value_parameters || keep_extras {
                query.single_values.clone()
            } else {
                Vec::new()
            },
            ..Query::default()
        };
        for (name, value) in &query.params {
            let keep = match known.iter().find(|(n, _)| n == name) {
                None => keep_extras,
                Some((_, ephemeral)) => !ephemeral || for_server,
            };
            if keep {
                kept.set(name, value.clone());
            }
        }
        let order = (!self.alphabetise_get_parameters).then_some(query.order.as_slice());
        Ok(kept.to_text(order))
    }

    /// The URL to fetch for `url`, if this class redirects to another URL.
    pub fn api_url(
        &self,
        url: &str,
        collapse_leading_slashes: bool,
    ) -> Result<String, UrlClassError> {
        let request_url = self.normalise(url, true, collapse_leading_slashes)?;
        self.api_lookup_converter
            .convert(&request_url)
            .map_err(|e| UrlClassError(e.to_string()))
    }

    /// The referral URL to send when requesting `url`, given the page it
    /// was found on (if known).
    pub fn referral_url(
        &self,
        url: &str,
        given: Option<&str>,
        collapse_leading_slashes: bool,
    ) -> Option<String> {
        let converted = || {
            self.normalise(url, true, collapse_leading_slashes)
                .ok()
                .and_then(|request_url| self.referral.converter.convert(&request_url).ok())
        };
        match self.referral.mode {
            ReferralMode::OnlyIfProvided => given.map(str::to_owned),
            ReferralMode::Never => None,
            ReferralMode::ConverterIfNoneProvided => match given {
                Some(given) => Some(given.to_owned()),
                None => converted(),
            },
            // (a converter that fails falls back to the given URL)
            ReferralMode::OnlyConverter => converted().or_else(|| given.map(str::to_owned)),
        }
    }

    /// Whether this class can make the next page of one of its galleries.
    pub fn can_generate_next_gallery_page(&self) -> bool {
        self.url_type == UrlType::Gallery && self.gallery_index.is_some()
    }

    /// The next page of a gallery URL: its page number moved on by the
    /// class's delta.
    pub fn next_gallery_page(
        &self,
        url: &str,
        collapse_leading_slashes: bool,
    ) -> Result<String, UrlClassError> {
        let Some(index) = &self.gallery_index else {
            return fail("Did not understand the next gallery page rules!");
        };
        let url = self.normalise(url, true, collapse_leading_slashes)?;
        let parts = parse_url(&url).map_err(|e| UrlClassError(e.to_string()))?;
        let not_an_integer =
            || fail("Could not generate next gallery page--index component was not an integer!");
        let mut path = parts.path.clone();
        let mut query_text = parts.query.clone();
        match &index.position {
            GalleryIndexPosition::PathComponent(i) => {
                let mut components = path_components(&parts.path, collapse_leading_slashes);
                let len = components.len() as i64;
                let at = if *i < 0 { len + i } else { *i };
                if at < 0 || at >= len {
                    return fail(
                        "Could not generate next gallery page--not enough path components!",
                    );
                }
                let component = &mut components[at as usize];
                let Some(page) = py_int(component) else {
                    return not_an_integer();
                };
                *component = (page + index.delta).to_string();
                path = format!("/{}", components.join("/"));
            }
            GalleryIndexPosition::Parameter(name) => {
                let mut query = Query::parse(&parts.query);
                let Some(value) = query.get(name) else {
                    return fail(format!(
                        "Could not generate next gallery page--did not find {name} in parameters!"
                    ));
                };
                let Some(page) = py_int(value) else {
                    return not_an_integer();
                };
                query.set(name, (page + index.delta).to_string());
                if !self.has_single_value_parameters {
                    query.single_values.clear();
                }
                let order = (!self.alphabetise_get_parameters).then_some(query.order.as_slice());
                query_text = query.to_text(order);
            }
        }
        Ok(urlunparse(&UrlParts {
            path,
            params: String::new(),
            query: query_text,
            ..parts
        }))
    }

    /// Classes with more specific rules are tried first.
    pub fn sorting_key(&self, collapse_leading_slashes: bool) -> [usize; 6] {
        let required_path = self
            .path_components
            .iter()
            .filter(|(_, d)| d.is_none())
            .count();
        let required_params = self
            .parameters
            .iter()
            .filter(|p| p.default.is_none())
            .count();
        let example_len = self
            .normalise(&self.example_url, true, collapse_leading_slashes)
            .unwrap_or_else(|_| self.example_url.clone())
            .chars()
            .count();
        [
            self.domain_mask.sorting_complexity(),
            required_path,
            self.path_components.len(),
            required_params,
            self.parameters.len(),
            example_len,
        ]
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::url::strings::{FlexibleMatch, MatchKind};

    pub(crate) fn booru_post() -> UrlClass {
        UrlClass {
            name: "booru file page".into(),
            key: vec![1; 32],
            url_type: UrlType::Post,
            preferred_scheme: "https".into(),
            domain_mask: DomainMask::new(
                vec!["booru.example.com".into()],
                Vec::new(),
                false,
                false,
            ),
            alphabetise_get_parameters: true,
            no_more_path_components_than_this: false,
            no_more_parameters_than_this: false,
            keep_extra_parameters_for_server: true,
            can_produce_multiple_files: false,
            should_be_associated_with_files: true,
            keep_fragment: false,
            path_components: vec![(StringMatch::fixed("index.php"), None)],
            parameters: vec![
                UrlParameter {
                    name: "page".into(),
                    value: StringMatch::fixed("post"),
                    ephemeral: false,
                    default: None,
                    default_processor: StringProcessor::default(),
                },
                UrlParameter {
                    name: "s".into(),
                    value: StringMatch::fixed("view"),
                    ephemeral: false,
                    default: None,
                    default_processor: StringProcessor::default(),
                },
                UrlParameter {
                    name: "id".into(),
                    value: StringMatch {
                        kind: MatchKind::Flexible(FlexibleMatch::Numeric),
                        ..StringMatch::any()
                    },
                    ephemeral: false,
                    default: None,
                    default_processor: StringProcessor::default(),
                },
            ],
            has_single_value_parameters: false,
            single_value_parameters_match: StringMatch::any(),
            header_overrides: Vec::new(),
            api_lookup_converter: StringConverter::default(),
            example_url: "https://booru.example.com/index.php?page=post&s=view&id=123".into(),
            referral: Referral::default(),
            gallery_index: None,
        }
    }

    #[test]
    fn classes_match_and_normalise() {
        let c = booru_post();
        let url = "http://www.booru.example.com/index.php?id=2000&s=view&page=post&extra=1#top";
        assert!(c.matches(url, false));
        assert_eq!(
            c.normalise(url, false, false).unwrap(),
            "https://booru.example.com/index.php?id=2000&page=post&s=view"
        );
        assert_eq!(
            c.normalise(url, true, false).unwrap(),
            "https://booru.example.com/index.php?extra=1&id=2000&page=post&s=view"
        );
        assert!(!c.matches(
            "https://booru.example.com/index.php?page=post&s=list",
            false
        ));
        assert!(!c.matches("https://example.com/index.php?page=post&s=view&id=1", false));
    }

    #[test]
    fn url_types_serialise_as_codes() {
        assert_eq!(serde_json::to_string(&UrlType::Unknown).unwrap(), "5");
    }
}
