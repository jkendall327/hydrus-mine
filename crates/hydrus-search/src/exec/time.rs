//! Time predicates as timestamp ranges.
//!
//! Ages ("imported in the last 3 days") are calendar quantities subtracted
//! from the local wall-clock time when the search runs: days and smaller
//! units first, then months and years with the day clamped to the month's
//! length, as the reference does. Dates are local wall-clock times.

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};

use crate::number::Comparison;
use crate::time::{CalendarDelta, CivilDateTime, RelativeOp, TimeTest};

/// When a search runs, and in which time zone dates are read.
#[derive(Debug, Clone)]
pub struct Clock {
    now: Timestamp,
    tz: TimeZone,
}

impl Clock {
    /// The system clock and time zone.
    pub fn system() -> Self {
        Self {
            now: Timestamp::now(),
            tz: TimeZone::system(),
        }
    }

    /// A fixed instant (milliseconds since the epoch) and time zone.
    pub fn fixed(now_ms: i64, tz: TimeZone) -> Self {
        Self {
            now: Timestamp::from_millisecond(now_ms).unwrap_or(Timestamp::UNIX_EPOCH),
            tz,
        }
    }

    fn millis(&self, civil: DateTime) -> Option<i64> {
        let zoned = self.tz.to_zoned(civil).ok()?;
        Some(zoned.timestamp().as_millisecond())
    }

    fn local_now(&self) -> DateTime {
        self.tz.to_datetime(self.now)
    }
}

/// An inclusive range of timestamps (milliseconds); `None` is unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct TimeRange {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

impl TimeRange {
    pub fn is_unbounded(self) -> bool {
        self.from.is_none() && self.to.is_none()
    }

    pub fn contains(self, t: i64) -> bool {
        self.from.is_none_or(|f| t >= f) && self.to.is_none_or(|u| t <= u)
    }
}

/// The instant `age` before now.
fn before_now(clock: &Clock, age: CalendarDelta) -> Option<DateTime> {
    let small = Span::new()
        .try_days(i64::from(age.days))
        .ok()?
        .try_hours(i64::from(age.hours))
        .ok()?
        .try_minutes(i64::from(age.minutes))
        .ok()?
        .try_seconds(i64::from(age.seconds))
        .ok()?;
    let big = Span::new()
        .try_years(i64::from(age.years))
        .ok()?
        .try_months(i64::from(age.months))
        .ok()?;
    clock
        .local_now()
        .checked_sub(small)
        .ok()?
        .checked_sub(big)
        .ok()
}

/// `age` in milliseconds, with months and years at their average length,
/// for "about" comparisons.
fn rough_millis(age: CalendarDelta) -> f64 {
    let days = f64::from(age.days)
        + f64::from(age.months) * (365.25 / 12.0)
        + f64::from(age.years) * 365.25;
    let seconds = days * 86_400.0
        + f64::from(age.hours) * 3_600.0
        + f64::from(age.minutes) * 60.0
        + f64::from(age.seconds);
    seconds * 1000.0
}

fn civil(at: CivilDateTime) -> Option<DateTime> {
    DateTime::new(
        at.year() as i16,
        at.month() as i8,
        at.day() as i8,
        at.hour() as i8,
        at.minute() as i8,
        0,
        0,
    )
    .ok()
}

/// The range of timestamps a time test accepts. A test the reference
/// ignores (`≠`), or one outside the representable calendar, is unbounded.
pub(crate) fn range(test: TimeTest, clock: &Clock) -> TimeRange {
    range_opt(test, clock).unwrap_or_default()
}

pub(crate) fn range_opt(test: TimeTest, clock: &Clock) -> Option<TimeRange> {
    Some(match test {
        TimeTest::Relative { op, age } => {
            let pivot = clock.millis(before_now(clock, age)?)?;
            match op {
                // younger than the age: since the pivot
                RelativeOp::Less => TimeRange {
                    from: Some(pivot),
                    to: None,
                },
                RelativeOp::Greater => TimeRange {
                    from: None,
                    to: Some(pivot),
                },
                RelativeOp::Approx => {
                    let gap = (rough_millis(age) * 0.15) as i64;
                    TimeRange {
                        from: Some(pivot.saturating_sub(gap)),
                        to: Some(pivot.saturating_add(gap)),
                    }
                }
                RelativeOp::NotEqual => TimeRange::default(),
            }
        }
        TimeTest::Absolute { op, at } => {
            let at = civil(at)?;
            match op {
                Comparison::Less => TimeRange {
                    from: None,
                    to: Some(clock.millis(at)?),
                },
                Comparison::Greater => TimeRange {
                    from: Some(clock.millis(at)?),
                    to: None,
                },
                Comparison::Equal => {
                    let start = at.date().to_datetime(jiff::civil::Time::midnight());
                    let end = start.checked_add(Span::new().days(1)).ok()?;
                    TimeRange {
                        from: Some(clock.millis(start)?),
                        to: Some(clock.millis(end)?),
                    }
                }
                Comparison::Approx => {
                    let month = Span::new().months(1);
                    TimeRange {
                        from: Some(clock.millis(at.checked_sub(month).ok()?)?),
                        to: Some(clock.millis(at.checked_add(month).ok()?)?),
                    }
                }
                Comparison::NotEqual => TimeRange::default(),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc_clock(now: &str) -> Clock {
        let ts: Timestamp = now.parse().unwrap();
        Clock::fixed(ts.as_millisecond(), TimeZone::UTC)
    }

    fn ms(s: &str) -> i64 {
        s.parse::<Timestamp>().unwrap().as_millisecond()
    }

    #[test]
    fn ages_subtract_days_then_months_with_clamping() {
        let clock = utc_clock("2024-03-31T12:00:00Z");
        let age = CalendarDelta {
            months: 1,
            ..CalendarDelta::ZERO
        };
        let r = range(
            TimeTest::Relative {
                op: RelativeOp::Less,
                age,
            },
            &clock,
        );
        // 31 March minus a month is the last day of February
        assert_eq!(r.from, Some(ms("2024-02-29T12:00:00Z")));
        assert_eq!(r.to, None);
        let age = CalendarDelta {
            days: 1,
            months: 1,
            ..CalendarDelta::ZERO
        };
        let r = range(
            TimeTest::Relative {
                op: RelativeOp::Greater,
                age,
            },
            &clock,
        );
        assert_eq!(r.to, Some(ms("2024-02-29T12:00:00Z")));
    }

    #[test]
    fn dates_are_local_days() {
        let clock = Clock::fixed(0, TimeZone::fixed(jiff::tz::offset(-5)));
        let at = CivilDateTime::new(2021, 1, 1, 0, 0).unwrap();
        let r = range(
            TimeTest::Absolute {
                op: Comparison::Equal,
                at,
            },
            &clock,
        );
        assert_eq!(r.from, Some(ms("2021-01-01T05:00:00Z")));
        assert_eq!(r.to, Some(ms("2021-01-02T05:00:00Z")));
        let r = range(
            TimeTest::Absolute {
                op: Comparison::NotEqual,
                at,
            },
            &clock,
        );
        assert!(r.is_unbounded());
    }
}
