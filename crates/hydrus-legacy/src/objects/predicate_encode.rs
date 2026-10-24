//! Search predicates in the reference's serialised form
//! (`SERIALISABLE_TYPE_PREDICATE` v8): the inverse of
//! [`crate::objects::predicates`], for exporting what the reference can read.
//!
//! The tuple is `[14, 8, [predicate type, value, inclusive]]`, with the value
//! shaped as the reference's editors and parser make it. What the text
//! parser leaves open is settled from the client, as the reference's parser
//! settles it ([`Context`]): a service named is written by its key, viewing
//! stats on the default canvases name the "interesting" canvases, and a
//! star rating is a fraction of its service's scale. An age's minutes and
//! seconds join its hours, rounded, as the reference's parser rounds a
//! relative date. A URL class rule can't be written (it needs the URL class
//! object, which the predicate does not hold), nor a service name the client
//! does not have: those are errors.

use hydrus_core::search::number::{NumberOp, NumberTest, RatingOp};
use hydrus_core::search::predicate::{
    FileHashes, FileProperty, NumericProperty, Predicate, RatingLogic, RatingTest, ServiceRef,
    ServiceSelection, SystemPredicate, TagDisplayType, UrlRule, ViewCanvas, ViewCanvases,
    ViewingStat,
};
use hydrus_core::search::time::{TimeKind, TimeTest};
use hydrus_core::{ServiceKey, ServiceType};

use crate::objects::predicates::StarScale;
use crate::pyjson::PyJson;

const PREDICATE: i64 = 14;
const PREDICATE_VERSION: i64 = 8;
const NUMBER_TEST: i64 = 93;
const SERVICE_SPECIFIER: i64 = 142;
const LIST: i64 = 26;

fn str(s: impl Into<String>) -> PyJson {
    PyJson::Str(s.into())
}

fn int(n: impl Into<i64>) -> PyJson {
    PyJson::Int(n.into())
}

fn count(n: u64) -> Result<PyJson, String> {
    i64::try_from(n)
        .map(PyJson::Int)
        .map_err(|_| format!("{n} is too big to export"))
}

fn list(items: Vec<PyJson>) -> PyJson {
    PyJson::List(items)
}

/// What the encoder looks up in the client.
pub struct Context<'a> {
    /// A numerical rating service's scale, for star ratings.
    pub scales: &'a dyn Fn(&ServiceKey) -> Option<StarScale>,
    /// A service's key from its name, for predicates naming services.
    pub service_key: &'a dyn Fn(&str) -> Option<ServiceKey>,
    /// A service's type, for ratings on an inc/dec service.
    pub service_type: &'a dyn Fn(&ServiceKey) -> Option<ServiceType>,
    /// The "interesting" canvases (`file_viewing_stats_interesting_canvas_types`),
    /// in the options' order, for viewing stats on the default canvases.
    pub default_canvases: &'a [ViewCanvas],
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("default_canvases", &self.default_canvases)
            .finish_non_exhaustive()
    }
}

fn service_key(service: &ServiceRef, ctx: &Context<'_>) -> Result<ServiceKey, String> {
    match service {
        ServiceRef::Key(key) => Ok(key.clone()),
        ServiceRef::Name(name) => {
            (ctx.service_key)(name).ok_or_else(|| format!("there is no service \"{name}\""))
        }
    }
}

fn key(service: &ServiceRef, ctx: &Context<'_>) -> Result<PyJson, String> {
    Ok(str(service_key(service, ctx)?.to_hex()))
}

fn canvas_name(c: ViewCanvas) -> PyJson {
    str(match c {
        ViewCanvas::MediaViewer => "media",
        ViewCanvas::Preview => "preview",
        ViewCanvas::ClientApi => "client api",
    })
}

/// A `NumberTest` object (`[93, 2, [operator, value, extra]]`).
///
/// # Errors
/// If the value is too big for the reference.
pub fn number_test(test: &NumberTest) -> Result<PyJson, String> {
    let (op, extra) = match test.op {
        NumberOp::Less => (0, PyJson::Null),
        NumberOp::Greater => (1, PyJson::Null),
        NumberOp::Equal => (2, PyJson::Null),
        NumberOp::ApproxPercent { percent } => (3, PyJson::Float(f64::from(percent) / 100.0)),
        NumberOp::NotEqual => (4, PyJson::Null),
        NumberOp::ApproxAbsolute { tolerance } => (5, count(tolerance)?),
        NumberOp::LessOrEqual => (6, PyJson::Null),
        NumberOp::GreaterOrEqual => (7, PyJson::Null),
    };
    Ok(list(vec![
        int(NUMBER_TEST),
        int(2),
        list(vec![int(op), count(test.value)?, extra]),
    ]))
}

