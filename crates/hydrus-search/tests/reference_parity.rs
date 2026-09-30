//! Checks the parsers against the reference implementation's output.
//!
//! `oracle/dump_system_predicates.py` ran the reference parser over a large
//! corpus and recorded, for each input, a normalised description of the
//! reference `Predicate` (or that it was rejected). Here we parse the same
//! inputs and turn our predicates into the same description.
//!
//! Our predicates keep services and URL classes as names, so producing the
//! reference's description means resolving them. The fixture includes the
//! stub services and URL classes the oracle used; [`Registry`] resolves
//! against them exactly as the reference does. Likewise relative times are
//! collapsed to the reference's `(0, 0, days, hours)` form at the oracle's
//! pinned "now".
//!
//! Every input must either match exactly, or differ in one of the documented
//! ways (see the crate README), each of which is checked specifically.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use serde_json::{Value, json};

use hydrus_search::{
    CalendarDelta, FileHashes, FileProperty, NumberOp, NumericProperty, ParseErrorKind, Predicate,
    RatingLogic, RatingTest, ServiceRef, ServiceSelection, SystemPredicate, TagDisplayType,
    TimeKind, TimeTest, UrlRule, ViewCanvas, ViewCanvases, ViewingStat, parse_api_search,
    parse_system_predicate,
};

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle/fixtures/system_predicates.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).expect("fixture is valid json")
}

// ---------------------------------------------------------------------------
// Resolving names like the reference

#[derive(Debug)]
struct Service {
    name: String,
    kind: u64,
    key: String,
    num_stars: Option<u64>,
    allow_zero: Option<bool>,
}

struct Registry {
    services: Vec<Service>,
    url_classes: Vec<String>,
    default_view_canvases: Vec<Value>,
    real_file_services: Vec<u64>,
    ratings_services: Vec<u64>,
    local_ratings_services: Vec<u64>,
    all_tag_services: Vec<u64>,
    base_now: CivilSeconds,
}

const LOCAL_RATING_NUMERICAL: u64 = 6;
const LOCAL_RATING_INCDEC: u64 = 22;

fn u64_list(v: &Value) -> Vec<u64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_u64().unwrap())
        .collect()
}

impl Registry {
    fn new(fixture: &Value) -> Self {
        let services = fixture["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| Service {
                name: s["name"].as_str().unwrap().to_owned(),
                kind: s["type"].as_u64().unwrap(),
                key: s["key"].as_str().unwrap().to_owned(),
                num_stars: s["num_stars"].as_u64(),
                allow_zero: s["allow_zero"].as_bool(),
            })
            .collect();
        let groups = &fixture["service_groups"];
        Self {
            services,
            url_classes: fixture["url_classes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect(),
            default_view_canvases: fixture["default_view_canvases"].as_array().unwrap().clone(),
            real_file_services: u64_list(&groups["real_file_services"]),
            ratings_services: u64_list(&groups["ratings_services"]),
            local_ratings_services: u64_list(&groups["local_ratings_services"]),
            all_tag_services: u64_list(&groups["all_tag_services"]),
            base_now: CivilSeconds::parse_iso(fixture["base_now"].as_str().unwrap()),
        }
    }

    /// `ServicesManager.GetServiceKeyFromName`: exact name, then case-insensitive.
    fn service(&self, allowed: &[u64], service: &ServiceRef) -> Result<&Service, String> {
        let name = match service {
            ServiceRef::Name(name) => name,
            ServiceRef::Key(key) => {
                // decoded predicates name their services by key
                return self
                    .services
                    .iter()
                    .find(|s| allowed.contains(&s.kind) && s.key == key.to_hex())
                    .ok_or_else(|| format!("no service with key {}", key.to_hex()));
            }
        };
        let allowed_services = || self.services.iter().filter(|s| allowed.contains(&s.kind));
        allowed_services()
            .find(|s| s.name == *name)
            .or_else(|| allowed_services().find(|s| s.name.to_lowercase() == name.to_lowercase()))
            .ok_or_else(|| format!("no service called {name:?}"))
    }

