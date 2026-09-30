//! Value and unit parsing: numbers, hashes, filetype lists, durations and the
//! like. Each function takes the text after the operator and returns what it
//! parsed plus whatever text is left.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use hydrus_core::{HashKind, Md5, Mime, PerceptualHash, Sha1, Sha256, Sha512};

use crate::error::ParseErrorKind;
use crate::filetype::{FILETYPE_WORDS, FiletypeSet};
use crate::number::Comparison;
use crate::parse::operators::{Result, expected, relational};
use crate::parse::text::{StaticRegex, fancy_regex, is_py_space, parse_u64, py_strip, regex};
use crate::predicate::{FileHashes, NamespaceFilter, PixelUnit, Relationship, SizeUnit};

static NUMBER: StaticRegex = LazyLock::new(|| regex(r"^-?[0-9,]+"));

/// A number with optional thousands separators. `has...`/`no...` count as 0
/// (for `system:width: has width`).
fn signed_number(s: &str) -> Result<(&str, bool, u64)> {
    let s = py_strip(s);
    if s.starts_with("has") || s.starts_with("no") {
        return Ok(("", false, 0));
    }
    let m = NUMBER.find(s).ok_or_else(|| expected("a number", s))?;
    let text = m.as_str();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, text),
    };
    let digits = digits.replace(',', "");
    if digits.is_empty() {
        return Err(expected("a number", text));
    }
    let magnitude = parse_u64(&digits)?;
    Ok((&s[m.end()..], negative && magnitude != 0, magnitude))
}

/// A whole number of zero or more.
pub(crate) fn natural(s: &str) -> Result<(&str, u64)> {
    let (rest, negative, magnitude) = signed_number(s)?;
    if negative {
        return Err(ParseErrorKind::Negative(format!("-{magnitude}")));
    }
    Ok((rest, magnitude))
}

/// A whole number, possibly negative.
pub(crate) fn integer(s: &str) -> Result<(&str, i64)> {
    let (rest, negative, magnitude) = signed_number(s)?;
    let too_large = || ParseErrorKind::NumberTooLarge(magnitude.to_string());
    let value = if negative {
        0i64.checked_sub_unsigned(magnitude).ok_or_else(too_large)?
    } else {
        i64::try_from(magnitude).map_err(|_| too_large())?
    };
    Ok((rest, value))
}

/// The unit of `system:filesize`.
pub(crate) fn size_unit(s: &str) -> Result<(&str, SizeUnit)> {
    let s = py_strip(s);
    // first match wins, so "bytes" is read as "b" + "ytes", as in the reference
    for (spellings, unit) in [
        (&["b", "byte", "bytes"], SizeUnit::Bytes),
        (&["kb", "kilobytes", "kilobyte"], SizeUnit::Kilobytes),
        (&["mb", "megabytes", "megabyte"], SizeUnit::Megabytes),
        (&["gb", "gigabytes", "gigabyte"], SizeUnit::Gigabytes),
    ] {
        if let Some(rest) = spellings.iter().find_map(|sp| s.strip_prefix(sp)) {
            return Ok((rest, unit));
        }
    }
    Err(expected("a size unit (B, KB, MB, GB)", s))
}

/// The unit of `system:num pixels`.
pub(crate) fn pixel_unit(s: &str) -> Result<(&str, PixelUnit)> {
    let s = py_strip(s);
    for (spellings, unit) in [
        (&["px", "pixels", "pixel"], PixelUnit::Pixels),
        (&["kpx", "kilopixels", "kilopixel"], PixelUnit::Kilopixels),
        (&["mpx", "megapixels", "megapixel"], PixelUnit::Megapixels),
    ] {
        if let Some(rest) = spellings.iter().find_map(|sp| s.strip_prefix(sp)) {
            return Ok((rest, unit));
        }
    }
    Err(expected("a pixel unit (px, kilopixels, megapixels)", s))
}

