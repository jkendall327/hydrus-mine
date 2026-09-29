//! The `system:` predicate text parser.
//!
//! Parsing follows the reference implementation's pipeline, because its
//! quirks *are* the accepted language:
//!
//! 1. Trim the text (Python's definition of whitespace), lowercase it, and
//!    require a `system:` prefix.
//! 2. Find the predicate name (`names`): the first entry of an ordered
//!    pattern table that matches at the start.
//! 3. Parse, in turn, an operator (`operators`), a value (`values`,
//!    `date`, `rating`, `tag_advanced`) and a unit, each in the style
//!    that predicate uses, each consuming text from the front.
//! 4. Reject any text left over.
//!
//! Free-text values that are matched case-sensitively when searching (URL
//! regexes, exact URLs and note names) are taken from the text as typed, not
//! the lowercased copy; see `docs/rust/DIFFERENCES.md`.

mod date;
mod names;
mod operators;
mod rating;
mod tag_advanced;
mod text;
mod values;

use std::collections::BTreeSet;

use crate::error::{ParseError, ParseErrorKind};
use crate::number::{Comparison, NumberTest, RatioOp, TagNumberOp};
use crate::predicate::{
    NamespaceFilter, NumericProperty, RatingTest, ServiceRef, SystemPredicate, UrlRule, ViewCanvas,
    ViewCanvases, ViewingStat,
};
use names::{Canvases, Name};
use operators::{Result, expected, skip_separators};
use text::{SystemText, py_strip};

/// Parse one `system:` predicate, e.g. `system:width > 1920` or
/// `system:import time < 7 days`.
///
/// The text is matched case-insensitively. Services and URL classes named
/// in the text are returned by name, to be resolved when the search runs.
pub fn parse_system_predicate(input: &str) -> std::result::Result<SystemPredicate, ParseError> {
    parse(input).map_err(|kind| ParseError {
        input: input.to_owned(),
        kind,
    })
}

fn parse(input: &str) -> Result<SystemPredicate> {
    let text = SystemText::new(input)?;
    let subtag = text.subtag();
    let (name, name_len) = names::match_name(subtag).ok_or(ParseErrorKind::UnknownPredicate)?;
    Predicates { text: &text }.parse(name, &subtag[name_len..])
}

/// Reject leftover text (after trimming).
fn finish(rest: &str) -> Result<()> {
    let rest = py_strip(rest);
    if rest.is_empty() {
        Ok(())
    } else {
        Err(ParseErrorKind::TrailingText(rest.to_owned()))
    }
}

fn number(property: NumericProperty, test: NumberTest) -> SystemPredicate {
    SystemPredicate::Number { property, test }
}

struct Predicates<'t> {
    text: &'t SystemText,
}