    fn url_class(&self, name: &str) -> Result<&str, String> {
        self.url_classes
            .iter()
            .find(|c| c.to_lowercase() == name.to_lowercase())
            .map(String::as_str)
            .ok_or_else(|| format!("no url class called {name:?}"))
    }
}

// ---------------------------------------------------------------------------
// Calendar arithmetic, for collapsing ages at the pinned "now"

/// Seconds since 1970-01-01T00:00:00 on a naive (timezone-less) clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CivilSeconds(i64);

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    days_from_civil(
        if m == 12 { y + 1 } else { y },
        if m == 12 { 1 } else { m + 1 },
        1,
    ) - days_from_civil(y, m, 1)
}

impl CivilSeconds {
    fn parse_iso(s: &str) -> Self {
        let n: Vec<i64> = s
            .split(['-', 'T', ':'])
            .map(|p| p.parse().unwrap())
            .collect();
        Self::from_parts(n[0], n[1], n[2], n[3], n[4], n[5])
    }

    fn from_parts(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> Self {
        Self(days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s)
    }

    fn parts(self) -> [i64; 6] {
        let days = self.0.div_euclid(86_400);
        let secs = self.0.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        [y, m, d, secs / 3600, secs % 3600 / 60, secs % 60]
    }

    /// `self - relativedelta(...)`, as dateparser computes an age: calendar
    /// years and months first (clamping the day), then the rest.
    fn minus(self, delta: &CalendarDelta) -> Self {
        let [year, month, day, hour, minute, second] = self.parts();
        let months =
            year * 12 + (month - 1) - i64::from(delta.years) * 12 - i64::from(delta.months);
        let (year, month) = (months.div_euclid(12), months.rem_euclid(12) + 1);
        let day = day.min(days_in_month(year, month));
        let shifted = Self::from_parts(year, month, day, hour, minute, second);
        Self(
            shifted.0
                - i64::from(delta.days) * 86_400
                - i64::from(delta.hours) * 3600
                - i64::from(delta.minutes) * 60
                - i64::from(delta.seconds),
        )
    }
}

/// Python's `round()` of `n / d` for non-negative values: halves go to even.
fn round_half_even(n: i64, d: i64) -> i64 {
    let q = n / d;
    let r = n % d;
    match (2 * r).cmp(&d) {
        std::cmp::Ordering::Less => q,
        std::cmp::Ordering::Greater => q + 1,
        std::cmp::Ordering::Equal => q + (q % 2),
    }
}

/// The reference's `(years, months, days, hours)` for an age typed at `now`.
fn reference_delta(now: CivilSeconds, age: &CalendarDelta) -> [i64; 4] {
    let gap = now.0 - now.minus(age).0;
    [
        0,
        0,
        gap.div_euclid(86_400),
        round_half_even(gap.rem_euclid(86_400), 3600),
    ]
}

// ---------------------------------------------------------------------------
// Normalising our predicates to the fixture's description

fn predicate_type(name: &str, value: Value, inclusive: bool) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("type".into(), json!(name));
    out.insert("value".into(), value);
    out.insert("inclusive".into(), json!(inclusive));
    Value::Object(out)
}

fn number_test_json(test: &hydrus_search::NumberTest) -> Value {
    let (op, extra) = match test.op {
        NumberOp::Less => ("less", Value::Null),
        NumberOp::LessOrEqual => ("less_or_equal", Value::Null),
        NumberOp::Greater => ("greater", Value::Null),
        NumberOp::GreaterOrEqual => ("greater_or_equal", Value::Null),
        NumberOp::Equal => ("equal", Value::Null),
        NumberOp::NotEqual => ("not_equal", Value::Null),
        NumberOp::ApproxPercent { percent } => {
            ("approx_percent", json!(f64::from(percent) / 100.0))
        }
        NumberOp::ApproxAbsolute { tolerance } => ("approx_absolute", json!(tolerance)),
    };
    json!({ "op": op, "value": test.value, "extra": extra })
}

