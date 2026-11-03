//! The parser test panel's results text, worded as the reference's
//! (`ContentParser.ParsePretty`, `PageParser.ParsePretty` and
//! `ParsableContentDescription.ToString`): a header with the number of
//! results, one line per parsed content with its kind's wording, the lines
//! of a post sorted, posts separated by a break, and a veto as its own line.
use hydrus_core::tag::{clean_tag, combine_tag, split_tag};
use hydrus_parse::content::{ContentKind, ParseFailure, ParsedContent, ParsedPost};
use jiff::tz::Offset;

/// What the test panels show when there is no document (`TestPanel.TestParse`).
pub const NOTHING_TO_PARSE: &str = "Nothing to parse!";

/// Whole numbers with thousands separators, as the reference's `ToHumanInt`.
fn human_int(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn url_wording(url_type: i64) -> &'static str {
    use hydrus_parse::content::url_type::{DESIRED, NEXT, SOURCE, SUB_GALLERY};
    match url_type {
        DESIRED => "downloadable/pursuable url",
        SOURCE => "associable/source url",
        NEXT => "next page url",
        SUB_GALLERY => "sub-gallery url",
        _ => "unknown url type!",
    }
}

/// Python's `bytes.fromhex`: pairs of hex digits, ASCII whitespace allowed
/// before a pair; the error names the first bad position in characters.
fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    let chars: Vec<char> = text.chars().collect();
    let bad = |at: usize| format!("non-hexadecimal number found in fromhex() arg at position {at}");
    // Python's `Py_ISSPACE`: space, \t, \n, \v, \f and \r
    let space = |c: char| matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r');
    let digit = |at: usize| chars.get(at).and_then(|c| c.to_digit(16));
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if space(chars[at]) {
            while at < chars.len() && space(chars[at]) {
                at += 1;
            }
            if at >= chars.len() {
                break;
            }
        }
        let top = digit(at).ok_or_else(|| bad(at))?;
        let bottom = digit(at + 1).ok_or_else(|| bad(at + 1))?;
        out.push((top * 16 + bottom) as u8);
        at += 2;
    }
    Ok(out)
}

/// Python's `base64.b64decode` (not strict, as Python 3.13's `binascii`
/// does it): characters outside the alphabet are skipped, padding does not
/// end the data, and a partial last group is accepted only when padding
/// completes it.
fn from_base64(text: &str) -> Result<Vec<u8>, String> {
    if !text.is_ascii() {
        return Err("string argument should contain only ASCII characters".into());
    }
    let mut out = Vec::new();
    let (mut quad, mut pads, mut left, mut padded) = (0, 0, 0u8, false);
    for c in text.bytes() {
        if c == b'=' {
            if quad >= 2 {
                pads += 1;
                padded |= quad + pads >= 4;
            }
            continue;
        }
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue,
        };
        pads = 0;
        padded = false;
        match quad {
            0 => left = value,
            1 => {
                out.push(left << 2 | value >> 4);
                left = value & 0x0f;
            }
            2 => {
                out.push(left << 4 | value >> 2);
                left = value & 0x03;
            }
            _ => out.push(left << 6 | value),
        }
        quad = (quad + 1) % 4;
    }
    match quad {
        _ if padded => Ok(out),
        0 => Ok(out),
        1 => Err(format!(
            "Invalid base64-encoded string: number of data characters ({}) cannot be 1 more than a multiple of 4",
            out.len() / 3 * 4 + 1
        )),
        _ => Err("Incorrect padding".into()),
    }
}

/// `GetHashFromParsedText` and the hash line's error text: a hex hash falls
/// back to base64, and the first decoding's error is the one shown.
fn hash_text(encoding: &str, text: &str) -> String {
    let decoded = match encoding {
        "hex" => from_hex(text).or_else(|error| from_base64(text).map_err(|_| error)),
        "base64" => from_base64(text),
        _ => Err("None".into()),
    };
    match decoded {
        Ok(bytes) => hex::encode(bytes),
        Err(error) => format!(
            "Could not decode a hash from {text}: Exception('Could not decode hash: {error}')"
        ),
    }
}

