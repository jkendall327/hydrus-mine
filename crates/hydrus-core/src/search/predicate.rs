//! The predicate model: everything a file search can contain.
//!
//! [`Predicate`] is one term of a search. Tag-level terms (tags, namespaces,
//! wildcards, OR groups) sit directly in it; every `system:` predicate is a
//! variant of [`SystemPredicate`], each carrying fully typed values.
//!
//! Some values name things that only exist at run time: services named in
//! search text, URL classes, and the user's default set of viewing canvases.
//! These are kept as references ([`ServiceRef`], [`UrlRule::UrlClass`],
//! [`ViewCanvases::Default`]) for the executor to resolve against the live
//! client, so parsing needs no access to client state.

use std::collections::BTreeSet;
use std::fmt;

use crate::{
    ContentStatus, DuplicateType, HashKind, Md5, PerceptualHash, ServiceKey, ServiceType, Sha1,
    Sha256, Sha512, Tag,
};

use crate::search::filetype::FiletypeSet;
use crate::search::number::{Comparison, NumberTest, RatingOp, RatioOp, TagNumberOp};
use crate::search::time::{TimeKind, TimeTest};

/// One term of a file search. A search matches the files that satisfy all of
/// its predicates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Predicate {
    /// Files that have the tag (or, if not `inclusive`, do not).
    Tag { tag: Tag, inclusive: bool },
    /// Files that have any tag in the namespace. The empty namespace means
    /// any unnamespaced tag.
    Namespace { namespace: String, inclusive: bool },
    /// Files that have a tag matching a `*` wildcard pattern.
    Wildcard { pattern: Wildcard, inclusive: bool },
    /// Files that match at least one of the predicates.
    Or(Vec<Predicate>),
    /// A `system:` predicate.
    System(SystemPredicate),
}

/// A tag pattern in which `*` matches any run of characters, e.g.
/// `character:sam*` or `*blue*`. It is a cleaned tag, so it is lowercase.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Wildcard(String);