/// `ClientRatings.ConvertStarsToRating`.
fn convert_stars_to_rating(num_stars: u64, allow_zero: bool, stars: u64) -> f64 {
    let stars = stars.min(num_stars);
    if allow_zero {
        stars as f64 / num_stars as f64
    } else {
        (stars.max(1) - 1) as f64 / (num_stars - 1) as f64
    }
}

fn selection_json(
    registry: &Registry,
    selection: Option<&ServiceSelection>,
) -> Result<Value, String> {
    let (mut types, mut keys): (Vec<u64>, Vec<String>) = (vec![], vec![]);
    match selection {
        None => {}
        Some(ServiceSelection::Types(t)) => types = t.iter().map(|t| u64::from(t.code())).collect(),
        Some(ServiceSelection::Names(names)) => {
            for name in names {
                let service = registry.service(
                    &registry.local_ratings_services,
                    &ServiceRef::Name(name.clone()),
                )?;
                keys.push(service.key.clone());
            }
        }
    }
    types.sort_unstable();
    keys.sort();
    keys.dedup();
    Ok(json!({ "types": types, "keys": keys }))
}

fn time_json(registry: &Registry, test: &TimeTest) -> Value {
    match test {
        TimeTest::Relative { op, age } => json!({
            "op": op.symbol(),
            "kind": "delta",
            "value": reference_delta(registry.base_now, age),
        }),
        TimeTest::Absolute { op, at } => json!({
            "op": op.symbol(),
            "kind": "date",
            "value": [at.year(), at.month(), at.day(), at.hour(), at.minute()],
        }),
    }
}

