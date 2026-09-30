//! String rules used by URL classes (and, later, by parsers): tests on a
//! string ([`StringMatch`]), transformations ([`StringConverter`]) and
//! pipelines of both ([`StringProcessor`]). Regexes are Python regexes, run
//! with `fancy-regex`.

use std::sync::{Arc, OnceLock};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::pyurl::{quote, unquote};
use crate::pybytes::{b64decode, fromhex, urlsafe_b64decode};

/// A regex written for Python's `re`, compiled on first use.
#[derive(Clone)]
pub struct PyRegex {
    pattern: String,
    compiled: Arc<OnceLock<Result<fancy_regex::Regex, String>>>,
}

impl PyRegex {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            compiled: Arc::new(OnceLock::new()),
        }
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// The compiled regex, or why it doesn't compile.
    pub fn regex(&self) -> Result<&fancy_regex::Regex, &str> {
        self.compiled
            .get_or_init(|| {
                fancy_regex::Regex::new(&translate_python_regex(&self.pattern))
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(String::as_str)
    }
}

/// Python spells "end of text" `\Z`; Rust spells it `\z`.
pub(crate) fn translate_python_regex(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('Z') => out.push_str(r"\z"),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl std::fmt::Debug for PyRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PyRegex({:?})", self.pattern)
    }
}

impl PartialEq for PyRegex {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern
    }
}

impl Serialize for PyRegex {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.pattern)
    }
}

impl<'de> Deserialize<'de> for PyRegex {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(PyRegex::new)
    }
}

/// The built-in character classes of a flexible [`StringMatch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlexibleMatch {
    Alpha,
    Alphanumeric,
    Numeric,
    Hex,
    Base64,
    Base64Url,
    Base64UrlEncoded,
}

impl FlexibleMatch {
    /// The reference's codes.
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => FlexibleMatch::Alpha,
            1 => FlexibleMatch::Alphanumeric,
            2 => FlexibleMatch::Numeric,
            3 => FlexibleMatch::Hex,
            4 => FlexibleMatch::Base64,
            5 => FlexibleMatch::Base64Url,
            6 => FlexibleMatch::Base64UrlEncoded,
            _ => return None,
        })
    }

    fn regex(self) -> &'static regex::Regex {
        static REGEXES: OnceLock<Vec<regex::Regex>> = OnceLock::new();
        let all = REGEXES.get_or_init(|| {
            [
                r"^[a-zA-Z]+$",
                r"^[a-zA-Z\d]+$",
                r"^\d+$",
                r"^[\da-fA-F]+$",
                r"^[a-zA-Z\d+/]+={0,2}$",
                r"^[a-zA-Z\d\-_]+={0,2}$",
                r"^([a-zA-Z\d]|%2B|%2F)+(%3D){0,2}$",
            ]
            .iter()
            .map(|r| regex::Regex::new(r).expect("static regex"))
            .collect()
        });
        &all[self as usize]
    }
}

/// What a [`StringMatch`] tests for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Any,
    Fixed(String),
    Flexible(FlexibleMatch),
    /// A regex that must be found somewhere in the text.
    Regex(PyRegex),
}

/// A test on a string: its kind plus optional length limits (in characters).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StringMatch {
    pub kind: MatchKind,
    pub min_chars: Option<usize>,
    pub max_chars: Option<usize>,
    pub example: String,
}

impl StringMatch {
    pub fn any() -> Self {
        Self {
            kind: MatchKind::Any,
            min_chars: None,
            max_chars: None,
            example: "example string".into(),
        }
    }

    pub fn fixed(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            example: text.clone(),
            kind: MatchKind::Fixed(text),
            min_chars: None,
            max_chars: None,
        }
    }

    pub fn matches(&self, text: &str) -> bool {
        let len = text.chars().count();
        if self.min_chars.is_some_and(|min| len < min)
            || self.max_chars.is_some_and(|max| len > max)
        {
            return false;
        }
        // regexes see the text on one line, trimmed
        let flattened = || -> String { text.lines().collect::<String>().trim().to_owned() };
        match &self.kind {
            MatchKind::Any => true,
            MatchKind::Fixed(fixed) => text == fixed,
            MatchKind::Flexible(flexible) => flexible.regex().is_match(&flattened()),
            MatchKind::Regex(regex) => regex
                .regex()
                .is_ok_and(|r| r.is_match(&flattened()).unwrap_or(false)),
        }
    }
}

