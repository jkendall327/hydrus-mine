//! Testing one file's metadata against predicates in memory, as the
//! reference's `Predicate.TestMediaResult` and
//! `ExtractComparableValueFromMediaResult` do. Duplicates auto-resolution
//! uses these on the two files of a pair: "A is a jpeg", "A is bigger than
//! B".
//!
//! Only some predicates can be tested this way ([`can_test`]): the rest
//! need the database. The semantics are the reference's in-memory ones,
//! which differ from a database search in places: a time test matches no
//! file without that time, a `≠` time test matches nothing, and number tests
//! treat a missing value as the reference's lambdas do.

use std::collections::{BTreeMap, BTreeSet};

use hydrus_core::search::comparable::Comparable;
use hydrus_core::search::number::{Comparison, NumberOp, NumberTest, RatioOp};
use hydrus_core::search::predicate::{
    FileProperty, NamespaceFilter, NumericProperty, Predicate, SystemPredicate, TagDisplayType,
    UrlRule,
};
use hydrus_core::search::time::TimeKind;
use hydrus_core::tag::split_tag;
use hydrus_core::{ContentStatus, Mime, ServiceKey};

use crate::exec::Clock;
use crate::exec::time::range_opt;

/// What an in-memory test can know about a file (the reference's
/// `MediaResult`).
#[derive(Debug, Clone, Default)]
pub struct FileFacts {
    pub mime: Option<Mime>,
    pub size: Option<u64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub duration_ms: Option<u64>,
    pub num_frames: Option<u64>,
    pub has_audio: bool,
    pub has_transparency: bool,
    pub has_exif: bool,
    pub has_xmp: bool,
    pub has_iptc: bool,
    pub has_human_readable_embedded_metadata: bool,
    pub has_software_source: bool,
    pub has_icc_profile: bool,
    /// The filetype was forced (the file has an original filetype).
    pub filetype_forced: bool,
    pub inbox: bool,
    pub urls: Vec<String>,
    /// The URL classes (by case-folded name) that match any of `urls`.
    pub url_classes: BTreeSet<String>,
    /// Current and pending tags, as displayed, in all known tags.
    pub tags: BTreeSet<String>,
    /// Tags by service (`None`: all known tags), display type and status,
    /// for `system:has tag` with those spelled out.
    pub service_tags:
        BTreeMap<(Option<ServiceKey>, TagDisplayType, ContentStatus), BTreeSet<String>>,
    /// Import time in hydrus local file storage.
    pub imported_ms: Option<i64>,
    /// The aggregate modified time (the earliest of the file's and its
    /// domains').
    pub modified_ms: Option<i64>,
    /// Last viewed in the media viewer.
    pub last_viewed_ms: Option<i64>,
    pub archived_ms: Option<i64>,
}

impl FileFacts {
    /// `FileInfoManager.GetFramerate`.
    pub fn framerate(&self) -> Option<f64> {
        match (self.duration_ms, self.num_frames) {
            (Some(d), Some(n)) if d > 0 && n > 0 => Some(n as f64 / (d as f64 / 1000.0)),
            _ => None,
        }
    }
}

/// Whether a predicate can be tested in memory
/// (`PREDICATE_TYPES_WE_CAN_TEST_ON_MEDIA_RESULTS`).
pub fn can_test(predicate: &Predicate) -> bool {
    use SystemPredicate as S;
    match predicate {
        Predicate::Tag { .. } | Predicate::Namespace { .. } | Predicate::Wildcard { .. } => true,
        Predicate::Or(members) => members.iter().all(can_test),
        Predicate::System(system) => match system {
            S::Inbox
            | S::Archive
            | S::Filetype { .. }
            | S::NumPixels { .. }
            | S::Ratio { .. }
            | S::NumTags { .. }
            | S::KnownUrl { .. }
            | S::FileProperty { .. }
            | S::TagAdvanced { .. }
            | S::Time { .. } => true,
            S::Number { property, .. } => matches!(
                property,
                NumericProperty::Width
                    | NumericProperty::Height
                    | NumericProperty::Duration
                    | NumericProperty::Framerate
                    | NumericProperty::NumFrames
                    | NumericProperty::NumUrls
            ),
            _ => false,
        },
    }
}