#[allow(clippy::too_many_lines)]
fn system_json(registry: &Registry, pred: &SystemPredicate) -> Result<Value, String> {
    use SystemPredicate as S;
    Ok(match pred {
        S::Everything => predicate_type("system_everything", Value::Null, true),
        S::Inbox => predicate_type("system_inbox", Value::Null, true),
        S::Archive => predicate_type("system_archive", Value::Null, true),
        S::Local => predicate_type("system_local", Value::Null, true),
        S::NotLocal => predicate_type("system_not_local", Value::Null, true),
        S::Limit(n) => predicate_type("system_limit", json!(n), true),
        S::Filetype {
            filetypes,
            inclusive,
        } => {
            let codes: Vec<u8> = filetypes.summary().iter().map(|m| m.code()).collect();
            predicate_type("system_mime", json!(codes), *inclusive)
        }
        S::Hash { hashes, inclusive } => {
            let kind = match hashes {
                FileHashes::Sha256(_) => "sha256",
                FileHashes::Md5(_) => "md5",
                FileHashes::Sha1(_) => "sha1",
                FileHashes::Sha512(_) => "sha512",
            };
            predicate_type(
                "system_hash",
                json!({ "hashes": hashes.to_hex(), "hash_type": kind }),
                *inclusive,
            )
        }
        S::SimilarToFiles {
            files,
            max_distance,
        } => predicate_type(
            "system_similar_to_files",
            json!({
                "hashes": files.iter().map(hydrus_core::Sha256::to_hex).collect::<Vec<_>>(),
                "distance": max_distance,
            }),
            true,
        ),
        S::SimilarToData {
            pixel_hashes,
            perceptual_hashes,
            max_distance,
        } => predicate_type(
            "system_similar_to_data",
            json!({
                "pixel_hashes": pixel_hashes.iter().map(hydrus_core::Sha256::to_hex).collect::<Vec<_>>(),
                "perceptual_hashes": perceptual_hashes.iter().map(hydrus_core::PerceptualHash::to_hex).collect::<Vec<_>>(),
                "distance": max_distance,
            }),
            true,
        ),
        S::FileProperty { property, has } => {
            let name = match property {
                FileProperty::Audio => "system_has_audio",
                FileProperty::Transparency => "system_has_transparency",
                FileProperty::Exif => "system_has_exif",
                FileProperty::Xmp => "system_has_xmp",
                FileProperty::Iptc => "system_has_iptc",
                FileProperty::HumanReadableEmbeddedMetadata => {
                    "system_has_human_readable_embedded_metadata"
                }
                FileProperty::SoftwareSourceMetadata => "system_has_software_source",
                FileProperty::IccProfile => "system_has_icc_profile",
                FileProperty::ForcedFiletype => "system_has_forced_filetype",
            };
            predicate_type(name, json!(has), true)
        }
        S::Number { property, test } => {
            let name = match property {
                NumericProperty::Width => "system_width",
                NumericProperty::Height => "system_height",
                NumericProperty::Duration => "system_duration",
                NumericProperty::Framerate => "system_framerate",
                NumericProperty::NumFrames => "system_num_frames",
                NumericProperty::NumNotes => "system_num_notes",
                NumericProperty::NumUrls => "system_num_urls",
                NumericProperty::NumWords => "system_num_words",
            };
            predicate_type(name, number_test_json(test), true)
        }
        S::NumTags {
            namespace,
            op,
            count,
        } => predicate_type(
            "system_num_tags",
            json!([namespace.as_reference(), op.symbol(), count]),
            true,
        ),
        S::FileSize { op, size, unit } => predicate_type(
            "system_size",
            json!([op.symbol(), size, unit.bytes()]),
            true,
        ),
        S::NumPixels { op, count, unit } => predicate_type(
            "system_num_pixels",
            json!([op.symbol(), count, unit.pixels()]),
            true,
        ),
        S::Ratio { op, width, height } => {
            predicate_type("system_ratio", json!([op.symbol(), width, height]), true)
        }
        S::Time { kind, test } => {
            let name = match kind {
                TimeKind::Imported => "system_import_time",
                TimeKind::Modified => "system_modified_time",
                TimeKind::LastViewed => "system_last_viewed_time",
                TimeKind::Archived => "system_archived_time",
            };
            predicate_type(name, time_json(registry, test), true)
        }
        S::FileService {
            service,
            status,
            is_in,
        } => {
            let service = registry.service(&registry.real_file_services, service)?;
            predicate_type(
                "system_file_service",
                json!({ "is_in": is_in, "status": status.code(), "service": service.key }),
                true,
            )
        }
        S::FileRelationshipCount {
            op,
            count,
            relationship,
        } => predicate_type(
            "system_file_relationships_count",
            json!([op.symbol(), count, relationship.duplicate_type().code()]),
            true,
        ),
        S::BestQualityOfGroup { is_best } => {
            predicate_type("system_file_relationships_king", json!(is_best), true)
        }
        S::FileViewingStats {
            stat,
            canvases,
            op,
            value,
        } => {
            let canvases: Vec<Value> = match canvases {
                ViewCanvases::Default => registry.default_view_canvases.clone(),
                ViewCanvases::Specific(set) => set
                    .iter()
                    .map(|c| match c {
                        ViewCanvas::MediaViewer => json!("media"),
                        ViewCanvas::Preview => json!("preview"),
                        ViewCanvas::ClientApi => json!("client api"),
                    })
                    .collect(),
            };
            let view_type = match stat {
                ViewingStat::Views => "views",
                ViewingStat::ViewTime => "viewtime",
            };
            predicate_type(
                "system_file_viewing_stats",
                json!({ "view_type": view_type, "canvases": canvases, "op": op.symbol(), "value": value }),
                true,
            )
        }
        S::KnownUrl { rule, has } => {
            let (rule_type, rule) = match rule {
                UrlRule::ExactMatch(url) => ("exact_match", url.as_str()),
                UrlRule::Domain(domain) => ("domain", domain.as_str()),
                UrlRule::Regex(regex) => ("regex", regex.as_str()),
                UrlRule::UrlClass(name) => ("url_class", registry.url_class(name)?),
            };
            predicate_type(
                "system_known_urls",
                json!({ "include": has, "rule_type": rule_type, "rule": rule }),
                true,
            )
        }
        S::TagAsNumber {
            namespace,
            op,
            value,
        } => predicate_type(
            "system_tag_as_number",
            json!([namespace.as_reference(), op.symbol(), value]),
            true,
        ),
        S::NoteName { name, has } => {
            predicate_type("system_has_note_name", json!([has, name]), true)
        }
        S::Rating { service, test } => rating_json(registry, service, test)?,
        S::RatingAdvanced {
            logic,
            services,
            rated,
        } => {
            let (logical_operator, secondary) = match logic {
                RatingLogic::All => (0, None),
                RatingLogic::Any => (1, None),
                RatingLogic::Only { amongst } => (2, Some(amongst)),
            };
            predicate_type(
                "system_rating_advanced",
                json!({
                    "logical_operator": logical_operator,
                    "primary": selection_json(registry, Some(services))?,
                    "secondary": selection_json(registry, secondary)?,
                    "rated": rated,
                }),
                true,
            )
        }
        S::TagAdvanced {
            service,
            display,
            statuses,
            tag,
            inclusive,
        } => {
            let service = match service {
                Some(s) => json!(registry.service(&registry.all_tag_services, s)?.key),
                None => Value::Null,
            };
            let display = match display {
                TagDisplayType::Storage => "storage",
                TagDisplayType::Display => "display",
            };
            let statuses: Vec<u8> = statuses.iter().map(|s| s.code()).collect();
            predicate_type(
                "system_tag_advanced",
                json!({ "service": service, "display": display, "statuses": statuses, "tag": tag.as_str() }),
                *inclusive,
            )
        }
    })
}

