//! Small text helpers whose exact behaviour the reference relies on.

use std::cmp::Ordering;

/// Whether the bytes parse as a complete JSON document (`HydrusText.LooksLikeJSON`).
///
/// The reference decodes as strict UTF-8 and runs Python's `json.loads`.
/// Python additionally accepts the non-standard `NaN`/`Infinity` literals;
/// documents relying on those are not recognised here.
pub(crate) fn looks_like_json(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(text).is_ok()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// `HydrusText.LooksLikeHTML` on the first bytes of a file.
pub(crate) fn looks_like_html(data: &[u8]) -> bool {
    [
        &b"<html"[..],
        b"<HTML",
        b"<!DOCTYPE html",
        b"<!DOCTYPE HTML",
    ]
    .iter()
    .any(|n| contains(data, n))
}

/// `HydrusText.LooksLikeSVG` on the first bytes of a file.
pub(crate) fn looks_like_svg(data: &[u8]) -> bool {
    [&b"<svg"[..], b"<SVG", b"<!DOCTYPE svg", b"<!DOCTYPE SVG"]
        .iter()
        .any(|n| contains(data, n))
}

/// Python's `str.splitlines()`: splits on every Unicode line boundary.
pub(crate) fn python_splitlines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{0b}'
                | '\u{0c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if is_break {
            out.push(&text[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r'
                && let Some(&(j, '\n')) = chars.peek()
            {
                chars.next();
                end = j + 1;
            }
            start = end;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// One piece of a human sort key: numbers sort before text, and by value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeyPart {
    Text(String),
    /// Decimal digits with leading zeros removed (compares by value).
    Number(String),
}

impl KeyPart {
    /// The reference key element is `( text, 0 )` or `( '', int )`.
    fn as_tuple(&self) -> (&str, &str) {
        match self {
            KeyPart::Text(t) => (t.as_str(), ""),
            KeyPart::Number(n) => ("", n.as_str()),
        }
    }
}

fn cmp_numeric(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

impl Ord for KeyPart {
    fn cmp(&self, other: &Self) -> Ordering {
        let (ta, na) = self.as_tuple();
        let (tb, nb) = other.as_tuple();
        ta.cmp(tb).then_with(|| cmp_numeric(na, nb))
    }
}

impl PartialOrd for KeyPart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `HydrusText.HumanTextSortKey`: 'page 2' sorts before 'page 10'.
///
/// Python casefolds; lowercasing differs only for a handful of characters
/// (e.g. 'ß'), which do not occur in the file names this is used on.
fn human_sort_key(text: &str) -> Vec<KeyPart> {
    // mirrors re.split('([0-9]+)', ...): text and number runs alternate, and
    // the list always starts and ends with a (possibly empty) text run
    let folded = text.to_lowercase();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_digits = false;
    for c in folded.chars() {
        let is_digit = c.is_ascii_digit();
        if is_digit != in_digits {
            parts.push(make_part(std::mem::take(&mut current), in_digits));
            in_digits = is_digit;
        }
        current.push(c);
    }
    parts.push(make_part(current, in_digits));
    if in_digits {
        parts.push(KeyPart::Text(String::new()));
    }
    parts
}

fn make_part(s: String, digits: bool) -> KeyPart {
    if digits {
        let trimmed = s.trim_start_matches('0');
        KeyPart::Number(trimmed.to_owned())
    } else {
        KeyPart::Text(s)
    }
}

/// Sort names the way the reference's `HumanTextSort` does (stable).
pub(crate) fn human_sort(names: &mut [String]) {
    names.sort_by_cached_key(|n| human_sort_key(n));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(python_splitlines("a\nb\r\nc\rd"), vec!["a", "b", "c", "d"]);
        assert_eq!(python_splitlines("a\n\nb\n"), vec!["a", "", "b"]);
        assert_eq!(python_splitlines(""), Vec::<&str>::new());
        assert_eq!(python_splitlines("x\u{0c}y"), vec!["x", "y"]);
    }

    #[test]
    fn human_sort_orders_numbers_by_value() {
        let mut names: Vec<String> = [
            "p10.jpg",
            "p9.jpg",
            "P1.jpg",
            "p2.jpg",
            "cover.jpg",
            "p02.jpg",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        human_sort(&mut names);
        assert_eq!(
            names,
            [
                "cover.jpg",
                "P1.jpg",
                "p2.jpg",
                "p02.jpg",
                "p9.jpg",
                "p10.jpg"
            ]
        );
    }

    #[test]
    fn json_detection() {
        assert!(looks_like_json(b"{\"a\": [1, 2]}"));
        assert!(looks_like_json(b" [1] \n"));
        assert!(!looks_like_json(b"{\"a\": 1} trailing"));
        assert!(!looks_like_json(b"{\xff}"));
    }
}
