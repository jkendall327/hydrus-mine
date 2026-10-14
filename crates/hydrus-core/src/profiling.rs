//! The reference's `HydrusProfiling`: Help > debug > profiling's profile
//! modes (one at a time: client api, db, ui) and query planner mode.
//!
//! A profile mode times the jobs of its kind; a job slower than the mode's
//! minimum is a *slow* job and is written to a `client profile (<mode>) -
//! <start>.log` in the database directory, the rest are counted as fast.
//! Turning a mode off says how many of each there were. Query planner mode
//! writes the `EXPLAIN QUERY PLAN` of each distinct query to a `client query
//! planner - <start>.log`. Both say so with a popup, as `ShowText` does.
use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::debug_flags::show_text;

/// The slowest a db job may be and still count as fast (`db_profile_min_job_time_ms`).
pub const DB_MIN_MS: u64 = 16;
/// The same for a Client API request (`server_profile_min_job_time_ms`).
pub const CLIENT_API_MIN_MS: u64 = 10;
/// The same for a menu command (`menu_profile_min_job_time_ms`).
pub const MENU_MIN_MS: u64 = 16;

/// A profile mode, named as the reference names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    ClientApi,
    Db,
    Ui,
}

impl Mode {
    /// The mode called `name`.
    pub fn from_name(name: &str) -> Option<Mode> {
        [Mode::ClientApi, Mode::Db, Mode::Ui]
            .into_iter()
            .find(|m| m.name() == name)
    }

    /// The mode's name in messages and log file names.
    pub fn name(self) -> &'static str {
        match self {
            Mode::ClientApi => "client_api",
            Mode::Db => "db",
            Mode::Ui => "ui",
        }
    }
}

#[derive(Default)]
struct State {
    mode: Option<Mode>,
    /// Seconds since the epoch.
    start_time: i64,
    slow: u64,
    fast: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    mode: None,
    start_time: 0,
    slow: 0,
    fast: 0,
});
/// `CURRENT_PROFILE_LOCK`: only one job is profiled at a time.
static CURRENT: Mutex<()> = Mutex::new(());

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now_seconds() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// `time.strftime( '%Y-%m-%d %H-%M-%S', time.localtime( t ) )`.
fn file_stamp(seconds: i64) -> String {
    jiff::Timestamp::from_second(seconds)
        .unwrap_or_default()
        .to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%d %H-%M-%S")
        .to_string()
}

/// `time.strftime( '%Y-%m-%d %H:%M:%S: ' )` for now.
fn line_prefix() -> String {
    jiff::Zoned::now()
        .strftime("%Y-%m-%d %H:%M:%S: ")
        .to_string()
}