/// An encoding a [`Conversion`] encodes to or decodes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    UrlPercent,
    UnicodeEscape,
    HtmlEntities,
    HexUtf8,
    Base64Utf8,
    Base64UrlUtf8,
}

impl Encoding {
    /// The reference's codes.
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Encoding::UrlPercent,
            1 => Encoding::UnicodeEscape,
            2 => Encoding::HtmlEntities,
            3 => Encoding::HexUtf8,
            4 => Encoding::Base64Utf8,
            5 => Encoding::Base64UrlUtf8,
            _ => return None,
        })
    }
}

/// A hash function a [`Conversion`] can apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashFunction {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

/// One transformation of a [`StringConverter`]. Counts are in characters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Conversion {
    RemoveFromStart(usize),
    RemoveFromEnd(usize),
    Prepend(String),
    Append(String),
    /// Append `count` characters chosen at random from `population`.
    AppendRandom {
        population: String,
        count: usize,
    },
    Encode(Encoding),
    Decode(Encoding),
    KeepStart(usize),
    KeepEnd(usize),
    Reverse,
    /// Python's `re.sub(pattern, replacement, text)`.
    RegexSub {
        pattern: PyRegex,
        replacement: String,
    },
    IntegerAddition(i64),
    Hash(HashFunction),
    /// A conversion hydrus-rs doesn't run yet (date formatting and parsing),
    /// kept as the reference's code and data (its JSON).
    Unsupported {
        code: i64,
        data: String,
    },
}

/// Why a conversion failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not apply {conversion} to {text:?}: {reason}")]
pub struct ConvertError {
    pub conversion: String,
    pub text: String,
    pub reason: String,
}

/// A chain of [`Conversion`]s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StringConverter {
    pub conversions: Vec<Conversion>,
    pub example: String,
}

impl Default for StringConverter {
    fn default() -> Self {
        Self {
            conversions: Vec::new(),
            example: "example string".into(),
        }
    }
}

fn char_slice(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

impl StringConverter {
    pub fn makes_changes(&self) -> bool {
        !self.conversions.is_empty()
    }

    pub fn convert(&self, text: &str) -> Result<String, ConvertError> {
        let mut s = text.to_owned();
        for conversion in &self.conversions {
            s = apply(conversion, &s).map_err(|reason| ConvertError {
                conversion: format!("{conversion:?}"),
                text: s.clone(),
                reason,
            })?;
        }
        Ok(s)
    }
}

fn apply(conversion: &Conversion, s: &str) -> Result<String, String> {
    let len = s.chars().count();
    Ok(match conversion {
        Conversion::RemoveFromStart(n) => char_slice(s, *n, len),
        // Python's s[:-0] is empty
        Conversion::RemoveFromEnd(n) => {
            if *n == 0 {
                String::new()
            } else {
                char_slice(s, 0, len.saturating_sub(*n))
            }
        }
        Conversion::KeepStart(n) => char_slice(s, 0, *n),
        // Python's s[-0:] is the whole string
        Conversion::KeepEnd(n) => {
            if *n == 0 {
                s.to_owned()
            } else {
                char_slice(s, len.saturating_sub(*n), len)
            }
        }
        Conversion::Prepend(text) => format!("{text}{s}"),
        Conversion::Append(text) => format!("{s}{text}"),
        Conversion::AppendRandom { population, count } => {
            let choices: Vec<char> = population.chars().collect();
            if choices.is_empty() && *count > 0 {
                return Err("nothing to choose random characters from".into());
            }
            let mut out = s.to_owned();
            for _ in 0..*count {
                out.push(choices[rand::random_range(0..choices.len())]);
            }
            out
        }
        Conversion::Encode(encoding) => encode(*encoding, s),
        Conversion::Decode(encoding) => decode(*encoding, s)?,
        Conversion::Reverse => s.chars().rev().collect(),
        Conversion::RegexSub {
            pattern,
            replacement,
        } => regex_sub(pattern, replacement, s)?,
        Conversion::IntegerAddition(delta) => {
            let digits: String = s.trim().chars().filter(|&c| c != '_').collect();
            let value: i128 = digits
                .parse()
                .map_err(|_| format!("{s:?} is not an integer"))?;
            (value + i128::from(*delta)).to_string()
        }
        Conversion::Hash(function) => {
            use sha2::Digest as _;
            let bytes = s.as_bytes();
            match function {
                HashFunction::Md5 => hex::encode(md5::Md5::digest(bytes)),
                HashFunction::Sha1 => hex::encode(sha1::Sha1::digest(bytes)),
                HashFunction::Sha256 => hex::encode(sha2::Sha256::digest(bytes)),
                HashFunction::Sha512 => hex::encode(sha2::Sha512::digest(bytes)),
            }
        }
        Conversion::Unsupported { code, .. } => {
            return Err(format!("conversion type {code} is not supported yet"));
        }
    })
}

fn encode(encoding: Encoding, s: &str) -> String {
    match encoding {
        Encoding::UrlPercent => quote(s, ""),
        Encoding::UnicodeEscape => unicode_escape_encode(s),
        Encoding::HtmlEntities => s
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;"),
        Encoding::HexUtf8 => hex::encode(s.as_bytes()),
        Encoding::Base64Utf8 => base64::engine::general_purpose::STANDARD.encode(s.as_bytes()),
        Encoding::Base64UrlUtf8 => {
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s.as_bytes())
        }
    }
}