/// An optional unit that means nothing (`px` after a width, `fps` after a
/// framerate).
pub(crate) fn optional_unit<'a>(s: &'a str, spellings: &[&str]) -> Result<&'a str> {
    let s = py_strip(s);
    if s.is_empty() {
        return Ok(s);
    }
    spellings
        .iter()
        .find_map(|sp| s.strip_prefix(sp))
        .ok_or_else(|| expected("no unit, or the property's unit", s))
}

/// The relationship kind of `system:num file relationships`.
pub(crate) fn relationship(s: &str) -> Result<(&str, Relationship)> {
    static FALSE_POSITIVE: StaticRegex = LazyLock::new(|| {
        regex(r"^(?:(not related/false positives?)|not related|(false positives?))")
    });
    let s = py_strip(s);
    if let Some(rest) = s.strip_prefix("duplicates") {
        return Ok((rest, Relationship::Duplicates));
    }
    if let Some(rest) = s.strip_prefix("alternates") {
        return Ok((rest, Relationship::Alternates));
    }
    if let Some(m) = FALSE_POSITIVE.find(s) {
        return Ok((&s[m.end()..], Relationship::FalsePositives));
    }
    if let Some(rest) = s.strip_prefix("potential duplicates") {
        return Ok((rest, Relationship::PotentialDuplicates));
    }
    Err(expected(
        "a relationship (duplicates, alternates, false positives, potential duplicates)",
        s,
    ))
}

/// Split a matched run of hashes on whitespace and commas.
fn split_hashes(run: &str) -> impl Iterator<Item = &str> {
    run.split(|c: char| c == ',' || is_py_space(c))
        .filter(|h| !h.is_empty())
}

fn hex_bytes(hash: &str) -> Result<Vec<u8>> {
    hex::decode(hash).map_err(|_| ParseErrorKind::InvalidHex {
        hash: hash.to_owned(),
    })
}

fn wrong_length(hash: &str, kind: HashKind, actual: usize) -> ParseErrorKind {
    ParseErrorKind::WrongHashLength {
        hash: hash.to_owned(),
        kind,
        expected: kind.byte_len(),
        actual,
    }
}

fn typed_hashes<T: Ord>(
    hashes: &[&str],
    kind: HashKind,
    from_slice: impl Fn(&[u8]) -> Option<T>,
) -> Result<BTreeSet<T>> {
    hashes
        .iter()
        .map(|&hash| {
            let bytes = hex_bytes(hash)?;
            from_slice(&bytes).ok_or_else(|| wrong_length(hash, kind, bytes.len()))
        })
        .collect()
}

/// The hashes of `system:hash`. As in the reference, the algorithm is chosen
/// by `md5`/`sha1`/`sha512` appearing anywhere in the predicate, the first run
/// of long hex words anywhere in the value is taken as the hashes, and
/// anything else is ignored.
pub(crate) fn hash_list(s: &str, whole_text: &str) -> Result<FileHashes> {
    static HASHES: StaticRegex = LazyLock::new(|| regex(r"([0-9a-f]{8}[0-9a-f]+(\s|,)*)+"));
    let kind = if whole_text.contains("md5") {
        HashKind::Md5
    } else if whole_text.contains("sha1") {
        HashKind::Sha1
    } else if whole_text.contains("sha512") {
        HashKind::Sha512
    } else {
        HashKind::Sha256
    };
    let s = py_strip(s);
    let run = HASHES
        .find(s)
        .ok_or_else(|| expected("a list of hex hashes", s))?;
    let hashes: Vec<&str> = split_hashes(run.as_str()).collect();
    Ok(match kind {
        HashKind::Sha256 => {
            FileHashes::Sha256(typed_hashes(&hashes, kind, |b| Sha256::from_slice(b).ok())?)
        }
        HashKind::Md5 => FileHashes::Md5(typed_hashes(&hashes, kind, |b| Md5::from_slice(b).ok())?),
        HashKind::Sha1 => {
            FileHashes::Sha1(typed_hashes(&hashes, kind, |b| Sha1::from_slice(b).ok())?)
        }
        HashKind::Sha512 => {
            FileHashes::Sha512(typed_hashes(&hashes, kind, |b| Sha512::from_slice(b).ok())?)
        }
    })
}

