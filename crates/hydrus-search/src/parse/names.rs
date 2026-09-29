//! Recognising which predicate a `system:` string names.
//!
//! The reference tries a list of patterns in order and takes the first that
//! matches at the start of the (lowercased) text after `system:`; later
//! stages then parse whatever follows. The order is significant (e.g.
//! `has duration` must be tried before `duration`), and a match commits: if
//! the rest fails to parse, no later pattern is tried.
//!
//! Patterns are written exactly as in the reference, which lets a space in a
//! name also be written as any run of spaces and underscores, and allows a
//! colon straight after the name.

use std::sync::LazyLock;

use crate::parse::text::fancy_regex;
use crate::predicate::{FileProperty, NumericProperty};
use crate::time::TimeKind;

/// A predicate name, i.e. which parse routine handles the rest of the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Name {
    Everything,
    Inbox,
    Archive,
    /// `has duration`, `has notes`, ...: the property is non-zero.
    Has(NumericProperty),
    /// `no duration`, `no notes`, ...: the property is zero.
    HasNo(NumericProperty),
    BestQuality(bool),
    Property(FileProperty, bool),
    HasTags,
    Untagged,
    NumTags,
    NumTagsInNamespace,
    NumUrls,
    NumWords,
    Height,
    Width,
    FileSize,
    SimilarToFiles,
    SimilarToData,
    Limit,
    Filetype,
    Hash,
    Time(TimeKind),
    Duration,
    Framerate,
    NumFrames,
    FileService,
    NumFileRelationships,
    Ratio,
    RatioShape,
    NumPixels,
    Views(Canvases),
    ViewsIn,
    Viewtime(Canvases),
    ViewtimeIn,
    UrlRegex(bool),
    Url(bool),
    Domain(bool),
    UrlClass(bool),
    TagAsNumber,
    NumNotes,
    Note(bool),
    HasRating(bool),
    RatingStars,
    RatingLike,
    RatingCount,
    TagAdvanced(bool),
    RatingAdvanced,
}

/// The fixed canvas sets of `media views`, `preview views` and `all views`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Canvases {
    Media,
    Preview,
    MediaAndPreview,
}

use FileProperty as F;
use NumericProperty as N;