/// The predicate type code a numeric property is stored under.
pub const fn numeric_property_type(property: NumericProperty) -> i64 {
    match property {
        NumericProperty::Width => 13,
        NumericProperty::Height => 14,
        NumericProperty::Duration => 16,
        NumericProperty::NumWords => 22,
        NumericProperty::Framerate => 36,
        NumericProperty::NumFrames => 37,
        NumericProperty::NumNotes => 38,
        NumericProperty::NumUrls => 52,
    }
}

/// A predicate with no value (`[14, 8, [type, null, true]]`), as the
/// reference's relative file info comparator names its property.
pub fn bare(predicate_type: i64) -> PyJson {
    wrap(predicate_type, PyJson::Null, true)
}

fn wrap(predicate_type: i64, value: PyJson, inclusive: bool) -> PyJson {
    list(vec![
        int(PREDICATE),
        int(PREDICATE_VERSION),
        list(vec![int(predicate_type), value, PyJson::Bool(inclusive)]),
    ])
}

fn rating_op(op: RatingOp) -> &'static str {
    match op {
        RatingOp::Equal => "=",
        RatingOp::Less => "<",
        RatingOp::Greater => ">",
        RatingOp::LessOrEqual => "\u{2264}",
        RatingOp::GreaterOrEqual => "\u{2265}",
        RatingOp::Approx => "\u{2248}",
    }
}

/// `ClientRatings.ConvertStarsToRating`.
#[allow(clippy::cast_precision_loss)] // (star counts are small)
fn stars_to_rating(stars: u64, out_of: u64, allow_zero: bool) -> f64 {
    if allow_zero {
        stars as f64 / out_of as f64
    } else if out_of <= 1 {
        1.0
    } else {
        stars.saturating_sub(1) as f64 / (out_of - 1) as f64
    }
}

fn service_selection(
    selection: Option<&ServiceSelection>,
    ctx: &Context<'_>,
) -> Result<PyJson, String> {
    let (types, keys) = match selection {
        None => (Vec::new(), Vec::new()),
        Some(ServiceSelection::Types(types)) => {
            let mut codes: Vec<u8> = types.iter().map(|t| t.code()).collect();
            codes.sort_unstable();
            (codes.into_iter().map(int).collect(), Vec::new())
        }
        Some(ServiceSelection::Keys(keys)) => (
            Vec::new(),
            keys.iter().map(|k: &ServiceKey| str(k.to_hex())).collect(),
        ),
        Some(ServiceSelection::Names(names)) => {
            let mut keys = names
                .iter()
                .map(|n| service_key(&ServiceRef::Name(n.clone()), ctx).map(|k| k.to_hex()))
                .collect::<Result<Vec<_>, _>>()?;
            keys.sort();
            (Vec::new(), keys.into_iter().map(str).collect())
        }
    };
    Ok(list(vec![
        int(SERVICE_SPECIFIER),
        int(1),
        list(vec![list(types), list(keys)]),
    ]))
}

