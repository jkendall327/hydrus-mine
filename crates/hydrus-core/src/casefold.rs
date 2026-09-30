//! Python's `str.casefold()`, which the reference uses to compare and sort
//! text case-insensitively.
//!
//! Folding is per character and needs no context (unlike lowercasing, where
//! a final sigma differs), so it is a table lookup. The table comes from the
//! Python the reference runs on (`oracle/dump_casefold.py`).

#[path = "casefold_table.rs"]
mod table;

use table::CASEFOLD;

/// `text.casefold()`.
pub fn casefold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
        } else {
            match CASEFOLD.binary_search_by_key(&c, |&(k, _)| k) {
                Ok(i) => out.push_str(CASEFOLD[i].1),
                Err(_) => out.push(c),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::casefold;

    #[test]
    fn folds_as_python_does() {
        for (text, folded) in [
            ("Booru Post", "booru post"),
            ("Straße", "strasse"),
            ("ẞ", "ss"),
            ("ΣΑΣ ς", "σασ σ"),
            ("ﬁle", "file"),
            ("İstanbul", "i\u{307}stanbul"),
            ("\u{ab70}", "\u{13a0}"),
            ("µ", "μ"),
            ("ǅ", "ǆ"),
        ] {
            assert_eq!(casefold(text), folded, "{text}");
        }
    }

    #[test]
    fn the_table_is_sorted() {
        assert!(super::CASEFOLD.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
