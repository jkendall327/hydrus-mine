//! Text rules for tag search: the "searchable" form of a subtag, how it
//! splits into words, and which subtags count as integers.
//!
//! These reproduce the reference's behaviour exactly because they decide
//! which tags autocomplete offers:
//! - searchable form: `[](){}/\"'-_` become spaces, whitespace collapses;
//! - words: the reference indexes searchable subtags with SQLite FTS4's
//!   "simple" tokenizer, where word characters are ASCII alphanumerics and
//!   *every* codepoint ≥ 128; everything else separates words;
//! - integers: Python's `str.isdecimal()` (any Unicode decimal digit script)
//!   and a value that fits in an `i64`.

/// Characters that autocomplete treats as spaces.
const IGNORED_SEARCH_CHARACTERS: &[char] =
    &['[', ']', '(', ')', '{', '}', '/', '\\', '"', '\'', '-', '_'];

/// The form of a subtag used for autocomplete matching.
pub fn searchable_subtag(subtag: &str) -> String {
    let mut collapsed_stars = String::with_capacity(subtag.len());
    let mut previous_star = false;
    for c in subtag.chars() {
        let star = c == '*';
        if !(star && previous_star) {
            collapsed_stars.push(if IGNORED_SEARCH_CHARACTERS.contains(&c) {
                ' '
            } else {
                c
            });
        }
        previous_star = star;
    }
    collapsed_stars
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether FTS4's simple tokenizer treats `c` as part of a word.
pub fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || u32::from(c) >= 128
}

/// Split searchable text into lowercase words, as FTS4's simple tokenizer does.
pub fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Whether a (possibly wildcarded) search string has anything a word index
/// can use.
pub fn has_word_chars(text: &str) -> bool {
    text.chars().any(|c| c != '*' && is_word_char(c))
}

/// The zero of every Unicode decimal digit run (Unicode 14, as the
/// reference's Python uses). Each is followed by the digits one to nine.
const DECIMAL_ZEROS: &[u32] = &[
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66,
    0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90,
    0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10,
    0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650,
    0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x16A60, 0x16AC0, 0x16B50,
    0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E950, 0x1FBF0,
];

fn decimal_value(c: char) -> Option<u32> {
    let cp = u32::from(c);
    let index = DECIMAL_ZEROS.partition_point(|&zero| zero <= cp);
    let zero = DECIMAL_ZEROS[index.checked_sub(1)?];
    let value = cp - zero;
    (value < 10).then_some(value)
}

/// The integer value of a subtag made only of decimal digits, if it fits an i64.
pub fn integer_subtag(subtag: &str) -> Option<i64> {
    if subtag.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for c in subtag.chars() {
        let digit = decimal_value(c)?;
        value = value.checked_mul(10)?.checked_add(i64::from(digit))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn searchable_forms() {
        assert_eq!(searchable_subtag("blue_eyes"), "blue eyes");
        assert_eq!(searchable_subtag("  (samus)  [aran] "), "samus aran");
        assert_eq!(searchable_subtag("a***b"), "a*b");
        assert_eq!(searchable_subtag("title:x"), "title:x");
        assert_eq!(searchable_subtag(""), "");
    }

    #[test]
    fn tokenises_like_fts4_simple() {
        let w: Vec<_> = words("samus aran:2 初音ミク、x").collect();
        // '、' is >= 128 so it is a word character to FTS4 simple
        assert_eq!(w, ["samus", "aran", "2", "初音ミク、x"]);
        assert!(!has_word_chars("**"));
        assert!(has_word_chars("*é"));
    }

    #[test]
    fn integers_follow_python_isdecimal() {
        assert_eq!(integer_subtag("123"), Some(123));
        assert_eq!(integer_subtag("００７"), Some(7));
        assert_eq!(integer_subtag("١٢"), Some(12));
        assert_eq!(integer_subtag("-1"), None);
        assert_eq!(integer_subtag("1.5"), None);
        assert_eq!(integer_subtag("½"), None);
        assert_eq!(integer_subtag("99999999999999999999"), None);
        assert_eq!(integer_subtag(""), None);
    }
}
