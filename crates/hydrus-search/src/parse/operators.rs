//! Operator parsing. Each predicate family accepts its own set of operator
//! spellings, mirroring the reference parser.

use crate::error::ParseErrorKind;
use crate::number::{Comparison, DEFAULT_APPROX_PERCENT, NumberOp, RatingOp};
use crate::parse::text::py_strip;

pub(crate) type Result<T> = std::result::Result<T, ParseErrorKind>;

/// An operator as the reference normalises it, before a predicate decides
/// what it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sym {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Approx,
}

/// Every operator spelling of the "full" operator set, longest first (so
/// `is about` wins over `is`), ties in the reference's order.
pub(crate) const OPERATOR_SPELLINGS: &[(&str, Sym)] = &[
    ("greater than or equal to", Sym::Ge),
    ("fewer than or equal to", Sym::Le),
    ("less than or equal to", Sym::Le),
    ("more than or equal to", Sym::Ge),
    ("greater than", Sym::Gt),
    ("fewer than", Sym::Lt),
    ("less than", Sym::Lt),
    ("more than", Sym::Gt),
    ("is about", Sym::Approx),
    ("is not", Sym::Ne),
    ("isn't", Sym::Ne),
    ("about", Sym::Approx),
    ("==", Sym::Eq),
    ("is", Sym::Eq),
    ("!=", Sym::Ne),
    ("<=", Sym::Le),
    (">=", Sym::Ge),
    ("~=", Sym::Approx),
    ("=", Sym::Eq),
    ("\u{2260}", Sym::Ne),
    ("<", Sym::Lt),
    (">", Sym::Gt),
    ("\u{2264}", Sym::Le),
    ("\u{2265}", Sym::Ge),
    ("\u{2248}", Sym::Approx),
];

pub(crate) fn expected(what: &'static str, found: &str) -> ParseErrorKind {
    ParseErrorKind::Expected {
        expected: what,
        found: found.to_owned(),
    }
}

/// Skip the colons and spaces that may separate a predicate name from its
/// operator (`system:width: > 5`).
pub(crate) fn skip_separators(s: &str) -> &str {
    let mut s = s;
    while s.starts_with(':') || s.starts_with(' ') {
        s = py_strip(s);
        if let Some(rest) = s.strip_prefix(':') {
            s = rest;
        }
    }
    s
}

/// The legacy relational operators: `=`, `<`, `>`, `≠`, `≈` and a few
/// spellings. `has`/`no` are accepted without being consumed, for
/// `system:width: has width`.
pub(crate) fn relational(s: &str) -> Result<(&str, Comparison)> {
    for (spelling, op) in [
        ("==", Comparison::Equal),
        ("!=", Comparison::NotEqual),
        ("is not", Comparison::NotEqual),
        ("isn't", Comparison::NotEqual),
        ("~=", Comparison::Approx),
        ("=", Comparison::Equal),
        ("<", Comparison::Less),
        (">", Comparison::Greater),
        ("\u{2260}", Comparison::NotEqual),
        ("\u{2248}", Comparison::Approx),
        ("is", Comparison::Equal),
    ] {
        if let Some(rest) = s.strip_prefix(spelling) {
            return Ok((rest, op));
        }
    }
    has_or_no(s).ok_or_else(|| expected("a comparison (=, <, >, \u{2260}, \u{2248})", s))
}

/// [`relational`] without `≠` and `≈`.
pub(crate) fn relational_exact(s: &str) -> Result<(&str, Comparison)> {
    for (spelling, op) in [
        ("==", Comparison::Equal),
        ("=", Comparison::Equal),
        ("<", Comparison::Less),
        (">", Comparison::Greater),
        ("is", Comparison::Equal),
    ] {
        if let Some(rest) = s.strip_prefix(spelling) {
            return Ok((rest, op));
        }
    }
    has_or_no(s).ok_or_else(|| expected("a comparison (=, <, >)", s))
}

fn has_or_no(s: &str) -> Option<(&str, Comparison)> {
    if s.starts_with("has") {
        Some((s, Comparison::Greater))
    } else if s.starts_with("no") {
        Some((s, Comparison::Equal))
    } else {
        None
    }
}

/// The full operator set of number tests, including `≤` and `≥`.
pub(crate) fn number_test(s: &str) -> Result<(&str, NumberOp)> {
    if s.starts_with("has") {
        return Ok((s, NumberOp::Greater));
    }
    if s.starts_with("no") {
        return Ok((s, NumberOp::Equal));
    }
    OPERATOR_SPELLINGS
        .iter()
        .find_map(|&(spelling, sym)| s.strip_prefix(spelling).map(|rest| (rest, number_op(sym))))
        .ok_or_else(|| {
            expected(
                "a comparison (=, \u{2260}, <, \u{2264}, >, \u{2265}, \u{2248})",
                s,
            )
        })
}

pub(crate) fn number_op(sym: Sym) -> NumberOp {
    match sym {
        Sym::Eq => NumberOp::Equal,
        Sym::Ne => NumberOp::NotEqual,
        Sym::Lt => NumberOp::Less,
        Sym::Gt => NumberOp::Greater,
        Sym::Le => NumberOp::LessOrEqual,
        Sym::Ge => NumberOp::GreaterOrEqual,
        Sym::Approx => NumberOp::ApproxPercent {
            percent: DEFAULT_APPROX_PERCENT,
        },
    }
}

/// A legacy comparison used as a number test (`num urls`, `framerate`).
pub(crate) fn comparison_to_number_op(op: Comparison) -> NumberOp {
    match op {
        Comparison::Less => NumberOp::Less,
        Comparison::Greater => NumberOp::Greater,
        Comparison::Equal => NumberOp::Equal,
        Comparison::NotEqual => NumberOp::NotEqual,
        Comparison::Approx => NumberOp::ApproxPercent {
            percent: DEFAULT_APPROX_PERCENT,
        },
    }
}

pub(crate) fn rating_op(sym: Sym) -> Result<RatingOp> {
    Ok(match sym {
        Sym::Eq => RatingOp::Equal,
        Sym::Lt => RatingOp::Less,
        Sym::Gt => RatingOp::Greater,
        Sym::Le => RatingOp::LessOrEqual,
        Sym::Ge => RatingOp::GreaterOrEqual,
        Sym::Approx => RatingOp::Approx,
        Sym::Ne => return Err(ParseErrorKind::RatingNotEqual),
    })
}

/// `=` or `≠`, for filetypes and hashes. Returns whether it is `=`.
pub(crate) fn equality(s: &str) -> Result<(&str, bool)> {
    for (spelling, is_equal) in [
        ("==", true),
        ("\u{2260}", false),
        ("!=", false),
        ("=", true),
        ("is not", false),
        ("isn't", false),
        ("is", true),
    ] {
        if let Some(rest) = s.strip_prefix(spelling) {
            return Ok((rest, is_equal));
        }
    }
    Err(expected("\"=\" or \"!=\"", s))
}

/// Only `=`, for `system:limit`.
pub(crate) fn only_equal(s: &str) -> Result<&str> {
    ["==", "=", "is"]
        .iter()
        .find_map(|spelling| s.strip_prefix(spelling))
        .ok_or_else(|| expected("\"=\"", s))
}