impl Predicates<'_> {
    fn parse(&self, name: Name, rest: &str) -> Result<SystemPredicate> {
        use NumericProperty as N;
        Ok(match name {
            Name::Everything => {
                finish(rest)?;
                SystemPredicate::Everything
            }
            Name::Inbox => {
                finish(rest)?;
                SystemPredicate::Inbox
            }
            Name::Archive => {
                finish(rest)?;
                SystemPredicate::Archive
            }
            Name::Has(property) => {
                finish(rest)?;
                number(property, NumberTest::nonzero())
            }
            Name::HasNo(property) => {
                finish(rest)?;
                number(property, NumberTest::zero())
            }
            Name::BestQuality(is_best) => {
                finish(rest)?;
                SystemPredicate::BestQualityOfGroup { is_best }
            }
            Name::Property(property, has) => {
                finish(rest)?;
                SystemPredicate::FileProperty { property, has }
            }
            Name::HasTags => {
                finish(rest)?;
                SystemPredicate::NumTags {
                    namespace: NamespaceFilter::Any,
                    op: Comparison::Greater,
                    count: 0,
                }
            }
            Name::Untagged => {
                finish(rest)?;
                SystemPredicate::NumTags {
                    namespace: NamespaceFilter::Any,
                    op: Comparison::Equal,
                    count: 0,
                }
            }
            Name::NumTags => {
                let (rest, op) = operators::relational(skip_separators(rest))?;
                let (rest, count) = values::natural(rest)?;
                finish(rest)?;
                SystemPredicate::NumTags {
                    namespace: NamespaceFilter::Any,
                    op,
                    count,
                }
            }
            Name::NumTagsInNamespace => {
                let (namespace, op, count) = values::namespace_tag_count(rest)?;
                SystemPredicate::NumTags {
                    namespace,
                    op,
                    count,
                }
            }
            Name::NumUrls => Self::legacy_number(N::NumUrls, rest, &[])?,
            Name::Framerate => Self::legacy_number(N::Framerate, rest, &["fps"])?,
            Name::NumWords => Self::number_test(N::NumWords, rest, &[])?,
            Name::NumFrames => Self::number_test(N::NumFrames, rest, &[])?,
            Name::Height => Self::number_test(N::Height, rest, &["pixels", "pixel", "px"])?,
            Name::Width => Self::number_test(N::Width, rest, &["pixels", "pixel", "px"])?,
            Name::Duration => {
                let (rest, op) = operators::number_test(skip_separators(rest))?;
                let (rest, value) = values::duration_ms(rest)?;
                finish(rest)?;
                number(N::Duration, NumberTest::new(op, value))
            }
            Name::NumNotes => {
                let (rest, op) = operators::relational_exact(skip_separators(rest))?;
                let (rest, value) = values::natural(rest)?;
                finish(rest)?;
                number(
                    N::NumNotes,
                    NumberTest::new(operators::comparison_to_number_op(op), value),
                )
            }
            Name::FileSize => {
                let (rest, op) = operators::relational(skip_separators(rest))?;
                let (rest, size) = values::natural(rest)?;
                let (rest, unit) = values::size_unit(rest)?;
                finish(rest)?;
                SystemPredicate::FileSize { op, size, unit }
            }
            Name::NumPixels => {
                let (rest, op) = operators::relational(skip_separators(rest))?;
                let (rest, count) = values::natural(rest)?;
                let (rest, unit) = values::pixel_unit(rest)?;
                finish(rest)?;
                SystemPredicate::NumPixels { op, count, unit }
            }
            Name::SimilarToFiles => {
                let (rest, files, max_distance) = values::similar_files(rest)?;
                finish(rest)?;
                SystemPredicate::SimilarToFiles {
                    files,
                    max_distance,
                }
            }
            Name::SimilarToData => {
                let (rest, pixel_hashes, perceptual_hashes, max_distance) =
                    values::similar_data(rest)?;
                finish(rest)?;
                SystemPredicate::SimilarToData {
                    pixel_hashes,
                    perceptual_hashes,
                    max_distance,
                }
            }
            Name::Limit => {
                let rest = operators::only_equal(skip_separators(rest))?;
                let (rest, limit) = values::natural(rest)?;
                finish(rest)?;
                SystemPredicate::Limit(limit)
            }
            Name::Filetype => {
                let (rest, inclusive) = operators::equality(skip_separators(rest))?;
                let (rest, filetypes) = values::filetype_list(rest)?;
                finish(rest)?;
                SystemPredicate::Filetype {
                    filetypes,
                    inclusive,
                }
            }
            Name::Hash => {
                let (rest, inclusive) = operators::equality(skip_separators(rest))?;
                // everything around the hashes is ignored, as in the reference
                let hashes = values::hash_list(rest, self.text.lower())?;
                SystemPredicate::Hash { hashes, inclusive }
            }
            Name::Time(kind) => SystemPredicate::Time {
                kind,
                test: date::time_test(skip_separators(rest))?,
            },
            Name::FileService => Self::file_service(rest)?,
            Name::NumFileRelationships => {
                let (rest, op) = operators::relational(skip_separators(rest))?;
                let (rest, count) = values::natural(rest)?;
                let (rest, relationship) = values::relationship(rest)?;
                finish(rest)?;
                SystemPredicate::FileRelationshipCount {
                    op,
                    count,
                    relationship,
                }
            }
            Name::Ratio => {
                let (rest, op) = ratio_operator(skip_separators(rest))?;
                let (rest, width, height) = values::ratio(rest)?;
                finish(rest)?;
                SystemPredicate::Ratio { op, width, height }
            }
            Name::RatioShape => {
                let rest = skip_separators(rest);
                let op = if rest.contains("square") {
                    RatioOp::Equal
                } else if rest.contains("portrait") {
                    RatioOp::TallerThan
                } else if rest.contains("landscape") {
                    RatioOp::WiderThan
                } else {
                    return Err(expected("square, portrait, landscape or a ratio", rest));
                };
                SystemPredicate::Ratio {
                    op,
                    width: 1,
                    height: 1,
                }
            }
            Name::Views(canvases) => {
                let (rest, op) = operators::relational(skip_separators(rest))?;
                let (rest, value) = values::natural(rest)?;
                finish(rest)?;
                viewing_stats(ViewingStat::Views, fixed_canvases(canvases), op, value)
            }
            Name::Viewtime(canvases) => {
                let (rest, op) = operators::relational(skip_separators(rest))?;
                let (rest, value) = values::interval_seconds(rest)?;
                finish(rest)?;
                viewing_stats(ViewingStat::ViewTime, fixed_canvases(canvases), op, value)
            }
            Name::ViewsIn | Name::ViewtimeIn => {
                let (remainder, canvases) = named_canvases(skip_separators(rest));
                let (rest, op) = operators::relational(&remainder)?;
                let (stat, (rest, value)) = if name == Name::ViewsIn {
                    (ViewingStat::Views, values::natural(rest)?)
                } else {
                    (ViewingStat::ViewTime, values::interval_seconds(rest)?)
                };
                finish(rest)?;
                viewing_stats(stat, canvases, op, value)
            }
            Name::UrlRegex(has) => SystemPredicate::KnownUrl {
                rule: UrlRule::Regex(self.as_typed(values::any_string(rest)).to_owned()),
                has,
            },
            Name::Url(has) => SystemPredicate::KnownUrl {
                rule: UrlRule::ExactMatch(self.as_typed(values::any_string(rest)).to_owned()),
                has,
            },
            Name::Domain(has) => SystemPredicate::KnownUrl {
                rule: UrlRule::Domain(values::any_string(rest).to_owned()),
                has,
            },
            Name::UrlClass(has) => SystemPredicate::KnownUrl {
                rule: UrlRule::UrlClass(values::any_string(rest).to_owned()),
                has,
            },
            Name::TagAsNumber => tag_as_number(skip_separators(rest))?,
            Name::Note(has) => SystemPredicate::NoteName {
                name: values::strip_quotes(self.as_typed(values::any_string(rest))).to_owned(),
                has,
            },
            Name::HasRating(rated) => SystemPredicate::Rating {
                service: ServiceRef::Name(values::any_string(rest).to_owned()),
                test: if rated {
                    RatingTest::Rated
                } else {
                    RatingTest::NotRated
                },
            },
            Name::RatingStars | Name::RatingCount => {
                let (text, sym) = rating::operator(skip_separators(rest))?;
                let (service, test) = if name == Name::RatingStars {
                    rating::stars(&text, sym)?
                } else {
                    rating::count(&text, sym)?
                };
                SystemPredicate::Rating {
                    service: ServiceRef::Name(service),
                    test,
                }
            }
            Name::RatingLike => {
                let (service, test) = rating::like(rest)?;
                SystemPredicate::Rating {
                    service: ServiceRef::Name(service),
                    test,
                }
            }
            Name::TagAdvanced(inclusive) => {
                let (options, tag_text) = tag_advanced::options(skip_separators(rest))?;
                SystemPredicate::TagAdvanced {
                    service: options.service,
                    display: options.display,
                    statuses: options.statuses,
                    tag: tag_advanced::tag(tag_text)?,
                    inclusive,
                }
            }
            Name::RatingAdvanced => {
                let (logic, services, rated) = rating::advanced(self.text.subtag())?;
                SystemPredicate::RatingAdvanced {
                    logic,
                    services,
                    rated,
                }
            }
        })
    }

    /// A predicate using the full number-test operator set, with an
    /// optional unit that means nothing.
    fn number_test(
        property: NumericProperty,
        rest: &str,
        units: &[&str],
    ) -> Result<SystemPredicate> {
        let (rest, op) = operators::number_test(skip_separators(rest))?;
        let (rest, value) = values::natural(rest)?;
        let rest = if units.is_empty() {
            rest
        } else {
            values::optional_unit(rest, units)?
        };
        finish(rest)?;
        Ok(number(property, NumberTest::new(op, value)))
    }

    /// A number-test predicate that only accepts the legacy operators.
    fn legacy_number(
        property: NumericProperty,
        rest: &str,
        units: &[&str],
    ) -> Result<SystemPredicate> {
        let (rest, op) = operators::relational(skip_separators(rest))?;
        let (rest, value) = values::natural(rest)?;
        let rest = if units.is_empty() {
            rest
        } else {
            values::optional_unit(rest, units)?
        };
        finish(rest)?;
        Ok(number(
            property,
            NumberTest::new(operators::comparison_to_number_op(op), value),
        ))
    }

    fn file_service(rest: &str) -> Result<SystemPredicate> {
        use hydrus_core::ContentStatus;
        static STATUS: std::sync::LazyLock<[(regex::Regex, bool, ContentStatus); 4]> =
            std::sync::LazyLock::new(|| {
                [
                    (
                        text::regex(r"^(is )?currently in"),
                        true,
                        ContentStatus::Current,
                    ),
                    (
                        text::regex(r"^(?:((is )?not currently in)|isn't currently in)"),
                        false,
                        ContentStatus::Current,
                    ),
                    (
                        text::regex(r"^(is )?pending to"),
                        true,
                        ContentStatus::Pending,
                    ),
                    (
                        text::regex(r"^(?:((is )?not pending to)|isn't pending to)"),
                        false,
                        ContentStatus::Pending,
                    ),
                ]
            });
        let rest = skip_separators(rest);
        let (end, is_in, status) = STATUS
            .iter()
            .find_map(|(re, is_in, status)| re.find(rest).map(|m| (m.end(), *is_in, *status)))
            .ok_or_else(|| {
                expected(
                    "\"is currently in\", \"is not currently in\", \"is pending to\" or \"is not pending to\"",
                    rest,
                )
            })?;
        Ok(SystemPredicate::FileService {
            service: ServiceRef::Name(values::any_string(&rest[end..]).to_owned()),
            status,
            is_in,
        })
    }

    /// The text as typed for a part of the lowercased text.
    fn as_typed<'a>(&'a self, lower: &'a str) -> &'a str {
        self.text.original_of(lower)
    }
}

