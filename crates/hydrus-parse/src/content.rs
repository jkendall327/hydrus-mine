//! Content parsers and page parsers: what a downloader gets from a page.
//!
//! A *content parser* runs a formula and labels what it finds (URLs of some
//! kind, tags in a namespace, a note, a hash, a time, a title, ...), or
//! vetoes the page. A *page parser* runs its content parsers over a page
//! into a *post*, and can split the page into several posts with
//! *subsidiary page parsers* (a gallery page's thumbnails, say), each post
//! also getting what the page itself said. The behaviour is the reference's
//! `ContentParser`/`PageParser` and its `ParsedPost`, checked on
//! `oracle/fixtures/page_parsers.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use hydrus_core::pybytes::{b64decode, fromhex};
use hydrus_core::tag::Tag;
use hydrus_core::url::functions::{check_full_url, ensure_url_is_encoded};
use hydrus_core::url::pyurl::urljoin;
use hydrus_core::url::strings::{StringConverter, StringMatch};

use crate::formula::{Formula, ParseError, ParsingContext};
use crate::text::{clean_note_text, html_unescape, py_strip};

/// `HC.URL_TYPE_*` for parsed URLs.
pub mod url_type {
    /// A file (or post) to download.
    pub const DESIRED: i64 = 7;
    /// Where the file came from, to associate with it.
    pub const SOURCE: i64 = 8;
    /// The gallery's next page.
    pub const NEXT: i64 = 6;
    /// A gallery within a gallery.
    pub const SUB_GALLERY: i64 = 9;
}

/// `HC.TIMESTAMP_TYPE_MODIFIED_DOMAIN`: when the site says a post was made.
pub const TIMESTAMP_MODIFIED_DOMAIN: i64 = 0;

/// What a content parser's results are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    Url {
        url_type: i64,
        priority: i64,
    },
    /// `None` keeps whatever namespace the text has.
    Tag {
        namespace: Option<String>,
    },
    Note {
        name: String,
    },
    Hash {
        hash_type: String,
        encoding: String,
    },
    Timestamp {
        timestamp_type: Option<i64>,
    },
    Title {
        priority: i64,
    },
    HttpHeader {
        name: String,
    },
    Variable {
        name: String,
    },
    /// Stop: the page is not wanted if a result matches (or if none does).
    Veto {
        if_matches_found: bool,
        string_match: StringMatch,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentParser {
    pub name: String,
    pub kind: ContentKind,
    pub formula: Formula,
}

/// One thing a content parser found.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedContent {
    /// The content parser's name.
    pub name: String,
    pub kind: ContentKind,
    pub text: String,
}

/// Why a page gave nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseFailure {
    #[error("{0}")]
    Error(#[from] ParseError),
    /// A veto content parser (named) stopped the page.
    #[error("veto: {0}")]
    Veto(String),
}

/// What was parsed for one post.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedPost {
    pub contents: Vec<ParsedContent>,
}

