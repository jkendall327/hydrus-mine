//! Subscriptions: saved gallery searches checked again and again, each
//! query keeping the history of what it found so nothing is fetched twice.
//!
//! How often a query is checked follows its *checker options*: aim for a
//! number of new files per check, from how fast files have been appearing,
//! within bounds; a query finding too few files over a period is dead. The
//! logic is the reference's `CheckerOptions`.

use serde::{Deserialize, Serialize};

use crate::import_options::{ImportOptionsSlice, TagImportOptions};

/// When to check a query again, and when to give up on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckerOptions {
    /// Aim for this many new files per check.
    pub intended_files_per_check: f64,
    /// Seconds.
    pub never_faster_than: i64,
    /// Seconds.
    pub never_slower_than: i64,
    /// Dead below this many files per this many seconds (0 files: never
    /// dies).
    pub death_file_velocity: (i64, i64),
}

impl Default for CheckerOptions {
    fn default() -> Self {
        Self {
            intended_files_per_check: 8.0,
            never_faster_than: 300,
            never_slower_than: 86400,
            death_file_velocity: (1, 86400),
        }
    }
}

/// What the timing needs to know about one found file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedTime {
    /// When the site says it was posted.
    pub source_time: Option<i64>,
    /// When the query found it.
    pub created: i64,
}

impl SeedTime {
    /// `_GetSourceTimestampForVelocityCalculations`: the post time, else
    /// just before it was found.
    fn velocity_time(self) -> i64 {
        self.source_time.unwrap_or(self.created - 30)
    }
}

impl CheckerOptions {
    /// `(files found, over how many seconds)` recently.
    fn current_velocity(&self, seeds: &[SeedTime], last_check_time: i64) -> (i64, i64) {
        let (_, death_period) = self.death_file_velocity;
        let since = last_check_time - death_period;
        let found = seeds.iter().filter(|s| s.velocity_time() >= since).count() as i64;
        let period = match seeds.iter().map(|s| s.velocity_time()).min() {
            None => death_period,
            Some(earliest) => (last_check_time - earliest).max(30).min(death_period),
        };
        (found, period)
    }

    pub fn has_static_check_time(&self) -> bool {
        self.never_faster_than == self.never_slower_than
    }

    pub fn never_dies(&self) -> bool {
        self.death_file_velocity.0 == 0
    }

    /// How far back file history matters (`GetDeathFileVelocityPeriod`).
    pub fn death_file_velocity_period(&self) -> i64 {
        let (_, mut period) = self.death_file_velocity;
        if self.has_static_check_time() {
            period = period.min(self.never_faster_than * 5);
        }
        if self.never_dies() || self.has_static_check_time() {
            period = period.min(6 * 60 * 86400);
        }
        period
    }

    /// When to check next (`GetNextCheckTime`); 0 means now.
    pub fn next_check_time(&self, seeds: &[SeedTime], last_check_time: i64, now: i64) -> i64 {
        if seeds.is_empty() {
            return if last_check_time == 0 {
                0
            } else {
                now + self.never_slower_than
            };
        }
        if self.has_static_check_time() {
            let period = self.never_slower_than;
            let mut next = last_check_time + period;
            // catch up on missed checks without checking over and over
            while period > 0 && now > next + period {
                next += period;
            }
            return next;
        }
        let (found, delta) = self.current_velocity(seeds, last_check_time);
        let period = if found == 0 {
            self.never_slower_than
        } else {
            let time_per_file = delta.div_euclid(found);
            let ideal = self.intended_files_per_check * time_per_file as f64;
            let latest = seeds.iter().map(|s| s.velocity_time()).max().unwrap_or(0);
            let since_latest = (last_check_time - latest).max(30);
            let never_faster = (self.never_faster_than as f64).max(since_latest as f64 * 0.25);
            never_faster.max(ideal).min(self.never_slower_than as f64) as i64
        };
        last_check_time + period
    }

    /// Whether files are appearing too slowly to keep checking (`IsDead`).
    pub fn is_dead(&self, seeds: &[SeedTime], last_check_time: i64) -> bool {
        if seeds.is_empty() && last_check_time == 0 {
            return false;
        }
        let (found, delta) = self.current_velocity(seeds, last_check_time);
        let (death_found, death_delta) = self.death_file_velocity;
        (found as f64 / delta as f64) < (death_found as f64 / death_delta as f64)
    }
}