fn decode(encoding: Encoding, s: &str) -> Result<String, String> {
    // the reference pads to a multiple of four characters first
    let padded = || format!("{s}{}", "=".repeat((4 - s.chars().count() % 4) % 4));
    let bytes = match encoding {
        Encoding::UrlPercent => return Ok(unquote(s)),
        Encoding::UnicodeEscape => return unicode_escape_decode(s.as_bytes()),
        Encoding::HtmlEntities => return Ok(crate::pyhtml::unescape(s)),
        Encoding::HexUtf8 => fromhex(s).ok_or("non-hexadecimal number found in fromhex() arg")?,
        Encoding::Base64Utf8 => b64decode(&padded()).ok_or("Incorrect padding")?,
        Encoding::Base64UrlUtf8 => urlsafe_b64decode(&padded()).ok_or("Incorrect padding")?,
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Python's `unicode-escape` encoding.
pub fn unicode_escape_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            ' '..='~' => out.push(c),
            c if u32::from(c) < 0x100 => out.push_str(&format!("\\x{:02x}", u32::from(c))),
            c if u32::from(c) < 0x10000 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push_str(&format!("\\U{:08x}", u32::from(c))),
        }
    }
    out
}

/// Python's `unicode-escape` decoding of bytes: escapes are interpreted and
/// every other byte is a Latin-1 character.
pub fn unicode_escape_decode(bytes: &[u8]) -> Result<String, String> {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |digits: &[u8]| -> Option<u32> {
        std::str::from_utf8(digits)
            .ok()
            .and_then(|d| u32::from_str_radix(d, 16).ok())
    };
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'\\' {
            out.push(char::from(b));
            i += 1;
            continue;
        }
        let Some(&e) = bytes.get(i + 1) else {
            return Err("\\ at end of string".into());
        };
        i += 2;
        match e {
            b'\n' => {}
            b'\\' => out.push('\\'),
            b'\'' => out.push('\''),
            b'"' => out.push('"'),
            b'a' => out.push('\u{7}'),
            b'b' => out.push('\u{8}'),
            b'f' => out.push('\u{c}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'v' => out.push('\u{b}'),
            b'0'..=b'7' => {
                let mut value = u32::from(e - b'0');
                let mut taken = 1;
                while taken < 3 && i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                    value = value * 8 + u32::from(bytes[i] - b'0');
                    i += 1;
                    taken += 1;
                }
                out.push(char::from_u32(value).ok_or("bad octal escape")?);
            }
            b'x' | b'u' | b'U' => {
                let width = match e {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let digits = bytes.get(i..i + width).ok_or("truncated escape")?;
                let value = hex(digits).ok_or("truncated escape")?;
                out.push(char::from_u32(value).ok_or("illegal Unicode character")?);
                i += width;
            }
            b'N' => return Err("named unicode escapes are not supported".into()),
            other => {
                out.push('\\');
                out.push(char::from(other));
            }
        }
    }
    Ok(out)
}