static HASHES_WITH_DISTANCE: StaticRegex = LazyLock::new(|| {
    regex(
        r"^(?P<hashes>([0-9a-f]{4}[0-9a-f]+(\s|,)*)+)(with\s+)?(distance\s+)?(of\s+)?(?P<distance>0|([1-9][0-9]*))?",
    )
});

/// Hex words followed by an optional `with distance of N`.
fn hashes_with_distance(s: &str, default_distance: u64) -> Result<(&str, Vec<&str>, u64)> {
    let s = py_strip(s);
    let caps = HASHES_WITH_DISTANCE
        .captures(s)
        .ok_or_else(|| expected("a list of hex hashes, then optionally a distance", s))?;
    let hashes = split_hashes(caps.name("hashes").map_or("", |m| m.as_str())).collect();
    let distance = match caps.name("distance") {
        Some(m) => parse_u64(m.as_str())?,
        None => default_distance,
    };
    let end = caps.get(0).map_or(0, |m| m.end());
    Ok((&s[end..], hashes, distance))
}

/// The files and distance of `system:similar to files`.
pub(crate) fn similar_files(s: &str) -> Result<(&str, BTreeSet<Sha256>, u64)> {
    let (rest, hashes, distance) = hashes_with_distance(s, 4)?;
    let files = typed_hashes(&hashes, HashKind::Sha256, |b| Sha256::from_slice(b).ok())?;
    Ok((rest, files, distance))
}

/// The pixel hashes, perceptual hashes and distance of
/// `system:similar to data`. Hex words of other lengths are ignored.
pub(crate) fn similar_data(
    s: &str,
) -> Result<(&str, BTreeSet<Sha256>, BTreeSet<PerceptualHash>, u64)> {
    let (rest, hashes, distance) = hashes_with_distance(s, 8)?;
    let mut pixel = BTreeSet::new();
    let mut perceptual = BTreeSet::new();
    for hash in hashes {
        match hash.len() {
            64 => {
                let bytes = hex_bytes(hash)?;
                pixel.insert(
                    Sha256::from_slice(&bytes)
                        .map_err(|_| wrong_length(hash, HashKind::Sha256, bytes.len()))?,
                );
            }
            16 => {
                let bytes = hex_bytes(hash)?;
                perceptual.insert(
                    PerceptualHash::from_slice(&bytes)
                        .map_err(|_| expected("a 16-character perceptual hash", hash))?,
                );
            }
            _ => {}
        }
    }
    Ok((rest, pixel, perceptual, distance))
}

static FILETYPE_LIST: StaticRegex = LazyLock::new(|| {
    // Like the reference, the words go into the pattern unescaped (so
    // `image/svg+xml` matches "image/svgxml"), longest first.
    let mut words: Vec<&str> = FILETYPE_WORDS.iter().map(|(w, _)| *w).collect();
    words.sort_by_key(|w| std::cmp::Reverse(w.chars().count()));
    let alternatives: Vec<String> = words.iter().map(|w| format!("({w})")).collect();
    let one = format!("({})", alternatives.join("|"));
    regex(&format!(r"^({one}(\s|,)+)*{one}"))
});

