//! Predicates written out as the reference writes them: its
//! `Predicate.ToString` without a count, the text its search pages list.
//!
//! [`predicate_text`] is checked against the reference's own text for every
//! predicate in `oracle/fixtures/system_predicates.json`. Where our
//! predicates hold what the reference's cannot, the text extends the
//! reference's wording:
//!
//! - an age in minutes or seconds is written like its days and hours (the
//!   reference turns such an age into a date when it parses it);
//! - a service named in search text that no service has is written by that
//!   name (the reference resolves names when it parses them);
//! - `num file relationships ≠ n` is written "has not n" (the reference
//!   fails to write it).

use std::collections::BTreeSet;

use hydrus_core::numbers::{float_to_percentage, human_int};
use hydrus_core::search::filetype::FiletypeSet;
use hydrus_core::search::number::{Comparison, NumberOp, NumberTest, RatioOp, TagNumberOp};
use hydrus_core::search::predicate::{
    FileHashes, FileProperty, NamespaceFilter, NumericProperty, PixelUnit, Predicate, RatingLogic,
    RatingTest, Relationship, ServiceRef, ServiceSelection, SizeUnit, SystemPredicate,
    TagDisplayType, UrlRule, ViewCanvas, ViewCanvases, ViewingStat,
};
use hydrus_core::search::time::{CalendarDelta, CivilDateTime, RelativeOp, TimeKind, TimeTest};
use hydrus_core::sort::human_sort_key;
use hydrus_core::tag::{combine_tag, split_tag};
use hydrus_core::time::pretty_time_delta;
use hydrus_core::{CanvasType, ContentStatus, HashKind, Mime, ServiceKey, ServiceType};
use hydrus_store::services::{ServiceKind, ServiceRegistry};
use hydrus_store::settings::FileViewingStatistics;

/// A service, as writing a predicate that names it needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedService {
    pub key: ServiceKey,
    pub name: String,
    pub service_type: ServiceType,
    /// A numerical rating service's number of stars, and whether zero stars
    /// is a rating.
    pub stars: Option<(u64, bool)>,
}

/// What writing predicates needs to know about the client.
#[derive(Debug, Clone, Default)]
pub struct TextContext {
    pub services: Vec<NamedService>,
    /// The canvases `system:views` counts when it names none.
    pub default_canvases: Vec<ViewCanvas>,
    /// How to show tags to the user (`render_for_user`), for the GUI's
    /// lists; none writes them as the Client API does.
    pub presentation: Option<hydrus_core::tag_presentation::TagPresentation>,
}

impl TextContext {
    /// The services and viewing options of a store.
    pub fn from_store(services: &ServiceRegistry, viewing: &FileViewingStatistics) -> Self {
        Self {
            services: services
                .all()
                .map(|s| NamedService {
                    key: s.key.clone(),
                    name: s.name.clone(),
                    service_type: s.service_type(),
                    stars: match &s.kind {
                        ServiceKind::RatingNumerical(c) => {
                            Some((u64::from(c.num_stars), c.allow_zero))
                        }
                        _ => None,
                    },
                })
                .collect(),
            default_canvases: viewing
                .interesting_canvases
                .iter()
                .filter_map(|c| match c {
                    CanvasType::MediaViewer => Some(ViewCanvas::MediaViewer),
                    CanvasType::Preview => Some(ViewCanvas::Preview),
                    CanvasType::ClientApi => Some(ViewCanvas::ClientApi),
                    _ => None,
                })
                .collect(),
            presentation: None,
        }
    }

    /// `ClientTags.RenderTag`, for the user or not as this context says.
    fn render_tag(&self, tag: &str) -> String {
        match &self.presentation {
            Some(presentation) => presentation.render(tag),
            None => render_tag(tag).to_owned(),
        }
    }

    /// The service a predicate names: by key, or by name among the services
    /// of the types `allowed` accepts (an exact match first, then a
    /// case-insensitive one), as the executor finds it.
    fn service(
        &self,
        service: &ServiceRef,
        allowed: fn(ServiceType) -> bool,
    ) -> Option<&NamedService> {
        match service {
            ServiceRef::Key(key) => self.services.iter().find(|s| s.key == *key),
            ServiceRef::Name(name) => {
                let candidates = || self.services.iter().filter(|s| allowed(s.service_type));
                candidates()
                    .find(|s| s.name == *name)
                    .or_else(|| candidates().find(|s| s.name.to_lowercase() == name.to_lowercase()))
            }
        }
    }

