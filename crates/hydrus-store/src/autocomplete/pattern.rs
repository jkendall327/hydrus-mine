//! How autocomplete text matches tags.
//!
//! Matching happens in two stages, exactly as in the reference:
//!
//! 1. A **subtag search** picks candidate subtags. The reference runs it on an
//!    SQLite FTS4 table of searchable subtags; we answer the same question
//!    with the `cache_subtag_words` word index plus a check in Rust:
//!    - a simple `foo ba*` is an FTS4 *phrase*: consecutive words, each equal
//!      to the query's word, the last (if followed by `*`) as a prefix;
//!    - a wildcard anywhere else is an SQLite `LIKE` over the whole
//!      searchable subtag (ASCII case-insensitive), narrowed first by the
//!      phrase before the first `*` when there is one.
//! 2. The resulting tags (and their siblings) are then **filtered** by a
//!    regex over the whole searchable tag ([`ResultFilter`]), which e.g.
//!    requires the text to start at a word boundary.

use regex::Regex;

use crate::text::{has_word_chars, is_word_char, searchable_tag, words};

/// One word of an FTS4 phrase query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryWord {
    pub word: String,
    /// Matches any word starting with `word`.
    pub prefix: bool,
}

/// Tokenise an FTS4 phrase query as FTS4's simple tokenizer does: a word is
/// a prefix if the character right after it is `*`.
pub fn phrase(query: &str) -> Vec<QueryWord> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in query.chars() {
        if is_word_char(c) {
            current.push(c.to_ascii_lowercase());
            continue;
        }
        if !current.is_empty() {
            out.push(QueryWord {
                word: std::mem::take(&mut current),
                prefix: c == '*',
            });
        }
    }
    if !current.is_empty() {
        out.push(QueryWord {
            word: current,
            prefix: false,
        });
    }
    out
}

/// Whether searchable text contains the phrase (an FTS4 phrase match).
pub fn phrase_matches(query: &[QueryWord], searchable: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let doc: Vec<String> = words(searchable).collect();
    doc.windows(query.len()).any(|window| {
        window.iter().zip(query).all(|(w, q)| {
            if q.prefix {
                w.starts_with(&q.word)
            } else {
                *w == q.word
            }
        })
    })
}

/// An SQLite `LIKE` pattern (no `ESCAPE`): `%` and `*` match any run of
/// characters, `_` any one character, and ASCII letters match either case.
pub fn like_matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    // classic wildcard matching with backtracking to the last star
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '%' || p[pi] == '*') {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && (p[pi] == '_' || p[pi].eq_ignore_ascii_case(&t[ti])) {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '%' || c == '*')
}

/// How candidate subtags are found for the subtag part of a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubtagSearch {
    /// Every subtag.
    All,
    /// An FTS4 phrase match.
    Phrase(Vec<QueryWord>),
    /// A `LIKE` over the searchable subtag, pre-narrowed by a phrase match
    /// when `narrow` is given (otherwise every subtag must be checked).
    Like {
        pattern: String,
        narrow: Option<Vec<QueryWord>>,
    },
    /// Subtags whose searchable form is exactly this.
    Exact(String),
}

impl SubtagSearch {
    /// The search the reference runs for the subtag part of a query.
    pub fn plan(subtag: &str) -> Self {
        if subtag == "*" {
            return SubtagSearch::All;
        }
        let stars = subtag.matches('*').count();
        if stars == 0 {
            return SubtagSearch::Exact(subtag.to_owned());
        }
        let complex = stars > 1 || !subtag.ends_with('*');
        let searchable_words = has_word_chars(subtag);
        if !complex && searchable_words {
            return SubtagSearch::Phrase(phrase(subtag));
        }
        let narrow = (searchable_words && !subtag.starts_with('*')).then(|| {
            let before_first_star = subtag.split('*').next().unwrap_or_default();
            phrase(&format!("{before_first_star}*"))
        });
        SubtagSearch::Like {
            pattern: subtag.to_owned(),
            narrow,
        }
    }

    /// Whether a subtag, in searchable form, is found by this search.
    pub fn matches(&self, searchable: &str) -> bool {
        match self {
            SubtagSearch::All => true,
            SubtagSearch::Phrase(query) => phrase_matches(query, searchable),
            SubtagSearch::Like { pattern, narrow } => {
                narrow
                    .as_ref()
                    .is_none_or(|q| phrase_matches(q, searchable))
                    && like_matches(pattern, searchable)
            }
            SubtagSearch::Exact(text) => searchable == text,
        }
    }
}