static GUMPF_BEFORE_SCHEME: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^.*\s(?P<scheme>https?://)").expect("valid"));
static REPEATED_SCHEMES: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^(?:https?://)+(?P<scheme>https?://)").expect("valid"));

fn looks_like_full_url(text: &str) -> bool {
    check_full_url(text).is_ok()
}

/// A parsed URL made absolute and properly encoded: text before an
/// `http(s)://` and repeated schemes are dropped, and relative URLs are
/// joined to the page's.
fn clean_url(url: &str, base: &str) -> String {
    let mut url = url.to_owned();
    if !looks_like_full_url(&url) && (url.contains("http://") || url.contains("https://")) {
        url = GUMPF_BEFORE_SCHEME.replace(&url, "$scheme").into_owned();
        url = REPEATED_SCHEMES.replace(&url, "$scheme").into_owned();
    }
    if !looks_like_full_url(&url) {
        url = urljoin(base, &url);
    }
    ensure_url_is_encoded(&url, true, false)
}

impl ContentParser {
    pub fn parse(&self, context: &ParsingContext, text: &str) -> Result<ParsedPost, ParseFailure> {
        let collapse_newlines = !matches!(self.kind, ContentKind::Note { .. });
        let mut texts = self
            .formula
            .parse(context, text, collapse_newlines)
            .map_err(|e| ParseError(format!("Content Parser {}: {e}", self.name)))?;
        match &self.kind {
            ContentKind::Note { .. } => {
                texts = texts.iter().map(|t| clean_note_text(t)).collect();
            }
            ContentKind::Url { .. } => {
                if let Some(base) = context.get("url") {
                    texts = texts.iter().map(|t| clean_url(t, base)).collect();
                }
            }
            ContentKind::Title { .. } => {
                texts = texts.iter().map(|t| html_unescape(t)).collect();
            }
            ContentKind::Veto {
                if_matches_found,
                string_match,
            } => {
                let found = texts.iter().any(|t| string_match.matches(t));
                return if found == *if_matches_found {
                    Err(ParseFailure::Veto(self.name.clone()))
                } else {
                    Ok(ParsedPost::default())
                };
            }
            _ => {}
        }
        Ok(ParsedPost {
            contents: texts
                .into_iter()
                .map(|text| ParsedContent {
                    name: self.name.clone(),
                    kind: self.kind.clone(),
                    text,
                })
                .collect(),
        })
    }
}

/// Python's `int(text)` for plain decimal text (surrounding whitespace,
/// a sign, `_` between digits).
fn py_int(text: &str) -> Option<i64> {
    let t = py_strip(text);
    let (negative, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let value: i64 = digits.replace('_', "").parse().ok()?;
    Some(if negative { -value } else { value })
}

impl ParsedPost {
    pub fn is_empty(&self) -> bool {
        self.contents.is_empty()
    }

    pub fn merge(&mut self, other: ParsedPost) {
        self.contents.extend(other.contents);
    }

    /// The tags, cleaned (namespaced as their parser says); invalid ones
    /// dropped.
    pub fn tags(&self) -> BTreeSet<String> {
        self.contents
            .iter()
            .filter_map(|c| match &c.kind {
                ContentKind::Tag { namespace } => Some(match namespace {
                    None => c.text.clone(),
                    Some(ns) if ns.is_empty() => {
                        if c.text.contains(':') {
                            format!(":{}", c.text)
                        } else {
                            c.text.clone()
                        }
                    }
                    Some(ns) => format!("{ns}:{}", c.text),
                }),
                _ => None,
            })
            .filter_map(|t| Tag::new(&t).map(|t| t.as_str().to_owned()))
            .collect()
    }

    /// URLs of the given types, first appearance kept: all of them grouped
    /// by priority (in the order priorities first appear), or only those of
    /// the highest priority.
    pub fn urls(&self, types: &[i64], top_priority_only: bool) -> Vec<String> {
        let mut by_priority: Vec<(i64, Vec<String>)> = Vec::new();
        for c in &self.contents {
            if let ContentKind::Url { url_type, priority } = &c.kind
                && types.contains(url_type)
            {
                match by_priority.iter_mut().find(|(p, _)| p == priority) {
                    Some((_, urls)) => urls.push(c.text.clone()),
                    None => by_priority.push((*priority, vec![c.text.clone()])),
                }
            }
        }
        let urls: Vec<String> = if top_priority_only {
            by_priority
                .into_iter()
                .max_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
                .map(|(_, urls)| urls)
                .unwrap_or_default()
        } else {
            by_priority.into_iter().flat_map(|(_, urls)| urls).collect()
        };
        let mut seen = BTreeSet::new();
        urls.into_iter()
            .filter(|u| seen.insert(u.clone()))
            .collect()
    }

    /// Notes by name (ordered by their content parser's name), tidied; empty
    /// ones dropped.
    pub fn notes(&self) -> Vec<(String, String)> {
        let mut contents: Vec<&ParsedContent> = self.contents.iter().collect();
        contents.sort_by(|a, b| a.name.cmp(&b.name));
        contents
            .into_iter()
            .filter_map(|c| match &c.kind {
                ContentKind::Note { name } => {
                    let text = clean_note_text(&c.text);
                    (!text.is_empty()).then(|| (name.clone(), text))
                }
                _ => None,
            })
            .collect()
    }

    /// Hashes as (type, bytes); ones that don't decode are dropped (hex
    /// falls back to base64).
    pub fn hashes(&self) -> Vec<(String, Vec<u8>)> {
        self.contents
            .iter()
            .filter_map(|c| match &c.kind {
                ContentKind::Hash {
                    hash_type,
                    encoding,
                } => {
                    let bytes = match encoding.as_str() {
                        "hex" => fromhex(&c.text).or_else(|| b64decode(&c.text)),
                        "base64" => b64decode(&c.text),
                        _ => None,
                    }?;
                    Some((hash_type.clone(), bytes))
                }
                _ => None,
            })
            .collect()
    }

    /// The earliest time of a type, in seconds; a site's post time is never
    /// later than five seconds before `now`.
    pub fn timestamp(&self, timestamp_type: i64, now: i64) -> Option<i64> {
        self.contents
            .iter()
            .filter_map(|c| match &c.kind {
                ContentKind::Timestamp {
                    timestamp_type: Some(t),
                } if *t == timestamp_type => {
                    let value = py_int(&c.text)?;
                    Some(if *t == TIMESTAMP_MODIFIED_DOMAIN {
                        value.min(now - 5)
                    } else {
                        value
                    })
                }
                _ => None,
            })
            .min()
    }

    /// The first variable set.
    pub fn variable(&self) -> Option<(String, String)> {
        self.contents.iter().find_map(|c| match &c.kind {
            ContentKind::Variable { name } => Some((name.clone(), c.text.clone())),
            _ => None,
        })
    }

    /// HTTP headers to send (a later value for the same header wins).
    pub fn http_headers(&self) -> BTreeMap<String, String> {
        let mut headers = BTreeMap::new();
        for c in &self.contents {
            if let ContentKind::HttpHeader { name } = &c.kind {
                headers.insert(name.clone(), c.text.clone());
            }
        }
        headers
    }

    /// Whether there is a file or post to go and get.
    pub fn has_pursuable_urls(&self) -> bool {
        self.contents.iter().any(|c| {
            matches!(
                c.kind,
                ContentKind::Url {
                    url_type: url_type::DESIRED,
                    ..
                }
            )
        })
    }
}

/// The page's title: the highest-priority one found in any post.
pub fn title(posts: &[ParsedPost]) -> Option<String> {
    posts
        .iter()
        .flat_map(|p| &p.contents)
        .filter_map(|c| match c.kind {
            ContentKind::Title { priority } => Some((priority, c.text.clone())),
            _ => None,
        })
        .max()
        .map(|(_, title)| title)
}

/// A page parser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageParser {
    pub name: String,
    /// The reference's parser key, in hex.
    pub key: String,
    /// Applied to the page before anything else.
    pub converter: StringConverter,
    pub subsidiary: Vec<SubsidiaryPageParser>,
    pub content_parsers: Vec<ContentParser>,
    pub example_urls: Vec<String>,
}

/// Splits a page into posts and parses each with its own page parser.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubsidiaryPageParser {
    pub formula: Formula,
    /// Newest post first, by the site's post time.
    pub sort_by_source_time: bool,
    pub parser: PageParser,
}

impl PageParser {
    /// Parse a page into posts. The context's `post_index` counts the posts
    /// with something to download so far.
    pub fn parse(
        &self,
        context: &mut ParsingContext,
        text: &str,
    ) -> Result<Vec<ParsedPost>, ParseFailure> {
        let converted = self
            .converter
            .convert(text)
            .map_err(|e| ParseError(e.to_string()))?;
        context
            .entry("post_index".to_owned())
            .or_insert_with(|| "0".to_owned());
        let mut mine = ParsedPost::default();
        for content_parser in &self.content_parsers {
            match content_parser.parse(context, &converted) {
                Ok(post) => mine.merge(post),
                Err(ParseFailure::Error(e)) => {
                    return Err(ParseError(format!("Page Parser {}: {e}", self.name)).into());
                }
                Err(veto) => return Err(veto),
            }
        }
        if mine.has_pursuable_urls() {
            let index: i64 = context
                .get("post_index")
                .and_then(|i| i.parse().ok())
                .unwrap_or(0);
            context.insert("post_index".to_owned(), (index + 1).to_string());
        }
        if self.subsidiary.is_empty() {
            return Ok(if mine.is_empty() {
                Vec::new()
            } else {
                vec![mine]
            });
        }
        let mut subsidiary: Vec<&SubsidiaryPageParser> = self.subsidiary.iter().collect();
        // the reference sorts by casefolded name; lowercasing differs only for
        // a few letters (such as ß), see DIFFERENCES.md
        subsidiary.sort_by_key(|s| s.parser.name.to_lowercase());
        let mut posts = Vec::new();
        for s in subsidiary {
            posts.extend(s.parse(&mine, context, &converted)?);
        }
        Ok(posts)
    }
}

impl SubsidiaryPageParser {
    fn parse(
        &self,
        parent: &ParsedPost,
        context: &mut ParsingContext,
        text: &str,
    ) -> Result<Vec<ParsedPost>, ParseFailure> {
        let Ok(post_texts) = self.formula.parse(context, text, false) else {
            return Ok(Vec::new());
        };
        let mut posts = Vec::new();
        for post_text in post_texts.iter().filter(|t| !t.is_empty()) {
            let parsed = match self.parser.parse(context, post_text) {
                Ok(parsed) => parsed,
                Err(ParseFailure::Veto(_)) => continue,
                Err(ParseFailure::Error(e)) => {
                    return Err(ParseError(format!(
                        "Subsidiary Page Parser {}: {e}",
                        self.parser.name
                    ))
                    .into());
                }
            };
            for mut post in parsed {
                post.merge(parent.clone());
                posts.push(post);
            }
        }
        if self.sort_by_source_time {
            let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
            posts.sort_by_key(|p| match p.timestamp(TIMESTAMP_MODIFIED_DOMAIN, now) {
                None => (1, 0),
                Some(t) => (0, -t),
            });
        }
        Ok(posts)
    }
}