impl Wildcard {
    /// Wrap a cleaned tag that contains `*`.
    pub fn from_clean(pattern: impl Into<String>) -> Self {
        Self(pattern.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The namespace part (before the first colon), or `""`.
    pub fn namespace(&self) -> &str {
        crate::tag::split_tag(&self.0).0
    }

    /// The subtag part.
    pub fn subtag(&self) -> &str {
        crate::tag::split_tag(&self.0).1
    }
}

impl fmt::Display for Wildcard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A `system:` predicate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SystemPredicate {
    /// Every file in the search domain.
    Everything,
    /// Files in the inbox.
    Inbox,
    /// Archived files.
    Archive,
    /// Files stored on this client (only meaningful when searching a remote
    /// file domain). The text parser has no syntax for this.
    Local,
    /// Files not stored on this client. The text parser has no syntax for this.
    NotLocal,
    /// At most this many results.
    Limit(u64),
    /// Files of (or, if not `inclusive`, not of) these types.
    Filetype {
        filetypes: FiletypeSet,
        inclusive: bool,
    },
    /// Files with (or, if not `inclusive`, without) one of these hashes.
    Hash { hashes: FileHashes, inclusive: bool },
    /// Files perceptually similar to the given files.
    SimilarToFiles {
        files: BTreeSet<Sha256>,
        max_distance: u64,
    },
    /// Files whose pixels match exactly or whose perceptual hash is similar.
    SimilarToData {
        pixel_hashes: BTreeSet<Sha256>,
        perceptual_hashes: BTreeSet<PerceptualHash>,
        max_distance: u64,
    },
    /// Files with (or without) some metadata, e.g. `system:has audio`.
    FileProperty { property: FileProperty, has: bool },
    /// A number test on a file property, e.g. `system:width > 1920`.
    Number {
        property: NumericProperty,
        test: NumberTest,
    },
    /// The number of tags (in a namespace) a file has.
    NumTags {
        namespace: NamespaceFilter,
        op: Comparison,
        count: u64,
    },
    /// File size; the size in bytes is `size * unit.bytes()`.
    FileSize {
        op: Comparison,
        size: u64,
        unit: SizeUnit,
    },
    /// Number of pixels; the count is `count * unit.pixels()`.
    NumPixels {
        op: Comparison,
        count: u64,
        unit: PixelUnit,
    },
    /// The width:height ratio.
    Ratio {
        op: RatioOp,
        width: u64,
        height: u64,
    },
    /// A timestamp, relative to now or to a date.
    Time { kind: TimeKind, test: TimeTest },
    /// Whether files are in (or pending to, deleted from, petitioned from) a
    /// file domain.
    FileService {
        service: ServiceRef,
        status: ContentStatus,
        is_in: bool,
    },
    /// The number of duplicate-system relationships of a kind.
    FileRelationshipCount {
        op: Comparison,
        count: u64,
        relationship: Relationship,
    },
    /// Whether the file is the best quality file (king) of its duplicate group.
    BestQualityOfGroup { is_best: bool },
    /// File viewing statistics: view count, or total view time in seconds.
    FileViewingStats {
        stat: ViewingStat,
        canvases: ViewCanvases,
        op: Comparison,
        value: u64,
    },
    /// Whether the file has a URL matching a rule.
    KnownUrl { rule: UrlRule, has: bool },
    /// Tags of the form `namespace:123` compared as numbers.
    TagAsNumber {
        namespace: NamespaceFilter,
        op: TagNumberOp,
        value: i64,
    },
    /// Whether the file has a note with this name (compared exactly).
    NoteName { name: String, has: bool },
    /// A rating on one rating service.
    Rating {
        service: ServiceRef,
        test: RatingTest,
    },
    /// Whether all/any/only of a set of rating services have rated the file.
    RatingAdvanced {
        logic: RatingLogic,
        services: ServiceSelection,
        rated: bool,
    },
    /// A tag search with explicit tag domain, display type and statuses.
    TagAdvanced {
        /// `None` means the search's own tag domain.
        service: Option<ServiceRef>,
        display: TagDisplayType,
        statuses: BTreeSet<ContentStatus>,
        tag: Tag,
        inclusive: bool,
    },
}

/// A set of hashes of one kind, for `system:hash`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileHashes {
    Sha256(BTreeSet<Sha256>),
    Md5(BTreeSet<Md5>),
    Sha1(BTreeSet<Sha1>),
    Sha512(BTreeSet<Sha512>),
}

impl FileHashes {
    pub fn kind(&self) -> HashKind {
        match self {
            FileHashes::Sha256(_) => HashKind::Sha256,
            FileHashes::Md5(_) => HashKind::Md5,
            FileHashes::Sha1(_) => HashKind::Sha1,
            FileHashes::Sha512(_) => HashKind::Sha512,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            FileHashes::Sha256(h) => h.len(),
            FileHashes::Md5(h) => h.len(),
            FileHashes::Sha1(h) => h.len(),
            FileHashes::Sha512(h) => h.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The hashes as lowercase hex, sorted.
    pub fn to_hex(&self) -> Vec<String> {
        match self {
            FileHashes::Sha256(h) => h.iter().map(Sha256::to_hex).collect(),
            FileHashes::Md5(h) => h.iter().map(Md5::to_hex).collect(),
            FileHashes::Sha1(h) => h.iter().map(Sha1::to_hex).collect(),
            FileHashes::Sha512(h) => h.iter().map(Sha512::to_hex).collect(),
        }
    }
}

/// Yes/no file properties.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum FileProperty {
    Audio,
    Transparency,
    Exif,
    Xmp,
    Iptc,
    HumanReadableEmbeddedMetadata,
    SoftwareSourceMetadata,
    IccProfile,
    ForcedFiletype,
}

/// Numeric file properties tested with a [`NumberTest`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum NumericProperty {
    /// Pixels.
    Width,
    /// Pixels.
    Height,
    /// Milliseconds.
    Duration,
    /// Frames per second.
    Framerate,
    NumFrames,
    NumNotes,
    NumUrls,
    NumWords,
}

/// Which tags a tag-counting predicate looks at.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum NamespaceFilter {
    /// Tags in any namespace, and unnamespaced tags.
    Any,
    /// Unnamespaced tags only.
    Unnamespaced,
    /// Tags in this namespace.
    Namespace(String),
}

impl NamespaceFilter {
    /// Interpret a namespace as the reference stores it: `*` is any
    /// namespace and the empty string is unnamespaced.
    pub fn from_reference(namespace: &str) -> Self {
        match namespace {
            "*" => NamespaceFilter::Any,
            "" => NamespaceFilter::Unnamespaced,
            other => NamespaceFilter::Namespace(other.to_owned()),
        }
    }

    /// The namespace string the reference stores.
    pub fn as_reference(&self) -> &str {
        match self {
            NamespaceFilter::Any => "*",
            NamespaceFilter::Unnamespaced => "",
            NamespaceFilter::Namespace(ns) => ns,
        }
    }
}

/// The unit a file size was given in (binary multiples, as in the reference).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum SizeUnit {
    Bytes,
    Kilobytes,
    Megabytes,
    Gigabytes,
    /// Offered by the reference's editor (which can't write it), not its
    /// parser.
    Terabytes,
}

impl SizeUnit {
    pub const fn bytes(self) -> u64 {
        match self {
            SizeUnit::Bytes => 1,
            SizeUnit::Kilobytes => 1024,
            SizeUnit::Megabytes => 1024 * 1024,
            SizeUnit::Gigabytes => 1024 * 1024 * 1024,
            SizeUnit::Terabytes => 1024 * 1024 * 1024 * 1024,
        }
    }
}

