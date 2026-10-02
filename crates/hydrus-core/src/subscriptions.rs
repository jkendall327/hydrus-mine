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

/// The checker timings new subscriptions and new watchers start with (the
/// client options' `default_subscription_checker_options` and
/// `default_thread_watcher_options`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckerDefaults {
    pub subscriptions: CheckerOptions,
    pub watchers: CheckerOptions,
}

impl Default for CheckerDefaults {
    /// The reference's: "artist subscription" and "thread".
    fn default() -> Self {
        Self {
            subscriptions: CheckerOptions {
                intended_files_per_check: 4.0,
                never_faster_than: 86400,
                never_slower_than: 90 * 86400,
                death_file_velocity: (1, 180 * 86400),
            },
            watchers: CheckerOptions {
                intended_files_per_check: 4.0,
                never_faster_than: 300,
                never_slower_than: 86400,
                death_file_velocity: (1, 3 * 86400),
            },
        }
    }
}

/// Settings for new gallery searches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GalleryDefaults {
    /// Stop a search after this many new files (`gallery_file_limit`;
    /// `None`: no limit).
    pub file_limit: Option<u64>,
    /// The downloader a new gallery page uses: its key (hex) and name
    /// (`default_gug_key`, `default_gug_name`; `None` while unset).
    #[serde(default)]
    pub gug: Option<(String, String)>,
}

impl Default for GalleryDefaults {
    fn default() -> Self {
        Self {
            file_limit: Some(2000),
            gug: None,
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
    pub fn velocity_time(self) -> i64 {
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

    /// How fast files were appearing at the last check, as a watcher's
    /// box words it (`GetPrettyCurrentVelocity`): "at last check, found 5
    /// files in previous 1 day".
    pub fn pretty_current_velocity(&self, seeds: &[SeedTime], last_check_time: i64) -> String {
        if seeds.is_empty() {
            return if last_check_time == 0 {
                "no files yet".into()
            } else {
                "no files, unable to determine velocity".into()
            };
        }
        let (found, delta) = self.current_velocity(seeds, last_check_time);
        format!(
            "at last check, found {} files in previous {}",
            crate::numbers::human_int(found.max(0) as u64),
            crate::time::pretty_time_delta(delta, false)
        )
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
        !self.dead && (self.check_now || now > self.next_check_time)
    }

    /// `CheckNow`: check at the next chance, alive again.
    pub fn check_now(&mut self) {
        self.check_now = true;
        self.paused = false;
        self.next_check_time = 0;
        self.dead = false;
    }

    /// `RegisterSyncComplete` (after the history is compacted): note the
    /// check, then time the next one from what the history now holds, or
    /// declare the query dead (and pause it if it has no files left to get).
    pub fn register_sync_complete(
        &mut self,
        checker: &CheckerOptions,
        seeds: &[SeedTime],
        has_file_work: bool,
        now: i64,
    ) {
        self.last_check_time = now;
        self.check_now = false;
        if checker.is_dead(seeds, self.last_check_time) {
            self.dead = true;
            if !has_file_work {
                self.paused = true;
            }
        }
        self.next_check_time = checker.next_check_time(seeds, self.last_check_time, now);
    }
}

/// What history compaction needs to know about a file seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileLogEntry<'a> {
    /// Not yet worked on.
    pub unknown: bool,
    /// The note of a post that found child files (status "successful and
    /// child files"), which says how many.
    pub child_files_note: Option<&'a str>,
    pub time: SeedTime,
}

/// `_GetListOfParentsWithChildren`: split a file history into posts with
/// the child files they found (index ranges, in order).
pub fn parents_with_children(entries: &[FileLogEntry<'_>]) -> Vec<std::ops::Range<usize>> {
    let mut groups = Vec::new();
    let mut start = 0;
    let mut additional_children = 0u64;
    let mut until_next_parent = false;
    for (i, entry) in entries.iter().enumerate() {
        if until_next_parent && entry.child_files_note.is_some() {
            groups.push(start..i);
            start = i;
            until_next_parent = false;
        }
        additional_children = additional_children.saturating_sub(1);
        if let Some(note) = entry.child_files_note {
            match found_count(note) {
                Some(n) => additional_children += n,
                None => until_next_parent = true,
            }
        }
        if !until_next_parent && additional_children == 0 {
            groups.push(start..i + 1);
            start = i + 1;
        }
    }
    if start < entries.len() {
        groups.push(start..entries.len());
    }
    groups
}

/// The N of a note starting "Found N" (`(?<=^Found )\d+`).
fn found_count(note: &str) -> Option<u64> {
    let rest = note.strip_prefix("Found ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    // (Python's int() of a huge count; never fewer than none)
    digits
        .parse::<u64>()
        .ok()
        .or_else(|| (!digits.is_empty()).then_some(u64::MAX))
}

/// `GetApproxNumMasterFileSeeds`: how many posts a history holds.
pub fn num_master_file_seeds(entries: &[FileLogEntry<'_>]) -> usize {
    parents_with_children(entries).len()
}

/// `CanCompact` and `Compact` for a file history: the entries to drop, the
/// posts (with their children) beyond the latest `keep` that are done and
/// no newer than `before`. Nothing is dropped unless one of those is older
/// than `before`.
pub fn compact_file_log(entries: &[FileLogEntry<'_>], keep: usize, before: i64) -> Vec<usize> {
    if entries.len() <= keep || keep == 0 {
        return Vec::new();
    }
    let groups = parents_with_children(entries);
    let compactible: Vec<usize> = groups[..groups.len().saturating_sub(keep)]
        .iter()
        .flat_map(Clone::clone)
        .collect();
    let can_compact = compactible.iter().any(|&i| {
        let e = &entries[i];
        !e.unknown && e.time.velocity_time() < before
    });
    if !can_compact {
        return Vec::new();
    }
    compactible
        .into_iter()
        .filter(|&i| {
            let e = &entries[i];
            !(e.unknown || e.time.velocity_time() > before)
        })
        .collect()
}

/// `CanCompact` and `Compact` for a gallery log of `(not yet worked on,
/// created)` entries: the entries to drop.
pub fn compact_gallery_log(entries: &[(bool, i64)], keep: usize, before: i64) -> Vec<usize> {
    if entries.len() <= keep || keep == 0 {
        return Vec::new();
    }
    let compactible = 0..entries.len() - keep;
    let can_compact = compactible
        .clone()
        .any(|i| !entries[i].0 && entries[i].1 < before);
    if !can_compact {
        return Vec::new();
    }
    compactible
        .filter(|&i| !(entries[i].0 || entries[i].1 > before))
        .collect()
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