fn append(path: &std::path::Path, text: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

/// The log the running mode writes in `dir` (the database directory):
/// `client profile (db) - 2026-10-13 12-00-00.log`.
fn profile_log_path(dir: &Path) -> Option<PathBuf> {
    let (mode, start) = {
        let s = state();
        (s.mode?, s.start_time)
    };
    Some(dir.join(format!(
        "client profile ({}) - {}.log",
        mode.name(),
        file_stamp(start)
    )))
}

/// `PrintProfile`.
fn print_profile(dir: &Path, summary: &str, profile_text: Option<&str>) {
    let Some(path) = profile_log_path(dir) else {
        return;
    };
    let mut text = format!("{}{summary}", line_prefix());
    if let Some(profile_text) = profile_text {
        text.push_str("\n\n");
        text.push_str(profile_text);
    }
    append(&path, &text);
}

/// Whether `mode` is the profile mode now on.
pub fn is_mode(mode: Mode) -> bool {
    state().mode == Some(mode)
}

/// The profile mode now on, if any.
pub fn current_mode() -> Option<Mode> {
    state().mode
}

/// `FlipProfileMode`: turn `mode` on, or off if it is on; turning another on
/// ends the one before.
pub fn flip_mode(mode: Mode) {
    let running = state().mode;
    if let Some(running) = running {
        stop_mode();
        if running == mode {
            return;
        }
    }
    start_mode(mode);
}

/// The profile mode that is on and when it began (seconds), for another
/// process to take up: the daemon does the Client API and most database
/// work, and writes to the same log.
pub fn mode_and_start() -> (Option<Mode>, i64) {
    let s = state();
    (s.mode, s.start_time)
}

/// Take up the profile mode and start another process made, without saying
/// so (that process did): its counts are this process's own.
pub fn adopt_mode(mode: Option<Mode>, start_time: i64) {
    let mut s = state();
    if s.mode == mode && s.start_time == start_time {
        return;
    }
    *s = State {
        mode,
        start_time,
        slow: 0,
        fast: 0,
    };
}

/// Whether query planner mode is on, and when it began.
pub fn planner_and_start() -> (bool, i64) {
    let slot = PLANNER.lock().unwrap_or_else(PoisonError::into_inner);
    slot.as_ref().map_or((false, 0), |p| (true, p.start_time))
}

/// Take up query planner mode as another process set it, without saying so.
pub fn adopt_planner(on: bool, start_time: i64) {
    let mut slot = PLANNER.lock().unwrap_or_else(PoisonError::into_inner);
    match (on, slot.as_ref()) {
        (true, Some(p)) if p.start_time == start_time => {}
        (true, _) => {
            *slot = Some(Planner {
                start_time,
                ..Planner::default()
            });
            PLANNER_ON.store(true, Ordering::Relaxed);
        }
        (false, _) => {
            *slot = None;
            PLANNER_ON.store(false, Ordering::Relaxed);
        }
    }
}

fn start_mode(mode: Mode) {
    {
        let mut s = state();
        *s = State {
            mode: Some(mode),
            start_time: now_seconds(),
            slow: 0,
            fast: 0,
        };
    }
    show_text(&format!("Profile mode \"{}\" on!", mode.name()));
}

fn stop_mode() {
    let (name, slow, fast) = {
        let mut s = state();
        let name = s.mode.take().map_or("", Mode::name);
        (name, s.slow, s.fast)
    };
    show_text(&format!(
        "Profiling \"{name}\" done: {} slow jobs, {} fast jobs",
        crate::numbers::human_int(slow),
        crate::numbers::human_int(fast),
    ));
}

/// A job that took `took`, counted as slow or fast; a slow one is logged
/// with `summary`.
pub fn record(dir: &Path, summary: &str, took: Duration, min_ms: u64) {
    let slow = took.as_millis() > u128::from(min_ms);
    {
        let mut s = state();
        if slow {
            s.slow += 1;
        } else {
            s.fast += 1;
        }
    }
    if slow {
        // (the reference follows its summary with Python's cProfile table,
        // which has no counterpart here; the time the job took stands in)
        print_profile(
            dir,
            summary,
            Some(&format!("Took {}\n\n", pretty_took(took))),
        );
    }
}

fn pretty_took(took: Duration) -> String {
    crate::time::pretty_time_delta_f64(took.as_secs_f64())
}

/// `HydrusProfiling.Profile`: run `f`, timing it. If another job is being
/// profiled already this one is written as `EXCLUSIVE` (the reference can
/// run one profiler at a time) and counted all the same.
pub fn profile<R>(dir: &Path, summary: &str, min_ms: u64, f: impl FnOnce() -> R) -> R {
    let held = CURRENT.try_lock();
    let started = Instant::now();
    let result = f();
    let took = started.elapsed();
    match held {
        Ok(_guard) => record(dir, summary, took, min_ms),
        Err(std::sync::TryLockError::Poisoned(_guard)) => record(dir, summary, took, min_ms),
        Err(std::sync::TryLockError::WouldBlock) => {
            let slow = took.as_millis() > u128::from(min_ms);
            {
                let mut s = state();
                if slow {
                    s.slow += 1;
                } else {
                    s.fast += 1;
                }
            }
            print_profile(
                dir,
                &format!("EXCLUSIVE: {summary} took {}\n\n", pretty_took(took)),
                None,
            );
        }
    }
    result
}

// -- query planner mode --

static PLANNER_ON: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct Planner {
    start_time: i64,
    count: u64,
    planned: HashSet<String>,
}

static PLANNER: Mutex<Option<Planner>> = Mutex::new(None);

/// Whether query planner mode is on.
pub fn query_planner_on() -> bool {
    PLANNER_ON.load(Ordering::Relaxed)
}

/// `FlipQueryPlannerMode`.
pub fn flip_query_planner() {
    let mut slot = PLANNER.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(planner) = slot.take() {
        PLANNER_ON.store(false, Ordering::Relaxed);
        drop(slot);
        show_text(&format!(
            "Query Planning done: {} queries analyzed",
            crate::numbers::human_int(planner.count)
        ));
    } else {
        *slot = Some(Planner {
            start_time: now_seconds(),
            ..Planner::default()
        });
        PLANNER_ON.store(true, Ordering::Relaxed);
        drop(slot);
        show_text("Query Planner mode on!");
    }
}

/// Whether `query` is one the planner has yet to write down.
pub fn needs_plan(query: &str) -> bool {
    PLANNER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|p| !p.planned.contains(query))
}

/// One `EXPLAIN QUERY PLAN` row, as Python prints the row tuple.
pub fn plan_row(id: i64, parent: i64, not_used: i64, detail: &str) -> String {
    format!("({id}, {parent}, {not_used}, {})", crate::pyjson::python_str_repr(detail))
}

/// `PrintQueryPlan`: count the query, and the first time write it down with
/// its plan.
pub fn print_query_plan(dir: &Path, query: &str, plan_lines: &[String]) {
    let start = {
        let mut slot = PLANNER.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(planner) = slot.as_mut() else { return };
        planner.count += 1;
        if !planner.planned.insert(query.to_owned()) {
            return;
        }
        planner.start_time
    };
    let path = dir.join(format!("client query planner - {}.log", file_stamp(start)));
    let first_word = if query.contains(' ') {
        query.split(' ').next().unwrap_or("unknown")
    } else {
        "unknown"
    };
    let mut text = format!("{}{first_word}\n{query}", line_prefix());
    if !plan_lines.is_empty() {
        text.push('\n');
        text.push_str(&plan_lines.join("\n"));
    }
    text.push_str("\n\n");
    append(&path, &text);
}

/// Put everything back as at boot (for tests).
pub fn reset() {
    *state() = State::default();
    *PLANNER.lock().unwrap_or_else(PoisonError::into_inner) = None;
    PLANNER_ON.store(false, Ordering::Relaxed);
}
