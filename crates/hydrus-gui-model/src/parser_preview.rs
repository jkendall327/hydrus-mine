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

/// The hash line's text: the hash in hex, or the reference's words for why
/// it does not decode (`GetHashFromParsedText`, shared with downloads).
fn hash_text(encoding: &str, text: &str) -> String {
    match hydrus_core::pybytes::hash_from_parsed_text(encoding, text) {
        Ok(bytes) => hex::encode(bytes),
        Err(error) => format!(
            "Could not decode a hash from {text}: Exception('Could not decode hash: {error}')"
        ),
    }
}

/// A source time's text (`HydrusTime.TimestampToPrettyTime`): local time at the
/// machine's current offset, or the reference's words when it is no date.
fn timestamp_text(text: &str, offset: Offset) -> String {
    let Some(number) = hydrus_core::numbers::py_int_text(text) else {
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
