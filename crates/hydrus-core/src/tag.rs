//! Tags: `namespace:subtag` strings, and the canonical cleaning rules.
//!
//! Cleaning is part of the user-visible contract (the Client API has a
//! `clean_tags` endpoint, and every tag written anywhere is cleaned first), so
//! it is checked against the reference implementation's output in
//! `oracle/fixtures/tag_cleaning.json`.

use std::borrow::Cow;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Tags longer than this many characters are truncated.
pub const MAX_TAG_CHARS: usize = 1024;

const HANGUL_FILLER: char = '\u{3164}';

static UNDESIRED_CONTROL_CHARACTERS: LazyLock<Regex> = LazyLock::new(|| {
    // Surrogates (\uD800-\uDFFF) cannot occur in a Rust str, so unlike the
    // reference regex they need no entry here.
    Regex::new(
        r"[\x{0000}-\x{001F}\x{007F}-\x{009F}\x{200B}\x{200E}\x{200F}\x{202A}-\x{202E}\x{2066}-\x{2069}\x{FEFF}\x{E000}-\x{F8FF}\x{F0000}-\x{FFFFD}\x{100000}-\x{10FFFD}]",
    )
    .expect("static regex")
});
static ONE_OR_MORE_WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("static regex"));
static LEADING_GARBAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:-|system:)+").expect("static regex"));
static LOOKS_LIKE_HANGUL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x{1100}-\x{11FF}\x{AC00}-\x{D7AF}]").expect("static regex"));
static ALL_LATIN_AND_ZERO_WIDTH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\x{0020}-\x{007E}\x{00A0}-\x{024F}\x{200C}\x{200D}]+$").expect("static regex")
});
static ZERO_WIDTH_JOINERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x{200C}\x{200D}]").expect("static regex"));
static ALL_ZERO_WIDTH_JOINERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\x{200C}\x{200D}]+$").expect("static regex"));

/// Split a tag into `(namespace, subtag)` on the first colon.
///
/// Unnamespaced tags have an empty namespace. Note that a tag beginning with a
/// colon (e.g. `::)`) therefore has an empty namespace and a subtag that itself
/// starts with a colon.
pub fn split_tag(tag: &str) -> (&str, &str) {
    tag.split_once(':').unwrap_or(("", tag))
}

/// Join a namespace and subtag. An unnamespaced subtag that contains a colon
/// gets a leading colon so it round-trips through [`split_tag`].
pub fn combine_tag(namespace: &str, subtag: &str) -> String {
    if namespace.is_empty() {
        if subtag.contains(':') {
            format!(":{subtag}")
        } else {
            subtag.to_owned()
        }
    } else {
        format!("{namespace}:{subtag}")
    }
}

fn collapse_whitespace(s: &str) -> Cow<'_, str> {
    ONE_OR_MORE_WHITESPACE.replace_all(s, " ")
}

/// Strip control characters, collapse whitespace, remove leading `-` and
/// `system:` garbage, and deal with stray zero-width/filler characters.
fn strip_gumpf(t: &str) -> String {
    let t = UNDESIRED_CONTROL_CHARACTERS.replace_all(t, "");
    let t = collapse_whitespace(&t);
    let t = t.trim();
    let t = LEADING_GARBAGE.replace(t, "");
    let mut t = t.trim().to_owned();

    // Hangul filler is only legitimate in Korean text.
    if !LOOKS_LIKE_HANGUL.is_match(&t) {
        t = t.replace(HANGUL_FILLER, "");
    }
    // "blue_eyes[ZWNJ]" style junk in otherwise-latin text.
    if ALL_LATIN_AND_ZERO_WIDTH.is_match(&t) {
        t = ZERO_WIDTH_JOINERS.replace_all(&t, "").into_owned();
    }
    if ALL_ZERO_WIDTH_JOINERS.is_match(&t) {
        t.clear();
    }
    collapse_whitespace(&t).trim().to_owned()
}

