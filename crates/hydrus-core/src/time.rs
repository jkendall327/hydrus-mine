//! Timestamps.
//!
//! All persisted timestamps are milliseconds since the unix epoch, UTC. The
//! Client API reports times as float seconds alongside the integer
//! milliseconds.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Milliseconds since the unix epoch.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimestampMs(pub i64);

impl TimestampMs {
    pub fn now() -> Self {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        Self(i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX))
    }

    pub const fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    /// From float seconds, as the Client API accepts them.
    pub fn from_secs_f64(secs: f64) -> Self {
        Self((secs * 1000.0).round() as i64)
    }

    pub const fn millis(self) -> i64 {
        self.0
    }

    /// Whole seconds, rounding down.
    pub const fn secs(self) -> i64 {
        self.0.div_euclid(1000)
    }

    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / 1000.0
    }

    #[must_use]
    pub fn saturating_add(self, d: Duration) -> Self {
        Self(
            self.0
                .saturating_add(i64::try_from(d.as_millis()).unwrap_or(i64::MAX)),
        )
    }
}

impl fmt::Debug for TimestampMs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TimestampMs({})", self.0)
    }
}

#[cfg(feature = "sqlite")]
impl rusqlite::ToSql for TimestampMs {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[cfg(feature = "sqlite")]
impl rusqlite::types::FromSql for TimestampMs {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_i64().map(Self)
    }
}

/// A span of seconds as the reference words it
/// (`HydrusTime.TimeDeltaToPrettyTimeDelta`), e.g. `2 days 1 hour`: the two
/// largest units, without months and years if `no_bigger_than_days`.
pub fn pretty_time_delta(seconds: i64, no_bigger_than_days: bool) -> String {
    if seconds == 0 {
        return "0 seconds".into();
    }
    if seconds >= 60 {
        const MINUTE: f64 = 60.0;
        const HOUR: f64 = 60.0 * MINUTE;
        const DAY: f64 = 24.0 * HOUR;
        const YEAR: f64 = 365.25 * DAY;
        const MONTH: f64 = YEAR / 12.0;
        let mut rest = seconds as f64;
        let mut parts: Vec<String> = Vec::new();
        for (name, unit) in [
            ("year", YEAR),
            ("month", MONTH),
            ("day", DAY),
            ("hour", HOUR),
            ("minute", MINUTE),
            ("second", 1.0),
        ] {
            if no_bigger_than_days && (name == "year" || name == "month") {
                continue;
            }
            let mut quantity = (rest / unit).floor();
            rest %= unit;
            if name == "month" && quantity > 11.0 {
                quantity = 11.0;
            }
            if quantity > 0.0 {
                let n = quantity as u64;
                parts.push(format!(
                    "{} {name}{}",
                    crate::numbers::human_int(n),
                    if n > 1 { "s" } else { "" }
                ));
                if parts.len() == 2 {
                    break;
                }
            } else if !parts.is_empty() {
                break;
            }
        }
        parts.join(" ")
    } else if seconds > 1 {
        format!("{seconds} seconds")
    } else {
        "1 second".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions() {
        let t = TimestampMs::from_secs_f64(1_700_000_000.123_4);
        assert_eq!(t.millis(), 1_700_000_000_123);
        assert_eq!(t.secs(), 1_700_000_000);
        assert_eq!(TimestampMs(-1).secs(), -1);
    }
}
