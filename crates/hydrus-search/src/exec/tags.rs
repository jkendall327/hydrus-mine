//! Finding tags: exact tags, namespace patterns and subtag wildcards, and
//! turning displayed tags into the stored tags that show as them.
//!
//! Wildcards match a subtag's *searchable* form (punctuation such as `_` and
//! `(` read as spaces; see `hydrus_store::text`), the way the reference's
//! full-text index does. The pattern itself is first put in searchable form
//! (so `blue_eyes*` means `blue eyes*`), then:
//!
//! - `*` alone matches everything;
//! - no `*` (as in `*:blue eyes`) matches the subtag exactly, or any subtag
//!   whose searchable form it is;
//! - one trailing `*` after a word character (`blue ey*`) is a phrase: the
//!   words in order, anywhere in the subtag, the last one as a prefix;
//! - anything else is an SQL `LIKE` pattern over the whole searchable form
//!   (`*` is any run of characters, `_` any one character, ASCII letters
//!   match either case).
//!
//! Candidates come from the `cache_subtag_words` word index whenever the
//! pattern has a word to anchor on; only patterns that start with `*` and
//! are not phrases have to scan every subtag, as in the reference.

use std::collections::HashSet;

use rusqlite::Connection;

use hydrus_core::{NamespaceId, SubtagId, Tag, TagId};
use hydrus_store::display::DisplayGraph;
use hydrus_store::text;

use super::Result;
use super::sql::int_array;

/// The stored tags whose display includes `tag`'s ideal: the tags a search
/// for `tag` must look for in a service with this display graph.
pub(crate) fn stored_tags_for_search(graph: &DisplayGraph, tag: TagId) -> Vec<TagId> {
    graph.stored_tags_for(tag)
}

