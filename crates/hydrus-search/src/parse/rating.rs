//! Rating predicates: `system:rating for <service> <op> <value>`,
//! `system:has rating for <service>` and `system:all|any|only <services> rated`.
//!
//! Service names can contain nearly anything, so, like the reference, the
//! value is found at the end of the text and the operator just before it;
//! whatever precedes the operator is the service name.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use hydrus_core::ServiceType;

use crate::error::ParseErrorKind;
use crate::parse::operators::{OPERATOR_SPELLINGS, Result, Sym, expected, rating_op};
use crate::parse::text::{StaticRegex, parse_u64, py_strip, regex};
use crate::predicate::{RatingLogic, RatingTest, ServiceSelection};

/// Split `<service> <operator> <value>` into the operator and the text
/// `"<service> <value>"` for the value parsers.
pub(crate) fn operator(s: &str) -> Result<(String, Sym)> {
    static TAIL: StaticRegex =
        LazyLock::new(|| regex(r"^(?P<first>.*?)(?P<second>(dislike|like|\d+/\d+|\d+))$"));
    if let Some(caps) = TAIL.captures(s) {
        let without_value = py_strip(&caps["first"]);
        for &(spelling, sym) in OPERATOR_SPELLINGS {
            if let Some(service) = without_value.strip_suffix(spelling) {
                if sym == Sym::Ne {
                    return Err(ParseErrorKind::RatingNotEqual);
                }
                return Ok((format!("{service} {}", &caps["second"]), sym));
            }
        }
    }
    Err(expected("\"<service name> <operator> <rating>\"", s))
}

/// `<service> <stars>/<out of>`.
pub(crate) fn stars(s: &str, sym: Sym) -> Result<(String, RatingTest)> {
    static STARS: StaticRegex =
        LazyLock::new(|| regex(r"^(?P<name>.+?)\s+(?P<num>\d+)/(?P<den>\d+)$"));
    let s = py_strip(s);
    let caps = STARS
        .captures(s)
        .ok_or_else(|| expected("a service name and a rating like 3/5", s))?;
    let stars = parse_u64(&caps["num"])?;
    let out_of = parse_u64(&caps["den"])?;
    if stars > out_of {
        return Err(ParseErrorKind::RatingOutOfRange { stars, out_of });
    }
    let op = rating_op(sym)?;
    Ok((
        caps["name"].to_owned(),
        RatingTest::Stars { op, stars, out_of },
    ))
}

/// `<service> <count>`, for inc/dec services.
pub(crate) fn count(s: &str, sym: Sym) -> Result<(String, RatingTest)> {
    static COUNT: StaticRegex = LazyLock::new(|| regex(r"^(?P<name>.+?)\s+(?P<num>\d+)$"));
    let s = py_strip(s);
    let caps = COUNT
        .captures(s)
        .ok_or_else(|| expected("a service name and a number", s))?;
    let value = parse_u64(&caps["num"])?;
    let op = rating_op(sym)?;
    Ok((caps["name"].to_owned(), RatingTest::Count { op, value }))
}

/// `<service> [operator] like|dislike`. Any operator is ignored.
pub(crate) fn like(s: &str) -> Result<(String, RatingTest)> {
    let s = py_strip(s);
    let (rest, test) = if let Some(rest) = s.strip_suffix("dislike") {
        (rest, RatingTest::Disliked)
    } else if let Some(rest) = s.strip_suffix("like") {
        (rest, RatingTest::Liked)
    } else {
        return Err(expected("\"like\" or \"dislike\"", s));
    };
    let mut service = py_strip(rest);
    if let Some(stripped) = OPERATOR_SPELLINGS
        .iter()
        .find_map(|(spelling, _)| service.strip_suffix(spelling))
    {
        service = py_strip(stripped);
    }
    Ok((service.to_owned(), test))
}