/// Clean namespace-editor text without adding colon escapes or requiring a subtag.
/// This is the shared control/whitespace/leading-garbage part of tag cleaning.
pub fn strip_tag_text_of_gumpf(text: &str) -> String {
    strip_gumpf(text)
}

/// Normalise a tag to its canonical stored form.
///
/// The result may be empty or have an empty subtag (e.g. `"series:"`); use
/// [`clean_tag_checked`] or [`Tag::new`] where such tags must be rejected.
pub fn clean_tag(raw: &str) -> String {
    let truncated = match raw.char_indices().nth(MAX_TAG_CHARS) {
        Some((byte_index, _)) => &raw[..byte_index],
        None => raw,
    };
    let mut tag = truncated.to_lowercase();

    // ":D" means the subtag ":d", which is stored as "::d". But ":weird:stuff"
    // is already a forced-unnamespaced tag with a colon in its subtag.
    if tag.starts_with(':') && tag.len() > 1 && !tag[1..].contains(':') {
        tag.insert(0, ':');
    }

    if tag.contains(':') {
        // Strip once as a whole first, to remove leading "system:" junk.
        let whole = strip_gumpf(&tag);
        let (namespace, subtag) = split_tag(&whole);
        combine_tag(&strip_gumpf(namespace), &strip_gumpf(subtag))
    } else {
        strip_gumpf(&tag)
    }
}

/// Clean a tag, returning `None` if it has no subtag after cleaning.
pub fn clean_tag_checked(raw: &str) -> Option<String> {
    let tag = clean_tag(raw);
    (!split_tag(&tag).1.is_empty()).then_some(tag)
}

/// A cleaned, non-empty tag.
///
/// Construct with [`Tag::new`] (which cleans) or [`Tag::from_clean`] for
/// strings already known to be clean, such as values read back from the
/// database.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tag(String);

impl Tag {
    /// Clean `raw` and wrap it, or `None` if nothing usable is left.
    pub fn new(raw: &str) -> Option<Tag> {
        clean_tag_checked(raw).map(Tag)
    }

    /// Wrap a string that is already in canonical form.
    pub fn from_clean(clean: impl Into<String>) -> Tag {
        Tag(clean.into())
    }

    pub fn from_parts(namespace: &str, subtag: &str) -> Tag {
        Tag(combine_tag(namespace, subtag))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn namespace(&self) -> &str {
        split_tag(&self.0).0
    }

    pub fn subtag(&self) -> &str {
        split_tag(&self.0).1
    }

    pub fn split(&self) -> (&str, &str) {
        split_tag(&self.0)
    }

    pub fn is_namespaced(&self) -> bool {
        !self.namespace().is_empty()
    }
}

impl fmt::Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Tag({:?})", self.0)
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Tag {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::fixture;

    #[test]
    fn cleaning_matches_reference_implementation() {
        let cases = fixture("tag_cleaning.json");
        let mut failures = Vec::new();
        for case in cases.as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let expected = case["clean"].as_str().unwrap();
            let expected_ok = case["ok"].as_bool().unwrap();
            let actual = clean_tag(input);
            if actual != expected || clean_tag_checked(input).is_some() != expected_ok {
                failures.push(format!("{input:?}: expected {expected:?}, got {actual:?}"));
            }
            let split = case["split"].as_array().unwrap();
            assert_eq!(
                split_tag(expected),
                (split[0].as_str().unwrap(), split[1].as_str().unwrap())
            );
        }
        assert!(
            failures.is_empty(),
            "{} mismatches:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn combine_round_trips() {
        for (ns, sub) in [("", "tag"), ("", ":d"), ("creator", "x:y"), ("a", "")] {
            let tag = combine_tag(ns, sub);
            assert_eq!(split_tag(&tag), (ns, sub));
        }
    }

    proptest::proptest! {
        #[test]
        fn cleaning_is_idempotent(s in "\\PC{0,40}") {
            let once = clean_tag(&s);
            proptest::prop_assert_eq!(clean_tag(&once), once);
        }
    }
}