/// The reference's `SYSTEM_PREDICATES` table, in its order.
const NAMES: &[(&str, Name)] = &[
    ("everything", Name::Everything),
    ("inbox", Name::Inbox),
    ("archived?$", Name::Archive),
    ("has duration", Name::Has(N::Duration)),
    ("no duration", Name::HasNo(N::Duration)),
    ("has framerate", Name::Has(N::Framerate)),
    ("no framerate", Name::HasNo(N::Framerate)),
    ("has frames", Name::Has(N::NumFrames)),
    ("no frames", Name::HasNo(N::NumFrames)),
    ("has width", Name::Has(N::Width)),
    ("no width", Name::HasNo(N::Width)),
    ("has height", Name::Has(N::Height)),
    ("no height", Name::HasNo(N::Height)),
    ("has notes", Name::Has(N::NumNotes)),
    ("no notes", Name::HasNo(N::NumNotes)),
    ("has urls", Name::Has(N::NumUrls)),
    ("no urls", Name::HasNo(N::NumUrls)),
    ("has words", Name::Has(N::NumWords)),
    ("no words", Name::HasNo(N::NumWords)),
    (
        "(is the )?best quality( file)? of( its)?( duplicate)? group",
        Name::BestQuality(true),
    ),
    (
        "(((is )?not)|(isn't))( the)? best quality( file)? of( its)?( duplicate)? group",
        Name::BestQuality(false),
    ),
    ("has audio", Name::Property(F::Audio, true)),
    ("no audio", Name::Property(F::Audio, false)),
    (
        "has (transparency|alpha)",
        Name::Property(F::Transparency, true),
    ),
    (
        "no (transparency|alpha)",
        Name::Property(F::Transparency, false),
    ),
    ("has exif", Name::Property(F::Exif, true)),
    ("no exif", Name::Property(F::Exif, false)),
    ("has xmp", Name::Property(F::Xmp, true)),
    ("no xmp", Name::Property(F::Xmp, false)),
    ("has iptc", Name::Property(F::Iptc, true)),
    ("no iptc", Name::Property(F::Iptc, false)),
    (
        "has.*embedded.*metadata",
        Name::Property(F::HumanReadableEmbeddedMetadata, true),
    ),
    (
        "no.*embedded.*metadata",
        Name::Property(F::HumanReadableEmbeddedMetadata, false),
    ),
    (
        "has.*human-readable.*metadata",
        Name::Property(F::HumanReadableEmbeddedMetadata, true),
    ),
    (
        "no.*human-readable.*metadata",
        Name::Property(F::HumanReadableEmbeddedMetadata, false),
    ),
    (
        "has software/source metadata",
        Name::Property(F::SoftwareSourceMetadata, true),
    ),
    (
        "no software/source metadata",
        Name::Property(F::SoftwareSourceMetadata, false),
    ),
    ("has icc profile", Name::Property(F::IccProfile, true)),
    ("no icc profile", Name::Property(F::IccProfile, false)),
    (
        "has forced filetype",
        Name::Property(F::ForcedFiletype, true),
    ),
    (
        "no forced filetype",
        Name::Property(F::ForcedFiletype, false),
    ),
    ("has tags", Name::HasTags),
    ("untagged|no tags", Name::Untagged),
    ("num(ber)?( of)? tags", Name::NumTags),
    (
        r"num(ber)?( of)? (?=[^\s].* tags)",
        Name::NumTagsInNamespace,
    ),
    ("num(ber)?( of)? urls", Name::NumUrls),
    ("num(ber)?( of)? words", Name::NumWords),
    ("height", Name::Height),
    ("width", Name::Width),
    ("file ?size", Name::FileSize),
    ("similar to(?! data)( files)?", Name::SimilarToFiles),
    ("similar to data", Name::SimilarToData),
    ("limit", Name::Limit),
    ("file ?type", Name::Filetype),
    (r"hash( \(?(md5|sha1|sha512)\)?)?", Name::Hash),
    (
        "archived? (date|time)|(date|time) archived|archived.",
        Name::Time(TimeKind::Archived),
    ),
    (
        "modified (date|time)|(date|time) modified|modified",
        Name::Time(TimeKind::Modified),
    ),
    (
        "last view(ed)? (date|time)|(date|time) last viewed|last viewed",
        Name::Time(TimeKind::LastViewed),
    ),
    (
        "import(ed)? (date|time)|(date|time) imported|imported",
        Name::Time(TimeKind::Imported),
    ),
    ("duration", Name::Duration),
    ("framerate", Name::Framerate),
    ("num(ber)?( of)? frames", Name::NumFrames),
    ("file service", Name::FileService),
    (
        "num(ber)?( of)? file relationships",
        Name::NumFileRelationships,
    ),
    (r"ratio(?=.*\d)", Name::Ratio),
    (r"ratio(?!.*\d)", Name::RatioShape),
    ("num(ber)?( of)? pixels", Name::NumPixels),
    ("media views", Name::Views(Canvases::Media)),
    ("preview views", Name::Views(Canvases::Preview)),
    ("all views", Name::Views(Canvases::MediaAndPreview)),
    ("^views (in )?", Name::ViewsIn),
    ("media viewtime", Name::Viewtime(Canvases::Media)),
    ("preview viewtime", Name::Viewtime(Canvases::Preview)),
    ("all viewtime", Name::Viewtime(Canvases::MediaAndPreview)),
    ("^viewtime (in )?", Name::ViewtimeIn),
    ("has (a )?url matching regex", Name::UrlRegex(true)),
    (
        "(does not|doesn't) have (a )?url matching regex",
        Name::UrlRegex(false),
    ),
    ("has url:? (?=http)", Name::Url(true)),
    ("(does not|doesn't) have url:? (?=http)", Name::Url(false)),
    ("has (an? )?(url with )?domain", Name::Domain(true)),
    (
        "(does not|doesn't) have (an? )?(url with )?domain",
        Name::Domain(false),
    ),
    ("has (an? )?url with (url )?class", Name::UrlClass(true)),
    (
        "(does not|doesn't) have (an? )?url with (url )?class",
        Name::UrlClass(false),
    ),
    ("tag as number", Name::TagAsNumber),
    ("has notes?$", Name::Has(N::NumNotes)),
    (
        "((has )?no|does not have( a)?|doesn't have) notes?$",
        Name::HasNo(N::NumNotes),
    ),
    ("num(ber)?( of)? notes?", Name::NumNotes),
    ("(has (a )?)?note (with name|named)", Name::Note(true)),
    (
        "((has )?no|does not have( a)?|doesn't have( a)?) note (with name|named)",
        Name::Note(false),
    ),
    ("has( a)? (rating|count)( for)?", Name::HasRating(true)),
    (
        "((has )?no|does not have( a)?|doesn't have( a)?) (rating|count)( for)?",
        Name::HasRating(false),
    ),
    (r"(rating|count)( for)?(?=.+?\d+/\d+$)", Name::RatingStars),
    (
        "(rating|count)( for)?(?=.+?(like|dislike)$)",
        Name::RatingLike,
    ),
    (r"(rating|count)( for)?(?=.+?[^/]\d+$)", Name::RatingCount),
    ("has tag", Name::TagAdvanced(true)),
    ("does not have tag", Name::TagAdvanced(false)),
    ("(all|any|only).+rated", Name::RatingAdvanced),
];

