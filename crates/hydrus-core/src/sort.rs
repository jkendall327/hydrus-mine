//! Human-friendly text ordering.

use std::cmp::Ordering;

/// One run of a sort key: text, or a number.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    /// A complete decimal chunk, normalised to ASCII without leading zeros.
    /// Empty chunks represent the same ('',0) tuple as a decimal zero.
    Number(String),
    Text(String),
}

impl Ord for Piece {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // numbers sort before text at the same position
            (Piece::Number(a), Piece::Number(b)) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
            (Piece::Number(_), Piece::Text(_)) => Ordering::Less,
            (Piece::Text(_), Piece::Number(_)) => Ordering::Greater,
            (Piece::Text(a), Piece::Text(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Piece {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A key that sorts `page 2` before `page 10`, case-insensitively.
///
/// Mirrors the reference's `HumanTextSortKey`: the case-folded text is split
/// into alternating text and ASCII-digit runs, and digit runs compare
/// numerically. Each complete non-ASCII decimal text chunk is also a number,
/// as Python's `str.isdecimal`/`int` conversion treats it. Empty chunks share
/// the same key tuple as zero.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HumanSortKey(Vec<Piece>);

pub fn human_sort_key(text: &str) -> HumanSortKey {
    let folded = crate::casefold::casefold(text);
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut in_digits = false;
    // Python's re.split with a capturing group yields text, digits, text, ...
    // always starting (and ending) with a possibly-empty text piece.
    for c in folded.chars() {
        let is_digit = c.is_ascii_digit();
        if is_digit != in_digits {
            pieces.push(finish(std::mem::take(&mut current)));
            in_digits = is_digit;
        }
        current.push(c);
    }
    pieces.push(finish(current));
    if in_digits {
        pieces.push(finish(String::new()));
    }
    HumanSortKey(pieces)
}

fn finish(run: String) -> Piece {
    // re.split('([0-9]+)', ...) splits only ASCII digits. Python converts each
    // resulting chunk independently if all its characters are decimal digits.
    let digits: Option<String> = run
        .chars()
        .map(|c| crate::tag_presentation::decimal_digit(c).and_then(|d| char::from_digit(d, 10)))
        .collect();
    if let Some(digits) = digits {
        let trimmed = digits.trim_start_matches('0');
        Piece::Number(if trimmed.is_empty() {
            "0".into()
        } else {
            trimmed.into()
        })
    } else {
        Piece::Text(run)
    }
}

/// Sort strings in human order.
pub fn human_sort<S: AsRef<str>>(items: &mut [S]) {
    items.sort_by_cached_key(|s| human_sort_key(s.as_ref()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_sort_numerically_and_before_text() {
        let mut v = vec![
            "page:10", "page:2", "Page:1", "page:a", "page:", "10", "9", "a", "",
        ];
        human_sort(&mut v);
        assert_eq!(
            v,
            [
                "", "9", "10", "a", "page:", "Page:1", "page:2", "page:10", "page:a"
            ]
        );
    }

    #[test]
    fn unicode_decimal_chunks_precede_letters_but_keep_ascii_split_boundaries() {
        let mut values = ["alpha", "１４", "１２", "2", "１３", "10"];
        human_sort(&mut values);
        assert_eq!(values, ["2", "10", "１２", "１３", "１４", "alpha"]);
        // ASCII split-leading zero is earlier than a nonzero decimal chunk;
        // this is intentionally not a global numerical sort of whole strings.
        assert!(human_sort_key("999") < human_sort_key("２"));
        assert_eq!(human_sort_key("０"), human_sort_key(""));
        assert_eq!(human_sort_key("１２"), human_sort_key("١٢"));
        assert_ne!(human_sort_key("1２"), human_sort_key("12"));
        assert!(human_sort_key("０") < human_sort_key("2"));
    }

    #[test]
    fn matches_recorded_clean_tags_order() {
        let mut v = vec![
            "negated",
            "inbox",
            "character:samus aran",
            "blue_eyes",
            "::d",
        ];
        human_sort(&mut v);
        assert_eq!(
            v,
            [
                "::d",
                "blue_eyes",
                "character:samus aran",
                "inbox",
                "negated"
            ]
        );
    }
}
