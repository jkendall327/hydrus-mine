//! The pace of the background workers `hydrus serve` runs (the reference's
//! `PotentialDuplicatesMaintenanceManager`, `DuplicatesAutoResolutionManager`
//! and `FilesMaintenanceManager`): one pass of each worker's loop, which
//! decides from the options and the GUI's published idle state whether to
//! work, for how long, and how long to rest after. Each pass takes its clock
//! and its work as arguments, so the loop the daemon runs is the loop a test
//! drives with time held still.
use std::cell::{Cell, RefCell};
use std::time::Duration;

use hydrus_core::bandwidth::{BandwidthType, Rule, Rules, Tracker};

use crate::Store;
use crate::duplicates::auto::AutoResolutionSettings;
use crate::file_maintenance::FileMaintenanceSettings;
use crate::idle_state::is_idle;
use crate::similar::SimilarFilesSettings;

/// The time a worker reads, in milliseconds since the epoch: for the GUI's
/// idle state, the throttle's buckets and how long a packet took.
pub trait WorkClock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// The system's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl WorkClock for SystemClock {
    fn now_ms(&self) -> i64 {
        hydrus_core::time::TimestampMs::now().millis()
    }
}

/// The wait after a pass that failed.
pub const AFTER_ERROR: Duration = Duration::from_secs(600);

/// The potential duplicates search's wait while its switch for now is off,
/// or there is nothing to search (as the reference's).
pub const SIMILAR_FILES_HOLD: Duration = Duration::from_secs(30);

/// Auto-resolution's wait while its switch for now is off (as the
/// reference's).
pub const AUTO_RESOLUTION_HOLD: Duration = Duration::from_secs(10);

/// Auto-resolution's wait once no rule has work left. The reference rests ten
/// minutes unless woken by new pairs or rules; nothing wakes the daemon's
/// loop, so checking every minute stands in for that.
pub const AUTO_RESOLUTION_DONE: Duration = Duration::from_secs(60);

/// File maintenance's wait while its throttle or switch holds it, before
/// asking again (the reference's one-second poll).
pub const FILE_MAINTENANCE_POLL: Duration = Duration::from_secs(1);

/// File maintenance's wait after a batch of work (as the reference's).
pub const FILE_MAINTENANCE_AFTER_WORK: Duration = Duration::from_millis(500);

/// File maintenance's wait when no job is due. The reference waits ten
/// minutes unless woken by new jobs; nothing wakes the daemon's loop from the
/// GUI's process, so checking every minute stands in for that.
pub const FILE_MAINTENANCE_NOTHING_DUE: Duration = Duration::from_secs(60);

/// The shortest rest between packets.
const SHORTEST_REST: Duration = Duration::from_millis(100);

fn since(clock: &dyn WorkClock, started_ms: i64) -> Duration {
    Duration::from_millis(u64::try_from(clock.now_ms() - started_ms).unwrap_or(0))
}

/// One pass of the potential duplicates search: `search` (a batch of files
/// each call, returning how many it searched) runs for the idle or normal
/// time packet, or not at all when that time's switch is off. Returns the
/// wait before the next pass: the packet's rest when files were searched.
pub fn similar_files_step(
    store: &Store,
    clock: &dyn WorkClock,
    mut search: impl FnMut(&Store) -> crate::Result<usize>,
) -> Duration {
    let settings: SimilarFilesSettings = match store.read(crate::settings::get) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "reading the similar-files search settings failed");
            return AFTER_ERROR;
        }
    };
    let mut pace = settings.pace(is_idle(store.dir(), clock.now_ms()));
    pace.allowed |= settings.work_hard;
    if !pace.allowed {
        return SIMILAR_FILES_HOLD;
    }
    let started = clock.now_ms();
    let mut total = 0;
    loop {
        let n = match search(store) {
            Ok(n) => n,
            Err(e) => {
                tracing::error!(error = %e, "the similar-files search failed");
                return AFTER_ERROR;
            }
        };
        total += n;
        let worked = since(clock, started);
        if n == 0 && total == 0 {
            return SIMILAR_FILES_HOLD;
        }
        if n == 0 || worked >= pace.work {
            tracing::debug!(files = total, "searched for similar files");
            return pace.rest(worked).max(SHORTEST_REST);
        }
    }
}

