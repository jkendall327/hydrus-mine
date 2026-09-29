//! Errors from parsing search text.

use hydrus_core::HashKind;

/// A `system:` predicate that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not parse \"{input}\": {kind}")]
pub struct ParseError {
    /// The text as given.
    pub input: String,
    pub kind: ParseErrorKind,
}

/// Why a `system:` predicate could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseErrorKind {
    #[error("system predicates start with \"system:\"")]
    NotSystemPredicate,
    #[error("system predicates cannot be negated with a leading \"-\"")]
    Negated,
    #[error("unknown system predicate")]
    UnknownPredicate,
    #[error("expected {expected}, found {found:?}")]
    Expected {
        expected: &'static str,
        found: String,
    },
    #[error("unexpected text at the end: {0:?}")]
    TrailingText(String),
    #[error("{0} is too large")]
    NumberTooLarge(String),
    #[error("{0:?} is not a number")]
    NonDigits(String),
    #[error("expected a whole number of zero or more, found {0}")]
    Negative(String),
    #[error("{hash:?} is not valid hex")]
    InvalidHex { hash: String },
    #[error("{hash:?} is {actual} bytes, but {} hashes are {expected} bytes", hash_kind_name(*.kind))]
    WrongHashLength {
        hash: String,
        kind: HashKind,
        expected: usize,
        actual: usize,
    },
    #[error("a rating of {stars}/{out_of} is out of range")]
    RatingOutOfRange { stars: u64, out_of: u64 },
    #[error("ratings cannot be searched for with \"is not\"")]
    RatingNotEqual,
    #[error("{0:?} is not a real date")]
    InvalidDate(String),
    #[error(
        "unsupported date or age {0:?}: use a date like 2011-06-04 (optionally with 13:45) or an age like \"3 days 4 hours ago\""
    )]
    UnsupportedDate(String),
    #[error("\"the day of\" and \"the month of\" need a calendar date, not an age")]
    CalendarOperatorNeedsDate,
    #[error("unknown service type {0:?}")]
    UnknownServiceType(String),
    #[error("tag {0:?} is empty once cleaned")]
    InvalidTag(String),
}

fn hash_kind_name(kind: HashKind) -> &'static str {
    match kind {
        HashKind::Sha256 => "sha256",
        HashKind::Md5 => "md5",
        HashKind::Sha1 => "sha1",
        HashKind::Sha512 => "sha512",
    }
}

/// A Client API `tags` search list that could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiSearchError {
    #[error("the search must be a list of tags")]
    NotAList,
    #[error("could not understand the tag: {0:?}")]
    InvalidTag(String),
    #[error(transparent)]
    SystemPredicate(#[from] ParseError),
}