/// The unit a pixel count was given in (decimal multiples, as in the reference).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum PixelUnit {
    Pixels,
    Kilopixels,
    Megapixels,
}

impl PixelUnit {
    pub const fn pixels(self) -> u64 {
        match self {
            PixelUnit::Pixels => 1,
            PixelUnit::Kilopixels => 1000,
            PixelUnit::Megapixels => 1_000_000,
        }
    }
}

/// A service named in search text, or identified by key.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum ServiceRef {
    /// A service by name, as typed but lowercased. Resolve it among the
    /// services of the types the predicate allows: an exact name match
    /// first, then a case-insensitive one.
    Name(String),
    /// A service by key.
    Key(ServiceKey),
}

/// A kind of duplicate-system relationship.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Relationship {
    Duplicates,
    Alternates,
    FalsePositives,
    PotentialDuplicates,
}

impl Relationship {
    /// The duplicate type the reference stores for this relationship.
    pub const fn duplicate_type(self) -> DuplicateType {
        match self {
            Relationship::Duplicates => DuplicateType::Member,
            Relationship::Alternates => DuplicateType::Alternate,
            Relationship::FalsePositives => DuplicateType::FalsePositive,
            Relationship::PotentialDuplicates => DuplicateType::Potential,
        }
    }
}

/// What a file viewing statistics predicate measures.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum ViewingStat {
    /// Number of views.
    Views,
    /// Total view time, in seconds.
    ViewTime,
}

/// Where views are counted.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum ViewCanvas {
    MediaViewer,
    Preview,
    ClientApi,
}

impl ViewCanvas {
    pub const fn canvas_type(self) -> crate::CanvasType {
        match self {
            ViewCanvas::MediaViewer => crate::CanvasType::MediaViewer,
            ViewCanvas::Preview => crate::CanvasType::Preview,
            ViewCanvas::ClientApi => crate::CanvasType::ClientApi,
        }
    }
}

/// The canvases a viewing statistic sums over.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ViewCanvases {
    /// The user's "interesting" canvases option, read when the search runs.
    Default,
    /// These canvases.
    Specific(BTreeSet<ViewCanvas>),
}

/// A rule a file's URLs are matched against.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum UrlRule {
    /// A URL, compared exactly (case preserved).
    ExactMatch(String),
    /// A domain, lowercase.
    Domain(String),
    /// A regular expression (case preserved) matched against each URL.
    Regex(String),
    /// A URL class by name (lowercase; matched case-insensitively).
    UrlClass(String),
}

/// The test a rating predicate applies, in the form it was written.
///
/// What it means depends on the kind of service it names, which is only
/// known when the search runs. The reference interprets each form as noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RatingTest {
    /// Has any rating. For inc/dec services: a count above zero.
    Rated,
    /// Has no rating. For inc/dec services: a count of zero.
    NotRated,
    /// `= like`: a rating of 1.0, the top of the range (a count of 1 on an
    /// inc/dec service).
    Liked,
    /// `= dislike`: a rating of 0.0, the bottom of the range.
    Disliked,
    /// A star rating written `stars/out_of`, with `stars <= out_of`. On a
    /// numerical service `stars` is read against the service's *own* number
    /// of stars (`out_of` is only checked); on other services `stars` is
    /// compared as a plain number.
    Stars {
        op: RatingOp,
        stars: u64,
        out_of: u64,
    },
    /// A plain number: a count on an inc/dec service, or a number of stars
    /// on a numerical service.
    Count { op: RatingOp, value: u64 },
}

/// How `system:all|any|only ... rated` combines its services.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum RatingLogic {
    All,
    Any,
    /// The given services are (not) rated and every other service in
    /// `amongst` is the opposite.
    Only {
        amongst: ServiceSelection,
    },
}

/// A set of rating services, by type or by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ServiceSelection {
    Types(BTreeSet<ServiceType>),
    /// Service names, lowercased; resolved among local rating services.
    Names(BTreeSet<String>),
    /// Services by key, as stored searches name them.
    Keys(BTreeSet<ServiceKey>),
}

impl ServiceSelection {
    /// Every local rating service.
    pub fn all_local_ratings() -> Self {
        ServiceSelection::Types(
            [
                ServiceType::LocalRatingLike,
                ServiceType::LocalRatingNumerical,
                ServiceType::LocalRatingIncDec,
            ]
            .into_iter()
            .collect(),
        )
    }
}

/// Whether a tag search sees tags as stored or as displayed.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum TagDisplayType {
    /// Tags as stored, ignoring siblings and parents.
    Storage,
    /// Tags with siblings and parents applied.
    Display,
}