/// `all|any|only <services> [(amongst <services>)] [not] rated`, given the
/// whole lowercased text after `system:`.
pub(crate) fn advanced(subtag: &str) -> Result<(RatingLogic, ServiceSelection, bool)> {
    enum Logic {
        All,
        Any,
        Only,
    }
    let mut workspace = py_strip(subtag);
    let mut logic = Logic::All;
    for (word, l) in [
        ("all", Logic::All),
        ("any", Logic::Any),
        ("only", Logic::Only),
    ] {
        if let Some(rest) = workspace.strip_prefix(word) {
            workspace = py_strip(rest);
            logic = l;
            break;
        }
    }
    let mut rated = true;
    for (word, r) in [("not rated", false), ("rated", true)] {
        if let Some(rest) = workspace.strip_suffix(word) {
            workspace = py_strip(rest);
            rated = r;
            break;
        }
    }
    let logic = match logic {
        Logic::All => RatingLogic::All,
        Logic::Any => RatingLogic::Any,
        Logic::Only => {
            let amongst = if let Some((primary, secondary)) = workspace.split_once("(amongst") {
                workspace = py_strip(primary);
                let mut secondary = py_strip(secondary);
                if let Some(inner) = secondary.strip_suffix(')') {
                    secondary = py_strip(inner);
                }
                service_selection(secondary)?
            } else {
                ServiceSelection::all_local_ratings()
            };
            RatingLogic::Only { amongst }
        }
    };
    Ok((logic, service_selection(workspace)?, rated))
}

/// `ratings`, a comma-separated list of service type names (`numerical
/// ratings`, `inc/dec ratings`, ...) or a comma-separated list of service
/// names.
fn service_selection(text: &str) -> Result<ServiceSelection> {
    if text == "rating" || text == "ratings" {
        return Ok(ServiceSelection::all_local_ratings());
    }
    let pieces: Vec<&str> = text.split(',').map(py_strip).collect();
    let type_named = |piece: &str| {
        ServiceType::ALL
            .iter()
            .find(|t| t.short_name() == piece)
            .copied()
    };
    if pieces.iter().any(|p| type_named(p).is_some()) {
        let types = pieces
            .iter()
            .map(|p| {
                type_named(p).ok_or_else(|| ParseErrorKind::UnknownServiceType((*p).to_owned()))
            })
            .collect::<Result<BTreeSet<ServiceType>>>()?;
        Ok(ServiceSelection::Types(types))
    } else {
        Ok(ServiceSelection::Names(
            pieces.into_iter().map(str::to_owned).collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::number::RatingOp;

    #[test]
    fn operator_is_found_before_the_value() {
        let (rest, sym) = operator("my favs = 3/5").unwrap();
        assert_eq!((rest.as_str(), sym), ("my favs  3/5", Sym::Eq));
        assert_eq!(
            stars(&rest, sym).unwrap(),
            (
                "my favs".to_owned(),
                RatingTest::Stars {
                    op: RatingOp::Equal,
                    stars: 3,
                    out_of: 5
                }
            )
        );
        // as in the reference, a name ending in an operator word loses it
        assert_eq!(operator("this 3/5").unwrap().0, "th 3/5");
        assert_eq!(
            operator("x != 3").unwrap_err(),
            ParseErrorKind::RatingNotEqual
        );
    }

    #[test]
    fn like_ignores_operators() {
        assert_eq!(
            like("faves < dislike").unwrap(),
            ("faves".to_owned(), RatingTest::Disliked)
        );
    }

    #[test]
    fn advanced_selections() {
        let (logic, services, rated) =
            advanced("only inc/dec ratings (amongst inc/dec ratings, numerical ratings) not rated")
                .unwrap();
        assert!(!rated);
        assert_eq!(
            services,
            ServiceSelection::Types([ServiceType::LocalRatingIncDec].into_iter().collect())
        );
        assert_eq!(
            logic,
            RatingLogic::Only {
                amongst: ServiceSelection::Types(
                    [
                        ServiceType::LocalRatingIncDec,
                        ServiceType::LocalRatingNumerical
                    ]
                    .into_iter()
                    .collect()
                )
            }
        );
        assert!(matches!(
            advanced("all numerical ratings, favourites rated"),
            Err(ParseErrorKind::UnknownServiceType(_))
        ));
    }
}