/// A piece of a Python `re.sub` replacement template.
enum ReplacementPart {
    Text(String),
    Group(usize),
    Named(String),
}

fn parse_replacement(template: &str) -> Result<Vec<ReplacementPart>, String> {
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        let Some(e) = chars.next() else {
            return Err("bad escape (end of pattern)".into());
        };
        let mut group = None;
        match e {
            '\\' => text.push('\\'),
            'n' => text.push('\n'),
            't' => text.push('\t'),
            'r' => text.push('\r'),
            'f' => text.push('\u{c}'),
            'v' => text.push('\u{b}'),
            'a' => text.push('\u{7}'),
            'b' => text.push('\u{8}'),
            'g' => {
                if chars.next() != Some('<') {
                    return Err("missing <".into());
                }
                let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                group = Some(match name.parse::<usize>() {
                    Ok(n) => ReplacementPart::Group(n),
                    Err(_) => ReplacementPart::Named(name),
                });
            }
            '1'..='9' => {
                let mut n = e.to_digit(10).unwrap_or(0) as usize;
                if let Some(d) = chars.peek().and_then(|c| c.to_digit(10)) {
                    n = n * 10 + d as usize;
                    chars.next();
                }
                group = Some(ReplacementPart::Group(n));
            }
            c if c.is_ascii_alphabetic() => return Err(format!("bad escape \\{c}")),
            c => {
                text.push('\\');
                text.push(c);
            }
        }
        if let Some(group) = group {
            if !text.is_empty() {
                parts.push(ReplacementPart::Text(std::mem::take(&mut text)));
            }
            parts.push(group);
        }
    }
    if !text.is_empty() {
        parts.push(ReplacementPart::Text(text));
    }
    Ok(parts)
}

fn regex_sub(pattern: &PyRegex, template: &str, s: &str) -> Result<String, String> {
    let regex = pattern.regex().map_err(str::to_owned)?;
    let parts = parse_replacement(template)?;
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for captures in regex.captures_iter(s) {
        let captures = captures.map_err(|e| e.to_string())?;
        let whole = captures.get(0).expect("group 0 always exists");
        out.push_str(&s[last..whole.start()]);
        for part in &parts {
            match part {
                ReplacementPart::Text(text) => out.push_str(text),
                ReplacementPart::Group(n) => {
                    if *n >= captures.len() {
                        return Err(format!("invalid group reference {n}"));
                    }
                    out.push_str(captures.get(*n).map_or("", |m| m.as_str()));
                }
                ReplacementPart::Named(name) => {
                    out.push_str(captures.name(name).map_or("", |m| m.as_str()));
                }
            }
        }
        last = whole.end();
    }
    out.push_str(&s[last..]);
    Ok(out)
}

/// One step of a [`StringProcessor`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStep {
    Convert(StringConverter),
    /// Keep the strings that match.
    Filter(StringMatch),
    /// Split each string on a separator (with Python escapes), dropping
    /// empty parts.
    Split {
        separator: String,
        max_splits: Option<usize>,
    },
    /// Keep a Python slice `[start:end]` of the strings.
    Slice {
        start: Option<i64>,
        end: Option<i64>,
    },
    /// Join all the strings, or each run of `tuple_size`, with a joiner
    /// (with Python escapes).
    Join {
        joiner: String,
        tuple_size: Option<usize>,
    },
    /// Sort the strings (`StringSorter`), by all of each string or by
    /// the first match of a regex in it; strings the regex doesn't match
    /// go last.
    Sort {
        kind: SortKind,
        ascending: bool,
        regex: Option<PyRegex>,
    },
    /// Clean the strings as tags, keep those the filter allows (with
    /// unnamespaced rules applying to namespaced tags too) and sort them
    /// in human order (`StringTagFilter`).
    TagFilter(crate::tag_filter::TagFilter),
    /// A step hydrus-rs doesn't run, kept as the reference's type id.
    Unsupported {
        type_id: u16,
    },
}

/// How a [`ProcessingStep::Sort`] orders (`CONTENT_PARSER_SORT_TYPE_*`).
/// "No sorting" sorts as lexicographic does, as in the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKind {
    None,
    Lexicographic,
    Human,
    Reverse,
}