/// A predicate in the reference's serialised form, with what it leaves open
/// looked up in `ctx`.
///
/// # Errors
/// If the predicate holds something the reference's form needs and the
/// native one does not keep (see the module notes).
#[allow(clippy::too_many_lines)]
pub fn predicate(p: &Predicate, ctx: &Context<'_>) -> Result<PyJson, String> {
    use SystemPredicate as S;
    Ok(match p {
        Predicate::Tag { tag, inclusive } => wrap(0, str(tag.as_str()), *inclusive),
        Predicate::Namespace {
            namespace,
            inclusive,
        } => wrap(1, str(namespace.as_str()), *inclusive),
        Predicate::Wildcard { pattern, inclusive } => wrap(3, str(pattern.as_str()), *inclusive),
        Predicate::Or(members) => {
            let members = members
                .iter()
                .map(|m| Ok(list(vec![int(2), predicate(m, ctx)?])))
                .collect::<Result<Vec<_>, String>>()?;
            wrap(30, list(vec![int(LIST), int(3), list(members)]), true)
        }
        Predicate::System(s) => match s {
            S::Everything => bare(4),
            S::Inbox => bare(5),
            S::Archive => bare(6),
            S::Local => bare(20),
            S::NotLocal => bare(21),
            S::Limit(n) => wrap(9, count(*n)?, true),
            S::Filetype {
                filetypes,
                inclusive,
            } => wrap(
                17,
                list(filetypes.summary().iter().map(|m| int(m.code())).collect()),
                *inclusive,
            ),
            S::Hash { hashes, inclusive } => {
                let kind = match hashes {
                    FileHashes::Sha256(_) => "sha256",
                    FileHashes::Md5(_) => "md5",
                    FileHashes::Sha1(_) => "sha1",
                    FileHashes::Sha512(_) => "sha512",
                };
                wrap(
                    12,
                    list(vec![
                        list(hashes.to_hex().into_iter().map(str).collect()),
                        str(kind),
                    ]),
                    *inclusive,
                )
            }
            S::SimilarToFiles {
                files,
                max_distance,
            } => wrap(
                19,
                list(vec![
                    list(files.iter().map(|h| str(h.to_hex())).collect()),
                    count(*max_distance)?,
                ]),
                true,
            ),
            S::SimilarToData {
                pixel_hashes,
                perceptual_hashes,
                max_distance,
            } => wrap(
                48,
                list(vec![
                    list(pixel_hashes.iter().map(|h| str(h.to_hex())).collect()),
                    list(perceptual_hashes.iter().map(|h| str(h.to_hex())).collect()),
                    count(*max_distance)?,
                ]),
                true,
            ),
            S::FileProperty { property, has } => wrap(
                match property {
                    FileProperty::Audio => 34,
                    FileProperty::IccProfile => 41,
                    FileProperty::HumanReadableEmbeddedMetadata => 44,
                    FileProperty::Exif => 46,
                    FileProperty::Transparency => 50,
                    FileProperty::ForcedFiletype => 51,
                    FileProperty::Xmp => 67,
                    FileProperty::Iptc => 68,
                    FileProperty::SoftwareSourceMetadata => 69,
                },
                PyJson::Bool(*has),
                true,
            ),
            S::Number { property, test } => {
                wrap(numeric_property_type(*property), number_test(test)?, true)
            }
            S::NumTags {
                namespace,
                op,
                count: n,
            } => wrap(
                8,
                list(vec![
                    str(namespace.as_reference()),
                    str(op.symbol()),
                    count(*n)?,
                ]),
                true,
            ),
            S::FileSize { op, size, unit } => wrap(
                10,
                list(vec![str(op.symbol()), count(*size)?, count(unit.bytes())?]),
                true,
            ),
            S::NumPixels { op, count: n, unit } => wrap(
                24,
                list(vec![str(op.symbol()), count(*n)?, count(unit.pixels())?]),
                true,
            ),
            S::Ratio { op, width, height } => wrap(
                15,
                list(vec![str(op.symbol()), count(*width)?, count(*height)?]),
                true,
            ),
            S::Time { kind, test } => wrap(
                match kind {
                    TimeKind::Imported => 11,
                    TimeKind::Modified => 35,
                    TimeKind::LastViewed => 43,
                    TimeKind::Archived => 47,
                },
                time_test(test)?,
                true,
            ),
            S::FileService {
                service,
                status,
                is_in,
            } => wrap(
                23,
                list(vec![
                    PyJson::Bool(*is_in),
                    int(status.code()),
                    key(service, ctx)?,
                ]),
                true,
            ),
            S::FileRelationshipCount {
                op,
                count: n,
                relationship,
            } => wrap(
                26,
                list(vec![
                    str(op.symbol()),
                    count(*n)?,
                    int(relationship.duplicate_type().code()),
                ]),
                true,
            ),
            S::BestQualityOfGroup { is_best } => wrap(32, PyJson::Bool(*is_best), true),
            S::FileViewingStats {
                stat,
                canvases,
                op,
                value,
            } => {
                // (the default: the "interesting" canvases, as the
                // reference's parser fills them in)
                let canvases = match canvases {
                    ViewCanvases::Specific(canvases) => {
                        canvases.iter().copied().map(canvas_name).collect()
                    }
                    ViewCanvases::Default => ctx
                        .default_canvases
                        .iter()
                        .copied()
                        .map(canvas_name)
                        .collect(),
                };
                let (kind, value) = match stat {
                    ViewingStat::Views => ("views", count(*value)?),
                    ViewingStat::ViewTime => ("viewtime", count(*value)?),
                    #[allow(clippy::cast_precision_loss)] // (milliseconds)
                    ViewingStat::ViewTimeMilliseconds => {
                        ("viewtime", PyJson::Float(*value as f64 / 1000.0))
                    }
                };
                wrap(
                    29,
                    list(vec![str(kind), list(canvases), str(op.symbol()), value]),
                    true,
                )
            }
            S::KnownUrl { rule, has } => {
                // the descriptions the reference's parser and editors write
                let (kind, rule, description) = match (rule, *has) {
                    (UrlRule::ExactMatch(url), true) => {
                        ("exact_match", url, format!("has url {url}"))
                    }
                    (UrlRule::ExactMatch(url), false) => {
                        ("exact_match", url, format!("does not have url {url}"))
                    }
                    (UrlRule::Domain(domain), true) => {
                        ("domain", domain, format!("has url with domain {domain}"))
                    }
                    (UrlRule::Domain(domain), false) => (
                        "domain",
                        domain,
                        format!("does not have url with domain {domain}"),
                    ),
                    (UrlRule::Regex(regex), true) => {
                        ("regex", regex, format!("has url matching regex {regex}"))
                    }
                    (UrlRule::Regex(regex), false) => (
                        "regex",
                        regex,
                        format!("does not have url matching regex {regex}"),
                    ),
                    (UrlRule::UrlClass(name), _) => {
                        return Err(format!(
                            "the url class \"{name}\" is known by name, not by its definition"
                        ));
                    }
                };
                wrap(
                    28,
                    list(vec![
                        PyJson::Bool(*has),
                        str(kind),
                        str(rule.as_str()),
                        str(description),
                    ]),
                    true,
                )
            }
            S::TagAsNumber {
                namespace,
                op,
                value,
            } => wrap(
                27,
                list(vec![
                    str(namespace.as_reference()),
                    str(op.symbol()),
                    int(*value),
                ]),
                true,
            ),
            S::NoteName { name, has } => {
                wrap(40, list(vec![PyJson::Bool(*has), str(name.as_str())]), true)
            }
            S::Rating { service, test } => {
                let resolved = service_key(service, ctx)?;
                let service_key = str(resolved.to_hex());
                // (on an inc/dec service the reference's parser writes
                // "rated" as "> 0" and "not rated" as "= 0")
                let incdec = (ctx.service_type)(&resolved) == Some(ServiceType::LocalRatingIncDec);
                let (op, value) = match test {
                    RatingTest::Rated if incdec => (">", int(0)),
                    RatingTest::NotRated if incdec => ("=", int(0)),
                    RatingTest::Rated => ("=", str("rated")),
                    RatingTest::NotRated => ("=", str("not rated")),
                    RatingTest::Liked => ("=", PyJson::Float(1.0)),
                    RatingTest::Disliked => ("=", PyJson::Float(0.0)),
                    RatingTest::Count { op, value } => (rating_op(*op), count(*value)?),
                    RatingTest::Stars { op, stars, out_of } => {
                        let (_, allow_zero) = (ctx.scales)(&resolved)
                            .ok_or_else(|| "a star rating needs its service's scale".to_owned())?;
                        (
                            rating_op(*op),
                            PyJson::Float(stars_to_rating(*stars, *out_of, allow_zero)),
                        )
                    }
                };
                wrap(18, list(vec![str(op), value, service_key]), true)
            }
            S::RatingAdvanced {
                logic,
                services,
                rated,
            } => {
                let (code, secondary) = match logic {
                    RatingLogic::All => (0, None),
                    RatingLogic::Any => (1, None),
                    RatingLogic::Only { amongst } => (2, Some(amongst)),
                };
                wrap(
                    66,
                    list(vec![
                        int(code),
                        service_selection(Some(services), ctx)?,
                        service_selection(secondary, ctx)?,
                        PyJson::Bool(*rated),
                    ]),
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
                let mut codes: Vec<u8> = statuses.iter().map(|s| s.code()).collect();
                codes.sort_unstable();
                wrap(
                    54,
                    list(vec![
                        match service {
                            None => PyJson::Null,
                            Some(s) => key(s, ctx)?,
                        },
                        int(match display {
                            TagDisplayType::Storage => 0,
                            TagDisplayType::Display => 1,
                        }),
                        list(codes.into_iter().map(int).collect()),
                        str(tag.as_str()),
                    ]),
                    *inclusive,
                )
            }
        },
    })
}

fn time_test(test: &TimeTest) -> Result<PyJson, String> {
    Ok(match test {
        TimeTest::Relative { op, age } => {
            // (the reference keeps whole hours: its parser rounds a relative
            // date's time of day to them, `round( seconds / 3600 )`)
            let (days, hours) = if age.minutes == 0 && age.seconds == 0 {
                (u64::from(age.days), i64::from(age.hours))
            } else {
                let seconds = u64::from(age.hours) * 3600
                    + u64::from(age.minutes) * 60
                    + u64::from(age.seconds);
                #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
                let hours = ((seconds % 86_400) as f64 / 3600.0).round_ties_even() as i64;
                (u64::from(age.days) + seconds / 86_400, hours)
            };
            list(vec![
                str(op.symbol()),
                str("delta"),
                list(vec![
                    int(age.years),
                    int(age.months),
                    count(days)?,
                    int(hours),
                ]),
            ])
        }
        TimeTest::Absolute { op, at } => list(vec![
            str(op.symbol()),
            str("date"),
            list(vec![
                int(at.year()),
                int(at.month()),
                int(at.day()),
                int(at.hour()),
                int(at.minute()),
            ]),
        ]),
    })
}
