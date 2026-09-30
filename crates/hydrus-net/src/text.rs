//! Turning a downloaded page into text.
//!
//! The reference trusts a charset the server states and otherwise guesses
//! (chardet), since the fallback `requests` fills in (ISO-8859-1) decodes
//! anything. We do the same with the web's own decoders: a stated charset,
//! else UTF-8 if the bytes are valid UTF-8, else a charset the page declares
//! in a `<meta>` tag, else chardetng's guess.

use encoding_rs::{Encoding, UTF_8};

/// The charset a `Content-Type` header names, if any.
pub fn header_charset(content_type: &str) -> Option<&str> {
    content_type.split(';').skip(1).find_map(|param| {
        let (name, value) = param.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("charset")
            .then(|| value.trim().trim_matches(|c| c == '"' || c == '\''))
    })
}

/// Decode a response body; bad sequences become U+FFFD.
pub fn decode(body: &[u8], content_type: Option<&str>) -> String {
    let stated = content_type
        .and_then(header_charset)
        .and_then(|label| Encoding::for_label(label.as_bytes()));
    let encoding = stated.unwrap_or_else(|| guess(body));
    let (text, _, _) = encoding.decode(body);
    text.into_owned()
}

fn guess(body: &[u8]) -> &'static Encoding {
    if std::str::from_utf8(body).is_ok() {
        return UTF_8;
    }
    if let Some(declared) = meta_charset(body) {
        return declared;
    }
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(body, true);
    detector.guess(None, chardetng::Utf8Detection::Allow)
}

/// A charset declared in the first 1024 bytes (`<meta charset=...>` or
/// `<meta http-equiv="Content-Type" content="...; charset=...">`).
fn meta_charset(body: &[u8]) -> Option<&'static Encoding> {
    let head = &body[..body.len().min(1024)];
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();
    let at = head.find("charset=")?;
    let rest = head[at + "charset=".len()..].trim_start_matches(['"', '\'']);
    let label: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
        .collect();
    Encoding::for_label(label.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_by_header_then_utf8_then_meta_then_guess() {
        assert_eq!(
            decode(b"caf\xe9", Some("text/html; charset=ISO-8859-1")),
            "café"
        );
        assert_eq!(decode("café".as_bytes(), Some("text/html")), "café");
        assert_eq!(
            decode(b"<meta charset=\"windows-1252\">caf\xe9", None),
            "<meta charset=\"windows-1252\">café"
        );
        assert_eq!(header_charset("text/html;Charset=\"utf-8\""), Some("utf-8"));
    }
}
