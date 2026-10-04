//! Stored search predicates (`SERIALISABLE_TYPE_PREDICATE`, v1-8), decoded
//! into [`hydrus_core::search::predicate::Predicate`].
//!
//! Predicates are stored in file searches: favourite searches, duplicates
//! auto-resolution rules, GUI pages. The layout is `(predicate type, value,
//! inclusive)`; the value's shape depends on the type. The reference's
//! upgrade chain (v1-7) only reshaped a few types' values and is applied
//! inline.
//!
//! A stored rating on a numerical service is a fraction of the service's
//! scale, which our predicate holds as stars, so decoding one needs the
//! service's scale ([`predicate_with_scales`]). Not decoded: the retired
//! "any rating" type and the GUI's placeholder types (`system:dimensions`,
//! `system:time` and the like, which are menus, not searches). They are
//! errors, so the caller can say which search it could not bring over.

use std::collections::BTreeSet;
use std::str::FromStr;

use hydrus_core::search::filetype::FiletypeSet;
use hydrus_core::search::number::{
    Comparison, NumberOp, NumberTest, RatingOp, RatioOp, TagNumberOp,
};
use hydrus_core::search::predicate::{
    FileHashes, FileProperty, NamespaceFilter, NumericProperty, PixelUnit, Predicate, RatingLogic,
    RatingTest, Relationship, ServiceRef, ServiceSelection, SizeUnit, SystemPredicate,
    TagDisplayType, UrlRule, ViewCanvas, ViewCanvases, ViewingStat, Wildcard,
};
use hydrus_core::search::time::{CalendarDelta, CivilDateTime, RelativeOp, TimeKind, TimeTest};
use hydrus_core::{ContentStatus, DuplicateType, Mime, ServiceKey, ServiceType, Tag};

use crate::objects::util::{
    DecodeResult, boolean, float, int, list, list_items, malformed, nested, service_key,
    service_keys, string, tuple,
};
use crate::pyjson::PyJson;
use crate::serialisable::{Meta, SerialisableObject, SerialisableType};

const KIND: SerialisableType = SerialisableType::PREDICATE;

/// The scale of a numerical rating service: its number of stars, and
/// whether zero stars is a rating.
pub type StarScale = (u64, bool);

/// Decode a stored predicate. A rating on a numerical service decodes only
/// at the top or bottom of its scale; see [`predicate_with_scales`].
pub fn predicate(object: &SerialisableObject) -> DecodeResult<Predicate> {
    predicate_with_scales(object, &|_| None)
}