/// A list of filetype words. Words are separated by commas; a
/// whitespace-separated group that is not itself a known word is ignored,
/// as in the reference.
pub(crate) fn filetype_list(s: &str) -> Result<(&str, FiletypeSet)> {
    let s = py_strip(s);
    let m = FILETYPE_LIST
        .find(s)
        .ok_or_else(|| expected("a list of filetypes", s))?;
    let mut mimes: Vec<Mime> = Vec::new();
    for word in m.as_str().split(',') {
        // the reference turns every whitespace character into a space first
        let word: String = word
            .chars()
            .map(|c| if is_py_space(c) { ' ' } else { c })
            .collect();
        let word = py_strip(&word);
        if let Some((_, types)) = FILETYPE_WORDS.iter().find(|(w, _)| *w == word) {
            mimes.extend_from_slice(types);
        }
    }
    Ok((&s[m.end()..], FiletypeSet::new(mimes)))
}

/// A duration such as `5 minutes 30 seconds`, `1h`, `5m30s` or `600 ms`,
/// in milliseconds.
pub(crate) fn duration_ms(s: &str) -> Result<(&str, u64)> {
    static DURATION: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
        fancy_regex(concat!(
            r"^((?P<hour>0|([1-9][0-9]*))\s*(hours|hour|hr|h))?\s*",
            r"((?P<min>0|([1-9][0-9]*))\s*(minutes|minute|mins|m$|m\s|m(?=\d)))?\s*",
            r"((?P<sec>0|([1-9][0-9]*))\s*(seconds|second|secs|sec|s))?\s*",
            r"((?P<msec>0|([1-9][0-9]*))\s*(milliseconds|millisecond|msecs|msec|ms))?",
        ))
    });
    let s = py_strip(s);
    if s.starts_with("has") || s.starts_with("no") {
        return Ok(("", 0));
    }
    let caps = DURATION
        .captures(s)
        .ok()
        .flatten()
        .ok_or_else(|| expected("a duration", s))?;
    let mut total: u64 = 0;
    let mut any = false;
    for (group, ms) in [
        ("hour", 3_600_000),
        ("min", 60_000),
        ("sec", 1000),
        ("msec", 1),
    ] {
        if let Some(m) = caps.name(group) {
            any = true;
            let part = parse_u64(m.as_str())?
                .checked_mul(ms)
                .ok_or_else(|| ParseErrorKind::NumberTooLarge(m.as_str().to_owned()))?;
            total = total
                .checked_add(part)
                .ok_or_else(|| ParseErrorKind::NumberTooLarge(s.to_owned()))?;
        }
    }
    if !any {
        return Err(expected("a duration, like \"5 minutes 30 seconds\"", s));
    }
    let end = caps.get(0).map_or(0, |m| m.end());
    Ok((&s[end..], total))
}

/// A time interval such as `1 day 3 hours 5 minutes`, in seconds.
pub(crate) fn interval_seconds(s: &str) -> Result<(&str, u64)> {
    static INTERVAL: StaticRegex = LazyLock::new(|| {
        regex(concat!(
            r"^((?P<day>0|([1-9][0-9]*))\s*(days|day))?\s*",
            r"((?P<hour>0|([1-9][0-9]*))\s*(hours|hour|h))?\s*",
            r"((?P<minute>0|([1-9][0-9]*))\s*(minutes|minute|mins|min))?\s*",
            r"((?P<second>0|([1-9][0-9]*))\s*(seconds|second|secs|sec|s))?",
        ))
    });
    let s = py_strip(s);
    let caps = INTERVAL
        .captures(s)
        .ok_or_else(|| expected("a time interval", s))?;
    let mut total: u64 = 0;
    let mut any = false;
    for (group, secs) in [
        ("day", 86_400),
        ("hour", 3600),
        ("minute", 60),
        ("second", 1),
    ] {
        if let Some(m) = caps.name(group) {
            any = true;
            let part = parse_u64(m.as_str())?
                .checked_mul(secs)
                .ok_or_else(|| ParseErrorKind::NumberTooLarge(m.as_str().to_owned()))?;
            total = total
                .checked_add(part)
                .ok_or_else(|| ParseErrorKind::NumberTooLarge(s.to_owned()))?;
        }
    }
    if !any {
        return Err(expected("a time interval, like \"1 day 3 hours\"", s));
    }
    let end = caps.get(0).map_or(0, |m| m.end());
    Ok((&s[end..], total))
}