/// `ClientSearchParseSystemPredicates.rating_service_pred_generator`.
fn rating_json(
    registry: &Registry,
    service: &ServiceRef,
    test: &RatingTest,
) -> Result<Value, String> {
    let service = registry.service(&registry.ratings_services, service)?;
    let (mut op, mut value) = match *test {
        RatingTest::Rated => ("=", json!("rated")),
        RatingTest::NotRated => ("=", json!("not rated")),
        RatingTest::Liked => ("=", json!(1.0)),
        RatingTest::Disliked => ("=", json!(0.0)),
        RatingTest::Stars { op, stars, .. } => (op.symbol(), json!(stars)),
        RatingTest::Count { op, value } => (op.symbol(), json!(value)),
    };
    if service.kind == LOCAL_RATING_NUMERICAL {
        if let Some(stars) = value.as_u64() {
            value = json!(convert_stars_to_rating(
                service.num_stars.unwrap(),
                service.allow_zero.unwrap(),
                stars
            ));
        }
    } else if service.kind == LOCAL_RATING_INCDEC {
        match value.as_str() {
            Some("rated") => (op, value) = (">", json!(0)),
            Some("not rated") => (op, value) = ("=", json!(0)),
            _ => {}
        }
    }
    Ok(predicate_type(
        "system_rating",
        json!({ "op": op, "value": value, "service": service.key }),
        true,
    ))
}

fn predicate_json(registry: &Registry, pred: &Predicate) -> Result<Value, String> {
    Ok(match pred {
        Predicate::Tag { tag, inclusive } => predicate_type("tag", json!(tag.as_str()), *inclusive),
        Predicate::Namespace {
            namespace,
            inclusive,
        } => predicate_type("namespace", json!(namespace), *inclusive),
        Predicate::Wildcard { pattern, inclusive } => {
            predicate_type("wildcard", json!(pattern.as_str()), *inclusive)
        }
        Predicate::Or(preds) => predicate_type(
            "or_container",
            Value::Array(
                preds
                    .iter()
                    .map(|p| predicate_json(registry, p))
                    .collect::<Result<_, _>>()?,
            ),
            true,
        ),
        Predicate::System(sys) => system_json(registry, sys)?,
    })
}

