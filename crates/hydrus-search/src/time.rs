//! Time predicates: `system:import time`, `modified time`, `last viewed time`
//! and `archived time`.
//!
//! A time predicate is either relative to when the search runs ("imported in
//! the last 7 days") or pinned to a calendar date ("imported before
//! 2011-06-04"). Relative ages are kept as calendar quantities (years and
//! months are not converted to days) and are only turned into timestamps when
//! the search executes, so a saved search keeps meaning what it says.

use std::fmt;

use crate::number::Comparison;

/// Which timestamp of a file a time predicate tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TimeKind {
    /// When the file was imported to the searched file domain.
    Imported,
    /// The file's modified time (the aggregate over file and domain times).
    Modified,
    /// When the file was last viewed.
    LastViewed,
    /// When the file was archived.
    Archived,
}

/// The test a time predicate applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeTest {
    /// Compare the file's *age* at search time with `age`.
    Relative { op: RelativeOp, age: CalendarDelta },
    /// Compare the timestamp with a calendar date and time (local time).
    ///
    /// `Less` is "before", `Greater` "since", `Equal` "on the day of" and
    /// `Approx` "a month either side of". The reference ignores `NotEqual`.
    Absolute { op: Comparison, at: CivilDateTime },
}

/// How a file's age is compared with a [`CalendarDelta`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelativeOp {
    /// Younger than the age: happened *since* that long ago (`<`).
    Less,
    /// Older than the age: happened *before* that long ago (`>`).
    Greater,
    /// Within ±15% of the age (`≈`; `=` means the same for ages).
    Approx,
    /// Accepted by the reference parser but ignored by its search (`≠`).
    NotEqual,
}

impl RelativeOp {
    /// The symbol the reference implementation stores for this operator.
    pub const fn symbol(self) -> &'static str {
        match self {
            RelativeOp::Less => "<",
            RelativeOp::Greater => ">",
            RelativeOp::Approx => "\u{2248}",
            RelativeOp::NotEqual => "\u{2260}",
        }
    }
}

/// A span of calendar time, e.g. "1 year 2 months 3 days 4 hours".
///
/// Years and months are calendar units: subtracting one month from 31 March
/// gives the last day of February. Weeks are parsed as 7 days.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct CalendarDelta {
    pub years: u32,
    pub months: u32,
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

impl CalendarDelta {
    pub const ZERO: CalendarDelta = CalendarDelta {
        years: 0,
        months: 0,
        days: 0,
        hours: 0,
        minutes: 0,
        seconds: 0,
    };

    pub fn is_zero(&self) -> bool {
        *self == Self::ZERO
    }
}

impl fmt::Display for CalendarDelta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts = [
            (self.years, "year"),
            (self.months, "month"),
            (self.days, "day"),
            (self.hours, "hour"),
            (self.minutes, "minute"),
            (self.seconds, "second"),
        ];
        let mut first = true;
        for (n, unit) in parts {
            if n == 0 {
                continue;
            }
            if !first {
                f.write_str(" ")?;
            }
            first = false;
            let plural = if n == 1 { "" } else { "s" };
            write!(f, "{n} {unit}{plural}")?;
        }
        if first {
            f.write_str("0 seconds")?;
        }
        Ok(())
    }
}

/// A valid calendar date with a time to the minute, without a timezone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CivilDateTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

impl CivilDateTime {
    /// Build a date-time, or `None` if it does not exist (e.g. 30 February)
    /// or the year is outside 1..=9999.
    pub fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> Option<Self> {
        let valid = (1..=9999).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month)
            && hour < 24
            && minute < 60;
        valid.then_some(Self {
            year,
            month,
            day,
            hour,
            minute,
        })
    }

    pub const fn year(self) -> u16 {
        self.year
    }

    pub const fn month(self) -> u8 {
        self.month
    }

    pub const fn day(self) -> u8 {
        self.day
    }

    pub const fn hour(self) -> u8 {
        self.hour
    }

    pub const fn minute(self) -> u8 {
        self.minute
    }
}

impl fmt::Display for CivilDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)?;
        if self.hour != 0 || self.minute != 0 {
            write!(f, " {:02}:{:02}", self.hour, self.minute)?;
        }
        Ok(())
    }
}

/// Whether `year` is a leap year in the proleptic Gregorian calendar.
pub const fn is_leap_year(year: u16) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// The number of days in a month (1-based), or 0 for an invalid month.
pub const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dates() {
        assert!(CivilDateTime::new(2020, 2, 29, 0, 0).is_some());
        assert!(CivilDateTime::new(2021, 2, 29, 0, 0).is_none());
        assert!(CivilDateTime::new(1900, 2, 29, 0, 0).is_none());
        assert!(CivilDateTime::new(2000, 2, 29, 0, 0).is_some());
        assert!(CivilDateTime::new(2011, 13, 1, 0, 0).is_none());
        assert!(CivilDateTime::new(0, 1, 1, 0, 0).is_none());
        assert!(CivilDateTime::new(2011, 6, 4, 24, 0).is_none());
    }

    #[test]
    fn renders_deltas() {
        let delta = CalendarDelta {
            years: 1,
            days: 2,
            hours: 1,
            ..CalendarDelta::ZERO
        };
        assert_eq!(delta.to_string(), "1 year 2 days 1 hour");
        assert_eq!(CalendarDelta::ZERO.to_string(), "0 seconds");
    }
}
