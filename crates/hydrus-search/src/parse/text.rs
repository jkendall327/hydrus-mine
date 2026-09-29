//! Text helpers that behave like the Python string and regex operations the
//! reference parser is written with, so that edge cases (odd whitespace,
//! case mapping) are accepted or rejected identically.

use std::sync::LazyLock;

use crate::error::ParseErrorKind;

/// Python's `str.isspace()`: Unicode whitespace plus the ASCII information
/// separators U+001C..U+001F, which Rust does not count as whitespace.
pub(crate) fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `str.strip()`.
pub(crate) fn py_strip(s: &str) -> &str {
    s.trim_matches(is_py_space)
}

/// Compile a static pattern. Patterns are written with `\s` meaning Python's
/// whitespace, so this widens every `\s` to include U+001C..U+001F.
pub(crate) fn regex(pattern: &str) -> regex::Regex {
    regex::Regex::new(&pythonise(pattern)).expect("static regex is valid")
}

/// Like [`regex`], for the few patterns that need look-around.
pub(crate) fn fancy_regex(pattern: &str) -> fancy_regex::Regex {
    fancy_regex::Regex::new(&pythonise(pattern)).expect("static regex is valid")
}

/// A lazily compiled static regex.
pub(crate) type StaticRegex = LazyLock<regex::Regex>;

