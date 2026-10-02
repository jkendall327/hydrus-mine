//! The checker options editor ("edit checker options", opened by a
//! "checker options" button), as the reference's `EditCheckerOptions`:
//! five "reasonable defaults", the file velocity below which checking
//! stops, whether to check at a static interval, and either the reactive
//! checking box (intended new files per check, never faster than, never
//! slower than) or the static one (the check period). Checked against the
//! reference's editor, recorded by `oracle/record_checker_options.py`.
//!
//! Its times are the reference's `TimeDeltaWidget`s, which signal on every
//! change of any of their fields: each change to never faster or never
//! slower than moves never slower than up to never faster than if it is
//! less (`_UpdateTimeDeltas`), and a time set whole is set a field at a
//! time, each field's change signalling, as the reference's are. So
//! presets and corrections land where the reference's do.

use hydrus_core::subscriptions::CheckerOptions;

use crate::options::{Unit, duration_fields, duration_seconds};

const DAY: i64 = 86400;

/// The reasonable defaults' buttons, in order, and what each sets
/// (`ClientDefaults.GetDefaultCheckerOptions`).
pub const PRESETS: [(&str, CheckerOptions); 5] = [
    (
        "thread",
        CheckerOptions {
            intended_files_per_check: 4.0,
            never_faster_than: 300,
            never_slower_than: DAY,
            death_file_velocity: (1, 3 * DAY),
        },
    ),
    (
        "slow thread",
        CheckerOptions {
            intended_files_per_check: 1.0,
            never_faster_than: 4 * 3600,
            never_slower_than: 7 * DAY,
            death_file_velocity: (1, 30 * DAY),
        },
    ),
    (
        "faster tag subscription",
        CheckerOptions {
            intended_files_per_check: 10.0,
            never_faster_than: 43200,
            never_slower_than: 30 * DAY,
            death_file_velocity: (1, 90 * DAY),
        },
    ),
    (
        "medium tag/artist subscription",
        CheckerOptions {
            intended_files_per_check: 4.0,
            never_faster_than: DAY,
            never_slower_than: 90 * DAY,
            death_file_velocity: (1, 180 * DAY),
        },
    ),
    (
        "slower tag subscription",
        CheckerOptions {
            intended_files_per_check: 1.0,
            never_faster_than: 7 * DAY,
            never_slower_than: 180 * DAY,
            death_file_velocity: (1, 365 * DAY),
        },
    ),
];

pub const TITLE: &str = "edit checker options";
pub const WARNING: &str = "If you do not understand this panel, use the buttons! The defaults are fine for most purposes!";
pub const DEFAULTS_BOX: &str = "reasonable defaults";
pub const VELOCITY_LABEL: &str = "stop checking if new files found falls below: ";
pub const VELOCITY_PER: &str = "files in";
pub const STATIC_LABEL: &str = "just check at a static, regular interval: ";
pub const ADVANCED_WARNING: &str = "As you are in advanced mode, these options have extremely low limits. This is intended only for testing and small scale private network tasks. Do not use very fast check times for real world use on public websites, as it is wasteful and rude, hydrus will be overloaded with high-CPU parsing work, and you may get your IP banned.";
pub const REACTIVE_BOX: &str = "reactive checking";
pub const REACTIVE_TEXT: &str = "This checks more or less frequently based on how fast the download source is producing new files.";
pub const INTENDED_LABEL: &str = "intended new files per check: ";
pub const FASTER_LABEL: &str = "never check faster than once per: ";
pub const SLOWER_LABEL: &str = "never check slower than once per: ";
pub const STATIC_BOX: &str = "static checking";
pub const PERIOD_LABEL: &str = "check period: ";
/// What "ok" asks with never faster and never slower than the same, while
/// checking reactively (`UserIsOKToOK`).
pub const SAME_QUESTION: &str = "The \"never check faster/slower than\" values are the same, which means this checker will always check at a static, regular interval. Is that OK?";

