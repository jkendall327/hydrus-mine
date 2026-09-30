//! Python's `html` module: character references resolved exactly as
//! `html.unescape` does, HTML5's named entities included.

/// Python's `html.unescape`: named and numeric character references
/// replaced as HTML5 does.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if let Some((replacement, used)) = char_reference(after) {
            out.push_str(&replacement);
            rest = &after[used..];
        } else {
            out.push('&');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// A reference at the start of `s` (after the `&`): its replacement and how
/// many bytes it took, as Python's `_charref` pattern matches.
fn char_reference(s: &str) -> Option<(String, usize)> {
    if let Some(number) = s.strip_prefix('#') {
        let (digits, radix, skip) = match number.strip_prefix(['x', 'X']) {
            Some(hex) => (
                hex.chars()
                    .take_while(char::is_ascii_hexdigit)
                    .collect::<String>(),
                16,
                2,
            ),
            None => (
                number
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>(),
                10,
                1,
            ),
        };
        if digits.is_empty() {
            return None;
        }
        let mut used = skip + digits.len();
        if s[used..].starts_with(';') {
            used += 1;
        }
        let code = u32::from_str_radix(&digits, radix).unwrap_or(u32::MAX);
        return Some((numeric_reference(code), used));
    }
    // a name: up to 32 characters that aren't whitespace or <&#;, then an optional ;
    let name: String = s
        .chars()
        .take_while(|c| !matches!(c, '\t' | '\n' | '\u{c}' | ' ' | '<' | '&' | '#' | ';'))
        .take(32)
        .collect();
    if name.is_empty() {
        return None;
    }
    let mut key = name.clone();
    if s[name.len()..].starts_with(';') {
        key.push(';');
    }
    let used = key.len();
    if let Some(text) = named_entity(&key) {
        return Some((text, used));
    }
    // the longest known prefix, the rest kept as it was
    let chars: Vec<(usize, char)> = key.char_indices().collect();
    for n in (2..chars.len()).rev() {
        let (end, _) = chars[n];
        if let Some(text) = named_entity(&key[..end]) {
            return Some((text + &key[end..], used));
        }
    }
    Some((format!("&{key}"), used))
}

fn named_entity(name: &str) -> Option<String> {
    let &(first, second) = web_atoms::NAMED_ENTITIES.get(name)?;
    if first == 0 {
        return None;
    }
    let mut text: String = char::from_u32(first).into_iter().collect();
    if second != 0 {
        text.extend(char::from_u32(second));
    }
    Some(text)
}

/// Python's rules for `&#...;`: the Windows-1252 remapping of 0x80-0x9F,
/// U+FFFD for impossible code points, nothing for control characters and
/// non-characters.
fn numeric_reference(code: u32) -> String {
    match code {
        0 | 0xd800..=0xdfff | 0x0011_0000.. => return "\u{fffd}".to_owned(),
        0x0d => return "\r".to_owned(),
        0x80..=0x9f => {
            return web_atoms::C1_REPLACEMENTS[(code - 0x80) as usize]
                .unwrap_or_else(|| char::from_u32(code).expect("a C1 control"))
                .to_string();
        }
        _ => {}
    }
    let invalid = matches!(code, 0x1..=0x8 | 0xb | 0xe..=0x1f | 0x7f | 0xfdd0..=0xfdef)
        || (code & 0xfffe) == 0xfffe;
    if invalid {
        return String::new();
    }
    char::from_u32(code).map(String::from).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_like_python() {
        for (input, expected) in [
            ("a &amp; b", "a & b"),
            ("&amp", "&"),
            ("&ampxyz;", "&xyz;"),
            ("&notit;", "¬it;"),
            ("&#38;&#x26;&#X26", "&&&"),
            (
                "&#0;&#128;&#x81;&#xD800;&#1114112;",
                "\u{fffd}€\u{81}\u{fffd}\u{fffd}",
            ),
            ("&#1;&#xfffe;x", "x"),
            ("&nosuch; & &#; &#x;", "&nosuch; & &#; &#x;"),
            ("&NotNestedGreaterGreater;", "\u{2aa2}\u{338}"),
        ] {
            assert_eq!(unescape(input), expected, "{input}");
        }
    }
}
