//! Time predicate values and operators.
//!
//! The reference hands the value to the `dateparser` library, which accepts
//! almost anything in many languages ("last tuesday", "4 march 2020", "x").
//! We accept a documented subset that covers the forms in the hydrus docs and
//! tests:
//!
//! - a date `YYYY-MM-DD` (also with `/` or `.`), optionally followed by a time
//!   `HH:MM` or `HH:MM:SS`, after a space or `T` (seconds are dropped, as in
//!   the reference);
//! - an age: one or more `<n> <unit>` terms, optionally separated by commas or
//!   "and", optionally followed by "ago". `<n>` is a whole number (in any
//!   script's decimal digits), `a` or `an`. Units: `year(s)`/`yr`/`y`, `month(s)`/`mo`, `week(s)`/`wk`,
//!   `day(s)`/`d`, `hour(s)`/`hr(s)`/`h`, `minute(s)`/`min(s)`/`m`,
//!   `second(s)`/`sec(s)`/`s`. A unit given twice keeps the last value, as
//!   `dateparser` does (weeks and days are separate units and add up);
//! - `yesterday` (an age of one day).

use std::sync::LazyLock;

use crate::error::ParseErrorKind;
use crate::number::Comparison;
use crate::parse::operators::{Result, relational};
use crate::parse::text::{StaticRegex, parse_u32, py_strip, regex};
use crate::time::{CalendarDelta, CivilDateTime, RelativeOp, TimeTest};

/// A parsed time value, before the operator gives it meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimeValue {
    Age(CalendarDelta),
    Date(CivilDateTime),
}

static DATE: StaticRegex = LazyLock::new(|| {
    regex(
        r"^(?P<y>[0-9]{4})[-/.](?P<m>[0-9]{1,2})[-/.](?P<d>[0-9]{1,2})(?:(?:[tT]|\s+)(?P<hh>[0-9]{1,2}):(?P<mm>[0-9]{2})(?::(?P<ss>[0-9]{2}))?)?$",
    )
});

static AGE_TERM: StaticRegex = LazyLock::new(|| {
    regex(concat!(
        r"^(?P<n>\d+|an|a)\s*",
        r"(?P<unit>years|year|yr|y|months|month|mo|weeks|week|wk|days|day|d|hours|hour|hrs|hr|h",
        r"|minutes|minute|mins|min|m|seconds|second|secs|sec|s)\b",
        r"(?:\s*,\s*|\s+and\s+|\s*)",
    ))
});

fn parse_date(s: &str) -> Option<Result<CivilDateTime>> {
    let caps = DATE.captures(s)?;
    let field = |name: &str| caps.name(name).map_or(Ok(0), |m| parse_u32(m.as_str()));
    let make = || -> Result<CivilDateTime> {
        let invalid = || ParseErrorKind::InvalidDate(s.to_owned());
        let narrow = |v: u32| u8::try_from(v).map_err(|_| invalid());
        let year = u16::try_from(field("y")?).map_err(|_| invalid())?;
        if field("ss")? >= 60 {
            return Err(invalid());
        }
        CivilDateTime::new(
            year,
            narrow(field("m")?)?,
            narrow(field("d")?)?,
            narrow(field("hh")?)?,
            narrow(field("mm")?)?,
        )
        .ok_or_else(invalid)
    };
    Some(make())
}