/// The stored tags that display exactly as one of `displayed`. Unlike a
/// search for a tag, a tag that is not its own ideal displays as nothing.
pub(crate) fn stored_tags_displaying(graph: &DisplayGraph, displayed: &[TagId]) -> Vec<TagId> {
    let mut out = Vec::with_capacity(displayed.len());
    for &tag in displayed {
        if graph.ideal(tag) == tag {
            if graph.is_empty() {
                out.push(tag);
            } else {
                out.extend(graph.stored_tags_for(tag));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The id of an existing tag.
pub(crate) fn tag_id(conn: &Connection, tag: &Tag) -> Result<Option<TagId>> {
    Ok(hydrus_store::master::tag_id(conn, tag)?)
}

/// A namespace pattern: `*` (any), `name`, or `na*e`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NamespacePattern {
    Any,
    Exact(String),
    Like(String),
}

impl NamespacePattern {
    pub fn parse(namespace: &str) -> Self {
        if namespace == "*" {
            NamespacePattern::Any
        } else if namespace.contains('*') {
            NamespacePattern::Like(namespace.to_owned())
        } else {
            NamespacePattern::Exact(namespace.to_owned())
        }
    }

    /// The matching namespace ids, or `None` for "any namespace".
    pub fn namespace_ids(&self, conn: &Connection) -> Result<Option<Vec<NamespaceId>>> {
        Ok(match self {
            NamespacePattern::Any => None,
            NamespacePattern::Exact(ns) => Some(
                hydrus_store::master::namespace_id(conn, ns)?
                    .into_iter()
                    .collect(),
            ),
            NamespacePattern::Like(pattern) => {
                let mut stmt =
                    conn.prepare_cached("SELECT namespace_id, namespace FROM namespaces")?;
                let mut rows = stmt.query([])?;
                let mut out = Vec::new();
                while let Some(row) = rows.next()? {
                    let namespace: String = row.get(1)?;
                    if like(&wildcard_to_like(pattern), &namespace) {
                        out.push(row.get(0)?);
                    }
                }
                Some(out)
            }
        })
    }
}

/// A subtag pattern; see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubtagPattern {
    Any,
    Exact(String),
    Phrase { words: Vec<String>, prefix: bool },
    Like(String),
}

impl SubtagPattern {
    pub fn parse(subtag: &str) -> Self {
        let searchable = text::searchable_subtag(subtag);
        let subtag = searchable.as_str();
        if subtag == "*" {
            return SubtagPattern::Any;
        }
        if !subtag.contains('*') {
            return SubtagPattern::Exact(subtag.to_owned());
        }
        let stars = subtag.matches('*').count();
        let simple = stars == 1 && subtag.ends_with('*');
        if !simple || !text::has_word_chars(subtag) {
            return SubtagPattern::Like(wildcard_to_like(subtag));
        }
        let body = &subtag[..subtag.len() - 1];
        let prefix = body.chars().next_back().is_some_and(text::is_word_char);
        SubtagPattern::Phrase {
            words: text::words(body).collect(),
            prefix,
        }
    }

    /// Whether a subtag matches.
    pub fn matches(&self, subtag: &str) -> bool {
        match self {
            SubtagPattern::Any => true,
            SubtagPattern::Exact(want) => {
                subtag == want || text::searchable_subtag(subtag) == *want
            }
            SubtagPattern::Phrase { words, prefix } => {
                let have: Vec<String> = text::words(&text::searchable_subtag(subtag)).collect();
                phrase_matches(&have, words, *prefix)
            }
            SubtagPattern::Like(pattern) => like(pattern, &text::searchable_subtag(subtag)),
        }
    }

    /// The matching subtag ids, or `None` for "any subtag".
    pub fn subtag_ids(&self, conn: &Connection) -> Result<Option<Vec<SubtagId>>> {
        let ids = match self {
            SubtagPattern::Any => return Ok(None),
            SubtagPattern::Exact(want) => {
                let mut ids: Vec<SubtagId> = hydrus_store::master::subtag_id(conn, want)?
                    .into_iter()
                    .collect();
                let mut stmt = conn.prepare_cached(
                    "SELECT subtag_id FROM cache_searchable_subtags WHERE searchable = ?",
                )?;
                let rows = stmt.query_map([want], |r| r.get(0))?;
                for row in rows {
                    ids.push(row?);
                }
                ids
            }
            SubtagPattern::Phrase { words, prefix } => {
                let Some(last) = words.last() else {
                    return Ok(Some(Vec::new()));
                };
                if words.len() == 1 && *prefix {
                    // every subtag with a word starting with it matches
                    words_with_prefix(conn, last)?
                } else {
                    // anchor on the longest whole word, then check the phrase
                    let whole = if *prefix {
                        &words[..words.len() - 1]
                    } else {
                        &words[..]
                    };
                    let anchor = whole
                        .iter()
                        .max_by_key(|w| w.len())
                        .expect("a phrase has a whole word");
                    let candidates = words_equal(conn, anchor)?;
                    self.filter_candidates(conn, &candidates)?
                }
            }
            SubtagPattern::Like(pattern) => match like_anchor(pattern) {
                Some(anchor) => {
                    let candidates = words_with_prefix(conn, &anchor)?;
                    self.filter_candidates(conn, &candidates)?
                }
                None => self.scan_subtags(conn)?,
            },
        };
        Ok(Some(ids))
    }

    fn filter_candidates(
        &self,
        conn: &Connection,
        candidates: &[SubtagId],
    ) -> Result<Vec<SubtagId>> {
        let mut stmt = conn
            .prepare_cached("SELECT subtag_id, subtag FROM subtags WHERE subtag_id IN rarray(?)")?;
        let mut rows = stmt.query([int_array(candidates.iter().map(|s| s.get()))])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let subtag: String = row.get(1)?;
            if self.matches(&subtag) {
                out.push(row.get(0)?);
            }
        }
        Ok(out)
    }

    fn scan_subtags(&self, conn: &Connection) -> Result<Vec<SubtagId>> {
        let mut stmt = conn.prepare_cached("SELECT subtag_id, subtag FROM subtags")?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let subtag: String = row.get(1)?;
            if self.matches(&subtag) {
                out.push(row.get(0)?);
            }
        }
        Ok(out)
    }
}