static COMPILED: LazyLock<Vec<(fancy_regex::Regex, Name)>> = LazyLock::new(|| {
    NAMES
        .iter()
        .map(|&(pattern, name)| {
            // exactly the reference's transformation of its table keys
            let pattern = format!("^(?:{}:?)", pattern.replace(' ', "([_ ]+)"));
            (fancy_regex(&pattern), name)
        })
        .collect()
});

/// Find the predicate named at the start of `subtag` (the lowercased text
/// after `system:`), returning it and the length of the name.
pub(crate) fn match_name(subtag: &str) -> Option<(Name, usize)> {
    COMPILED.iter().find_map(|(re, name)| {
        // An `Err` means the backtracking limit was hit, which these small
        // patterns cannot reach on real input; treat it as no match.
        re.find(subtag).ok().flatten().map(|m| (*name, m.end()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_compiles() {
        assert_eq!(COMPILED.len(), NAMES.len());
    }

    #[test]
    fn order_decides_between_overlapping_names() {
        assert_eq!(
            match_name("has duration"),
            Some((Name::Has(N::Duration), 12))
        );
        assert_eq!(
            match_name("duration < 5s").map(|m| m.0),
            Some(Name::Duration)
        );
        assert_eq!(match_name("archived").map(|m| m.0), Some(Name::Archive));
        assert_eq!(
            match_name("archived < 5 days").map(|m| m.0),
            Some(Name::Time(TimeKind::Archived))
        );
        assert_eq!(
            match_name("number of character tags > 5").map(|m| m.0),
            Some(Name::NumTagsInNamespace)
        );
        assert_eq!(
            match_name("similar to data abcdef").map(|m| m.0),
            Some(Name::SimilarToData)
        );
        assert_eq!(
            match_name("no__ _audio"),
            Some((Name::Property(F::Audio, false), 11))
        );
        assert_eq!(match_name("inbox:").map(|m| m.1), Some(6));
        assert_eq!(match_name(" inbox"), None);
    }
}