/// The intended new files per check's range, and its decimals.
pub const INTENDED_RANGE: (f64, f64) = (0.25, 1000.0);
const INTENDED_DECIMALS: usize = 2;
/// The velocity's number's range, and its time's least value.
pub const VELOCITY_FILES: (i64, i64) = (0, 1000);
pub const VELOCITY_MIN: i64 = 60;

pub const TIME_UNITS: &[Unit] = &[Unit::Days, Unit::Hours, Unit::Minutes, Unit::Seconds];
pub const VELOCITY_UNITS: &[Unit] = &[Unit::Days, Unit::Hours, Unit::Minutes];

/// The least never faster than, never slower than and check period: tiny
/// in advanced mode.
pub fn minimums(advanced: bool) -> (i64, i64, i64) {
    if advanced { (1, 1, 1) } else { (30, 600, 180) }
}

/// One of the editor's times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    Faster,
    Slower,
    Period,
    /// The velocity's time.
    Velocity,
}

/// A time's control: its fields, and its least value (held to when set
/// whole, or when "ok" is pressed, as the reference's is when it loses the
/// focus; not while its fields are typed in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Time {
    pub units: &'static [Unit],
    pub fields: Vec<i64>,
    pub min: i64,
}

impl Time {
    fn new(units: &'static [Unit], min: i64) -> Self {
        Self {
            units,
            fields: vec![0; units.len()],
            min,
        }
    }

    #[allow(clippy::cast_possible_truncation)] // (whole seconds)
    pub fn seconds(&self) -> i64 {
        duration_seconds(&self.fields, self.units) as i64
    }
}

/// The editor's state.
#[derive(Debug, Clone, PartialEq)]
pub struct Editor {
    pub advanced: bool,
    pub intended: f64,
    pub faster: Time,
    pub slower: Time,
    pub period: Time,
    pub velocity_files: i64,
    pub velocity: Time,
    /// Checking at a static interval: the static box shown, not the
    /// reactive one.
    pub flat: bool,
}

impl Editor {
    /// Opened on `options` (in advanced mode, its times' least values
    /// tiny), as the reference's opens: each time set whole (so raised to
    /// its least value), the check period as never faster than, and
    /// static checking if never faster and never slower than are the same.
    pub fn new(options: &CheckerOptions, advanced: bool) -> Self {
        let (faster, slower, period) = minimums(advanced);
        let mut editor = Self {
            advanced,
            intended: 0.0,
            faster: Time::new(TIME_UNITS, faster),
            slower: Time::new(TIME_UNITS, slower),
            period: Time::new(TIME_UNITS, period),
            velocity_files: 0,
            velocity: Time::new(VELOCITY_UNITS, VELOCITY_MIN),
            flat: false,
        };
        // (the reference's controls start at their least values, set
        // without signalling)
        for which in [Which::Faster, Which::Slower, Which::Period, Which::Velocity] {
            let time = editor.time_mut(which);
            time.fields = duration_fields(time.min as f64, time.units);
        }
        editor.set_value(options);
        editor
    }

    fn time_mut(&mut self, which: Which) -> &mut Time {
        match which {
            Which::Faster => &mut self.faster,
            Which::Slower => &mut self.slower,
            Which::Period => &mut self.period,
            Which::Velocity => &mut self.velocity,
        }
    }

    pub fn time(&self, which: Which) -> &Time {
        match which {
            Which::Faster => &self.faster,
            Which::Slower => &self.slower,
            Which::Period => &self.period,
            Which::Velocity => &self.velocity,
        }
    }

    /// All of it set to `options` (a reasonable default's button), as the
    /// reference's `SetValue`.
    pub fn set_value(&mut self, options: &CheckerOptions) {
        self.set_intended(options.intended_files_per_check);
        self.set_time(Which::Faster, options.never_faster_than);
        self.set_time(Which::Slower, options.never_slower_than);
        let (files, seconds) = options.death_file_velocity;
        self.set_velocity_files(files);
        self.set_time(Which::Velocity, seconds);
        self.set_time(Which::Period, options.never_faster_than);
        self.flat = options.never_faster_than == options.never_slower_than;
        self.times_changed();
    }

