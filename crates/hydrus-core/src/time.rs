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

/// A span of seconds, which may be fractional or negative, as the
/// reference words it (`HydrusTime.TimeDeltaToPrettyTimeDelta`): under a
/// second in milliseconds (`600 milliseconds`, `12.5 milliseconds`) or
/// microseconds, under a minute with a decimal place if it isn't whole
/// (`12.3 seconds`).
#[allow(clippy::float_cmp)] // (compared as the reference compares)
pub fn pretty_time_delta_f64(seconds: f64) -> String {
    if seconds == 0.0 {
        return "0 seconds".into();
    }
    let (sign, seconds) = if seconds < 0.0 {
        ("-", -seconds)
    } else {
        ("", seconds)
    };
    let text = if seconds >= 60.0 {
        pretty_time_delta(seconds as i64, false)
    } else if seconds > 1.0 {
        if seconds.fract() == 0.0 {
            format!("{} seconds", seconds as i64)
        } else {
            format!("{seconds:.1} seconds")
        }
    } else if seconds == 1.0 {
        "1 second".into()
    } else {
        let ms = seconds * 1000.0;
        if ms > 100.0 || ms.fract() == 0.0 {
            format!("{} milliseconds", ms as i64)
        } else if ms > 10.0 {
            format!("{ms:.1} milliseconds")
        } else if ms >= 1.0 {
            format!("{ms:.2} milliseconds")
        } else {
            format!("{} microseconds", (ms * 1000.0) as i64)
        }
    };
    format!("{sign}{text}")
}

/// How long ago (or until) `timestamp` is from `now`, both in seconds
/// (`HydrusTime.TimestampToPrettyTimeDelta`): `now` within three seconds,
/// else the span with `history_suffix` (`" ago"`, `" old"`) if it is past,
/// or `in ` before it if it is to come.
pub fn timestamp_to_pretty_time_delta(timestamp: i64, now: i64, history_suffix: &str) -> String {
    let delta = (timestamp - now).abs();
    if delta <= 3 {
        return "now".into();
    }
    let span = pretty_time_delta(delta, false);
    if now > timestamp {
        format!("{span}{history_suffix}")
    } else {
        format!("in {span}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_as_the_reference_words_them() {
        // (checked against TimeDeltaToPrettyTimeDelta)
        for (seconds, text) in [
            (0.6, "600 milliseconds"),
            (0.0125, "12.5 milliseconds"),
            (0.0012, "1.20 milliseconds"),
            (0.000_5, "500 microseconds"),
            (1.0, "1 second"),
            (12.25, "12.2 seconds"),
            (5.0, "5 seconds"),
            (90.5, "1 minute 30 seconds"),
            (-2.5, "-2.5 seconds"),
        ] {
            assert_eq!(pretty_time_delta_f64(seconds), text, "{seconds}");
        }
        assert_eq!(
            timestamp_to_pretty_time_delta(1000, 1000 + 86_400 * 40, " old"),
            "1 month 9 days old"
        );
        assert_eq!(timestamp_to_pretty_time_delta(1000, 1002, " old"), "now");
        assert_eq!(
            timestamp_to_pretty_time_delta(1000, 900, " ago"),
            "in 1 minute 40 seconds"
        );
    }

    #[test]
    fn conversions() {
        let t = TimestampMs::from_secs_f64(1_700_000_000.123_4);
        assert_eq!(t.millis(), 1_700_000_000_123);
        assert_eq!(t.secs(), 1_700_000_000);
        assert_eq!(TimestampMs(-1).secs(), -1);
    }
}