/// Tags matching a namespace pattern and a subtag pattern.
#[derive(Debug, Clone)]
pub(crate) enum TagMatch {
    /// Every tag in these namespaces (`None`: every tag at all).
    InNamespaces(Option<Vec<NamespaceId>>),
    /// Exactly these tags.
    Tags(Vec<TagId>),
}

pub(crate) fn matching_tags(
    conn: &Connection,
    namespace: &NamespacePattern,
    subtag: &SubtagPattern,
) -> Result<TagMatch> {
    let namespaces = namespace.namespace_ids(conn)?;
    let Some(subtags) = subtag.subtag_ids(conn)? else {
        return Ok(TagMatch::InNamespaces(namespaces));
    };
    tags_from_parts(conn, namespaces.as_deref(), &subtags).map(TagMatch::Tags)
}

/// Tags with one of `subtags` and (unless `None`) one of `namespaces`.
pub(crate) fn tags_from_parts(
    conn: &Connection,
    namespaces: Option<&[NamespaceId]>,
    subtags: &[SubtagId],
) -> Result<Vec<TagId>> {
    if subtags.is_empty() || namespaces.is_some_and(<[NamespaceId]>::is_empty) {
        return Ok(Vec::new());
    }
    let subtag_ids = int_array(subtags.iter().map(|s| s.get()));
    let mut out = Vec::new();
    match namespaces {
        None => {
            let mut stmt =
                conn.prepare_cached("SELECT tag_id FROM tags WHERE subtag_id IN rarray(?)")?;
            let rows = stmt.query_map([subtag_ids], |r| r.get(0))?;
            for row in rows {
                out.push(row?);
            }
        }
        Some(namespaces) => {
            let allowed: HashSet<NamespaceId> = namespaces.iter().copied().collect();
            let mut stmt = conn.prepare_cached(
                "SELECT tag_id, namespace_id FROM tags WHERE subtag_id IN rarray(?)",
            )?;
            let mut rows = stmt.query([subtag_ids])?;
            while let Some(row) = rows.next()? {
                let ns: NamespaceId = row.get(1)?;
                if allowed.contains(&ns) {
                    out.push(row.get(0)?);
                }
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

fn words_equal(conn: &Connection, word: &str) -> Result<Vec<SubtagId>> {
    let mut stmt =
        conn.prepare_cached("SELECT subtag_id FROM cache_subtag_words WHERE word = ?")?;
    let rows = stmt.query_map([word], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn words_with_prefix(conn: &Connection, prefix: &str) -> Result<Vec<SubtagId>> {
    // with no successor (a prefix of U+10FFFFs) every later word matches
    let end = successor(prefix).unwrap_or_else(|| char::MAX.to_string().repeat(prefix.len() + 1));
    let mut stmt = conn.prepare_cached(
        "SELECT subtag_id FROM cache_subtag_words WHERE word >= ?1 AND word < ?2",
    )?;
    let mut out: Vec<SubtagId> = stmt
        .query_map([prefix, end.as_str()], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// The smallest string greater than every string starting with `prefix`
/// (SQLite compares text by UTF-8 bytes, i.e. by code point).
fn successor(prefix: &str) -> Option<String> {
    let mut chars: Vec<char> = prefix.chars().collect();
    while let Some(last) = chars.pop() {
        let mut next = u32::from(last) + 1;
        if (0xD800..=0xDFFF).contains(&next) {
            next = 0xE000;
        }
        if let Some(c) = char::from_u32(next) {
            chars.push(c);
            return Some(chars.into_iter().collect());
        }
    }
    None
}

/// Whether `have` contains `want` as consecutive words, the last of `want`
/// only as a prefix if `prefix`.
fn phrase_matches(have: &[String], want: &[String], prefix: bool) -> bool {
    if want.is_empty() || have.len() < want.len() {
        return false;
    }
    let (whole, last) = want.split_at(want.len() - 1);
    have.windows(want.len()).any(|window| {
        window[..whole.len()] == *whole
            && if prefix {
                window[whole.len()].starts_with(&last[0])
            } else {
                window[whole.len()] == last[0]
            }
    })
}

/// A hydrus wildcard as a `LIKE` pattern.
fn wildcard_to_like(wildcard: &str) -> String {
    wildcard.replace('*', "%")
}

/// A word every match's searchable form must start with, if the `LIKE`
/// pattern begins with literal word characters.
fn like_anchor(pattern: &str) -> Option<String> {
    let anchor: String = pattern
        .chars()
        .take_while(|&c| c != '%' && c != '_' && text::is_word_char(c))
        .map(|c| c.to_ascii_lowercase())
        .collect();
    (!anchor.is_empty()).then_some(anchor)
}

/// SQLite's `LIKE`: `%` matches any run of characters, `_` any one
/// character, and ASCII letters match regardless of case.
pub(crate) fn like(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut backtrack: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '%' {
            backtrack = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '_' || p[pi].eq_ignore_ascii_case(&t[ti])) {
            pi += 1;
            ti += 1;
        } else if let Some((star, matched)) = backtrack {
            pi = star + 1;
            ti = matched + 1;
            backtrack = Some((star, matched + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '%')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_follows_sqlite() {
        assert!(like("%eyes", "blue eyes"));
        assert!(like("blue%", "blue"));
        assert!(like("b_ue", "blue"));
        assert!(like("B%E", "blue"));
        assert!(!like("blue", "blue eyes"));
        assert!(like("%a%b%", "xxaxxbxx"));
        assert!(!like("%a%b", "xxaxxbxx"));
        assert!(like("%", ""));
        assert!(like("é%", "éclair"));
        assert!(!like("É%", "éclair"), "only ASCII folds case");
    }

    #[test]
    fn patterns_follow_the_reference_modes() {
        assert_eq!(SubtagPattern::parse("*"), SubtagPattern::Any);
        assert_eq!(
            SubtagPattern::parse("blue eyes"),
            SubtagPattern::Exact("blue eyes".into())
        );
        assert_eq!(
            SubtagPattern::parse("blue ey*"),
            SubtagPattern::Phrase {
                words: vec!["blue".into(), "ey".into()],
                prefix: true
            }
        );
        assert_eq!(
            SubtagPattern::parse("blue *"),
            SubtagPattern::Phrase {
                words: vec!["blue".into()],
                prefix: false
            }
        );
        assert_eq!(
            SubtagPattern::parse("*eyes"),
            SubtagPattern::Like("%eyes".into())
        );
        assert_eq!(
            SubtagPattern::parse("b*e*"),
            SubtagPattern::Like("b%e%".into())
        );
        assert_eq!(SubtagPattern::parse(":*"), SubtagPattern::Like(":%".into()));
    }

    #[test]
    fn matching_uses_the_searchable_form() {
        let phrase = SubtagPattern::parse("blue ey*");
        assert!(phrase.matches("blue_eyes"));
        assert!(phrase.matches("light blue eyes"));
        assert!(!phrase.matches("eyes blue"));
        let word = SubtagPattern::parse("sam*");
        assert!(word.matches("big samus"));
        assert!(!word.matches("ssamus"));
        assert!(SubtagPattern::parse("*eyes").matches("(blue) eyes"));
        assert!(SubtagPattern::parse("blue eyes").matches("blue_eyes"));
        // the pattern is searchable text too
        assert_eq!(
            SubtagPattern::parse("blue_eyes"),
            SubtagPattern::Exact("blue eyes".into())
        );
        assert_eq!(
            SubtagPattern::parse("(blue)**eyes"),
            SubtagPattern::Like("blue %eyes".into())
        );
        assert!(!SubtagPattern::parse("blue eyes").matches("blue_eyes_2"));
    }

    #[test]
    fn successors_bound_prefix_ranges() {
        assert_eq!(successor("ab").as_deref(), Some("ac"));
        assert_eq!(successor("a\u{10FFFF}").as_deref(), Some("b"));
        assert_eq!(successor("\u{D7FF}").as_deref(), Some("\u{E000}"));
        assert_eq!(successor(""), None);
    }
}