    /// The reasonable default `index` chosen.
    pub fn preset(&mut self, index: usize) {
        if let Some((_, options)) = PRESETS.get(index) {
            self.set_value(options);
        }
    }

    /// The intended new files per check, held to its range and rounded to
    /// its decimals as the reference's spin box holds it.
    pub fn set_intended(&mut self, n: f64) {
        self.intended = rounded(n, INTENDED_DECIMALS).clamp(INTENDED_RANGE.0, INTENDED_RANGE.1);
    }

    pub fn set_velocity_files(&mut self, n: i64) {
        self.velocity_files = n.clamp(VELOCITY_FILES.0, VELOCITY_FILES.1);
    }

    /// The static checkbox clicked.
    pub fn toggle_flat(&mut self) {
        self.flat = !self.flat;
        self.times_changed();
    }

    /// A time's field typed in (held to the field's range, not the time's
    /// least value).
    pub fn set_field(&mut self, which: Which, field: usize, n: i64) {
        let time = self.time_mut(which);
        let Some(unit) = time.units.get(field) else {
            return;
        };
        let n = n.clamp(0, unit.max());
        if time.fields[field] != n {
            time.fields[field] = n;
            if matches!(which, Which::Faster | Which::Slower) {
                self.times_changed();
            }
        }
    }

    /// A time set whole (raised to its least value), a field at a time.
    fn set_time(&mut self, which: Which, seconds: i64) {
        let time = self.time(which);
        let fields = duration_fields(seconds.max(time.min) as f64, time.units);
        for (field, n) in fields.into_iter().enumerate() {
            self.set_field(which, field, n);
        }
    }

    /// Never slower than moved up to never faster than if less, while
    /// checking reactively (`_UpdateTimeDeltas`).
    fn times_changed(&mut self) {
        if !self.flat && self.slower.seconds() < self.faster.seconds() {
            self.set_time(Which::Slower, self.faster.seconds());
        }
    }

    /// The checker options it holds (`GetValue`): checking statically,
    /// never faster and never slower than are both the check period.
    pub fn value(&self) -> CheckerOptions {
        let (faster, slower) = if self.flat {
            (self.period.seconds(), self.period.seconds())
        } else {
            (self.faster.seconds(), self.slower.seconds())
        };
        CheckerOptions {
            intended_files_per_check: self.intended,
            never_faster_than: faster,
            never_slower_than: slower,
            death_file_velocity: (self.velocity_files, self.velocity.seconds()),
        }
    }

    /// "ok" pressed: each time below its least value raised to it (as the
    /// reference's is when the focus leaves it); then, with never faster
    /// and never slower than the same while checking reactively, the
    /// question to ask first.
    pub fn ok(&mut self) -> Option<&'static str> {
        for which in [Which::Faster, Which::Slower, Which::Period, Which::Velocity] {
            let time = self.time(which);
            if time.seconds() < time.min {
                self.set_time(which, time.min);
            }
        }
        (!self.flat && self.faster.seconds() == self.slower.seconds()).then_some(SAME_QUESTION)
    }
}

/// `n` to `decimals` places as Qt's spin box rounds it: its shortest
/// decimal form rounded half up (so 2.345, held as 2.34499..., is 2.35).
fn rounded(n: f64, decimals: usize) -> f64 {
    let text = format!("{}", n.abs());
    let Some((whole, fraction)) = text.split_once('.') else {
        return n;
    };
    if fraction.len() <= decimals {
        return n;
    }
    let kept: f64 = format!("{whole}.{}", &fraction[..decimals])
        .parse()
        .unwrap_or(n);
    let up = fraction.as_bytes()[decimals] >= b'5';
    let step = 10f64.powi(-i32::try_from(decimals).unwrap_or(0));
    let out = if up { kept + step } else { kept };
    // (the sum, as the nearest of that many places)
    let out: f64 = format!("{out:.decimals$}").parse().unwrap_or(out);
    out.copysign(n)
}