/// `NumberTest.GetLambda(replacement_test_value)(x)`: `test`'s operator and
/// range against `value` (the test's own value, or a replacement). Equality
/// is exact, as in Python.
#[allow(clippy::float_cmp)]
pub fn number_test(op: NumberOp, value: f64, x: Option<f64>) -> bool {
    let is_none_or = |f: &dyn Fn(f64) -> bool| x.is_none_or(f);
    let is_some_and = |f: &dyn Fn(f64) -> bool| x.is_some_and(f);
    match op {
        NumberOp::Less if value > 0.0 => is_none_or(&|x| x < value),
        NumberOp::Less => is_some_and(&|x| x < value),
        NumberOp::LessOrEqual if value >= 0.0 => is_none_or(&|x| x <= value),
        NumberOp::LessOrEqual => is_some_and(&|x| x <= value),
        NumberOp::Greater if value < 0.0 => is_none_or(&|x| x > value),
        NumberOp::Greater => is_some_and(&|x| x > value),
        NumberOp::GreaterOrEqual if value <= 0.0 => is_none_or(&|x| x >= value),
        NumberOp::GreaterOrEqual => is_some_and(&|x| x >= value),
        NumberOp::Equal if value == 0.0 => is_none_or(&|x| x == value),
        NumberOp::Equal => is_some_and(&|x| x == value),
        NumberOp::NotEqual if value == 0.0 => is_some_and(&|x| x != value),
        NumberOp::NotEqual => is_none_or(&|x| x != value),
        NumberOp::ApproxPercent { percent } => {
            let extra = f64::from(percent) / 100.0;
            approx(value * (1.0 - extra), value * (1.0 + extra), x)
        }
        NumberOp::ApproxAbsolute { tolerance } => {
            let tolerance = tolerance as f64;
            approx(value - tolerance, value + tolerance, x)
        }
    }
}

fn approx(lower: f64, upper: f64, x: Option<f64>) -> bool {
    if lower <= 0.0 {
        x.is_none_or(|x| x <= upper)
    } else {
        x.is_some_and(|x| lower <= x && x <= upper)
    }
}

/// `NumberTest.STATICCreateFromCharacters` for the legacy comparison
/// operators ("about" is ±15%).
fn comparison_op(op: Comparison) -> NumberOp {
    match op {
        Comparison::Less => NumberOp::Less,
        Comparison::Greater => NumberOp::Greater,
        Comparison::Equal => NumberOp::Equal,
        Comparison::NotEqual => NumberOp::NotEqual,
        Comparison::Approx => NumberOp::ApproxPercent { percent: 15 },
    }
}

fn test_number(test: &NumberTest, x: Option<f64>) -> bool {
    number_test(test.op, test.value as f64, x)
}

fn as_f64(n: Option<u64>) -> Option<f64> {
    n.map(|n| n as f64)
}

/// A `*` wildcard matched against the whole text.
fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, rest) = parts.split_first().expect("split gives one part");
    let Some(mut remaining) = text.strip_prefix(first) else {
        return false;
    };
    let Some((last, middle)) = rest.split_last() else {
        return remaining.is_empty();
    };
    for part in middle {
        match remaining.find(part) {
            Some(i) => remaining = &remaining[i + part.len()..],
            None => return false,
        }
    }
    remaining.len() >= last.len() && remaining.ends_with(last)
}

/// The tags in a namespace; a namespace with `*` is a wildcard.
fn tags_in_namespace<'a>(
    tags: &'a BTreeSet<String>,
    namespace: &'a str,
) -> Box<dyn Iterator<Item = &'a String> + 'a> {
    if namespace.contains('*') {
        let pattern = format!("{namespace}:*");
        Box::new(tags.iter().filter(move |t| wildcard_matches(&pattern, t)))
    } else {
        Box::new(tags.iter().filter(move |t| split_tag(t).0 == namespace))
    }
}