/// Sort object keys, and the members of OR groups, so descriptions compare
/// regardless of order.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> =
                map.iter().map(|(k, v)| (k, canonical(v))).collect();
            let mut out = serde_json::Map::new();
            for (k, v) in sorted {
                out.insert(k.clone(), v);
            }
            if out.get("type").and_then(Value::as_str) == Some("or_container")
                && let Some(Value::Array(items)) = out.get_mut("value")
            {
                items.sort_by_key(ToString::to_string);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Comparing

/// How an input's result relates to the reference's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Outcome {
    /// Same predicate.
    Same,
    /// Both rejected it.
    BothRejected,
    /// A URL regex, exact URL or note name kept the case it was typed in.
    CasePreserved,
    /// The reference turned an age into a wall-clock date ("2 weeks",
    /// "30 minutes", "1d"); we keep it an age, which lands on the same
    /// instant at the pinned "now".
    AgeKeptRelative,
    /// A date or age form that only `dateparser` understands.
    UnsupportedDate,
    /// A hash of the wrong length for its hash type.
    WrongHashLength,
    /// A number too large for 64 bits.
    NumberTooLarge,
    /// A tag that is empty once cleaned (the reference searches for the
    /// literal tag "invalid tag").
    InvalidTag,
    Mismatch,
}

fn lowercase_free_text(v: &Value) -> Value {
    let mut v = v.clone();
    if let Some(rule) = v.pointer_mut("/value/rule")
        && let Some(s) = rule.as_str()
    {
        *rule = json!(s.to_lowercase());
    }
    if v["type"] == "system_has_note_name"
        && let Some(name) = v.pointer_mut("/value/1")
    {
        *name = json!(name.as_str().unwrap_or_default().to_lowercase());
    }
    v
}

fn age_lands_on_reference_date(
    registry: &Registry,
    ours: &SystemPredicate,
    expected: &Value,
) -> bool {
    let SystemPredicate::Time {
        test: TimeTest::Relative { age, .. },
        ..
    } = ours
    else {
        return false;
    };
    let Some(date) = expected.pointer("/value/value").and_then(Value::as_array) else {
        return false;
    };
    let when = registry.base_now.minus(age).parts();
    date.iter()
        .zip(when)
        .all(|(reference, ours)| reference.as_i64() == Some(ours))
}

fn has_wrong_length_hash(expected: &Value) -> bool {
    let value = &expected["value"];
    let expected_len = match value["hash_type"].as_str() {
        Some("md5") => 32,
        Some("sha1") => 40,
        Some("sha512") => 128,
        _ => 64,
    };
    value["hashes"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|h| h.as_str().is_some_and(|h| h.len() != expected_len))
}

fn compare(registry: &Registry, case: &Value) -> Outcome {
    let input = case["input"].as_str().unwrap();
    let ours = parse_system_predicate(input);
    let ours_json = ours
        .as_ref()
        .map_err(Clone::clone)
        .map(|p| system_json(registry, p).map(|v| canonical(&v)));
    if case["ok"] != true {
        // rejected by the reference, whether by the parser or a failed lookup
        return match ours_json {
            Err(_) | Ok(Err(_)) => Outcome::BothRejected,
            Ok(Ok(_)) => Outcome::Mismatch,
        };
    }
    let expected = canonical(&case["predicate"]);
    let is_time = expected["type"]
        .as_str()
        .is_some_and(|t| t.ends_with("_time"));
    match (ours, ours_json) {
        (Ok(pred), Ok(Ok(json))) => {
            if json == expected {
                Outcome::Same
            } else if lowercase_free_text(&json) == expected {
                Outcome::CasePreserved
            } else if case["depends_on_now"] == true
                && expected.pointer("/value/kind") == Some(&json!("date"))
                && age_lands_on_reference_date(registry, &pred, &expected)
            {
                Outcome::AgeKeptRelative
            } else {
                Outcome::Mismatch
            }
        }
        (Err(e), _) => match e.kind {
            ParseErrorKind::UnsupportedDate(_) | ParseErrorKind::CalendarOperatorNeedsDate
                if is_time =>
            {
                Outcome::UnsupportedDate
            }
            ParseErrorKind::WrongHashLength { .. } if has_wrong_length_hash(&expected) => {
                Outcome::WrongHashLength
            }
            ParseErrorKind::NumberTooLarge(_)
                if input
                    .split(|c: char| !c.is_ascii_digit())
                    .any(|run| run.len() >= 20) =>
            {
                Outcome::NumberTooLarge
            }
            ParseErrorKind::InvalidTag(_)
                if expected.pointer("/value/tag") == Some(&json!("invalid tag")) =>
            {
                Outcome::InvalidTag
            }
            _ => Outcome::Mismatch,
        },
        (Ok(_), _) => Outcome::Mismatch,
    }
}

