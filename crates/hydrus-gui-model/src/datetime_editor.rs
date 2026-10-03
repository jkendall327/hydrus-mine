//! The date-time editor "manage times" opens on a time over some files
//! (the reference's `DateTimesCtrl`): a date, a time to the millisecond,
//! a cascading step when several files have the time, "now", copy and
//! paste. Its value is the files' [`TimeRange`] set to the time shown
//! (never later than now) with the step.

use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;

use hydrus_core::pyjson::PyJson;

use crate::times_editor::TimeRange;

/// What copy, paste and their mistakes say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    Notice(String),
    Warning(String),
    /// A title and message.
    Critical(String, String),
}

#[derive(Debug, Clone)]
pub struct DateTimeEditor {
    original: TimeRange,
    current: TimeRange,
    pub date: Date,
    pub time: Time,
    pub step_ms: i64,
    now_ms: i64,
    tz: TimeZone,
}

/// `ms` as a date and time in `tz`.
fn civil(ms: i64, tz: &TimeZone) -> DateTime {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| t.to_zoned(tz.clone()).datetime())
        .unwrap_or_default()
}

/// Python's `repr` of a string, as its errors quote one.
fn py_repr(s: &str) -> String {
    if s.contains('\'') && !s.contains('"') {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\'', "\\'"))
    }
}

/// A date string, as the reference's date parsing reads the plain forms
/// ("2023-11-01", "2023-11-01 12:30:00", "2023-10-05T01:02:03.456"), in
/// seconds.
fn parse_date(text: &str, tz: &TimeZone) -> Option<f64> {
    let text = text.trim();
    let datetime = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|format| DateTime::strptime(format, text).ok())
    .or_else(|| {
        Date::strptime("%Y-%m-%d", text)
            .ok()
            .map(|d| d.to_datetime(Time::midnight()))
    })?;
    let zoned = datetime.to_zoned(tz.clone()).ok()?;
    Some(zoned.timestamp().as_millisecond() as f64 / 1000.0)
}

impl DateTimeEditor {
    /// The editor on `value` (the time shown its earliest).
    pub fn new(value: TimeRange, now_ms: i64, tz: TimeZone) -> Self {
        let start = TimeRange::of(Some(now_ms), 1);
        let shown = civil(now_ms, &tz);
        let mut editor = Self {
            original: start,
            current: start,
            date: shown.date(),
            time: shown.time(),
            step_ms: 0,
            now_ms,
            tz,
        };
        editor.set_value(value, true);
        editor
    }

    /// Show `value` (`SetValue`); one with no time is refused.
    fn set_value(&mut self, value: TimeRange, first: bool) {
        if value.is_all_null() {
            return;
        }
        if first {
            self.original = value;
        }
        self.current = value;
        if let Some(ms) = value.best() {
            let shown = civil(ms, &self.tz);
            self.date = shown.date();
            self.time = shown.time();
        }
        self.step_ms = value.step_ms;
    }

    /// The label over it: what the files had, when several had a time.
    pub fn label(&self, now: i64) -> Option<String> {
        (self.original.set_count() > 1)
            .then(|| format!("Originally, {}", self.original.text(now, &self.tz)))
    }

    /// Whether the cascading step shows: several files had a time.
    pub fn step_shown(&self) -> bool {
        self.original.is_multiple_files()
    }

    /// The time shown, in milliseconds, but not later than now.
    fn shown_ms(&self) -> i64 {
        let ms = self
            .date
            .to_datetime(self.time)
            .to_zoned(self.tz.clone())
            .map_or(self.now_ms, |z| z.timestamp().as_millisecond());
        ms.min(self.now_ms)
    }

    /// What it gives: the files set to the time shown, with the step.
    pub fn value(&self) -> TimeRange {
        let mut value = self.current.with(Some(self.shown_ms()), false);
        value.step_ms = self.step_ms;
        value
    }

    pub fn has_changes(&self) -> bool {
        self.value() != self.original
    }

    /// "now".
    pub fn now(&mut self) {
        let value = self.current.with(Some(self.now_ms), false);
        self.set_value(value, false);
    }

    /// Copy: the time shown, in seconds, as JSON.
    pub fn copy(&self) -> (String, Said) {
        (copy_text(&self.value()), Said::Notice("Copied!".into()))
    }

    /// Paste a timestamp in seconds (as JSON, or a string of one) or a
    /// date string, as the reference's paste takes them, saying what it
    /// says (a timestamp it can't read sets nothing, but is still
    /// "Pasted!", as the reference has it).
    pub fn paste(&mut self, text: &str) -> Vec<Said> {
        let (read, mut said) = read_pasted(text, &self.tz);
        if let Some(ms) = read {
            let value = self.current.with(ms, false);
            self.set_value(value, false);
            said.push(Said::Notice("Pasted!".into()));
        }
        said
    }
}

/// Pasted text read as a time (`GetQtDateTimeFromClipboard`): a timestamp
/// in seconds (as JSON, or a string of one) or a date string, in
/// milliseconds; `Some(None)` for what isn't a time, which the reference
/// goes on to set (and refuses); `None` if it gave up. And what it said.
pub fn read_pasted(text: &str, tz: &TimeZone) -> (Option<Option<i64>>, Vec<Said>) {
    let mut said = Vec::new();
    let seconds: Option<Option<f64>> = match PyJson::parse(text).ok() {
        Some(PyJson::Int(i)) => Some(Some(i as f64)),
        Some(PyJson::Float(f)) => Some(Some(f)),
        Some(PyJson::Null) => Some(None),
        Some(PyJson::Str(s)) => {
            if let Ok(f) = s.trim().parse::<f64>() {
                Some(Some(f))
            } else {
                said.push(Said::Critical(
                    "Problem pasting!".into(),
                    format!("could not convert string to float: {}", py_repr(&s)),
                ));
                None
            }
        }
        Some(_) => None,
        None => {
            let Some(seconds) = parse_date(text, tz) else {
                said.push(Said::Warning(format!(
                    "Sorry, I did not understand that! I am looking for a simple timestamp integer or parseable datestring, but I got:\n\n{text}"
                )));
                return (None, said);
            };
            Some(Some(seconds))
        }
    };
    let ms = if let Some(seconds) = seconds {
        seconds.map(|s| (s * 1000.0) as i64)
    } else {
        said.push(Said::Critical(
            "Clipboard Error!".into(),
            crate::notes_editor::clipboard_parse_error(
                "A parseable timestamp string",
                text,
                "Exception('Not a timestamp!')",
            ),
        ));
        None
    };
    (Some(ms), said)
}

/// Copy of a time over some files (`CopyDateTimeValueRangeToClipboard`):
/// its earliest, in seconds, as JSON.
pub fn copy_text(value: &TimeRange) -> String {
    value.best().map_or_else(
        || "null".to_owned(),
        |ms| {
            let mut out = String::new();
            hydrus_core::pyjson::write_python_float(ms as f64 / 1000.0, &mut out);
            out
        },
    )
}