fn ratio_operator(s: &str) -> Result<(&str, RatioOp)> {
    [
        ("wider than", RatioOp::WiderThan),
        ("taller than", RatioOp::TallerThan),
        ("is wider than", RatioOp::WiderThan),
        ("is taller than", RatioOp::TallerThan),
        ("==", RatioOp::Equal),
        ("=", RatioOp::Equal),
        ("is", RatioOp::Equal),
        ("~=", RatioOp::Approx),
        ("\u{2248}", RatioOp::Approx),
    ]
    .into_iter()
    .find_map(|(spelling, op)| s.strip_prefix(spelling).map(|rest| (rest, op)))
    .ok_or_else(|| expected("=, \u{2248}, \"wider than\" or \"taller than\"", s))
}

fn fixed_canvases(canvases: Canvases) -> ViewCanvases {
    let set: BTreeSet<ViewCanvas> = match canvases {
        Canvases::Media => [ViewCanvas::MediaViewer].into(),
        Canvases::Preview => [ViewCanvas::Preview].into(),
        Canvases::MediaAndPreview => [ViewCanvas::MediaViewer, ViewCanvas::Preview].into(),
    };
    ViewCanvases::Specific(set)
}

/// Take the canvas names (`media`, `preview`, `client api`) out of the text,
/// wherever they are.
fn named_canvases(s: &str) -> (String, ViewCanvases) {
    let mut remainder = s.to_owned();
    let mut canvases = BTreeSet::new();
    for (word, canvas) in [
        ("media", ViewCanvas::MediaViewer),
        ("preview", ViewCanvas::Preview),
        ("client api", ViewCanvas::ClientApi),
    ] {
        if remainder.contains(word) {
            canvases.insert(canvas);
            remainder = remainder.replace(word, "");
        }
    }
    let remainder = remainder.trim_start_matches([',', ' ']).to_owned();
    let canvases = if canvases.is_empty() {
        ViewCanvases::Default
    } else {
        ViewCanvases::Specific(canvases)
    };
    (remainder, canvases)
}

