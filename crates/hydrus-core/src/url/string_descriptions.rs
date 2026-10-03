//! What string processing says of itself, as the reference says it: each
//! step's `ToString` ("SORT: sorting human sort (ascending)"; `simple` for
//! its kind alone, `with_type` with the kind in capitals first), a
//! converter's conversions one by one, and a processor's list of them
//! (`GetProcessingStrings`).

use crate::numbers::human_int;
use crate::pyjson::PyJson;

use super::strings::{
    Conversion, Encoding, FlexibleMatch, HashFunction, MatchKind, ProcessingStep, SortKind,
    StringConverter, StringMatch, StringProcessor,
};

/// The example a [`StringMatch`] has unless given another
/// (`DEFAULT_EXAMPLE_STRING`), which it doesn't mention.
const DEFAULT_EXAMPLE: &str = "example string";

/// Python's `repr` of a string: single quotes unless it holds a single
/// quote and no double one, backslashes and the quote escaped, and
/// characters Python doesn't print written as escapes.
pub fn python_repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7f..0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python's `repr` of a value as JSON holds it, a list as the tuple the
/// reference made of it.
fn python_repr(value: &PyJson, top: bool) -> String {
    match value {
        PyJson::Null => "None".to_owned(),
        PyJson::Bool(true) => "True".to_owned(),
        PyJson::Bool(false) => "False".to_owned(),
        PyJson::Str(s) => python_repr_str(s),
        PyJson::List(items) => {
            let inner: Vec<String> = items.iter().map(|i| python_repr(i, false)).collect();
            if top {
                if inner.len() == 1 {
                    format!("({},)", inner[0])
                } else {
                    format!("({})", inner.join(", "))
                }
            } else {
                format!("[{}]", inner.join(", "))
            }
        }
        other => other.to_python_string(),
    }
}

/// "1st", "2nd", "last", "3rd from last" (`IndexToPrettyOrdinalString`).
pub fn index_to_pretty_ordinal(index: i64) -> String {
    let num = if index >= 0 { index + 1 } else { index };
    if num == 0 {
        return "unknown position".to_owned();
    }
    let n = num.unsigned_abs();
    let ordinal = if (n % 100) / 10 == 1 {
        "th"
    } else {
        match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        }
    };
    let s = format!("{}{ordinal}", human_int(n));
    match num {
        -1 => "last".to_owned(),
        n if n < 0 => format!("{s} from last"),
        _ => s,
    }
}

fn encoding_name(encoding: Encoding) -> &'static str {
    match encoding {
        Encoding::UrlPercent => "url percent encoding",
        Encoding::UnicodeEscape => "unicode escape characters",
        Encoding::HtmlEntities => "html entities",
        Encoding::HexUtf8 => "hex (utf-8)",
        Encoding::Base64Utf8 => "base64 (utf-8)",
        Encoding::Base64UrlUtf8 => "base64url (utf-8)",
    }
}

impl Conversion {
    /// What it does (`StringConverter.ConversionToString`).
    pub fn describe(&self) -> String {
        let count = |n: &usize| human_int(*n as u64);
        match self {
            Conversion::RemoveFromStart(n) => format!("remove the first {} characters", count(n)),
            Conversion::RemoveFromEnd(n) => format!("remove the last {} characters", count(n)),
            Conversion::KeepStart(n) => format!("take the first {} characters", count(n)),
            Conversion::KeepEnd(n) => format!("take the last {} characters", count(n)),
            Conversion::Prepend(text) => format!("prepend with \"{text}\""),
            Conversion::Append(text) => format!("append with \"{text}\""),
            Conversion::AppendRandom {
                population,
                count: n,
            } => format!(
                "append with {} random characters, from \"{population}\"",
                count(n)
            ),
            Conversion::Encode(e) => format!("encode to {}", encoding_name(*e)),
            Conversion::Decode(e) => format!("decode from {}", encoding_name(*e)),
            Conversion::Reverse => "reverse text".to_owned(),
            Conversion::RegexSub {
                pattern,
                replacement,
            } => format!(
                "regex substitution: ({}, {})",
                python_repr_str(pattern.pattern()),
                python_repr_str(replacement)
            ),
            Conversion::IntegerAddition(n) => format!("integer addition: add {n}"),
            Conversion::Hash(h) => format!(
                "hash string by {}",
                match h {
                    HashFunction::Md5 => "md5",
                    HashFunction::Sha1 => "sha1",
                    HashFunction::Sha256 => "sha256",
                    HashFunction::Sha512 => "sha512",
                }
            ),
            Conversion::Unsupported { code, data } => {
                let data =
                    PyJson::parse(data).map_or_else(|_| data.clone(), |d| python_repr(&d, true));
                match code {
                    10 => format!("datestring to timestamp: {data}"),
                    14 => "datestring to timestamp: automatic".to_owned(),
                    12 => format!("timestamp to datestring: {data}"),
                    _ => "unknown conversion".to_owned(),
                }
            }
        }
    }
}