fn pythonise(pattern: &str) -> String {
    // `\s` appears both bare and inside classes like `[^\s]`; the widened
    // form is valid in both positions.
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(next) = chars.next() else {
                    out.push(c);
                    break;
                };
                if next == 's' {
                    out.push_str(if in_class {
                        r"\s\x1c-\x1f"
                    } else {
                        r"[\s\x1c-\x1f]"
                    });
                } else {
                    out.push(c);
                    out.push(next);
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
                // a leading `]` or `^]` is literal
                if chars.peek() == Some(&'^') {
                    out.push('^');
                    chars.next();
                }
                if chars.peek() == Some(&']') {
                    out.push(']');
                    chars.next();
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whether `c` is a Unicode decimal digit (general category Nd), as
/// Python's `\d` and `int()` understand them.
fn is_decimal_digit(c: char) -> bool {
    static DIGIT: StaticRegex = LazyLock::new(|| regex(r"^\d$"));
    let mut buf = [0; 4];
    c.is_ascii_digit() || (!c.is_ascii() && DIGIT.is_match(c.encode_utf8(&mut buf)))
}

/// The value of a Unicode decimal digit. Unicode guarantees these come in
/// contiguous runs of ten, from zero to nine, so the value is the offset
/// from the start of the run.
fn decimal_value(c: char) -> Option<u64> {
    if !is_decimal_digit(c) {
        return None;
    }
    let mut start = u32::from(c);
    while let Some(previous) = start.checked_sub(1).and_then(char::from_u32) {
        if !is_decimal_digit(previous) {
            break;
        }
        start -= 1;
    }
    Some(u64::from((u32::from(c) - start) % 10))
}

/// Parse a run of digits as Python's `int()` would (any Unicode decimal
/// digits), requiring the value to fit.
pub(crate) fn parse_u64(digits: &str) -> Result<u64, ParseErrorKind> {
    if digits.is_empty() {
        return Err(ParseErrorKind::NonDigits(String::new()));
    }
    digits.chars().try_fold(0u64, |value, c| {
        let digit = decimal_value(c).ok_or_else(|| ParseErrorKind::NonDigits(digits.to_owned()))?;
        value
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or_else(|| ParseErrorKind::NumberTooLarge(digits.to_owned()))
    })
}

/// [`parse_u64`], for values stored as `u32`.
pub(crate) fn parse_u32(digits: &str) -> Result<u32, ParseErrorKind> {
    let value = parse_u64(digits)?;
    u32::try_from(value).map_err(|_| ParseErrorKind::NumberTooLarge(digits.to_owned()))
}

/// The input text of a system predicate, with the lowercase form the parser
/// works on and a way back to the text as typed.
#[derive(Debug)]
pub(crate) struct SystemText {
    original: String,
    lower: String,
    /// `(lower byte offset, original byte offset)` at every character
    /// boundary of the original.
    boundaries: Vec<(usize, usize)>,
}

pub(crate) const SYSTEM_PREFIX: &str = "system:";

impl SystemText {
    pub(crate) fn new(input: &str) -> Result<Self, ParseErrorKind> {
        let original = py_strip(input).to_owned();
        let lower = original.to_lowercase();
        let mut boundaries = Vec::with_capacity(original.len() + 1);
        let mut lower_offset = 0;
        for (original_offset, c) in original.char_indices() {
            boundaries.push((lower_offset, original_offset));
            // Final sigma maps to a different letter in context, but both
            // forms are the same length, so per-char lengths line up with
            // `to_lowercase` of the whole string.
            lower_offset += c.to_lowercase().map(char::len_utf8).sum::<usize>();
        }
        boundaries.push((lower_offset, original.len()));
        debug_assert_eq!(lower_offset, lower.len());

        if lower.starts_with('-') {
            return Err(ParseErrorKind::Negated);
        }
        if !lower.starts_with(SYSTEM_PREFIX) {
            return Err(ParseErrorKind::NotSystemPredicate);
        }
        Ok(Self {
            original,
            lower,
            boundaries,
        })
    }

    /// The whole text, lowercased.
    pub(crate) fn lower(&self) -> &str {
        &self.lower
    }

    /// The lowercased text after `system:`.
    pub(crate) fn subtag(&self) -> &str {
        &self.lower[SYSTEM_PREFIX.len()..]
    }

    /// The text as typed that corresponds to `part`, which must be a slice
    /// of [`Self::lower`].
    pub(crate) fn original_of<'a>(&'a self, part: &'a str) -> &'a str {
        let base = self.lower.as_ptr() as usize;
        let start = (part.as_ptr() as usize).wrapping_sub(base);
        let end = start.wrapping_add(part.len());
        if start > self.lower.len() || end > self.lower.len() || end < start {
            debug_assert!(false, "part is not a slice of the lowercase text");
            return part;
        }
        let from = self
            .boundaries
            .iter()
            .rev()
            .find(|(lower, _)| *lower <= start)
            .map_or(0, |(_, original)| *original);
        let to = self
            .boundaries
            .iter()
            .find(|(lower, _)| *lower >= end)
            .map_or(self.original.len(), |(_, original)| *original);
        &self.original[from..to]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unicode_decimal_digits_like_python() {
        assert_eq!(parse_u64("0123"), Ok(123));
        assert_eq!(parse_u64("\u{661}\u{662}"), Ok(12)); // Arabic-Indic
        assert_eq!(parse_u64("\u{ff15}"), Ok(5)); // fullwidth
        assert_eq!(parse_u64("\u{1d7d7}"), Ok(9)); // mathematical bold, in a run of 50
        assert!(parse_u64("1a").is_err());
        assert!(parse_u64("\u{b2}").is_err()); // superscript two is not a decimal digit
        assert!(matches!(
            parse_u64("18446744073709551616"),
            Err(ParseErrorKind::NumberTooLarge(_))
        ));
        assert_eq!(parse_u64("18446744073709551615"), Ok(u64::MAX));
    }

    #[test]
    fn strips_python_whitespace() {
        assert_eq!(py_strip("\u{1c} a b\u{3000}\t"), "a b");
    }

    #[test]
    fn widens_whitespace_classes() {
        assert_eq!(pythonise(r"a\sb"), r"a[\s\x1c-\x1f]b");
        assert_eq!(pythonise(r"[^\s]x"), r"[^\s\x1c-\x1f]x");
        assert_eq!(pythonise(r"(\s|,)*"), r"([\s\x1c-\x1f]|,)*");
        assert!(regex(r"^a\sb$").is_match("a\u{1f}b"));
    }

    #[test]
    fn maps_lowercase_slices_back_to_the_original() {
        let text = SystemText::new("  System:Has URL İstanbul.COM/X ").unwrap();
        let lower = text.lower();
        let at = lower.find("i̇stanbul").unwrap();
        assert_eq!(text.original_of(&lower[at..]), "İstanbul.COM/X");
        assert_eq!(text.original_of(&lower[..6]), "System");
    }

    #[test]
    fn rejects_non_system_text() {
        assert_eq!(
            SystemText::new("-system:inbox").unwrap_err(),
            ParseErrorKind::Negated
        );
        assert_eq!(
            SystemText::new("inbox").unwrap_err(),
            ParseErrorKind::NotSystemPredicate
        );
    }
}