/// A subscription's settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionSettings {
    /// The downloader (GUG or nested GUG): key (hex) and name, found by key
    /// and then by name.
    pub gug_key: String,
    pub gug_name: String,
    pub checker: CheckerOptions,
    /// New files a query may find on its first check (`None`: no limit).
    pub initial_file_limit: Option<u64>,
    /// ... and on later checks.
    pub periodic_file_limit: Option<u64>,
    /// Don't warn when a check hits its file limit (a sample of a big
    /// search, not a full history).
    pub this_is_a_random_sample: bool,
    pub paused: bool,
    /// The subscription's own import options.
    pub import_options: ImportOptionsSlice,
    /// No work until this time (seconds), and why.
    pub no_work_until: i64,
    pub no_work_until_reason: String,
    pub show_a_popup_while_working: bool,
    pub publish_files_to_popup_button: bool,
    pub publish_files_to_page: bool,
    pub publish_label_override: Option<String>,
    pub merge_query_publish_events: bool,
}

impl Default for SubscriptionSettings {
    fn default() -> Self {
        Self {
            gug_key: String::new(),
            gug_name: String::new(),
            checker: CheckerOptions::default(),
            initial_file_limit: Some(100),
            periodic_file_limit: Some(100),
            this_is_a_random_sample: false,
            paused: false,
            import_options: ImportOptionsSlice::default(),
            no_work_until: 0,
            no_work_until_reason: String::new(),
            show_a_popup_while_working: true,
            publish_files_to_popup_button: true,
            publish_files_to_page: false,
            publish_label_override: None,
            merge_query_publish_events: true,
        }
    }
}

/// One query of a subscription.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryState {
    pub query_text: String,
    pub display_name: Option<String>,
    /// Check at the next chance, whatever the timing says.
    pub check_now: bool,
    /// Seconds; 0 before the first check.
    pub last_check_time: i64,
    pub next_check_time: i64,
    pub paused: bool,
    /// Stopped for finding too few files.
    pub dead: bool,
    /// How many file and gallery log entries are kept before old ones are
    /// dropped.
    pub file_seed_compaction_number: u64,
    pub gallery_seed_compaction_number: u64,
    /// Tags added to everything this query finds.
    pub tag_import_options: TagImportOptions,
}

impl QueryState {
    pub fn new(query_text: impl Into<String>) -> Self {
        Self {
            query_text: query_text.into(),
            display_name: None,
            check_now: false,
            last_check_time: 0,
            next_check_time: 0,
            paused: false,
            dead: false,
            file_seed_compaction_number: 250,
            gallery_seed_compaction_number: 100,
            tag_import_options: TagImportOptions::default(),
        }
    }

    /// What people see: the display name if set, else the query.
    pub fn human_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.query_text)
    }

    pub fn is_initial_sync(&self) -> bool {
        self.last_check_time == 0
    }

    /// `IsSyncDue`.
    pub fn is_sync_due(&self, now: i64) -> bool {
        !self.dead && (self.check_now || self.next_check_time <= now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_follow_file_velocity() {
        let checker = CheckerOptions::default();
        let now = 1_000_000;
        assert_eq!(checker.next_check_time(&[], 0, now), 0);
        assert_eq!(checker.next_check_time(&[], now, now), now + 86400);
        // 24 files over the 23.5 hours since the first: 84600 / 24 = 3525
        // seconds a file, times 8 files a check
        let seeds: Vec<SeedTime> = (0..24)
            .map(|i| SeedTime {
                source_time: Some(now - 3600 * i - 1800),
                created: now,
            })
            .collect();
        assert_eq!(checker.next_check_time(&seeds, now, now), now + 8 * 3525);
        assert!(!checker.is_dead(&seeds, now));
        // nothing for two days: dead
        let old = [SeedTime {
            source_time: Some(now - 2 * 86400),
            created: now - 2 * 86400,
        }];
        assert!(checker.is_dead(&old, now));
    }
}