/// One pass of duplicates auto-resolution: `work` the rules for the idle or
/// normal time packet it is given (returning whether work is left), or hold
/// when that time's switch is off. Returns the wait before the next pass.
pub fn auto_resolution_step(
    store: &Store,
    clock: &dyn WorkClock,
    work: impl FnOnce(&Store, Duration) -> crate::Result<bool>,
) -> Duration {
    let settings: AutoResolutionSettings = match store.read(crate::settings::get) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "reading the auto-resolution settings failed");
            return AFTER_ERROR;
        }
    };
    let pace = settings.pace(is_idle(store.dir(), clock.now_ms()));
    if !pace.allowed {
        return AUTO_RESOLUTION_HOLD;
    }
    let started = clock.now_ms();
    match work(store, pace.work) {
        Ok(true) => pace.rest(since(clock, started)).max(SHORTEST_REST),
        Ok(false) => AUTO_RESOLUTION_DONE,
        Err(e) => {
            tracing::error!(error = %e, "duplicates auto-resolution failed");
            AFTER_ERROR
        }
    }
}

/// File maintenance's throttle (the reference's work rules and tracker): at
/// most the idle or normal time's "heavy work units" (a hundred weight each)
/// started in its span of seconds.
#[derive(Debug)]
pub struct FileMaintenanceThrottle {
    tracker: RefCell<Tracker>,
}

impl FileMaintenanceThrottle {
    pub fn new(clock: &dyn WorkClock) -> Self {
        Self {
            tracker: RefCell::new(Tracker::new(clock.now_ms().div_euclid(1000))),
        }
    }

    /// One pass: `run` works one batch of due jobs, asking the throttle
    /// before each file (`able`) and reporting each file's jobs' weight once
    /// done (`used`), and returns how many jobs it did. Returns the wait
    /// before the next pass: a second while the throttle or the switch holds
    /// work back.
    pub fn step<E: std::fmt::Display>(
        &self,
        store: &Store,
        clock: &dyn WorkClock,
        run: impl FnOnce(&dyn Fn() -> bool, &mut dyn FnMut(u64)) -> Result<u64, E>,
    ) -> Duration {
        let settings: FileMaintenanceSettings = match store.read(crate::settings::get) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!(error = %e, "reading the file maintenance settings failed");
                return AFTER_ERROR;
            }
        };
        let held = Cell::new(false);
        let able = || {
            let now = clock.now_ms();
            let (allowed, files, seconds) = settings.allowance(is_idle(store.dir(), now));
            let rules = Rules::new([Rule::new(
                BandwidthType::Requests,
                Some(seconds),
                files.saturating_mul(100),
            )]);
            let able = allowed
                && rules.can_start_request(&mut self.tracker.borrow_mut(), now.div_euclid(1000));
            if !able {
                held.set(true);
            }
            able
        };
        let mut used = |weight| {
            self.tracker
                .borrow_mut()
                .report_requests(weight, clock.now_ms().div_euclid(1000));
        };
        match run(&able, &mut used) {
            Ok(_) if held.get() => FILE_MAINTENANCE_POLL,
            Ok(0) => FILE_MAINTENANCE_NOTHING_DUE,
            Ok(jobs) => {
                tracing::debug!(jobs, "file maintenance");
                FILE_MAINTENANCE_AFTER_WORK
            }
            Err(e) => {
                tracing::error!(error = %e, "file maintenance failed");
                AFTER_ERROR
            }
        }
    }
}

/// A clock that moves only when told to, for tests.
#[derive(Debug, Default)]
pub struct HeldClock(std::sync::atomic::AtomicI64);

impl HeldClock {
    pub fn at(ms: i64) -> Self {
        Self(std::sync::atomic::AtomicI64::new(ms))
    }

    pub fn advance(&self, by: Duration) {
        let ms = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        self.0.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }
}

impl WorkClock for HeldClock {
    fn now_ms(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