/// Decode a stored predicate, given the scale of each numerical rating
/// service (`None` for other services).
pub fn predicate_with_scales(
    object: &SerialisableObject,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> DecodeResult<Predicate> {
    object.expect_kind(KIND)?;
    object.check_not_future()?;
    let info = object.info();
    let [kind, value, inclusive] = tuple::<3>(KIND, &info, "predicate")?;
    let inclusive = match inclusive {
        PyJson::Null => true,
        other => boolean(KIND, other, "inclusive")?,
    };
    decode(
        object.version,
        int(KIND, kind, "predicate type")?,
        value,
        inclusive,
        scales,
    )
}

/// Decode a predicate stored as a plain serialised tuple.
pub fn predicate_from_tuple(value: &PyJson) -> DecodeResult<Predicate> {
    predicate(&nested(KIND, value, "predicate")?)
}

fn unsupported(what: &str) -> crate::serialisable::SerialisableError {
    malformed(KIND, format!("{what} predicates are not supported yet"))
}

#[allow(clippy::too_many_lines)]
fn decode(
    version: u32,
    kind: i64,
    value: &PyJson,
    inclusive: bool,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> DecodeResult<Predicate> {
    use SystemPredicate as S;
    let system = |p: SystemPredicate| Ok(Predicate::System(p));
    match kind {
        0 => Ok(Predicate::Tag {
            tag: Tag::from_clean(string(KIND, value, "tag")?),
            inclusive,
        }),
        1 => Ok(Predicate::Namespace {
            namespace: string(KIND, value, "namespace")?,
            inclusive,
        }),
        3 => Ok(Predicate::Wildcard {
            pattern: Wildcard::from_clean(string(KIND, value, "wildcard")?),
            inclusive,
        }),
        4 => system(S::Everything),
        5 => system(S::Inbox),
        6 => system(S::Archive),
        8 => {
            // v1-3 had no namespace: every tag
            let (namespace, op, count) = match list(KIND, value, "num tags")? {
                [op, count] => ("*".to_owned(), op, count),
                [namespace, op, count] => (string(KIND, namespace, "namespace")?, op, count),
                _ => return Err(malformed(KIND, "num tags should have 3 items")),
            };
            system(S::NumTags {
                namespace: NamespaceFilter::from_reference(&namespace),
                op: comparison(op)?,
                count: count_value(count, "tag count")?,
            })
        }
        9 => system(S::Limit(count_value(value, "limit")?)),
        10 => {
            let [op, size, unit] = tuple::<3>(KIND, value, "size")?;
            system(S::FileSize {
                op: comparison(op)?,
                size: count_value(size, "size")?,
                unit: match int(KIND, unit, "size unit")? {
                    1 => SizeUnit::Bytes,
                    1024 => SizeUnit::Kilobytes,
                    1_048_576 => SizeUnit::Megabytes,
                    1_073_741_824 => SizeUnit::Gigabytes,
                    1_099_511_627_776 => SizeUnit::Terabytes,
                    other => return Err(malformed(KIND, format!("unknown size unit {other}"))),
                },
            })
        }
        11 | 35 | 43 | 47 => system(S::Time {
            kind: match kind {
                11 => TimeKind::Imported,
                35 => TimeKind::Modified,
                43 => TimeKind::LastViewed,
                _ => TimeKind::Archived,
            },
            test: time_test(value)?,
        }),
        12 => {
            let [hashes, hash_type] = tuple::<2>(KIND, value, "hash")?;
            // before v3, a single hash
            let hashes: Vec<String> = match hashes {
                PyJson::Str(one) => vec![one.clone()],
                other => list(KIND, other, "hashes")?
                    .iter()
                    .map(|h| string(KIND, h, "hash"))
                    .collect::<DecodeResult<_>>()?,
            };
            let hash_type = string(KIND, hash_type, "hash type")?;
            let hashes = match hash_type.as_str() {
                "sha256" => FileHashes::Sha256(parse_hashes(&hashes)?),
                "md5" => FileHashes::Md5(parse_hashes(&hashes)?),
                "sha1" => FileHashes::Sha1(parse_hashes(&hashes)?),
                "sha512" => FileHashes::Sha512(parse_hashes(&hashes)?),
                other => return Err(malformed(KIND, format!("unknown hash type {other:?}"))),
            };
            system(S::Hash { hashes, inclusive })
        }
        13 | 14 | 16 | 22 | 36 | 37 | 38 | 52 => system(S::Number {
            property: match kind {
                13 => NumericProperty::Width,
                14 => NumericProperty::Height,
                16 => NumericProperty::Duration,
                22 => NumericProperty::NumWords,
                36 => NumericProperty::Framerate,
                37 => NumericProperty::NumFrames,
                38 => NumericProperty::NumNotes,
                _ => NumericProperty::NumUrls,
            },
            test: stored_number_test(version, kind, value)?,
        }),
        15 => {
            let [op, width, height] = tuple::<3>(KIND, value, "ratio")?;
            system(S::Ratio {
                op: match string(KIND, op, "ratio operator")?.as_str() {
                    "=" => RatioOp::Equal,
                    "wider than" => RatioOp::WiderThan,
                    "taller than" => RatioOp::TallerThan,
                    "\u{2248}" => RatioOp::Approx,
                    "\u{2260}" => RatioOp::NotEqual,
                    other => {
                        return Err(malformed(KIND, format!("unknown ratio operator {other:?}")));
                    }
                },
                width: count_value(width, "ratio width")?,
                height: count_value(height, "ratio height")?,
            })
        }
        17 => {
            let codes = list(KIND, value, "filetypes")?;
            let mut mimes = Vec::with_capacity(codes.len());
            for code in codes {
                let code = int(KIND, code, "filetype")?;
                let mime = u8::try_from(code)
                    .ok()
                    .and_then(Mime::from_code)
                    .ok_or_else(|| malformed(KIND, format!("unknown filetype {code}")))?;
                mimes.push(mime);
            }
            // v6 added the archive and image project classes to "application"
            if version < 7 && mimes.contains(&Mime::GeneralApplication) {
                mimes.extend([Mime::GeneralApplicationArchive, Mime::GeneralImageProject]);
            }
            system(S::Filetype {
                filetypes: FiletypeSet::new(mimes),
                inclusive,
            })
        }
        18 => {
            let [op, value, key] = tuple::<3>(KIND, value, "rating")?;
            let service = service_key(KIND, key, "rating service")?;
            let test = rating_test(op, value, scales(&service))?;
            system(S::Rating {
                service: ServiceRef::Key(service),
                test,
            })
        }
        55 => Err(unsupported("\"any rating\"")),
        66 => {
            let [logic, primary, secondary, rated] = tuple::<4>(KIND, value, "advanced rating")?;
            let logic = match int(KIND, logic, "logical operator")? {
                0 => RatingLogic::All,
                1 => RatingLogic::Any,
                2 => RatingLogic::Only {
                    amongst: service_selection(secondary)?,
                },
                other => return Err(malformed(KIND, format!("unknown logical operator {other}"))),
            };
            system(S::RatingAdvanced {
                logic,
                services: service_selection(primary)?,
                rated: boolean(KIND, rated, "rated")?,
            })
        }
        19 => {
            let [hashes, distance] = tuple::<2>(KIND, value, "similar files")?;
            let hashes: Vec<&PyJson> = match hashes {
                PyJson::Str(_) => vec![hashes],
                other => list(KIND, other, "hashes")?.iter().collect(),
            };
            system(S::SimilarToFiles {
                files: hashes
                    .into_iter()
                    .map(hex_hash)
                    .collect::<DecodeResult<_>>()?,
                max_distance: count_value(distance, "distance")?,
            })
        }
        20 => system(S::Local),
        21 => system(S::NotLocal),
        23 => {
            let [is_in, status, key] = tuple::<3>(KIND, value, "file service")?;
            system(S::FileService {
                service: ServiceRef::Key(service_key(KIND, key, "file service")?),
                status: code_of(status, "status", ContentStatus::from_code)?,
                is_in: boolean(KIND, is_in, "is in")?,
            })
        }
        24 => {
            let [op, count, unit] = tuple::<3>(KIND, value, "num pixels")?;
            system(S::NumPixels {
                op: comparison(op)?,
                count: count_value(count, "pixels")?,
                unit: match int(KIND, unit, "pixel unit")? {
                    1 => PixelUnit::Pixels,
                    1000 => PixelUnit::Kilopixels,
                    1_000_000 => PixelUnit::Megapixels,
                    other => return Err(malformed(KIND, format!("unknown pixel unit {other}"))),
                },
            })
        }
        26 => {
            let [op, count, relationship] = tuple::<3>(KIND, value, "relationship count")?;
            let relationship =
                match code_of(relationship, "relationship", DuplicateType::from_code)? {
                    DuplicateType::Member => Relationship::Duplicates,
                    DuplicateType::Alternate => Relationship::Alternates,
                    DuplicateType::FalsePositive => Relationship::FalsePositives,
                    DuplicateType::Potential => Relationship::PotentialDuplicates,
                    other => {
                        return Err(malformed(KIND, format!("relationship count of {other:?}")));
                    }
                };
            system(S::FileRelationshipCount {
                op: comparison(op)?,
                count: count_value(count, "count")?,
                relationship,
            })
        }
        27 => {
            let [namespace, op, value] = tuple::<3>(KIND, value, "tag as number")?;
            system(S::TagAsNumber {
                namespace: NamespaceFilter::from_reference(&string(KIND, namespace, "namespace")?),
                op: match string(KIND, op, "operator")?.as_str() {
                    "<" => TagNumberOp::Less,
                    ">" => TagNumberOp::Greater,
                    "\u{2248}" => TagNumberOp::Approx,
                    other => return Err(malformed(KIND, format!("unknown operator {other:?}"))),
                },
                value: int(KIND, value, "number")?,
            })
        }
        28 => {
            let [has, rule_type, rule, _description] = tuple::<4>(KIND, value, "known url")?;
            let rule = match string(KIND, rule_type, "url rule type")?.as_str() {
                "exact_match" => UrlRule::ExactMatch(string(KIND, rule, "url")?),
                "domain" => UrlRule::Domain(string(KIND, rule, "domain")?),
                "regex" => UrlRule::Regex(string(KIND, rule, "regex")?),
                "url_class" | "url_match" => {
                    let class = nested(KIND, rule, "url class")?;
                    UrlRule::UrlClass(
                        class
                            .name
                            .ok_or_else(|| malformed(KIND, "url class has no name"))?,
                    )
                }
                other => return Err(malformed(KIND, format!("unknown url rule {other:?}"))),
            };
            system(S::KnownUrl {
                rule,
                has: boolean(KIND, has, "has")?,
            })
        }
        29 => {
            let [view_type, canvases, op, value] = tuple::<4>(KIND, value, "viewing stats")?;
            let (stat, value) = match string(KIND, view_type, "view type")?.as_str() {
                "views" => (ViewingStat::Views, count_value(value, "view count")?),
                "viewtime" => viewtime_value(value)?,
                other => return Err(malformed(KIND, format!("unknown view type {other:?}"))),
            };
            let canvases = list(KIND, canvases, "canvases")?
                .iter()
                .map(|c| match string(KIND, c, "canvas")?.as_str() {
                    "media" => Ok(ViewCanvas::MediaViewer),
                    "preview" => Ok(ViewCanvas::Preview),
                    "client api" => Ok(ViewCanvas::ClientApi),
                    other => Err(malformed(KIND, format!("unknown canvas {other:?}"))),
                })
                .collect::<DecodeResult<BTreeSet<_>>>()?;
            system(S::FileViewingStats {
                stat,
                canvases: ViewCanvases::Specific(canvases),
                op: comparison(op)?,
                value,
            })
        }
        30 => {
            let container = nested(KIND, value, "or predicates")?;
            let items = list_items(&container)?;
            let predicates = items
                .iter()
                .map(|item| match item {
                    Meta::Object(object) => predicate_with_scales(object, scales),
                    _ => Err(malformed(KIND, "an OR member is not a predicate")),
                })
                .collect::<DecodeResult<_>>()?;
            Ok(Predicate::Or(predicates))
        }
        32 => system(S::BestQualityOfGroup {
            is_best: boolean(KIND, value, "is best")?,
        }),
        34 | 41 | 44 | 46 | 50 | 51 | 67 | 68 | 69 => system(S::FileProperty {
            property: match kind {
                34 => FileProperty::Audio,
                41 => FileProperty::IccProfile,
                44 => FileProperty::HumanReadableEmbeddedMetadata,
                46 => FileProperty::Exif,
                50 => FileProperty::Transparency,
                51 => FileProperty::ForcedFiletype,
                67 => FileProperty::Xmp,
                68 => FileProperty::Iptc,
                _ => FileProperty::SoftwareSourceMetadata,
            },
            // None meant "has"
            has: match value {
                PyJson::Null => true,
                other => boolean(KIND, other, "has")?,
            },
        }),
        40 => {
            let [has, name] = tuple::<2>(KIND, value, "note name")?;
            system(S::NoteName {
                name: string(KIND, name, "note name")?,
                has: boolean(KIND, has, "has")?,
            })
        }
        48 => {
            let [pixels, perceptual, distance] = tuple::<3>(KIND, value, "similar to data")?;
            system(S::SimilarToData {
                pixel_hashes: list(KIND, pixels, "pixel hashes")?
                    .iter()
                    .map(hex_hash)
                    .collect::<DecodeResult<_>>()?,
                perceptual_hashes: list(KIND, perceptual, "perceptual hashes")?
                    .iter()
                    .map(hex_hash)
                    .collect::<DecodeResult<_>>()?,
                max_distance: count_value(distance, "distance")?,
            })
        }
        54 => {
            let [service, display, statuses, tag] = tuple::<4>(KIND, value, "tag advanced")?;
            let service = match service {
                PyJson::Null => None,
                other => Some(ServiceRef::Key(service_key(KIND, other, "tag service")?)),
            };
            system(S::TagAdvanced {
                service,
                display: match int(KIND, display, "tag display type")? {
                    0 => TagDisplayType::Storage,
                    1 => TagDisplayType::Display,
                    other => {
                        return Err(malformed(KIND, format!("unknown tag display type {other}")));
                    }
                },
                statuses: list(KIND, statuses, "statuses")?
                    .iter()
                    .map(|s| code_of(s, "status", ContentStatus::from_code))
                    .collect::<DecodeResult<_>>()?,
                tag: Tag::from_clean(string(KIND, tag, "tag")?),
                inclusive,
            })
        }
        2 | 7 | 25 | 31 | 33 | 39 | 42 | 45 | 49 | 53 => Err(malformed(
            KIND,
            format!("predicate type {kind} is a menu placeholder, not a search"),
        )),
        other => Err(malformed(KIND, format!("unknown predicate type {other}"))),
    }
}

fn parse_hashes<T: FromStr + Ord>(hashes: &[String]) -> DecodeResult<BTreeSet<T>> {
    hashes
        .iter()
        .map(|h| {
            h.parse()
                .map_err(|_| malformed(KIND, format!("bad hash {h:?}")))
        })
        .collect()
}

fn hex_hash<T: FromStr>(value: &PyJson) -> DecodeResult<T> {
    let text = string(KIND, value, "hash")?;
    text.parse()
        .map_err(|_| malformed(KIND, format!("bad hash {text:?}")))
}

fn code_of<T>(value: &PyJson, what: &str, from: impl Fn(u8) -> Option<T>) -> DecodeResult<T> {
    let code = int(KIND, value, what)?;
    u8::try_from(code)
        .ok()
        .and_then(from)
        .ok_or_else(|| malformed(KIND, format!("unknown {what} {code}")))
}

/// A whole, non-negative number (stored as an int, or a float with no
/// fractional part).
fn count_value(value: &PyJson, what: &str) -> DecodeResult<u64> {
    let n = match value {
        PyJson::Int(n) => *n as f64,
        other => float(KIND, other, what)?,
    };
    if n < 0.0 || n.fract() != 0.0 || n > u64::MAX as f64 {
        return Err(malformed(
            KIND,
            format!("{what} {n} is not a whole number we can hold"),
        ));
    }
    Ok(n as u64)
}

/// A non-negative viewing time; the editor stores fractional seconds to ms.
#[allow(clippy::float_cmp)] // whole values retain the previous serialized unit
fn viewtime_value(value: &PyJson) -> DecodeResult<(ViewingStat, u64)> {
    let n = match value {
        PyJson::Int(n) => *n as f64,
        other => float(KIND, other, "viewing time")?,
    };
    if n.fract() == 0.0 {
        return Ok((ViewingStat::ViewTime, count_value(value, "viewing time")?));
    }
    if !(0.0..=u64::MAX as f64).contains(&n) {
        return Err(malformed(
            KIND,
            format!("viewing time {n} is not a non-negative number we can hold"),
        ));
    }
    let milliseconds = (n * 1000.0).round();
    if milliseconds > u64::MAX as f64 {
        return Err(malformed(KIND, "viewing time does not fit in milliseconds"));
    }
    Ok(ViewingStat::from_viewtime_milliseconds(milliseconds as u64))
}

/// A stored rating test: `"rated"` or `"not rated"`, a count (an int), or
/// a rating (a float, a fraction of the service's scale).
#[allow(clippy::float_cmp)] // the ends of a scale are stored exactly
fn rating_test(op: &PyJson, value: &PyJson, scale: Option<StarScale>) -> DecodeResult<RatingTest> {
    let op = match string(KIND, op, "operator")?.as_str() {
        "=" => RatingOp::Equal,
        "<" => RatingOp::Less,
        ">" => RatingOp::Greater,
        "\u{2264}" => RatingOp::LessOrEqual,
        "\u{2265}" => RatingOp::GreaterOrEqual,
        "\u{2248}" => RatingOp::Approx,
        other => {
            return Err(malformed(
                KIND,
                format!("unknown rating operator {other:?}"),
            ));
        }
    };
    match value {
        PyJson::Str(s) if s == "rated" => Ok(RatingTest::Rated),
        PyJson::Str(s) if s == "not rated" => Ok(RatingTest::NotRated),
        PyJson::Int(_) => Ok(RatingTest::Count {
            op,
            value: count_value(value, "rating")?,
        }),
        PyJson::Float(rating) => match scale {
            Some((num_stars, allow_zero)) => Ok(RatingTest::Stars {
                op,
                stars: rating_to_stars(num_stars, allow_zero, *rating),
                out_of: num_stars,
            }),
            None if op == RatingOp::Equal && *rating == 1.0 => Ok(RatingTest::Liked),
            None if op == RatingOp::Equal && *rating == 0.0 => Ok(RatingTest::Disliked),
            None => Err(malformed(
                KIND,
                "a rating between the ends of a scale needs its service's number of stars",
            )),
        },
        other => Err(malformed(KIND, format!("unknown rating {other:?}"))),
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

/// A stored `ServiceSpecifier`: service types, or else service keys.
fn service_selection(value: &PyJson) -> DecodeResult<ServiceSelection> {
    let object = nested(KIND, value, "service specifier")?;
    object.expect_kind(SerialisableType::SERVICE_SPECIFIER)?;
    let info = object.info();
    let [types, keys] = tuple::<2>(SerialisableType::SERVICE_SPECIFIER, &info, "specifier")?;
    let types = list(KIND, types, "service types")?;
    if types.is_empty() {
        Ok(ServiceSelection::Keys(
            service_keys(KIND, keys, "service keys")?
                .into_iter()
                .collect(),
        ))
    } else {
        Ok(ServiceSelection::Types(
            types
                .iter()
                .map(|t| code_of(t, "service type", ServiceType::from_code))
                .collect::<DecodeResult<_>>()?,
        ))
    }
}

fn comparison(value: &PyJson) -> DecodeResult<Comparison> {
    match string(KIND, value, "operator")?.as_str() {
        "<" => Ok(Comparison::Less),
        ">" => Ok(Comparison::Greater),
        "=" => Ok(Comparison::Equal),
        "\u{2260}" => Ok(Comparison::NotEqual),
        "\u{2248}" => Ok(Comparison::Approx),
        other => Err(malformed(KIND, format!("unknown operator {other:?}"))),
    }
}

fn time_test(value: &PyJson) -> DecodeResult<TimeTest> {
    let items = list(KIND, value, "time")?;
    // v1: (operator, years, months, days, hours)
    let (op, age_type, age): (&PyJson, &str, Vec<i64>) = match items {
        [op, y, m, d, h] if !matches!(y, PyJson::Str(_)) => (
            op,
            "delta",
            [y, m, d, h]
                .iter()
                .map(|v| int(KIND, v, "age"))
                .collect::<DecodeResult<_>>()?,
        ),
        [op, age_type, age] => (
            op,
            age_type.as_str().unwrap_or(""),
            list(KIND, age, "age")?
                .iter()
                .map(|v| int(KIND, v, "age"))
                .collect::<DecodeResult<_>>()?,
        ),
        _ => return Err(malformed(KIND, "time should have 3 items")),
    };
    let op = string(KIND, op, "operator")?;
    let unsigned =
        |n: i64| u32::try_from(n).map_err(|_| malformed(KIND, format!("negative time part {n}")));
    match (age_type, age.as_slice()) {
        ("delta", [years, months, days, hours]) => Ok(TimeTest::Relative {
            op: match op.as_str() {
                "<" => RelativeOp::Less,
                ">" => RelativeOp::Greater,
                "\u{2248}" => RelativeOp::Approx,
                "\u{2260}" => RelativeOp::NotEqual,
                other => return Err(malformed(KIND, format!("unknown operator {other:?}"))),
            },
            age: CalendarDelta {
                years: unsigned(*years)?,
                months: unsigned(*months)?,
                days: unsigned(*days)?,
                hours: unsigned(*hours)?,
                ..CalendarDelta::ZERO
            },
        }),
        // before v6, dates had no time of day
        ("date", [year, month, day] | [year, month, day, _, _]) => {
            let (hour, minute) = match age.as_slice() {
                [_, _, _, h, m] => (*h, *m),
                _ => (0, 0),
            };
            let part = |n: i64| u8::try_from(n).ok();
            let at = u16::try_from(*year)
                .ok()
                .zip(
                    part(*month)
                        .zip(part(*day))
                        .zip(part(hour).zip(part(minute))),
                )
                .and_then(|(y, ((mo, d), (h, mi)))| CivilDateTime::new(y, mo, d, h, mi))
                .ok_or_else(|| malformed(KIND, format!("bad date {age:?}")))?;
            Ok(TimeTest::Absolute {
                op: comparison(&PyJson::Str(op))?,
                at,
            })
        }
        (other, _) => Err(malformed(KIND, format!("unknown age {other:?} {age:?}"))),
    }
}

/// A number test value: v8 stores a `NumberTest` object, v1-7 an
/// `(operator, value)` pair.
fn stored_number_test(version: u32, kind: i64, value: &PyJson) -> DecodeResult<NumberTest> {
    if version >= 8 {
        return number_test(&nested(KIND, value, "number test")?);
    }
    // NumberTest.STATICCreateFromCharacters, and v7's special cases
    let [op, n] = tuple::<2>(KIND, value, "number")?;
    let op = string(KIND, op, "operator")?;
    let n = count_value(n, "number")?;
    let duration_or_framerate = kind == 16 || kind == 36;
    let op = match op.as_str() {
        "=" if duration_or_framerate => NumberOp::ApproxPercent { percent: 5 },
        "\u{2260}" if duration_or_framerate => NumberOp::Less,
        "<" => NumberOp::Less,
        ">" => NumberOp::Greater,
        "=" => NumberOp::Equal,
        "\u{2260}" => NumberOp::NotEqual,
        "\u{2248}" => NumberOp::ApproxPercent { percent: 15 },
        "\u{2264}" => NumberOp::LessOrEqual,
        "\u{2265}" => NumberOp::GreaterOrEqual,
        other => return Err(malformed(KIND, format!("unknown operator {other:?}"))),
    };
    Ok(NumberTest::new(op, n))
}

/// A stored `NumberTest` (v1-2).
pub fn number_test(object: &SerialisableObject) -> DecodeResult<NumberTest> {
    let kind = SerialisableType::NUMBER_TEST;
    object.expect_kind(kind)?;
    object.check_not_future()?;
    let info = object.info();
    let items = list(kind, &info, "number test")?;
    let (op, value, extra) = match items {
        [op, value] => (op, value, None),
        [op, value, extra] => (op, value, Some(extra)),
        _ => return Err(malformed(kind, "number test should have 3 items")),
    };
    let value = count_value(value, "number test value")?;
    let extra = |default: f64| -> DecodeResult<f64> {
        match extra {
            None | Some(PyJson::Null) => Ok(default),
            Some(PyJson::Int(n)) => Ok(*n as f64),
            Some(other) => float(kind, other, "number test range"),
        }
    };
    let op = match int(kind, op, "operator")? {
        0 => NumberOp::Less,
        1 => NumberOp::Greater,
        2 => NumberOp::Equal,
        3 => {
            let percent = extra(0.15)? * 100.0;
            if percent.fract().abs() > 1e-6 && (percent.fract() - 1.0).abs() > 1e-6 {
                return Err(malformed(kind, format!("a {percent}% range")));
            }
            NumberOp::ApproxPercent {
                percent: percent.round() as u32,
            }
        }
        4 => NumberOp::NotEqual,
        5 => {
            let tolerance = extra(1.0)?;
            if tolerance < 0.0 || tolerance.fract() != 0.0 {
                return Err(malformed(kind, format!("a range of {tolerance}")));
            }
            NumberOp::ApproxAbsolute {
                tolerance: tolerance as u64,
            }
        }
        6 => NumberOp::LessOrEqual,
        7 => NumberOp::GreaterOrEqual,
        other => return Err(malformed(kind, format!("unknown operator {other}"))),
    };
    Ok(NumberTest::new(op, value))
}
