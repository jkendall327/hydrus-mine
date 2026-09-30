//! Thread watchers: a thread (or any watchable URL) checked again and again
//! for new files, on the same timing rules as subscriptions, until it 404s
//! or files stop appearing (`ClientImportWatchers.WatcherImport`).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::subscriptions::{CheckerOptions, SeedTime};

/// `WatcherImport.MIN_CHECK_PERIOD`: seconds after a check before "check
/// now" checks again.
pub const MIN_CHECK_PERIOD: i64 = 30;

/// `ClientImporting.CHECKER_STATUS_*`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckerStatus {
    #[default]
    Ok,
    /// Files stopped appearing.
    Dead,
    /// The thread is gone.
    NotFound,
}

/// A watcher's state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatcherState {
    /// The thread (normalised for requests).
    pub url: String,
    /// The thread's title, once read.
    pub subject: String,
    pub checker: CheckerOptions,
    /// Seconds; 0 before the first check.
    pub last_check_time: i64,
    pub next_check_time: i64,
    pub check_now: bool,
    pub checking_paused: bool,
    pub status: CheckerStatus,
    /// No network work until this time (seconds), and why.
    pub no_work_until: i64,
    pub no_work_until_reason: String,
    pub created: i64,
    /// Tags from outside (the Client API's) for everything the thread has.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub external_filterable_tags: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_additional_tags: Vec<(String, BTreeSet<String>)>,
}

impl WatcherState {
    pub fn new(url: impl Into<String>, checker: CheckerOptions, now: i64) -> Self {
        Self {
            url: url.into(),
            subject: "unknown subject".into(),
            checker,
            last_check_time: 0,
            next_check_time: 0,
            check_now: false,
            checking_paused: false,
            status: CheckerStatus::Ok,
            no_work_until: 0,
            no_work_until_reason: String::new(),
            created: now,
            external_filterable_tags: BTreeSet::new(),
            external_additional_tags: Vec::new(),
        }
    }

    /// `_UpdateNextCheckTime`: time the next check from the files found so
    /// far, declaring the thread dead (and pausing checks) if they have
    /// stopped coming.
    pub fn update_next_check_time(&mut self, seeds: &[SeedTime], now: i64) {
        if self.check_now {
            self.next_check_time = self.last_check_time + MIN_CHECK_PERIOD;
        } else if now <= self.no_work_until {
            self.next_check_time = self.no_work_until + 1;
        } else {
            if self.status == CheckerStatus::Ok && self.checker.is_dead(seeds, self.last_check_time)
            {
                self.status = CheckerStatus::Dead;
            }
            if self.status != CheckerStatus::Ok {
                self.checking_paused = true;
            }
            self.next_check_time = self
                .checker
                .next_check_time(seeds, self.last_check_time, now);
        }
    }

    /// `CheckNow`: check again soon, alive again.
    pub fn check_now(&mut self, seeds: &[SeedTime], now: i64) {
        self.check_now = true;
        self.checking_paused = false;
        self.no_work_until = 0;
        self.no_work_until_reason.clear();
        self.status = CheckerStatus::Ok;
        self.update_next_check_time(seeds, now);
    }

    /// Whether network work may happen now (`CheckCanDoNetworkWork`).
    pub fn can_do_network_work(&self, now: i64) -> bool {
        now > self.no_work_until
    }

    /// `CheckCanDoCheckerWork`: whether a check is due now.
    pub fn check_due(&self, now: i64) -> bool {
        !self.checking_paused
            && !self.url.is_empty()
            && self.status == CheckerStatus::Ok
            && now > self.next_check_time
            && self.can_do_network_work(now)
    }

    /// `_DelayWork`.
    pub fn delay(&mut self, seconds: i64, reason: &str, now: i64) {
        self.no_work_until = now + seconds;
        reason
            .lines()
            .next()
            .unwrap_or_default()
            .clone_into(&mut self.no_work_until_reason);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_watcher_checks_at_once_and_a_quiet_thread_dies() {
        let now = 1_000_000;
        let checker = crate::subscriptions::CheckerDefaults::default().watchers;
        let mut w = WatcherState::new("https://a.example/thread/1", checker, now);
        w.update_next_check_time(&[], now);
        assert!(w.check_due(now));
        // one file, four days ago: dead
        w.last_check_time = now;
        let old = [SeedTime {
            source_time: Some(now - 4 * 86400),
            created: now - 4 * 86400,
        }];
        w.update_next_check_time(&old, now);
        assert_eq!(w.status, CheckerStatus::Dead);
        assert!(w.checking_paused && !w.check_due(now + 86400));
        w.check_now(&old, now);
        assert_eq!(w.next_check_time, now + MIN_CHECK_PERIOD);
        assert!(w.check_due(now + MIN_CHECK_PERIOD + 1));
    }
}