fn parse_age(s: &str) -> Option<Result<CalendarDelta>> {
    if s == "yesterday" {
        return Some(Ok(CalendarDelta {
            days: 1,
            ..CalendarDelta::ZERO
        }));
    }
    let mut rest = s;
    let mut delta = CalendarDelta::ZERO;
    // weeks and days are separate units that add up
    let mut weeks: u32 = 0;
    let mut terms = 0;
    while let Some(caps) = AGE_TERM.captures(rest) {
        let n = match &caps["n"] {
            "a" | "an" => 1,
            digits => match parse_u32(digits) {
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            },
        };
        match &caps["unit"] {
            "years" | "year" | "yr" | "y" => delta.years = n,
            "months" | "month" | "mo" => delta.months = n,
            "weeks" | "week" | "wk" => weeks = n,
            "days" | "day" | "d" => delta.days = n,
            "hours" | "hour" | "hrs" | "hr" | "h" => delta.hours = n,
            "minutes" | "minute" | "mins" | "min" | "m" => delta.minutes = n,
            _ => delta.seconds = n,
        }
        terms += 1;
        rest = &rest[caps.get(0).map_or(0, |m| m.end())..];
    }
    if terms == 0 {
        return None;
    }
    let rest = py_strip(rest);
    if !(rest.is_empty() || rest == "ago") {
        return None;
    }
    let days = weeks
        .checked_mul(7)
        .and_then(|week_days| week_days.checked_add(delta.days));
    Some(match days {
        Some(days) => Ok(CalendarDelta { days, ..delta }),
        None => Err(ParseErrorKind::NumberTooLarge(s.to_owned())),
    })
}

/// Parse a time value, or say why it is not one.
pub(crate) fn time_value(s: &str) -> Result<TimeValue> {
    let s = py_strip(s);
    if let Some(date) = parse_date(s) {
        return date.map(TimeValue::Date);
    }
    if let Some(age) = parse_age(s) {
        return age.map(TimeValue::Age);
    }
    Err(ParseErrorKind::UnsupportedDate(s.to_owned()))
}

/// The reference's guess at whether a value is a date rather than an age.
fn looks_like_date(s: &str) -> bool {
    !["year", "month", "day", "hour", "second", "ago"]
        .iter()
        .any(|word| s.contains(word))
}

/// The operator and value of a time predicate.
///
/// Besides the usual comparisons the reference accepts words before the
/// value: "since" and "before", "around" (for ages), "the day of" and
/// "the month of" (for dates). It guesses whether the value is an age or a
/// date from keywords ("day", "ago", ...), which mistakes e.g. "2 weeks" for
/// a date. We follow its guess to decide which words apply, so the same
/// inputs are accepted, but use the value's actual form to decide what
/// "since" and "before" mean, and refuse "the day of"/"the month of" an age.
pub(crate) fn time_test(s: &str) -> Result<TimeTest> {
    static FIRST_DIGIT: StaticRegex = LazyLock::new(|| regex(r"\d.*"));
    let mut worded: Option<(&str, Sym)> = None;
    let mut needs_date = false;
    if let Some(m) = FIRST_DIGIT.find(s) {
        let words = &s[..m.start()];
        let value_text = m.as_str();
        let guess_date = looks_like_date(value_text);
        let parsed = time_value(value_text);
        let is_age = matches!(parsed, Ok(TimeValue::Age(_)));
        let is_date = match parsed {
            Ok(TimeValue::Date(_)) => true,
            Ok(TimeValue::Age(_)) => false,
            Err(_) => guess_date,
        };
        needs_date = is_age && (words.contains("month") || words.contains("day"));
        worded = if words.contains("month") && guess_date {
            if is_age {
                return Err(ParseErrorKind::CalendarOperatorNeedsDate);
            }
            Some((value_text, Sym::Approx))
        } else if words.contains("around") && !guess_date {
            Some((value_text, Sym::Approx))
        } else if words.contains("day") && guess_date {
            if is_age {
                return Err(ParseErrorKind::CalendarOperatorNeedsDate);
            }
            Some((value_text, Sym::Equal))
        } else if words.contains("since") {
            Some((value_text, if is_date { Sym::Greater } else { Sym::Less }))
        } else if words.contains("before") {
            Some((value_text, if is_date { Sym::Less } else { Sym::Greater }))
        } else {
            None
        };
    }
    let (value_text, op) = match worded {
        Some(found) => found,
        None => match relational(s) {
            Ok((mut rest, op)) => {
                // `dateparser` skips the "=" of "<=" and ">=", so they mean
                // "<" and ">" in the reference
                if matches!(op, Comparison::Less | Comparison::Greater) {
                    rest = rest.strip_prefix('=').unwrap_or(rest);
                }
                (rest, Sym::from(op))
            }
            Err(_) if needs_date => return Err(ParseErrorKind::CalendarOperatorNeedsDate),
            Err(e) => return Err(e),
        },
    };
    Ok(match time_value(value_text)? {
        TimeValue::Age(age) => TimeTest::Relative {
            op: match op {
                Sym::Less => RelativeOp::Less,
                Sym::Greater => RelativeOp::Greater,
                Sym::Equal | Sym::Approx => RelativeOp::Approx,
                Sym::NotEqual => RelativeOp::NotEqual,
            },
            age,
        },
        TimeValue::Date(at) => TimeTest::Absolute { op: op.into(), at },
    })
}