impl StringConverter {
    /// Its conversions, each described (`GetConversionStrings`).
    pub fn conversion_strings(&self) -> Vec<String> {
        self.conversions.iter().map(Conversion::describe).collect()
    }

    /// `ToString`: its conversions, or "2 changes" when `simple`.
    pub fn describe(&self, simple: bool, with_type: bool) -> String {
        let n = self.conversions.len();
        let label = match (n, simple) {
            (0, true) => "no changes".to_owned(),
            (0, false) => "no string conversions".to_owned(),
            (n, true) => format!("{} changes", human_int(n as u64)),
            (_, false) => self.conversion_strings().join(", "),
        };
        if with_type {
            format!("CONVERT: {label}")
        } else {
            label
        }
    }
}

impl StringMatch {
    /// `ToString`: what it matches ("at least 5 numeric characters, such
    /// as \"123\""), or "filter" when `simple`.
    pub fn describe(&self, simple: bool, with_type: bool) -> String {
        if simple {
            return "filter".to_owned();
        }
        let mut result = match (self.min_chars, self.max_chars) {
            (None, None) => "any number of ".to_owned(),
            (None, Some(max)) => format!("at most {max} "),
            (Some(min), None) => format!("at least {min} "),
            (Some(min), Some(max)) => format!("between {min} and {max} "),
        };
        let mut show_example = true;
        match &self.kind {
            MatchKind::Any => result.push_str("characters"),
            MatchKind::Fixed(value) => {
                result.clone_from(value);
                show_example = false;
            }
            MatchKind::Flexible(f) => result.push_str(match f {
                FlexibleMatch::Alpha => "alphabetical characters",
                FlexibleMatch::Alphanumeric => "alphanumeric characters",
                FlexibleMatch::Numeric => "numeric characters",
                FlexibleMatch::Hex => "hex characters",
                FlexibleMatch::Base64 => "base64 characters",
                FlexibleMatch::Base64UrlEncoded => "base64 (url-encoded) characters",
                FlexibleMatch::Base64Url => "base64url characters",
            }),
            MatchKind::Regex(regex) => {
                result.push_str(&format!(
                    "characters, matching regex \"{}\"",
                    regex.pattern()
                ));
            }
        }
        if show_example && self.example != DEFAULT_EXAMPLE {
            result.push_str(&format!(", such as \"{}\"", self.example));
        }
        if with_type {
            format!("MATCH: {result}")
        } else {
            result
        }
    }
}

/// A slice that can never select anything (`SelectsNothingEver`).
fn selects_nothing(start: Option<i64>, end: Option<i64>) -> bool {
    if end == Some(0) {
        return true;
    }
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    let same_sign = (start >= 0 && end >= 0) || (start < 0 && end < 0);
    same_sign && start >= end
}

/// A slice that selects one string (`SelectsOne`).
fn selects_one(start: Option<i64>, end: Option<i64>) -> bool {
    if selects_nothing(start, end) {
        return false;
    }
    if start == Some(-1) && end.is_none() {
        return true;
    }
    let (Some(start), Some(end)) = (start, end) else {
        return false;
    };
    let same_sign = (start >= 0 && end >= 0) || (start < 0 && end < 0);
    same_sign && start == end - 1
}

