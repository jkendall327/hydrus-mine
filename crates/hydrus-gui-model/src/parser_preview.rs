//! The parser test panel's results text, worded as the reference's
//! (`ContentParser.ParsePretty`, `PageParser.ParsePretty` and
//! `ParsableContentDescription.ToString`): a header with the number of
//! results, one line per parsed content with its kind's wording, the lines
//! of a post sorted, posts separated by a break, and a veto as its own line.
use hydrus_core::tag::{clean_tag, combine_tag, split_tag};
use hydrus_parse::content::{ContentKind, ParseFailure, ParsedContent, ParsedPost};

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

/// A standard-alphabet base64 decode that, like Python's `b64decode`, ignores
/// characters outside the alphabet and needs correct padding.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::new();
    let mut padding = 0;
    for c in text.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                continue;
            }
            _ => continue,
        };
        bits = bits << 6 | u32::from(value);
        count += 1;
        if count == 4 {
            out.extend([(bits >> 16) as u8, (bits >> 8) as u8, bits as u8]);
            bits = 0;
            count = 0;
        }
    }
    match (count, padding) {
        (0, _) => Some(out),
        (2, 2) => {
            out.push((bits >> 4) as u8);
            Some(out)
        }
        (3, 1) => {
            out.extend([(bits >> 10) as u8, (bits >> 2) as u8]);
            Some(out)
        }
        _ => None,
    }
}

fn hash_text(encoding: &str, text: &str) -> String {
    let by_hex = || hex::decode(text).ok();
    let decoded = match encoding {
        "hex" => by_hex().or_else(|| base64_decode(text)),
        "base64" => base64_decode(text),
        _ => None,
    };
    match decoded {
        Some(bytes) => hex::encode(bytes),
        None => format!("Could not decode a hash from {text}: Exception('Could not decode hash')"),
    }
}

/// One parsed content's line (`ParsedContent.ToString`).
pub fn content_line(content: &ParsedContent) -> String {
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
        ContentKind::Timestamp { .. } => match text.parse::<i64>() {
            Ok(seconds) => format!(
                "source/post time: {}",
                hydrus_import::status::pretty_time(hydrus_core::time::TimestampMs(
                    seconds.saturating_mul(1000)
                ))
            ),
            Err(_) => "source/post time: could not convert to integer".into(),
        },
        ContentKind::Title { priority } => {
            format!("watcher page title (priority {priority}): {text}")
        }
        ContentKind::HttpHeader { name } => format!("http header \"{name}\": {text}"),
        ContentKind::Variable { name } => format!("temp variable \"{name}\": {text}"),
        ContentKind::Veto { .. } => format!("veto: {}", content.name),
    }
}

fn sorted_lines(post: &ParsedPost) -> Vec<String> {
    let mut lines: Vec<String> = post.contents.iter().map(content_line).collect();
    lines.sort();
    lines
}

/// The content parser panel's results (`ContentParser.ParsePretty`); a parse
/// error other than a veto is its own message.
pub fn content_preview(result: Result<ParsedPost, ParseFailure>) -> String {
    let lines = match result {
        Ok(post) => sorted_lines(&post),
        Err(ParseFailure::Veto(name)) => vec![format!("veto: veto: {name}")],
        Err(error) => return error.to_string(),
    };
    let mut out = vec![format!("*** {} RESULTS BEGIN ***", human_int(lines.len()))];
    out.extend(lines);
    out.push("*** RESULTS END ***".into());
    out.join("\n")
}

/// The page parser panel's results (`PageParser.ParsePretty`).
pub fn page_preview(result: Result<Vec<ParsedPost>, ParseFailure>) -> String {
    let (count, text) = match result {
        Ok(posts) => (
            posts.len(),
            posts
                .iter()
                .map(|post| sorted_lines(post).join("\n"))
                .collect::<Vec<_>>()
                .join("\n\n*** SEPARATE FILE RESULTS BREAK ***\n\n"),
        ),
        Err(ParseFailure::Veto(name)) => (1, format!("veto: veto: {name}")),
        Err(error) => return error.to_string(),
    };
    format!(
        "*** {} RESULTS BEGIN ***\n\n{text}\n\n*** RESULTS END ***",
        human_int(count)
    )
}