    /// The name to write for a service: its own, or as named if no service
    /// has that name. `None` for a key no service has.
    fn service_name(
        &self,
        service: &ServiceRef,
        allowed: fn(ServiceType) -> bool,
    ) -> Option<String> {
        match (self.service(service, allowed), service) {
            (Some(s), _) => Some(s.name.clone()),
            (None, ServiceRef::Name(name)) => Some(name.clone()),
            (None, ServiceRef::Key(_)) => None,
        }
    }
}

/// A predicate as the reference writes it, e.g. `-blue eyes`,
/// `character:*anything*` or `system:width > 1,920`.
pub fn predicate_text(predicate: &Predicate, context: &TextContext) -> String {
    let minus = |inclusive: bool| if inclusive { "" } else { "-" };
    match predicate {
        Predicate::Tag { tag, inclusive } => {
            format!("{}{}", minus(*inclusive), context.render_tag(tag.as_str()))
        }
        Predicate::Namespace {
            namespace,
            inclusive,
        } => {
            let anything = combine_tag(namespace_for_user(namespace), "*anything*");
            format!("{}{}", minus(*inclusive), context.render_tag(&anything))
        }
        Predicate::Wildcard { pattern, inclusive } => {
            let pattern = pattern.as_str();
            let text = if pattern.starts_with("*:") {
                format!("{} (any namespace)", split_tag(pattern).1)
            } else {
                format!("{pattern} (wildcard search)")
            };
            format!("{}{text}", minus(*inclusive))
        }
        Predicate::Or(predicates) => {
            // the reference sorts an OR's members by their text when it
            // makes one (not as the user sees them)
            let plain = TextContext {
                presentation: None,
                ..context.clone()
            };
            let mut texts: Vec<(String, String)> = predicates
                .iter()
                .map(|p| {
                    let text = predicate_text(p, &plain);
                    let shown = if context.presentation.is_some() {
                        predicate_text(p, context)
                    } else {
                        text.clone()
                    };
                    (text, shown)
                })
                .collect();
            texts.sort_by_cached_key(|(t, _)| human_sort_key(t));
            texts
                .into_iter()
                .map(|(_, shown)| shown)
                .collect::<Vec<_>>()
                .join(" OR ")
        }
        Predicate::System(p) => context.render_tag(&format!("system:{}", system_text(p, context))),
    }
}

/// `ClientTags.RenderTag` as the reference writes predicates: an
/// unnamespaced tag loses the leading colon that marks a colon in it.
fn render_tag(tag: &str) -> &str {
    match split_tag(tag) {
        ("", subtag) => subtag,
        _ => tag,
    }
}

/// `ClientTags.RenderNamespaceForUser`.
fn namespace_for_user(namespace: &str) -> &str {
    match namespace {
        "" => "unnamespaced",
        ":" => "namespaced",
        other => other,
    }
}

/// `HydrusNumbers.ToHumanInt` of a signed number.
fn human_signed(n: i64) -> String {
    let digits = human_int(n.unsigned_abs());
    if n < 0 { format!("-{digits}") } else { digits }
}