impl SortKind {
    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            0 => Self::None,
            1 => Self::Lexicographic,
            2 => Self::Human,
            3 => Self::Reverse,
            _ => return None,
        })
    }
}

/// `StringSorter.Sort`; `None` where the reference's sort raises (a bad
/// regex), which leaves the strings as they were.
fn sort_strings(
    strings: &[String],
    kind: SortKind,
    ascending: bool,
    regex: Option<&PyRegex>,
) -> Option<Vec<String>> {
    let mut texts = strings.to_vec();
    if kind == SortKind::Reverse {
        texts.reverse();
        return Some(texts);
    }
    let mut unmatched = Vec::new();
    let mut keyed: Vec<(String, String)> = match regex {
        None => texts.into_iter().map(|t| (t.clone(), t)).collect(),
        Some(regex) => {
            let regex = regex.regex().ok()?;
            let mut keyed = Vec::new();
            for text in texts {
                let found = regex.find(&text).ok()?.map(|m| m.as_str().to_owned());
                match found {
                    Some(key) if !key.is_empty() => keyed.push((key, text)),
                    _ => unmatched.push(text),
                }
            }
            keyed
        }
    };
    let order = |a: &str, b: &str| {
        if kind == SortKind::Human {
            crate::sort::human_sort_key(a).cmp(&crate::sort::human_sort_key(b))
        } else {
            a.cmp(b)
        }
    };
    // Python's sort with reverse=True keeps equal items in their order
    let directed = |a: &str, b: &str| {
        if ascending { order(a, b) } else { order(b, a) }
    };
    keyed.sort_by(|(a, _), (b, _)| directed(a, b));
    unmatched.sort_by(|a, b| directed(a, b));
    let mut out: Vec<String> = keyed.into_iter().map(|(_, t)| t).collect();
    out.extend(unmatched);
    Some(out)
}

/// `StringTagFilter.ConvertAndFilter`.
fn filter_tags(strings: &[String], filter: &crate::tag_filter::TagFilter) -> Vec<String> {
    let clean: std::collections::BTreeSet<String> = strings
        .iter()
        .filter_map(|s| crate::tag::clean_tag_checked(s))
        .collect();
    let mut tags: Vec<String> = clean
        .into_iter()
        .filter(|t| filter.tag_ok(t, true))
        .collect();
    crate::sort::human_sort(&mut tags);
    tags
}

/// A pipeline of [`ProcessingStep`]s over a list of strings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StringProcessor {
    pub steps: Vec<ProcessingStep>,
}

/// Python's `text.encode('latin-1', 'backslashreplace').decode('unicode-escape')`.
fn unescape(text: &str) -> Result<String, String> {
    let mut latin1 = Vec::with_capacity(text.len());
    for c in text.chars() {
        match u32::from(c) {
            v if v < 0x100 => latin1.push(v as u8),
            v if v < 0x10000 => latin1.extend(format!("\\u{v:04x}").bytes()),
            v => latin1.extend(format!("\\U{v:08x}").bytes()),
        }
    }
    unicode_escape_decode(&latin1)
}

fn python_slice<T: Clone>(items: &[T], start: Option<i64>, end: Option<i64>) -> Vec<T> {
    let len = items.len() as i64;
    let resolve = |i: i64| if i < 0 { (len + i).max(0) } else { i.min(len) };
    let start = start.map_or(0, resolve);
    let end = end.map_or(len, resolve);
    if start >= end {
        return Vec::new();
    }
    items[start as usize..end as usize].to_vec()
}