fn viewing_stats(
    stat: ViewingStat,
    canvases: ViewCanvases,
    op: Comparison,
    value: u64,
) -> SystemPredicate {
    SystemPredicate::FileViewingStats {
        stat,
        canvases,
        op,
        value,
    }
}

/// `<namespace> <operator> <integer>`, where the namespace may be `any
/// namespace` or `unnamespaced`, and any operator other than `<` and `>`
/// means "about".
fn tag_as_number(s: &str) -> Result<SystemPredicate> {
    static NAMESPACE_AND_OPERATOR: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| {
            let spellings: Vec<&str> = operators::OPERATOR_SPELLINGS
                .iter()
                .map(|(spelling, _)| *spelling)
                .collect();
            text::regex(&format!(
                r"^(?P<namespace>.*)\s+(?P<op>({}))",
                spellings.join("|")
            ))
        });
    let caps = NAMESPACE_AND_OPERATOR
        .captures(s)
        .ok_or_else(|| expected("a namespace followed by <, > or \u{2248}", s))?;
    let namespace = match &caps["namespace"] {
        "any namespace" => NamespaceFilter::Any,
        "unnamespaced" => NamespaceFilter::Unnamespaced,
        other => NamespaceFilter::from_reference(other),
    };
    let op = match operators::OPERATOR_SPELLINGS
        .iter()
        .find(|(spelling, _)| *spelling == &caps["op"])
        .map(|(_, sym)| *sym)
    {
        Some(operators::Sym::Lt) => TagNumberOp::Less,
        Some(operators::Sym::Gt) => TagNumberOp::Greater,
        _ => TagNumberOp::Approx,
    };
    let end = caps.get(0).map_or(0, |m| m.end());
    let (rest, value) = values::integer(&s[end..])?;
    finish(rest)?;
    Ok(SystemPredicate::TagAsNumber {
        namespace,
        op,
        value,
    })
}