/// Python's `int( text )` for the digits a timestamp may be: surrounding
/// whitespace, a sign, and single underscores between digits. The number is
/// given back as Python writes it (no sign for zero, no leading zeros).
fn python_int(text: &str) -> Option<String> {
    let text = text.trim();
    let (negative, digits) = match text.as_bytes().first()? {
        b'-' => (true, &text[1..]),
        b'+' => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    let digits: String = digits.chars().filter(|c| *c != '_').collect();
    let digits = digits.trim_start_matches('0');
    Some(match (digits.is_empty(), negative) {
        (true, _) => "0".into(),
        (false, true) => format!("-{digits}"),
        (false, false) => digits.into(),
    })
}

/// A source time's text (`HydrusTime.TimestampToPrettyTime`): local time at the
/// machine's current offset, or the reference's words when it is no date.
fn timestamp_text(text: &str, offset: Offset) -> String {
    let Some(number) = python_int(text) else {
        return "could not convert to integer".into();
    };
    match number.parse::<i64>() {
        Ok(seconds) => hydrus_core::time::timestamp_to_iso_with_offset(Some(seconds), offset),
        Err(_) => format!("unparseable time {number}"),
    }
}

/// The machine's offset from UTC now, which the reference applies to every
/// time it shows (`HC.TIMEZONE_LOCAL`).
pub fn local_offset() -> Offset {
    jiff::Zoned::now().offset()
}

/// One parsed content's line (`ParsedContent.ToString`).
pub fn content_line(content: &ParsedContent, offset: Offset) -> String {
    let text = &content.text;
    match &content.kind {
        ContentKind::Url { url_type, priority } => {
            format!("{} (priority {priority}): {text}", url_wording(*url_type))
        }
        ContentKind::Tag { namespace } => {
            let combined = match namespace {
                None => text.clone(),
                Some(namespace) => combine_tag(namespace, text),
            };
            let tag = clean_tag(&combined);
            if split_tag(&tag).1.is_empty() {
                "tag: empty tag, will be discarded".into()
            } else {
                format!("tag: {tag}")
            }
        }
        ContentKind::Note { name } => format!("note \"{name}\":\n{text}"),
        ContentKind::Hash {
            hash_type,
            encoding,
        } => format!("{hash_type} hash: {}", hash_text(encoding, text)),
        ContentKind::Timestamp { .. } => {
            format!("source/post time: {}", timestamp_text(text, offset))
        }
        ContentKind::Title { priority } => {
            format!("watcher page title (priority {priority}): {text}")
        }
        ContentKind::HttpHeader { name } => format!("http header \"{name}\": {text}"),
        ContentKind::Variable { name } => format!("temp variable \"{name}\": {text}"),
        ContentKind::Veto { .. } => format!("veto: {}", content.name),
    }
}

fn sorted_lines(post: &ParsedPost, offset: Offset) -> Vec<String> {
    let mut lines: Vec<String> = post
        .contents
        .iter()
        .map(|content| content_line(content, offset))
        .collect();
    lines.sort();
    lines
}

/// The panel's text for a parse error. The reference follows it with Python's
/// traceback, which hydrus-rs has no equivalent of.
fn exception(message: &str) -> String {
    format!("Exception:\nParseException: {message}")
}

/// The content parser panel's results (`ContentParser.ParsePretty`) for the
/// parser named `name`; a parse error is named twice, as the reference's is.
pub fn content_preview(
    name: &str,
    result: Result<ParsedPost, ParseFailure>,
    offset: Offset,
) -> String {
    let lines = match result {
        Ok(post) => sorted_lines(&post, offset),
        Err(ParseFailure::Veto(name)) => vec![format!("veto: veto: {name}")],
        Err(ParseFailure::Error(error)) => {
            return exception(&format!("Content Parser {name}: {error}"));
        }
    };
    let mut out = vec![format!("*** {} RESULTS BEGIN ***", human_int(lines.len()))];
    out.extend(lines);
    out.push("*** RESULTS END ***".into());
    out.join("\n")
}

/// The page parser panel's results (`PageParser.ParsePretty`).
pub fn page_preview(result: Result<Vec<ParsedPost>, ParseFailure>, offset: Offset) -> String {
    let (count, text) = match result {
        Ok(posts) => (
            posts.len(),
            posts
                .iter()
                .map(|post| sorted_lines(post, offset).join("\n"))
                .collect::<Vec<_>>()
                .join("\n\n*** SEPARATE FILE RESULTS BREAK ***\n\n"),
        ),
        Err(ParseFailure::Veto(name)) => (1, format!("veto: veto: {name}")),
        Err(ParseFailure::Error(error)) => return exception(&error.to_string()),
    };
    format!(
        "*** {} RESULTS BEGIN ***\n\n{text}\n\n*** RESULTS END ***",
        human_int(count)
    )
}