/// `Predicate.TestMediaResult`. Predicates that cannot be tested in memory
/// ([`can_test`]) do not match.
pub fn test(predicate: &Predicate, facts: &FileFacts, clock: &Clock) -> bool {
    match predicate {
        Predicate::Tag { tag, inclusive } => facts.tags.contains(tag.as_str()) == *inclusive,
        Predicate::Namespace {
            namespace,
            inclusive,
        } => tags_in_namespace(&facts.tags, namespace).next().is_some() == *inclusive,
        Predicate::Wildcard { pattern, inclusive } => {
            facts
                .tags
                .iter()
                .any(|t| wildcard_matches(pattern.as_str(), t))
                == *inclusive
        }
        Predicate::Or(members) => members.iter().any(|p| test(p, facts, clock)),
        Predicate::System(system) => test_system(system, facts, clock),
    }
}

fn test_system(system: &SystemPredicate, facts: &FileFacts, clock: &Clock) -> bool {
    use SystemPredicate as S;
    match system {
        S::Inbox => facts.inbox,
        S::Archive => !facts.inbox,
        S::Filetype {
            filetypes,
            inclusive,
        } => {
            facts
                .mime
                .is_some_and(|m| filetypes.specific_mimes().contains(&m))
                == *inclusive
        }
        S::Number { property, test } => {
            let x = match property {
                NumericProperty::Width => as_f64(facts.width),
                NumericProperty::Height => as_f64(facts.height),
                NumericProperty::Duration => as_f64(facts.duration_ms),
                NumericProperty::Framerate => facts.framerate(),
                NumericProperty::NumFrames => as_f64(facts.num_frames),
                NumericProperty::NumUrls => Some(facts.urls.len() as f64),
                NumericProperty::NumNotes | NumericProperty::NumWords => return false,
            };
            test_number(test, x)
        }
        S::NumPixels { op, count, unit } => number_test(
            comparison_op(*op),
            (count * unit.pixels()) as f64,
            extract(Comparable::NumPixels, facts),
        ),
        S::Ratio { op, width, height } => {
            let op = match op {
                RatioOp::TallerThan => NumberOp::Less,
                RatioOp::WiderThan => NumberOp::Greater,
                RatioOp::Equal => NumberOp::Equal,
                RatioOp::Approx => NumberOp::ApproxPercent { percent: 15 },
            };
            number_test(
                op,
                *width as f64 / *height as f64,
                extract(Comparable::Ratio, facts),
            )
        }
        S::NumTags {
            namespace,
            op,
            count,
        } => {
            let n = match namespace {
                NamespaceFilter::Any => facts.tags.len(),
                NamespaceFilter::Unnamespaced => tags_in_namespace(&facts.tags, "").count(),
                NamespaceFilter::Namespace(ns) => tags_in_namespace(&facts.tags, ns).count(),
            };
            number_test(comparison_op(*op), *count as f64, Some(n as f64))
        }
        S::KnownUrl { rule, has } => {
            let matches = match rule {
                UrlRule::UrlClass(name) => facts
                    .url_classes
                    .contains(&hydrus_core::casefold::casefold(name)),
                UrlRule::Regex(pattern) => match fancy_regex::Regex::new(pattern) {
                    Ok(re) => facts.urls.iter().any(|u| re.is_match(u).unwrap_or(false)),
                    Err(_) => false,
                },
                UrlRule::ExactMatch(url) => facts.urls.iter().any(|u| u == url),
                UrlRule::Domain(domain) => {
                    let domain = hydrus_core::url::psl::remove_www(domain);
                    facts.urls.iter().any(|u| {
                        hydrus_core::url::functions::url_domain(u)
                            .is_ok_and(|d| d.ends_with(domain))
                    })
                }
            };
            matches == *has
        }
        S::FileProperty { property, has } => {
            let value = match property {
                FileProperty::Audio => facts.has_audio,
                FileProperty::Transparency => facts.has_transparency,
                FileProperty::Exif => facts.has_exif,
                FileProperty::Xmp => facts.has_xmp,
                FileProperty::Iptc => facts.has_iptc,
                FileProperty::HumanReadableEmbeddedMetadata => {
                    facts.has_human_readable_embedded_metadata
                }
                FileProperty::SoftwareSourceMetadata => facts.has_software_source,
                FileProperty::IccProfile => facts.has_icc_profile,
                FileProperty::ForcedFiletype => facts.filetype_forced,
            };
            value == *has
        }
        S::Time { kind, test } => {
            let Some(range) = range_opt(*test, clock) else {
                return false;
            };
            if range.is_unbounded() {
                return false;
            }
            let t = match kind {
                TimeKind::Imported => facts.imported_ms,
                TimeKind::Modified => facts.modified_ms,
                TimeKind::LastViewed => facts.last_viewed_ms,
                TimeKind::Archived => facts.archived_ms,
            };
            t.is_some_and(|t| range.contains(t))
        }
        S::TagAdvanced {
            service,
            display,
            statuses,
            tag,
            inclusive,
        } => {
            let service = match service {
                None => None,
                Some(hydrus_core::search::predicate::ServiceRef::Key(key)) => Some(key.clone()),
                // names are resolved to keys before a rule is stored
                Some(hydrus_core::search::predicate::ServiceRef::Name(_)) => return false,
            };
            let found = statuses.iter().any(|status| {
                facts
                    .service_tags
                    .get(&(service.clone(), *display, *status))
                    .is_some_and(|tags| tags.contains(tag.as_str()))
            });
            found == *inclusive
        }
        _ => false,
    }
}

