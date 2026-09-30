//! `system:has tag` / `system:does not have tag`:
//!
//! ```text
//! system:has tag in "my tags", ignoring siblings/parents, with status pending: "skirt"
//! ```
//!
//! Before the first colon that is not inside quotes come comma-separated
//! options (a quoted tag service name, "ignoring siblings/parents", status
//! words); after it comes the tag, ideally in quotes.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use hydrus_core::{ContentStatus, Tag};

use crate::error::ParseErrorKind;
use crate::parse::operators::{Result, expected};
use crate::parse::text::{StaticRegex, py_strip, regex};
use crate::predicate::{ServiceRef, TagDisplayType};

/// The parsed options of an advanced tag predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Options {
    pub service: Option<ServiceRef>,
    pub display: TagDisplayType,
    pub statuses: BTreeSet<ContentStatus>,
}

static QUOTED: StaticRegex = LazyLock::new(|| regex(r#"^[^"]*"(?P<inner>.+)"[^"]*$"#));

/// Byte offsets of `separator` characters that are followed by an even
/// number of double quotes, i.e. are not inside a quoted string.
fn unquoted(s: &str, separator: char) -> Vec<usize> {
    let mut quotes_after = s.matches('"').count();
    let mut out = Vec::new();
    for (i, c) in s.char_indices() {
        if c == '"' {
            quotes_after -= 1;
        } else if c == separator && quotes_after.is_multiple_of(2) {
            out.push(i);
        }
    }
    out
}

/// Split the options from the tag and parse the options.
pub(crate) fn options(s: &str) -> Result<(Options, &str)> {
    let (gubbins, tag) = match unquoted(s, ':').first() {
        Some(&colon) => (&s[..colon], &s[colon + 1..]),
        None if unquoted(s, ',').is_empty() => ("", s),
        None => return Err(expected("options, then a colon, then the tag", s)),
    };
    let mut options = Options {
        service: None,
        display: TagDisplayType::Display,
        statuses: BTreeSet::new(),
    };
    let mut start = 0;
    let mut components = Vec::new();
    for comma in unquoted(gubbins, ',') {
        components.push(&gubbins[start..comma]);
        start = comma + 1;
    }
    components.push(&gubbins[start..]);
    for component in components {
        if let Some(caps) = QUOTED.captures(component) {
            options.service = Some(ServiceRef::Name(caps["inner"].to_owned()));
        } else if component.contains("siblings") || component.contains("parents") {
            options.display = TagDisplayType::Storage;
        } else {
            for (word, status) in [
                ("current", ContentStatus::Current),
                ("pending", ContentStatus::Pending),
                ("deleted", ContentStatus::Deleted),
                ("petitioned", ContentStatus::Petitioned),
            ] {
                if component.contains(word) {
                    options.statuses.insert(status);
                }
            }
        }
    }
    if options.statuses.is_empty() {
        options.statuses = [ContentStatus::Current, ContentStatus::Pending]
            .into_iter()
            .collect();
    }
    Ok((options, tag))
}

/// The tag, in quotes or not, cleaned.
pub(crate) fn tag(s: &str) -> Result<Tag> {
    let s = py_strip(s);
    let raw = QUOTED
        .captures(s)
        .map_or(s, |caps| caps.name("inner").map_or(s, |m| m.as_str()));
    // The reference searches for the literal tag "invalid tag" here instead.
    Tag::new(raw).ok_or_else(|| ParseErrorKind::InvalidTag(raw.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::fixture;

    #[test]
    fn status_words_match_reference_implementation() {
        let constants = fixture("constants.json");
        for status in ContentStatus::ALL {
            let word = constants["content_statuses"][status.code().to_string()]
                .as_str()
                .unwrap();
            let (options, _) = options(&format!("status {word}: x")).unwrap();
            assert_eq!(options.statuses, [*status].into_iter().collect(), "{word}");
        }
    }

    #[test]
    fn splits_on_the_first_unquoted_colon() {
        let (options, tag_text) =
            options(r#" in "my: tags", ignoring siblings/parents, status deleted: "a:b""#).unwrap();
        assert_eq!(options.service, Some(ServiceRef::Name("my: tags".into())));
        assert_eq!(options.display, TagDisplayType::Storage);
        assert_eq!(
            options.statuses,
            [ContentStatus::Deleted].into_iter().collect()
        );
        assert_eq!(tag(tag_text).unwrap().as_str(), "a:b");
    }

    #[test]
    fn empty_tags_are_rejected() {
        assert!(matches!(tag(r#"" ""#), Err(ParseErrorKind::InvalidTag(_))));
    }
}