impl StringProcessor {
    /// Run the pipeline; an unsupported step is an error.
    pub fn process(&self, strings: Vec<String>) -> Result<Vec<String>, String> {
        let mut current = strings;
        for step in &self.steps {
            current = match step {
                ProcessingStep::Convert(converter) => current
                    .iter()
                    .filter_map(|s| converter.convert(s).ok())
                    .collect(),
                ProcessingStep::Filter(m) => current.into_iter().filter(|s| m.matches(s)).collect(),
                ProcessingStep::Split {
                    separator,
                    max_splits,
                } => match unescape(separator) {
                    Err(_) => Vec::new(),
                    Ok(separator) if separator.is_empty() => Vec::new(),
                    Ok(separator) => current
                        .iter()
                        .flat_map(|s| {
                            let parts: Vec<String> = match max_splits {
                                Some(n) => s.splitn(n + 1, &separator).map(str::to_owned).collect(),
                                None => s.split(&separator).map(str::to_owned).collect(),
                            };
                            parts.into_iter().filter(|p| !p.is_empty())
                        })
                        .collect(),
                },
                ProcessingStep::Slice { start, end } => python_slice(&current, *start, *end),
                ProcessingStep::Join { joiner, tuple_size } => match unescape(joiner) {
                    Err(_) => current,
                    Ok(joiner) => match tuple_size {
                        None => vec![current.join(&joiner)],
                        Some(0) => current,
                        Some(n) => current
                            .chunks(*n)
                            .filter(|chunk| chunk.len() == *n)
                            .map(|chunk| chunk.join(&joiner))
                            .collect(),
                    },
                },
                ProcessingStep::Sort {
                    kind,
                    ascending,
                    regex,
                } => sort_strings(&current, *kind, *ascending, regex.as_ref()).unwrap_or(current),
                ProcessingStep::TagFilter(filter) => filter_tags(&current, filter),
                ProcessingStep::Unsupported { type_id } => {
                    return Err(format!(
                        "string processing step type {type_id} is not supported yet"
                    ));
                }
            };
        }
        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_like_the_reference() {
        let numeric = StringMatch {
            kind: MatchKind::Flexible(FlexibleMatch::Numeric),
            min_chars: None,
            max_chars: Some(5),
            example: "123".into(),
        };
        assert!(numeric.matches("123"));
        assert!(!numeric.matches("12a"));
        assert!(!numeric.matches("123456"));
        let regex = StringMatch {
            kind: MatchKind::Regex(PyRegex::new(r"^post-(?P<id>\d+)\Z")),
            ..StringMatch::any()
        };
        assert!(regex.matches("post-12"));
        assert!(!regex.matches("post-12x"));
        assert!(StringMatch::fixed("view").matches("view"));
    }

    #[test]
    fn converts_like_the_reference() {
        let c = StringConverter {
            conversions: vec![
                Conversion::RegexSub {
                    pattern: PyRegex::new(r"https://site\.com/post/(\d+)"),
                    replacement: r"https://api.site.com/posts/\1.json".into(),
                },
                Conversion::Append("?x=1".into()),
            ],
            example: String::new(),
        };
        assert_eq!(
            c.convert("https://site.com/post/42").unwrap(),
            "https://api.site.com/posts/42.json?x=1"
        );
        let apply1 = |c: Conversion, s: &str| apply(&c, s).unwrap();
        assert_eq!(apply1(Conversion::RemoveFromEnd(0), "abc"), "");
        assert_eq!(apply1(Conversion::KeepEnd(0), "abc"), "abc");
        assert_eq!(apply1(Conversion::KeepEnd(2), "abc"), "bc");
        assert_eq!(
            apply1(Conversion::Encode(Encoding::UrlPercent), "a/b c"),
            "a%2Fb%20c"
        );
        assert_eq!(
            apply1(Conversion::Decode(Encoding::Base64UrlUtf8), "aGk"),
            "hi"
        );
        assert_eq!(
            apply1(Conversion::Encode(Encoding::UnicodeEscape), "é\n"),
            "\\xe9\\n"
        );
        assert_eq!(
            apply1(Conversion::Decode(Encoding::UnicodeEscape), "\\u00e9\\x41"),
            "éA"
        );
        assert_eq!(apply1(Conversion::IntegerAddition(-1), " 10 "), "9");
    }

    #[test]
    fn processes_pipelines() {
        let p = StringProcessor {
            steps: vec![
                ProcessingStep::Split {
                    separator: ",".into(),
                    max_splits: None,
                },
                ProcessingStep::Slice {
                    start: Some(-2),
                    end: None,
                },
                ProcessingStep::Join {
                    joiner: "\\n".into(),
                    tuple_size: None,
                },
            ],
        };
        assert_eq!(p.process(vec!["a,b,,c".into()]).unwrap(), ["b\nc"]);
    }
}