/// The reference's final filter on autocomplete results
/// (`FilterPredicatesBySearchText`): a tag is kept if its searchable form, or
/// that of one of its known siblings, matches the search text as a regex
/// where `*` is "anything" and the text must start at the beginning, after
/// the namespace colon, or after whitespace.
#[derive(Debug, Clone)]
pub struct ResultFilter {
    regex: Regex,
}

impl ResultFilter {
    pub fn new(search_text: &str) -> Self {
        let glob = |s: &str| {
            s.split('*')
                .map(regex::escape)
                .collect::<Vec<_>>()
                .join(".*")
        };
        let (beginning, body, last_part) = match search_text.split_once(':') {
            Some(("*", subtag)) => (r"(?:\A|:|\s)", glob(subtag), subtag),
            Some((namespace, subtag)) => (
                r"\A",
                format!(r"{}:(?:.*\s)?{}", glob(namespace), glob(subtag)),
                subtag,
            ),
            None if search_text.starts_with('*') => (r"(?:\A|:)", glob(search_text), search_text),
            None => (r"(?:\A|:|\s)", glob(search_text), search_text),
        };
        let end = if last_part.ends_with('*') {
            r"\z"
        } else {
            r"(?:\s|\z)"
        };
        let regex = Regex::new(&format!("{beginning}{body}{end}"))
            .expect("escaped literals joined by .* always form a valid regex");
        Self { regex }
    }

    /// Whether a tag (not yet in searchable form) passes.
    pub fn matches(&self, tag: &str) -> bool {
        self.regex.is_match(&searchable_tag(tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(word: &str, prefix: bool) -> QueryWord {
        QueryWord {
            word: word.into(),
            prefix,
        }
    }

    #[test]
    fn phrases_tokenise_like_fts4() {
        assert_eq!(phrase("samus ar*"), [w("samus", false), w("ar", true)]);
        assert_eq!(phrase("a*b"), [w("a", true), w("b", false)]);
        assert_eq!(phrase("a.*"), [w("a", false)]);
        assert!(phrase("..*").is_empty());
        assert!(phrase_matches(&phrase("samus.a*"), "big samus aran"));
        assert!(phrase_matches(&phrase("ey*"), "blue eyes"));
        assert!(!phrase_matches(&phrase("b ey*"), "blue eyes"));
        assert!(!phrase_matches(&phrase("..*"), "..x"));
    }

    #[test]
    fn like_is_sqlite_like() {
        assert!(like_matches(":%", ":)"));
        assert!(like_matches("*eyes*", "blue eyes"));
        assert!(like_matches("BL_E%", "blue eyes"));
        assert!(!like_matches("ÉCL%", "éclair"));
        assert!(like_matches("a%b%c", "aXbYbc"));
        assert!(!like_matches("a%b", "ab c"));
    }

    #[test]
    fn plans_follow_the_reference() {
        assert_eq!(SubtagSearch::plan("*"), SubtagSearch::All);
        assert_eq!(
            SubtagSearch::plan("blue*"),
            SubtagSearch::Phrase(vec![w("blue", true)])
        );
        assert_eq!(
            SubtagSearch::plan("*eyes*"),
            SubtagSearch::Like {
                pattern: "*eyes*".into(),
                narrow: None
            }
        );
        assert_eq!(
            SubtagSearch::plan("bl*es*"),
            SubtagSearch::Like {
                pattern: "bl*es*".into(),
                narrow: Some(vec![w("bl", true)])
            }
        );
        assert_eq!(
            SubtagSearch::plan(":*"),
            SubtagSearch::Like {
                pattern: ":*".into(),
                narrow: None
            }
        );
    }

    #[test]
    fn result_filter_matches_word_starts() {
        let f = ResultFilter::new("samus*");
        assert!(f.matches("samus"));
        assert!(f.matches("character:samus aran"));
        assert!(f.matches("big samus"));
        assert!(!f.matches("xsamus"));
        let f = ResultFilter::new("series:met*");
        assert!(f.matches("series:metroid"));
        assert!(f.matches("series:super metroid"));
        assert!(!f.matches("metroid"));
        let f = ResultFilter::new("*eyes*");
        assert!(f.matches("blue_eyes"));
        assert!(f.matches("blueeyes"));
        let f = ResultFilter::new("::*");
        assert!(f.matches("::)"));
        let f = ResultFilter::new("*:sam*");
        assert!(f.matches("character:samus"));
        assert!(f.matches("samus"));
    }
}