impl ProcessingStep {
    /// `ToString` of the step.
    pub fn describe(&self, simple: bool, with_type: bool) -> String {
        let typed = |kind: &str, text: String| {
            if with_type {
                format!("{kind}: {text}")
            } else {
                text
            }
        };
        match self {
            ProcessingStep::Convert(converter) => converter.describe(simple, with_type),
            ProcessingStep::Filter(m) => m.describe(simple, with_type),
            ProcessingStep::Split {
                separator,
                max_splits,
            } => {
                if simple {
                    return "splitter".to_owned();
                }
                let mut text = format!("splitting by \"{separator}\"");
                if let Some(max) = max_splits {
                    text.push_str(&format!(", at most {} times", human_int(*max as u64)));
                }
                typed("SPLIT", text)
            }
            ProcessingStep::Slice { start, end } => {
                if simple {
                    return "selector/slicer".to_owned();
                }
                let (start, end) = (*start, *end);
                let one = selects_one(start, end);
                let text = if selects_nothing(start, end) {
                    "selecting nothing".to_owned()
                } else if one {
                    format!(
                        "selecting the {} string",
                        index_to_pretty_ordinal(start.unwrap_or(0))
                    )
                } else {
                    match (start, end) {
                        (None, None) => "selecting everything".to_owned(),
                        (Some(s), None) => format!(
                            "selecting the {} string and onwards",
                            index_to_pretty_ordinal(s)
                        ),
                        (None, Some(e)) => format!(
                            "selecting up to and including the {} string",
                            index_to_pretty_ordinal(e - 1)
                        ),
                        (Some(s), Some(e)) => format!(
                            "selecting the {} string up to and including the {} string",
                            index_to_pretty_ordinal(s),
                            index_to_pretty_ordinal(e - 1)
                        ),
                    }
                };
                typed(if one { "SELECT" } else { "SLICE" }, text)
            }
            ProcessingStep::Join { joiner, tuple_size } => {
                if simple {
                    return "joiner".to_owned();
                }
                let text = match tuple_size {
                    None => format!("joining all strings using \"{joiner}\""),
                    Some(n) => format!("joining every {n} strings using \"{joiner}\""),
                };
                typed("JOIN", text)
            }
            ProcessingStep::Sort {
                kind,
                ascending,
                regex,
            } => {
                if simple {
                    return "sorter".to_owned();
                }
                let kind = match kind {
                    SortKind::None => "no sorting",
                    SortKind::Lexicographic => "strict lexicographic",
                    SortKind::Human => "human sort",
                    SortKind::Reverse => "reverse",
                };
                let mut text = format!(
                    "sorting {kind} ({})",
                    if *ascending {
                        "ascending"
                    } else {
                        "descending"
                    }
                );
                if regex.is_some() {
                    text.push_str(" (with regex)");
                }
                typed("SORT", text)
            }
            ProcessingStep::TagFilter(step) => {
                if simple {
                    return "tag filter".to_owned();
                }
                typed(
                    "TAG FILTER",
                    format!(
                        "{}, such as {}",
                        step.filter.to_permitted_string(),
                        step.example
                    ),
                )
            }
            ProcessingStep::Unsupported { type_id } => format!("unknown step {type_id}"),
        }
    }
}

impl StringProcessor {
    /// Its steps, a converter's conversions one by one
    /// (`GetProcessingStrings`).
    pub fn processing_strings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for step in &self.steps {
            match step {
                ProcessingStep::Convert(converter) => out.extend(converter.conversion_strings()),
                step => out.push(step.describe(false, false)),
            }
        }
        out
    }

    /// Its button's label (`StringProcessorButton._UpdateLabel`): its
    /// steps a line each, each elided to 64 characters, or its summary.
    pub fn button_label(&self) -> String {
        let statements = self.processing_strings();
        if statements.is_empty() {
            return self.summary();
        }
        statements
            .iter()
            .map(|s| {
                if s.chars().count() > 64 {
                    let mut short: String = s.chars().take(63).collect();
                    short.push('\u{2026}');
                    short
                } else {
                    s.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