fn describe(registry: &Registry, case: &Value) -> String {
    let input = case["input"].as_str().unwrap();
    let ours = match parse_system_predicate(input) {
        Ok(pred) => match system_json(registry, &pred) {
            Ok(json) => canonical(&json).to_string(),
            Err(e) => format!("unresolved: {e}"),
        },
        Err(e) => format!("error: {e}"),
    };
    let reference = if case["ok"] == true {
        canonical(&case["predicate"]).to_string()
    } else {
        format!("error: {}", case["error"])
    };
    format!("{input:?}\n    reference: {reference}\n    ours:      {ours}")
}

/// Time values only `dateparser` understands: natural language, day-first
/// or month-first dates, fractions, number words, future times and junk.
/// The corpus tries each with these operators on `system:import time`.
const UNSUPPORTED_DATE_VALUES: &[&str] = &[
    "today",
    "now",
    "tomorrow",
    "last week",
    "in 2 days",
    "one day",
    "two days",
    "1.5 days",
    "1 decade",
    "ago",
    "03/04/2020",
    "02-03-2011",
    "4 march 2020",
    "march 2020",
    "june 4 2011",
    "2020",
    "x",
    "2011-06-04x",
    "2011-06-04 ago",
    "5 days ago ago",
    "1 hour ago 1 day",
    "ago 1 day",
    "1,000 days",
    "3 w",
    "3w",
];
const UNSUPPORTED_DATE_OPERATORS: &[&str] = &["<", ">", "~=", "="];

/// "The day of" or "the month of" an age: the reference turns the age into
/// a date at parse time; we cannot, and refuse.
const CALENDAR_OPERATOR_ON_AGE: &[&str] = &[
    "system:modified time the day of 2 weeks",
    "system:modified time the day of 30 minutes",
    "system:modified time the month of 2 weeks",
    "system:modified time the month of 30 minutes",
    "system:modified time on the day of 2 weeks",
    "system:modified time on the day of 30 minutes",
    "system:modified time a month either side of 2 weeks",
    "system:modified time a month either side of 30 minutes",
    "system:modified time since the day of 2 weeks",
    "system:modified time since the day of 30 minutes",
];

#[test]
fn system_predicates_match_the_reference() {
    let fixture = fixture();
    let registry = Registry::new(&fixture);
    let cases = fixture["system_predicates"].as_array().unwrap();
    let verbose = std::env::var_os("HYDRUS_PARITY_VERBOSE").is_some();
    let mut counts: BTreeMap<Outcome, usize> = BTreeMap::new();
    let mut report = String::new();
    let mut unsupported = Vec::new();
    for case in cases {
        let outcome = compare(&registry, case);
        *counts.entry(outcome).or_default() += 1;
        match outcome {
            Outcome::Mismatch => writeln!(report, "{}", describe(&registry, case)).unwrap(),
            Outcome::UnsupportedDate => {
                unsupported.push(case["input"].as_str().unwrap().to_owned());
            }
            _ => {}
        }
        if verbose && !matches!(outcome, Outcome::Same | Outcome::BothRejected) {
            println!("{outcome:?}: {}", describe(&registry, case));
        }
    }
    let summary: Vec<String> = counts.iter().map(|(k, v)| format!("{k:?}: {v}")).collect();
    println!("{} inputs; {}", cases.len(), summary.join(", "));
    assert!(
        report.is_empty(),
        "{} inputs differ from the reference:\n{report}",
        counts.get(&Outcome::Mismatch).copied().unwrap_or(0)
    );
    let mut expected: Vec<String> = UNSUPPORTED_DATE_VALUES
        .iter()
        .flat_map(|value| {
            UNSUPPORTED_DATE_OPERATORS
                .iter()
                .map(move |op| format!("system:import time {op} {value}"))
        })
        .chain(CALENDAR_OPERATOR_ON_AGE.iter().map(|s| (*s).to_owned()))
        .collect();
    expected.sort();
    unsupported.sort();
    assert_eq!(
        unsupported, expected,
        "the set of unsupported date inputs changed"
    );
}