/// `ExtractComparableValueFromMediaResult`: the value, where a missing one
/// counts as 0 except for times.
pub fn extract(what: Comparable, facts: &FileFacts) -> Option<f64> {
    let value = match what {
        Comparable::Size => as_f64(facts.size),
        Comparable::Width => as_f64(facts.width),
        Comparable::Height => as_f64(facts.height),
        Comparable::NumPixels => facts.width.zip(facts.height).map(|(w, h)| (w * h) as f64),
        Comparable::Ratio => match (facts.width, facts.height) {
            (Some(w), Some(h)) => Some(w as f64 / h as f64),
            _ => None,
        },
        Comparable::Duration => as_f64(facts.duration_ms),
        Comparable::Framerate => facts.framerate(),
        Comparable::NumFrames => as_f64(facts.num_frames),
        Comparable::NumTags => Some(facts.tags.len() as f64),
        Comparable::NumUrls => Some(facts.urls.len() as f64),
        Comparable::ImportTime => facts.imported_ms.map(|t| t as f64),
        Comparable::ModifiedTime => facts.modified_ms.map(|t| t as f64),
        Comparable::LastViewedTime => facts.last_viewed_ms.map(|t| t as f64),
        Comparable::ArchivedTime => facts.archived_ms.map(|t| t as f64),
    };
    if value.is_none() && !what.is_time() {
        Some(0.0)
    } else {
        value
    }
}

