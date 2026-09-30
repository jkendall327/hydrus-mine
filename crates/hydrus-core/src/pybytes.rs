//! Python's text-to-bytes decoders, with their exact leniencies.
//!
//! Parsers and string converters hand arbitrary scraped text to these, so
//! what they accept (and where they stop) is visible behaviour.

/// Python's `bytes.fromhex`: pairs of hex digits, with ASCII whitespace
/// (including vertical tab) allowed between pairs but not inside one.
pub fn fromhex(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.peek().is_some_and(|&c| is_py_ascii_space(c)) {
            chars.next();
        }
        let Some(high) = chars.next() else { break };
        let low = chars.next()?;
        out.push(u8::try_from(high.to_digit(16)? * 16 + low.to_digit(16)?).ok()?);
    }
    Some(out)
}

fn is_py_ascii_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// Python's `base64.b64decode` without `validate`: text must be ASCII;
/// characters outside the alphabet are skipped; a pad sequence that
/// completes a quad ends the data (anything after it is ignored); stray pads
/// are skipped; leftover characters are an error.
pub fn b64decode(text: &str) -> Option<Vec<u8>> {
    if !text.is_ascii() {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut quad_pos = 0u8;
    let mut pads = 0u8;
    let mut left = 0u8;
    for b in text.bytes() {
        if b == b'=' {
            if quad_pos >= 2 {
                pads += 1;
                if quad_pos + pads >= 4 {
                    return Some(out);
                }
            }
            continue;
        }
        let Some(value) = sextet(b) else { continue };
        pads = 0;
        match quad_pos {
            0 => {
                left = value;
                quad_pos = 1;
            }
            1 => {
                out.push((left << 2) | (value >> 4));
                left = value & 0x0f;
                quad_pos = 2;
            }
            2 => {
                out.push((left << 4) | (value >> 2));
                left = value & 0x03;
                quad_pos = 3;
            }
            _ => {
                out.push((left << 6) | value);
                quad_pos = 0;
            }
        }
    }
    (quad_pos == 0).then_some(out)
}

/// Python's `base64.urlsafe_b64decode`: `-` and `_` stand for `+` and `/`.
pub fn urlsafe_b64decode(text: &str) -> Option<Vec<u8>> {
    let translated: String = text
        .chars()
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    b64decode(&translated)
}

fn sextet(b: u8) -> Option<u8> {
    Some(match b {
        b'A'..=b'Z' => b - b'A',
        b'a'..=b'z' => b - b'a' + 26,
        b'0'..=b'9' => b - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_like_python() {
        assert_eq!(
            fromhex("de ad\x0bbe\tef"),
            Some(vec![0xde, 0xad, 0xbe, 0xef])
        );
        assert_eq!(fromhex(""), Some(vec![]));
        assert_eq!(fromhex("d e"), None);
        assert_eq!(fromhex("abc"), None);
        assert_eq!(fromhex("zz"), None);
    }

    #[test]
    fn base64_like_python() {
        assert_eq!(b64decode("aGk="), Some(b"hi".to_vec()));
        // junk is skipped
        assert_eq!(b64decode("a!G k=").as_deref(), Some(&b"hi"[..]));
        // a completing pad ends the data
        assert_eq!(b64decode("aGk=garbage").as_deref(), Some(&b"hi"[..]));
        assert_eq!(b64decode("aG==Zm9v").as_deref(), Some(&b"h"[..]));
        // stray pads are skipped; one pad at quad position two is not enough
        assert_eq!(
            b64decode("=aG=Zm9vYQ==").as_deref(),
            Some(&[0x68, 0x66, 0x66, 0xf6, 0xf6, 0x10][..])
        );
        // leftovers are an error
        assert_eq!(b64decode("aGk"), None);
        assert_eq!(b64decode("a"), None);
        assert_eq!(b64decode("é"), None);
        // `sectionh` + `ref=`: the pad in the markup ends it
        assert_eq!(
            b64decode("<section href=\"page.html\" id=\"\"></section>").map(|b| b.len()),
            Some(8)
        );
        assert_eq!(urlsafe_b64decode("-_8="), Some(vec![0xfb, 0xff]));
    }
}