/// The operators a time predicate can end up with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sym {
    Less,
    Greater,
    Equal,
    NotEqual,
    Approx,
}

impl From<Comparison> for Sym {
    fn from(op: Comparison) -> Self {
        match op {
            Comparison::Less => Sym::Less,
            Comparison::Greater => Sym::Greater,
            Comparison::Equal => Sym::Equal,
            Comparison::NotEqual => Sym::NotEqual,
            Comparison::Approx => Sym::Approx,
        }
    }
}

impl From<Sym> for Comparison {
    fn from(op: Sym) -> Self {
        match op {
            Sym::Less => Comparison::Less,
            Sym::Greater => Comparison::Greater,
            Sym::Equal => Comparison::Equal,
            Sym::NotEqual => Comparison::NotEqual,
            Sym::Approx => Comparison::Approx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn age(s: &str) -> CalendarDelta {
        match time_value(s).unwrap() {
            TimeValue::Age(a) => a,
            TimeValue::Date(d) => panic!("{s:?} parsed as date {d}"),
        }
    }

    #[test]
    fn ages() {
        assert_eq!(
            age("7 years 45 days 7h"),
            CalendarDelta {
                years: 7,
                days: 45,
                hours: 7,
                ..CalendarDelta::ZERO
            }
        );
        assert_eq!(age("2 weeks ago").days, 14);
        assert_eq!(age("1 day, 2 hours").hours, 2);
        assert_eq!(age("1 day and 2 hours").hours, 2);
        assert_eq!(age("an hour").hours, 1);
        assert_eq!(age("yesterday").days, 1);
        assert_eq!(age("1 day 3 days").days, 3);
        assert!(time_value("3 yrs").is_err());
        assert!(time_value("5 days ago ago").is_err());
        assert!(time_value("in 2 days").is_err());
    }

    #[test]
    fn dates() {
        assert_eq!(
            time_value("2011-1-3").unwrap(),
            TimeValue::Date(CivilDateTime::new(2011, 1, 3, 0, 0).unwrap())
        );
        assert_eq!(
            time_value("2011-06-04 13:45:30").unwrap(),
            TimeValue::Date(CivilDateTime::new(2011, 6, 4, 13, 45).unwrap())
        );
        assert!(matches!(
            time_value("2011-02-30"),
            Err(ParseErrorKind::InvalidDate(_))
        ));
        assert!(matches!(
            time_value("03/04/2020"),
            Err(ParseErrorKind::UnsupportedDate(_))
        ));
    }

    #[test]
    fn operator_words() {
        let since_age = time_test("since 7 days").unwrap();
        assert!(matches!(
            since_age,
            TimeTest::Relative {
                op: RelativeOp::Less,
                ..
            }
        ));
        let since_date = time_test("since 2011-06-04").unwrap();
        assert!(matches!(
            since_date,
            TimeTest::Absolute {
                op: Comparison::Greater,
                ..
            }
        ));
        assert!(matches!(
            time_test("the day of 2011-06-04").unwrap(),
            TimeTest::Absolute {
                op: Comparison::Equal,
                ..
            }
        ));
        assert_eq!(
            time_test("the day of 2 weeks").unwrap_err(),
            ParseErrorKind::CalendarOperatorNeedsDate
        );
        assert!(matches!(
            time_test("= 1 day").unwrap(),
            TimeTest::Relative {
                op: RelativeOp::Approx,
                ..
            }
        ));
        assert!(matches!(
            time_test("<= 7 days").unwrap(),
            TimeTest::Relative {
                op: RelativeOp::Less,
                ..
            }
        ));
    }
}