/// Load the facts about `hash_ids` that in-memory tests need, from the
/// store's media results. Tags are as displayed (siblings and parents
/// applied to current and pending tags), per service and combined.
pub fn load_facts(
    conn: &rusqlite::Connection,
    snapshot: &hydrus_store::Snapshot,
    hash_ids: &[hydrus_core::HashId],
) -> hydrus_store::Result<std::collections::HashMap<hydrus_core::HashId, FileFacts>> {
    use hydrus_core::service::builtin_keys;
    let batch =
        hydrus_store::media::load(conn, &snapshot.services, Some(&snapshot.display), hash_ids)?;
    let local_storage = snapshot
        .services
        .builtin(builtin_keys::HYDRUS_LOCAL_FILE_STORAGE)
        .map(|s| s.id)
        .ok();
    let tag_name = |id: &hydrus_core::TagId| batch.tags.get(id).map(|t| t.as_str().to_owned());
    let url_settings = snapshot.url_classes.settings();
    let mut out = std::collections::HashMap::with_capacity(batch.results.len());
    for m in &batch.results {
        let mut facts = FileFacts {
            inbox: m.inbox,
            urls: m.urls.clone(),
            imported_ms: local_storage.and_then(|s| m.added_to(s)).map(|t| t.0),
            modified_ms: m.aggregate_modified().map(|t| t.0),
            last_viewed_ms: m
                .viewing
                .iter()
                .find(|v| v.canvas == hydrus_core::CanvasType::MediaViewer)
                .and_then(|v| v.last_viewed)
                .map(|t| t.0),
            archived_ms: m.archived.map(|t| t.0),
            ..FileFacts::default()
        };
        if let Some(info) = &m.info {
            type F = hydrus_store::media::FileFlags;
            let flags = info.flags;
            facts.mime = Some(info.mime);
            facts.size = Some(info.size);
            facts.width = info.width.map(u64::from);
            facts.height = info.height.map(u64::from);
            facts.duration_ms = info.duration_ms;
            facts.num_frames = info.num_frames;
            facts.has_audio = info.has_audio;
            facts.filetype_forced = info.original_mime.is_some();
            facts.has_exif = flags.has(F::EXIF);
            facts.has_icc_profile = flags.has(F::ICC_PROFILE);
            facts.has_human_readable_embedded_metadata = flags.has(F::HUMAN_READABLE_METADATA);
            facts.has_transparency = flags.has(F::TRANSPARENCY);
            facts.has_xmp = flags.has(F::XMP);
            facts.has_iptc = flags.has(F::IPTC);
            facts.has_software_source = flags.has(F::SOFTWARE_SOURCE);
        }
        // by case-folded name, as searches name them; of classes whose names
        // fold the same, a search means the first
        let mut seen = BTreeSet::new();
        for class in &url_settings.url_classes {
            let name = hydrus_core::casefold::casefold(&class.name);
            if seen.insert(name.clone())
                && m.urls
                    .iter()
                    .any(|u| class.matches(u, url_settings.collapse_leading_slashes))
            {
                facts.url_classes.insert(name);
            }
        }
        for service in snapshot.services.tag_services() {
            let Some(service_tags) = m.tags.get(&service.id) else {
                continue;
            };
            let graph = snapshot.display.get(service.id);
            for (status, ids) in &service_tags.by_status {
                let storage: BTreeSet<String> = ids.iter().filter_map(tag_name).collect();
                let display: BTreeSet<String> = match status {
                    ContentStatus::Current | ContentStatus::Pending => ids
                        .iter()
                        .flat_map(|t| graph.display_tags(*t))
                        .filter_map(|t| tag_name(&t))
                        .collect(),
                    // siblings and parents only apply to tags a file has
                    ContentStatus::Deleted | ContentStatus::Petitioned => storage.clone(),
                };
                if matches!(status, ContentStatus::Current | ContentStatus::Pending) {
                    facts.tags.extend(display.iter().cloned());
                }
                for (display_type, tags) in [
                    (TagDisplayType::Storage, storage),
                    (TagDisplayType::Display, display),
                ] {
                    for key in [Some(service.key.clone()), None] {
                        facts
                            .service_tags
                            .entry((key, display_type, *status))
                            .or_default()
                            .extend(tags.iter().cloned());
                    }
                }
            }
        }
        out.insert(m.hash_id, facts);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_whole_tags() {
        assert!(wildcard_matches("char*:sam*", "character:samus"));
        assert!(wildcard_matches("*", ""));
        assert!(wildcard_matches("a*a", "aa"));
        assert!(!wildcard_matches("a*a", "a"));
        assert!(wildcard_matches("*blue*", "light blue sky"));
        assert!(!wildcard_matches("blue", "blue sky"));
    }

    #[test]
    fn number_tests_treat_missing_values_like_the_reference() {
        assert!(number_test(NumberOp::Less, 5.0, None));
        assert!(!number_test(NumberOp::Greater, 5.0, None));
        assert!(number_test(NumberOp::Equal, 0.0, None));
        assert!(!number_test(NumberOp::NotEqual, 0.0, None));
        assert!(number_test(NumberOp::NotEqual, 3.0, None));
        assert!(number_test(
            NumberOp::ApproxPercent { percent: 15 },
            100.0,
            Some(114.0)
        ));
        assert!(!number_test(
            NumberOp::ApproxPercent { percent: 15 },
            100.0,
            Some(116.0)
        ));
    }
}
