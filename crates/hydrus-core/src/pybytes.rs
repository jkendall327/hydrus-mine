//! Python's text-to-bytes decoders, with their exact leniencies.
//!
//! Parsers and string converters hand arbitrary scraped text to these, so
//! what they accept (and where they stop) is visible behaviour.

/// Python's `bytes.fromhex`: pairs of hex digits, with ASCII whitespace
/// (including vertical tab) allowed before a pair but not inside one. The
/// error is Python's, naming the first bad position in characters.
pub fn fromhex(text: &str) -> Result<Vec<u8>, String> {
    let chars: Vec<char> = text.chars().collect();
    let bad = |at: usize| format!("non-hexadecimal number found in fromhex() arg at position {at}");
    let digit = |at: usize| chars.get(at).and_then(|c| c.to_digit(16));
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if is_py_ascii_space(chars[at]) {
            while at < chars.len() && is_py_ascii_space(chars[at]) {
                at += 1;
            }
            if at >= chars.len() {
                break;
            }
        }
        let high = digit(at).ok_or_else(|| bad(at))?;
        let low = digit(at + 1).ok_or_else(|| bad(at + 1))?;
        out.push(u8::try_from(high * 16 + low).unwrap_or(0));
        at += 2;
    }
    Ok(out)
}

fn is_py_ascii_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// Python 3.13's `base64.b64decode` without `validate`: text must be ASCII;
/// characters outside the alphabet are skipped; padding does not end the
/// data, but a partial last group is accepted only when padding completes it;
/// stray pads are skipped. The errors are `binascii`'s.
pub fn b64decode(text: &str) -> Result<Vec<u8>, String> {
    if !text.is_ascii() {
        return Err("string argument should contain only ASCII characters".into());
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut quad_pos = 0u8;
    let mut pads = 0u8;
    let mut left = 0u8;
    let mut padded = false;
    for b in text.bytes() {
        if b == b'=' {
            if quad_pos >= 2 {
                pads += 1;
                padded |= quad_pos + pads >= 4;
            }
            continue;
        }
        let Some(value) = sextet(b) else { continue };
        pads = 0;
        padded = false;
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
    match quad_pos {
        _ if padded => Ok(out),
        0 => Ok(out),
        1 => Err(format!(
            "Invalid base64-encoded string: number of data characters ({}) cannot be 1 more than a multiple of 4",
            out.len() / 3 * 4 + 1
        )),
        _ => Err("Incorrect padding".into()),
    }
}

/// Python's `base64.urlsafe_b64decode`: `-` and `_` stand for `+` and `/`.
pub fn urlsafe_b64decode(text: &str) -> Result<Vec<u8>, String> {
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

/// The reference's `GetHashFromParsedText`: hex falls back to base64, and
/// the first decoding's error is the one given.
pub fn hash_from_parsed_text(encoding: &str, text: &str) -> Result<Vec<u8>, String> {
    match encoding {
        "hex" => fromhex(text).or_else(|error| b64decode(text).map_err(|_| error)),
        "base64" => b64decode(text),
        _ => Err("None".into()),
    }
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

    // (what Python 3.13 gives; the error words where it raises)
    #[test]
    fn hex_like_python() {
        assert_eq!(fromhex("de ad\x0bbe\tef"), Ok(vec![0xde, 0xad, 0xbe, 0xef]));
        assert_eq!(fromhex(""), Ok(vec![]));
        let bad = |at: usize| Err(format!("non-hexadecimal number found in fromhex() arg at position {at}"));
        assert_eq!(fromhex("d e"), bad(1));
        assert_eq!(fromhex("abc"), bad(3));
        assert_eq!(fromhex("zz"), bad(0));
        assert_eq!(fromhex("ab\u{3000}cd"), bad(2));
        assert_eq!(fromhex("abé1"), bad(2));
    }

    #[test]
    fn base64_like_python() {
        let padding = || Err("Incorrect padding".to_owned());
        assert_eq!(b64decode("aGk="), Ok(b"hi".to_vec()));
        // junk is skipped
        assert_eq!(b64decode("a!G k="), Ok(b"hi".to_vec()));
        // padding does not end the data
        assert_eq!(b64decode("aGk=garbage"), padding());
        assert_eq!(b64decode("aG==Zm9v"), padding());
        assert_eq!(b64decode("QQ==QQ=="), Ok(vec![0x41, 0x04, 0x10]));
        assert_eq!(b64decode("QQ=a=b=="), Ok(vec![0x41, 0x06, 0x9b]));
        // stray pads are skipped; one pad at quad position two is not enough
        assert_eq!(
            b64decode("=aG=Zm9vYQ=="),
            Ok(vec![0x68, 0x66, 0x66, 0xf6, 0xf6, 0x10])
        );
        // leftovers are an error
        assert_eq!(b64decode("aGk"), padding());
        assert_eq!(
            b64decode("a"),
            Err("Invalid base64-encoded string: number of data characters (1) cannot be 1 more than a multiple of 4".into())
        );
        assert_eq!(
            b64decode("é"),
            Err("string argument should contain only ASCII characters".into())
        );
        assert_eq!(
            b64decode("<section href=\"page.html\" id=\"\"></section>"),
            Err("Invalid base64-encoded string: number of data characters (29) cannot be 1 more than a multiple of 4".into())
        );
        assert_eq!(urlsafe_b64decode("-_8="), Ok(vec![0xfb, 0xff]));
    }

    #[test]
    fn hashes_fall_back_from_hex_to_base64() {
        assert_eq!(hash_from_parsed_text("hex", "aGVsbG8="), Ok(b"hello".to_vec()));
        assert_eq!(
            hash_from_parsed_text("hex", "zz"),
            Err("non-hexadecimal number found in fromhex() arg at position 0".into())
        );
    }
}