#[test]
fn api_searches_match_the_reference() {
    let fixture = fixture();
    let registry = Registry::new(&fixture);
    let mut report = String::new();
    let cases = fixture["api_searches"].as_array().unwrap();
    for case in cases {
        let tags = &case["tags"];
        let ours: Result<Vec<Value>, String> = parse_api_search(tags)
            .map_err(|e| e.to_string())
            .and_then(|preds| {
                preds
                    .iter()
                    .map(|p| predicate_json(&registry, p).map(|v| canonical(&v)))
                    .collect()
            });
        let matches = match (&ours, case["ok"] == true) {
            (Ok(ours), true) => {
                let mut ours: Vec<String> = ours.iter().map(ToString::to_string).collect();
                let mut expected: Vec<String> = case["predicates"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| canonical(p).to_string())
                    .collect();
                ours.sort();
                expected.sort();
                ours == expected
            }
            (Err(_), false) => true,
            _ => false,
        };
        if !matches {
            writeln!(
                report,
                "{tags}\n    reference: {}\n    ours:      {ours:?}",
                if case["ok"] == true {
                    case["predicates"].to_string()
                } else {
                    case["error"].to_string()
                }
            )
            .unwrap();
        }
    }
    assert!(report.is_empty(), "API searches differ:\n{report}");
}

/// Stored predicates (the reference's serialised form of each predicate it
/// parsed) decode to what the reference parsed. Ratings are not decoded yet
/// (see `hydrus_legacy::objects::predicates`), and URL class predicates are
/// not recorded (the fixture's URL classes are stubs).
#[test]
fn stored_predicates_decode_to_what_the_reference_parsed() {
    let fixture = fixture();
    let registry = Registry::new(&fixture);
    let mut report = String::new();
    let (mut decoded, mut ratings) = (0, 0);
    for case in fixture["system_predicates"].as_array().unwrap() {
        let Some(serialised) = case.get("serialised") else {
            continue;
        };
        let stored = hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
            &serialised.to_string(),
        )
        .expect("a serialised tuple");
        let expected = canonical(&case["predicate"]);
        match hydrus_legacy::objects::predicates::predicate(&stored) {
            Ok(pred) => {
                decoded += 1;
                let mut ours = predicate_json(&registry, &pred).map(|v| canonical(&v));
                // a stored age is kept as stored (the parser's ages are
                // normalised to days and hours at "now" by `time_json`)
                if let (
                    Predicate::System(SystemPredicate::Time {
                        test: TimeTest::Relative { age, .. },
                        ..
                    }),
                    Ok(json),
                ) = (&pred, ours.as_mut())
                {
                    json["value"]["value"] = json!([age.years, age.months, age.days, age.hours]);
                }
                if ours.as_ref() != Ok(&expected) {
                    writeln!(
                        report,
                        "{serialised}\n    reference: {expected}\n    ours:      {ours:?}"
                    )
                    .unwrap();
                }
            }
            Err(e) => {
                if expected["type"] == "system_rating"
                    || expected["type"] == "system_rating_advanced"
                {
                    ratings += 1;
                } else if matches!(
                    compare(&registry, case),
                    Outcome::WrongHashLength | Outcome::NumberTooLarge | Outcome::UnsupportedDate
                ) {
                    // values we cannot hold, whether typed or stored
                } else {
                    writeln!(report, "{serialised}\n    could not decode: {e}").unwrap();
                }
            }
        }
    }
    println!("{decoded} decoded, {ratings} ratings skipped");
    assert!(report.is_empty(), "stored predicates differ:\n{report}");
    assert!(decoded > 3000);
}
