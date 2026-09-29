//! Human-friendly text ordering.

use std::cmp::Ordering;

/// One run of a sort key: text, or a number.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    /// A run of ASCII digits, leading zeros stripped (so `007` equals `7`).
    Number(String),
    Text(String),
}

impl Ord for Piece {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            // numbers sort before text at the same position
            (Piece::Number(a), Piece::Number(b)) => a.len().cmp(&b.len()).then_with(|| a.cmp(b)),
            (Piece::Number(_), Piece::Text(t)) => {
                if t.is_empty() {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (Piece::Text(t), Piece::Number(_)) => {
                if t.is_empty() {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
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
/// numerically. (Case folding is Unicode lowercase plus `ß` → `ss`, which
/// covers what occurs in tags.)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct HumanSortKey(Vec<Piece>);

pub fn human_sort_key(text: &str) -> HumanSortKey {
    let folded = text.to_lowercase().replace('ß', "ss");
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut in_digits = false;
    // Python's re.split with a capturing group yields text, digits, text, ...
    // always starting (and ending) with a possibly-empty text piece.
    for c in folded.chars() {
        let is_digit = c.is_ascii_digit();
        if is_digit != in_digits {
            pieces.push(finish(std::mem::take(&mut current), in_digits));
            in_digits = is_digit;
        }
        current.push(c);
    }
    pieces.push(finish(current, in_digits));
    if in_digits {
        pieces.push(Piece::Text(String::new()));
    }
    HumanSortKey(pieces)
}

fn finish(run: String, digits: bool) -> Piece {
    if digits {
        let trimmed = run.trim_start_matches('0');
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