#[allow(clippy::too_many_lines)]
fn system_text(p: &SystemPredicate, c: &TextContext) -> String {
    use SystemPredicate as S;
    match p {
        S::Everything => "everything".into(),
        S::Inbox => "inbox".into(),
        S::Archive => "archive".into(),
        S::Local => "local".into(),
        S::NotLocal => "not local".into(),
        S::Limit(n) => format!("limit is {}", human_int(*n)),
        S::Filetype {
            filetypes,
            inclusive,
        } => format!(
            "filetype {} {}",
            if *inclusive { "is" } else { "is not" },
            filetypes_text(filetypes)
        ),
        S::Hash { hashes, inclusive } => hash_text(hashes, *inclusive),
        S::SimilarToFiles {
            files,
            max_distance,
        } => format!(
            "similar to {} files with distance of {max_distance}",
            human_int(files.len() as u64)
        ),
        S::SimilarToData {
            pixel_hashes,
            perceptual_hashes,
            max_distance,
        } => {
            let mut components = Vec::new();
            if !pixel_hashes.is_empty() {
                components.push(format!("{} pixel", human_int(pixel_hashes.len() as u64)));
            }
            if !perceptual_hashes.is_empty() {
                components.push(format!(
                    "{} perceptual",
                    human_int(perceptual_hashes.len() as u64)
                ));
            }
            let hashes = format!("({} hashes)", components.join(", "));
            if perceptual_hashes.is_empty() {
                format!("similar to data {hashes}")
            } else {
                format!("similar to data {hashes} with distance of {max_distance}")
            }
        }
        S::FileProperty { property, has } => {
            let what = match property {
                FileProperty::Audio => "audio",
                FileProperty::Transparency => "transparency",
                FileProperty::Exif => "exif",
                FileProperty::Xmp => "xmp",
                FileProperty::Iptc => "iptc",
                FileProperty::HumanReadableEmbeddedMetadata => "human-readable metadata",
                FileProperty::SoftwareSourceMetadata => "software/source metadata",
                FileProperty::IccProfile => "icc profile",
                FileProperty::ForcedFiletype => "forced filetype",
            };
            format!("{} {what}", if *has { "has" } else { "no" })
        }
        S::Number { property, test } => number_property_text(*property, test),
        S::NumTags {
            namespace,
            op,
            count,
        } => num_tags_text(namespace, *op, *count),
        S::FileSize { op, size, unit } => {
            let unit = match unit {
                SizeUnit::Bytes => "B",
                SizeUnit::Kilobytes => "KB",
                SizeUnit::Megabytes => "MB",
                SizeUnit::Gigabytes => "GB",
                SizeUnit::Terabytes => "TB",
            };
            format!("filesize {} {}{unit}", op.symbol(), human_int(*size))
        }
        S::NumPixels { op, count, unit } => {
            let unit = match unit {
                PixelUnit::Pixels => "pixels",
                PixelUnit::Kilopixels => "kilopixels",
                PixelUnit::Megapixels => "megapixels",
            };
            format!(
                "number of pixels {} {} {unit}",
                op.symbol(),
                human_int(*count)
            )
        }
        S::Ratio { op, width, height } => {
            if (*width, *height) == (1, 1) {
                match op {
                    RatioOp::WiderThan => return "ratio is landscape".into(),
                    RatioOp::TallerThan => return "ratio is portrait".into(),
                    RatioOp::Equal => return "ratio is square".into(),
                    RatioOp::Approx | RatioOp::NotEqual => {}
                }
            }
            format!(
                "ratio {} {}:{}",
                op.symbol(),
                human_int(*width),
                human_int(*height)
            )
        }
        S::Time { kind, test } => time_text(*kind, test),
        S::FileService {
            service,
            status,
            is_in,
        } => {
            let allowed = |t: ServiceType| t.is_file_service() && t != ServiceType::CombinedFile;
            let Some(name) = c.service_name(service, allowed) else {
                return "unknown file service system predicate".into();
            };
            let status = match status {
                ContentStatus::Current => "currently in",
                ContentStatus::Deleted => "deleted from",
                ContentStatus::Pending => "pending to",
                ContentStatus::Petitioned => "petitioned from",
            };
            format!("{} {status} {name}", if *is_in { "is" } else { "is not" })
        }
        S::FileRelationshipCount {
            op,
            count,
            relationship,
        } => {
            let op = match op {
                Comparison::Approx => " about ",
                Comparison::Less => " less than ",
                Comparison::Greater => " more than ",
                Comparison::Equal => " ",
                Comparison::NotEqual => " not ",
            };
            let relationship = match relationship {
                Relationship::Duplicates => "duplicates",
                Relationship::Alternates => "alternates",
                Relationship::FalsePositives => "not related/false positive",
                Relationship::PotentialDuplicates => "potential duplicates",
            };
            format!(
                "num file relationships - has{op}{} {relationship}",
                human_int(*count)
            )
        }
        S::BestQualityOfGroup { is_best } => format!(
            "{} the best quality file of its duplicate group",
            if *is_best { "is" } else { "is not" }
        ),
        S::FileViewingStats {
            stat,
            canvases,
            op,
            value,
        } => {
            let canvases: Vec<ViewCanvas> = match canvases {
                ViewCanvases::Default => c.default_canvases.clone(),
                ViewCanvases::Specific(canvases) => canvases.iter().copied().collect(),
            };
            let domain = if canvases.is_empty() {
                "unknown".to_owned()
            } else {
                canvases
                    .iter()
                    .map(|canvas| match canvas {
                        ViewCanvas::MediaViewer => "media",
                        ViewCanvas::Preview => "preview",
                        ViewCanvas::ClientApi => "client api",
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let (stat, value) = match stat {
                ViewingStat::Views => ("views", human_int(*value)),
                ViewingStat::ViewTime => (
                    "viewtime",
                    pretty_time_delta(i64::try_from(*value).unwrap_or(i64::MAX), false),
                ),
            };
            format!("{stat} in {domain} {} {value}", op.symbol())
        }
        S::KnownUrl { rule, has } => {
            let (what, rule) = match rule {
                UrlRule::ExactMatch(url) => ("url", url),
                UrlRule::Domain(domain) => ("url with domain", domain),
                UrlRule::Regex(regex) => ("url matching regex", regex),
                UrlRule::UrlClass(class) => ("url with class", class),
            };
            format!(
                "{} {what} {rule}",
                if *has { "has" } else { "does not have" }
            )
        }
        S::TagAsNumber {
            namespace,
            op,
            value,
        } => {
            let namespace = match namespace {
                NamespaceFilter::Any => "any namespace",
                NamespaceFilter::Unnamespaced => "unnamespaced",
                NamespaceFilter::Namespace(namespace) => namespace,
            };
            let op = match op {
                TagNumberOp::Approx => "about",
                TagNumberOp::Less => "less than",
                TagNumberOp::Greater => "more than",
            };
            format!("tag as number: {namespace} {op} {}", human_signed(*value))
        }
        S::NoteName { name, has } => format!(
            "{} note with name \"{name}\"",
            if *has { "has" } else { "does not have" }
        ),
        S::Rating { service, test } => rating_text(c, service, test),
        S::RatingAdvanced {
            logic,
            services,
            rated,
        } => {
            let (primary, one_service) = selection_text(c, services);
            let mut text = match logic {
                RatingLogic::All if !one_service => "all ".to_owned(),
                RatingLogic::Any if !one_service => "any ".to_owned(),
                RatingLogic::Only { .. } => "only ".to_owned(),
                _ => String::new(),
            };
            text.push_str(&primary);
            if let RatingLogic::Only { amongst } = logic
                && !is_all_ratings(amongst)
            {
                text.push_str(&format!(" (amongst {})", selection_text(c, amongst).0));
            }
            text.push_str(if *rated { " rated" } else { " not rated" });
            text
        }
        S::TagAdvanced {
            service,
            display,
            statuses,
            tag,
            inclusive,
        } => {
            let mut text = if *inclusive {
                "has tag"
            } else {
                "does not have tag"
            }
            .to_owned();
            if let Some(service) = service {
                match c.service_name(service, ServiceType::is_tag_service) {
                    Some(name) => text.push_str(&format!(" in \"{name}\"")),
                    None => return "unknown tag service advanced tag predicate".into(),
                }
            }
            if *display == TagDisplayType::Storage {
                text.push_str(", ignoring siblings/parents");
            }
            let usual: BTreeSet<ContentStatus> = [ContentStatus::Current, ContentStatus::Pending]
                .into_iter()
                .collect();
            if *statuses != usual {
                let mut statuses: Vec<ContentStatus> = statuses.iter().copied().collect();
                statuses.sort_by_key(|s| s.code());
                let names: Vec<&str> = statuses.iter().map(|s| status_name(*s)).collect();
                if names.len() == 1 {
                    text.push_str(&format!(", with status {}", names[0]));
                } else {
                    text.push_str(&format!(", with status in {}", names.join(", ")));
                }
            }
            text.push_str(&format!(": \"{tag}\""));
            text
        }
    }
}

fn status_name(status: ContentStatus) -> &'static str {
    match status {
        ContentStatus::Current => "current",
        ContentStatus::Pending => "pending",
        ContentStatus::Deleted => "deleted",
        ContentStatus::Petitioned => "petitioned",
    }
}

/// `ConvertSummaryFiletypesToString`.
fn filetypes_text(filetypes: &FiletypeSet) -> String {
    let summary = filetypes.summary();
    let classes: BTreeSet<Mime> = Mime::general_classes().into_iter().collect();
    if *summary == classes || summary.contains(&Mime::GeneralFile) {
        return "all filetypes".into();
    }
    let mut mimes: Vec<Mime> = summary.iter().copied().collect();
    mimes.sort_by_key(|m| m.mimetype());
    mimes
        .iter()
        .map(|m| m.human_name())
        .collect::<Vec<_>>()
        .join(", ")
}

fn hash_text(hashes: &FileHashes, inclusive: bool) -> String {
    let mut is = if inclusive { "is" } else { "is not" }.to_owned();
    if hashes.len() > 1 {
        is.push_str(" in");
    }
    let base = match hashes.kind() {
        HashKind::Sha256 => "hash",
        HashKind::Md5 => "hash (md5)",
        HashKind::Sha1 => "hash (sha1)",
        HashKind::Sha512 => "hash (sha512)",
    };
    if hashes.len() == 1 {
        format!("{base} {is} {}", hashes.to_hex().join(", "))
    } else {
        format!("{base} {is} {} hashes", human_int(hashes.len() as u64))
    }
}

fn number_property_text(property: NumericProperty, test: &NumberTest) -> String {
    let (base, what) = match property {
        NumericProperty::Width => ("width", "width"),
        NumericProperty::Height => ("height", "height"),
        NumericProperty::Duration => ("duration", "duration"),
        NumericProperty::Framerate => ("framerate", "framerate"),
        NumericProperty::NumFrames => ("number of frames", "frames"),
        NumericProperty::NumNotes => ("number of notes", "notes"),
        NumericProperty::NumUrls => ("number of urls", "urls"),
        NumericProperty::NumWords => ("number of words", "words"),
    };
    let is_zero = matches!(
        (test.op, test.value),
        (NumberOp::Equal | NumberOp::LessOrEqual, 0) | (NumberOp::Less, 1)
    );
    let is_anything_but_zero = matches!(
        (test.op, test.value),
        (NumberOp::NotEqual | NumberOp::Greater, 0)
    );
    if is_zero {
        format!("no {what}")
    } else if is_anything_but_zero {
        format!("has {what}")
    } else {
        let render: fn(u64) -> String = match property {
            NumericProperty::Duration => duration_text,
            NumericProperty::Framerate => |n| format!("{}fps", human_int(n)),
            _ => human_int,
        };
        format!("{base} {}", number_test_text(test, render))
    }
}

/// `NumberTest.ToString`.
fn number_test_text(test: &NumberTest, render: fn(u64) -> String) -> String {
    let (op, extra) = match test.op {
        NumberOp::Less => ("<", String::new()),
        NumberOp::LessOrEqual => ("\u{2264}", String::new()),
        NumberOp::Greater => (">", String::new()),
        NumberOp::GreaterOrEqual => ("\u{2265}", String::new()),
        NumberOp::Equal => ("=", String::new()),
        NumberOp::NotEqual => ("\u{2260}", String::new()),
        NumberOp::ApproxPercent { percent } => (
            "\u{2248}",
            format!(" \u{b1}{}", float_to_percentage(f64::from(percent) / 100.0)),
        ),
        NumberOp::ApproxAbsolute { tolerance } => {
            ("\u{2248}", format!(" \u{b1}{}", render(tolerance)))
        }
    };
    format!("{op} {}{extra}", render(test.value))
}

/// `HydrusTime.MillisecondsDurationToPrettyTime` with `force_numbers`.
fn duration_text(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = ms % 3_600_000 / 60_000;
    let seconds = ms % 60_000 / 1000;
    let millis = ms % 1000;
    let plural = |n: u64, unit: &str| {
        if n == 1 {
            format!("1 {unit}")
        } else {
            format!("{n} {unit}s")
        }
    };
    if hours > 0 {
        format!("{} {}", plural(hours, "hour"), plural(minutes, "minute"))
    } else if minutes > 0 {
        format!(
            "{} {}",
            plural(minutes, "minute"),
            plural(seconds, "second")
        )
    } else if seconds > 0 {
        if millis == 0 {
            format!("{} seconds", human_int(seconds))
        } else {
            format!("{:.1} seconds", seconds as f64 + millis as f64 / 1000.0)
        }
    } else {
        plural(millis, "millisecond")
    }
}

fn num_tags_text(namespace: &NamespaceFilter, op: Comparison, count: u64) -> String {
    let any = *namespace == NamespaceFilter::Any;
    let namespace = namespace_for_user(namespace.as_reference());
    let is_anything_but_zero =
        matches!(op, Comparison::NotEqual | Comparison::Greater) && count == 0;
    let is_zero = (op == Comparison::Equal && count == 0) || (op == Comparison::Less && count == 1);
    match (is_anything_but_zero, is_zero, any) {
        (true, _, true) => "has tags".into(),
        (true, _, false) => format!("has {namespace} tags"),
        (_, true, true) => "untagged".into(),
        (_, true, false) => format!("no {namespace} tags"),
        (false, false, true) => format!("number of tags {} {}", op.symbol(), human_int(count)),
        (false, false, false) => format!(
            "number of {namespace} tags {} {}",
            op.symbol(),
            human_int(count)
        ),
    }
}

fn time_text(kind: TimeKind, test: &TimeTest) -> String {
    let base = match kind {
        TimeKind::Imported => "import time",
        TimeKind::LastViewed => "last viewed time",
        TimeKind::Modified => "modified time",
        TimeKind::Archived => "archived time",
    };
    match test {
        TimeTest::Relative { op, age } => {
            let op = match op {
                RelativeOp::Less => "since",
                RelativeOp::Greater => "before",
                RelativeOp::Approx => "around",
                RelativeOp::NotEqual => "unknown operator",
            };
            format!("{base}: {op} {} ago", age_text(age))
        }
        TimeTest::Absolute { op, at } => {
            let op_text = match op {
                Comparison::Less => "before ",
                Comparison::Greater => "since ",
                Comparison::Equal => "on the day of ",
                Comparison::Approx => "a month either side of ",
                Comparison::NotEqual => "unknown operator",
            };
            let with_time = *op != Comparison::Equal && (at.hour() > 0 || at.minute() > 0);
            format!("{base}: {op_text}{}", date_text(*at, with_time))
        }
    }
}

/// An age's two largest parts, e.g. `1 year 3 days`.
fn age_text(age: &CalendarDelta) -> String {
    let mut parts = Vec::new();
    for (quantity, unit) in [
        (age.years, "year"),
        (age.months, "month"),
        (age.days, "day"),
        (age.hours, "hour"),
        (age.minutes, "minute"),
        (age.seconds, "second"),
    ] {
        if quantity > 0 {
            let s = if quantity > 1 { "s" } else { "" };
            parts.push(format!("{} {unit}{s}", human_int(u64::from(quantity))));
        }
        if parts.len() == 2 {
            break;
        }
    }
    parts.join(" ")
}

/// `HydrusTime.DateTimeToPrettyTime` (the year unpadded, as C's `%Y`).
fn date_text(at: CivilDateTime, with_time: bool) -> String {
    let date = format!("{}-{:02}-{:02}", at.year(), at.month(), at.day());
    if with_time {
        format!("{date} {:02}:{:02}:00", at.hour(), at.minute())
    } else {
        date
    }
}

/// A rating predicate: our test is first put in the form the reference
/// stores (`rating_service_pred_generator`), then written as it writes that.
#[allow(clippy::float_cmp)] // as the reference compares its stored values
fn rating_text(c: &TextContext, service: &ServiceRef, test: &RatingTest) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Value {
        Rated,
        NotRated,
        Float(f64),
        Int(u64),
    }
    let Some(service) = c.service(service, ServiceType::is_rating_service) else {
        return "missing rating service system predicate".into();
    };
    let (mut op, mut value) = match *test {
        RatingTest::Rated => ("=", Value::Rated),
        RatingTest::NotRated => ("=", Value::NotRated),
        RatingTest::Liked => ("=", Value::Float(1.0)),
        RatingTest::Disliked => ("=", Value::Float(0.0)),
        RatingTest::Stars { op, stars, .. } => (op.symbol(), Value::Int(stars)),
        RatingTest::Count { op, value } => (op.symbol(), Value::Int(value)),
    };
    let name = &service.name;
    let pretty_op = match op {
        "<" => "less than",
        ">" => "more than",
        "=" => "is",
        "\u{2248}" => "is about",
        "\u{2264}" => "less than or equal to",
        _ => "more than or equal to",
    };
    match service.service_type {
        ServiceType::LocalRatingIncDec => {
            match value {
                Value::Rated => (op, value) = (">", Value::Int(0)),
                Value::NotRated => (op, value) = ("=", Value::Int(0)),
                _ => {}
            }
            let is = |n: f64| match value {
                Value::Int(v) => v as f64 == n,
                Value::Float(v) => v == n,
                _ => false,
            };
            if op == ">" && is(0.0) {
                format!("has count for {name}")
            } else if (op == "<" && is(1.0)) || (op == "=" && is(0.0)) {
                format!("no count for {name}")
            } else {
                let value = match value {
                    Value::Int(v) => human_int(v),
                    _ => "unknown".into(),
                };
                format!("count for {name} {pretty_op} {value}")
            }
        }
        service_type => {
            if let (Some((num_stars, allow_zero)), Value::Int(stars)) = (service.stars, value) {
                value = Value::Float(stars_to_rating(num_stars, allow_zero, stars));
            }
            let value = match (value, service_type, service.stars) {
                (Value::Rated, ..) => return format!("has rating for {name}"),
                (Value::NotRated, ..) => return format!("no rating for {name}"),
                (
                    Value::Float(r),
                    ServiceType::LocalRatingNumerical,
                    Some((num_stars, allow_zero)),
                ) => {
                    let stars = rating_to_stars(num_stars, allow_zero, r).min(num_stars);
                    format!("{}/{}", human_int(stars), human_int(num_stars))
                }
                (Value::Float(r), ServiceType::LocalRatingLike, _) => {
                    if r < 0.5 { "dislike" } else { "like" }.to_owned()
                }
                (Value::Int(r), ServiceType::LocalRatingLike, _) => {
                    if r == 0 { "dislike" } else { "like" }.to_owned()
                }
                (_, ServiceType::LocalRatingNumerical, _) => "unknown".to_owned(),
                _ => return "unknown rating service system predicate".into(),
            };
            format!("rating for {name} {pretty_op} {value}")
        }
    }
}

/// `ClientRatings.ConvertStarsToRating`.
fn stars_to_rating(num_stars: u64, allow_zero: bool, stars: u64) -> f64 {
    let stars = stars.min(num_stars);
    if allow_zero {
        stars as f64 / num_stars as f64
    } else {
        (stars.max(1) - 1) as f64 / (num_stars.max(2) - 1) as f64
    }
}

/// `ClientRatings.ConvertRatingToStars`.
fn rating_to_stars(num_stars: u64, allow_zero: bool, rating: f64) -> u64 {
    let stars = if allow_zero {
        (rating * num_stars as f64).round_ties_even()
    } else {
        (rating * num_stars.saturating_sub(1) as f64).round_ties_even() + 1.0
    };
    stars.max(0.0) as u64
}

fn is_all_ratings(selection: &ServiceSelection) -> bool {
    *selection == ServiceSelection::all_local_ratings()
}

/// `ServiceSpecifier.ToString`, and whether it names exactly one service.
fn selection_text(c: &TextContext, selection: &ServiceSelection) -> (String, bool) {
    let named = |names: Vec<String>| match names.as_slice() {
        [] => ("(no services selected)".to_owned(), false),
        [name] => (name.clone(), true),
        _ => (format!("{} services", human_int(names.len() as u64)), false),
    };
    match selection {
        ServiceSelection::Types(types) if types.is_empty() => {
            ("(no services selected)".into(), false)
        }
        ServiceSelection::Types(_) if is_all_ratings(selection) => ("ratings".into(), false),
        ServiceSelection::Types(types) => {
            let mut names: Vec<&str> = types.iter().map(|t| t.short_name()).collect();
            names.sort_unstable();
            (names.join(", "), false)
        }
        ServiceSelection::Keys(keys) => named(
            keys.iter()
                .map(|key| {
                    c.service(&ServiceRef::Key(key.clone()), |_| true)
                        .map_or_else(|| "unknown service".to_owned(), |s| s.name.clone())
                })
                .collect(),
        ),
        ServiceSelection::Names(names) => {
            // as the reference resolves them: one entry per service
            let mut seen: Vec<String> = Vec::new();
            let mut keys: BTreeSet<&ServiceKey> = BTreeSet::new();
            for name in names {
                let service = ServiceRef::Name(name.clone());
                match c.service(&service, ServiceType::is_local_rating_service) {
                    Some(s) if keys.insert(&s.key) => seen.push(s.name.clone()),
                    Some(_) => {}
                    None => seen.push(name.clone()),
                }
            }
            named(seen)
        }
    }
}
