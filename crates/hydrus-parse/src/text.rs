//! Python's string handling where the reference's parsing depends on it.

use crate::dom::is_py_space;

/// Python's `str.strip()`.
pub fn py_strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

/// Python's `str.splitlines()` (without keeping the ends).
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if is_break {
            lines.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r'
                && let Some(&(_, '\n')) = chars.peek()
            {
                chars.next();
                end += 1;
            }
            start = end;
        }
    }
    if start < s.len() {
        lines.push(&s[start..]);
    }
    lines
}

/// The reference's `RemoveNewlines`: each line stripped, blank lines
/// dropped, the rest joined with nothing between them.
pub fn remove_newlines(s: &str) -> String {
    splitlines(s)
        .into_iter()
        .map(py_strip)
        .filter(|l| !l.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_like_python() {
        assert_eq!(splitlines("a\r\nb\rc\n"), ["a", "b", "c"]);
        assert_eq!(splitlines("a\n\nb"), ["a", "", "b"]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(remove_newlines("  a \n\n b\u{2028}c "), "abc");
        assert_eq!(py_strip("\u{1f} x\u{a0}"), "x");
    }
}