/// A `width:height` ratio.
pub(crate) fn ratio(s: &str) -> Result<(&str, u64, u64)> {
    static RATIO: StaticRegex =
        LazyLock::new(|| regex(r"^(?P<first>0|([1-9][0-9]*)):(?P<second>0|([1-9][0-9]*))"));
    let s = py_strip(s);
    let caps = RATIO
        .captures(s)
        .ok_or_else(|| expected("a ratio like 16:9", s))?;
    let width = parse_u64(&caps["first"])?;
    let height = parse_u64(&caps["second"])?;
    let end = caps.get(0).map_or(0, |m| m.end());
    Ok((&s[end..], width, height))
}

/// `<namespace> tags <operator> <n>`, for `system:number of character tags > 5`.
pub(crate) fn namespace_tag_count(s: &str) -> Result<(NamespaceFilter, Comparison, u64)> {
    static NAMESPACE_COUNT: StaticRegex =
        LazyLock::new(|| regex(r"^(?P<namespace>.+) tags (?P<operator>.+?)\s?(?P<num>\d+)\s*$"));
    let s = py_strip(s);
    let caps = NAMESPACE_COUNT
        .captures(s)
        .ok_or_else(|| expected("\"<namespace> tags <operator> <number>\"", s))?;
    let namespace = match &caps["namespace"] {
        "unnamespaced" => NamespaceFilter::Unnamespaced,
        other => NamespaceFilter::from_reference(other),
    };
    // Only the operator is used; any text after it is ignored, as in the
    // reference (so ">=" means ">").
    let (_, op) = relational(&caps["operator"])?;
    let count = parse_u64(&caps["num"])?;
    Ok((namespace, op, count))
}

/// A free-text value: the rest of the predicate, trimmed.
pub(crate) fn any_string(s: &str) -> &str {
    py_strip(s)
}

/// Remove one pair of matching quotes around a note name.
pub(crate) fn strip_quotes(s: &str) -> &str {
    if s.chars().count() > 2 {
        for quote in ['\'', '"'] {
            if s.starts_with(quote) && s.ends_with(quote) {
                return &s[1..s.len() - 1];
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_allow_commas_and_has() {
        assert_eq!(natural(" 1,000 px").unwrap(), (" px", 1000));
        assert_eq!(natural("has width").unwrap(), ("", 0));
        assert_eq!(natural("-0").unwrap(), ("", 0));
        assert!(natural("-5").is_err());
        assert!(natural(",").is_err());
        assert_eq!(integer("-5").unwrap(), ("", -5));
        assert!(matches!(
            natural("99999999999999999999"),
            Err(ParseErrorKind::NumberTooLarge(_))
        ));
    }

    #[test]
    fn size_units_match_the_first_spelling() {
        assert_eq!(size_unit("kilobytes").unwrap(), ("", SizeUnit::Kilobytes));
        assert_eq!(size_unit("bytes").unwrap(), ("ytes", SizeUnit::Bytes));
    }

    #[test]
    fn durations() {
        assert_eq!(duration_ms("5m30s").unwrap(), ("", 330_000));
        assert_eq!(duration_ms("5 sec 6000 msecs").unwrap(), ("", 11_000));
        assert_eq!(duration_ms("1 hour").unwrap(), ("", 3_600_000));
        assert!(duration_ms("x").is_err());
    }

    #[test]
    fn filetype_lists() {
        let (rest, set) = filetype_list("image/jpg, image/png, apng").unwrap();
        assert_eq!(rest, "");
        assert_eq!(set.summary().len(), 3);
        let (_, empty) = filetype_list("jpeg png").unwrap();
        assert!(empty.is_empty());
    }
}
