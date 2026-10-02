//! A watcher downloader page's watchers (the reference's
//! `SidebarImporterMultipleWatcher`): each watcher's row in the page's
//! list, as the daemon works it, the highlighted one's box, and the
//! page's totals.

use hydrus_core::pages::DownloaderPageSettings;
use hydrus_core::time::{pretty_time_delta, timestamp_to_pretty_time_delta};
use hydrus_core::watchers::{CheckerStatus, WatcherState};
use hydrus_store::live::{self, QueueLive};
use hydrus_store::queues::{self, StatusCounts};
use rusqlite::Connection;

use crate::gallery::{SimpleStatus, has_work, live_line};

/// How long a watcher just added, or a thread the page watches already,
/// says so in the list (`ADDED_TIMESTAMP_DURATION`).
pub const SAID_FOR: i64 = 15;

/// One of a watcher page's watchers, as last read.
#[derive(Debug, Clone, PartialEq)]
pub struct WatcherRow {
    pub queue: i64,
    pub state: WatcherState,
    /// Its file log's seeds by status, and its check log's.
    pub files: StatusCounts,
    pub checks: StatusCounts,
    pub files_paused: bool,
    pub live: QueueLive,
}

/// A watcher page's own part: its watchers, as its list shows them, and
/// what it gives the watchers it makes.
#[derive(Debug, Clone)]
pub struct WatcherView {
    /// The page, whose key and name its watchers carry.
    pub page_key: hydrus_core::pages::PageKey,
    pub page_name: String,
    /// Its watchers' queues, in the page's order.
    pub queues: Vec<i64>,
    /// As last read, in the list's order.
    pub watchers: Vec<WatcherRow>,
    /// Its checker and import options, and the watcher it shows.
    pub state: hydrus_core::pages::DownloaderPageState,
    /// The list's sort.
    pub sort: (Column, bool),
    /// The watcher selected in the list.
    pub selected: Option<i64>,
    /// Watchers just added, and those a URL entered again was already
    /// watched by, and when (for "just added", "already watching").
    pub added: Vec<(i64, i64)>,
    pub already: Vec<(i64, i64)>,
    pub settings: DownloaderPageSettings,
    pub short_summary: (bool, bool),
}

impl WatcherView {
    /// Read the watchers again, and those the page has made since (its
    /// key's) at its end; whether anything changed.
    pub fn refresh(&mut self, conn: &Connection) -> hydrus_store::Result<bool> {
        for queue in queues::queues_with_page_key(conn, &self.page_key.0)? {
            if queue.kind == queues::QueueKind::Watcher && !self.queues.contains(&queue.id) {
                self.queues.push(queue.id);
            }
        }
        let mut read = Vec::with_capacity(self.queues.len());
        let mut gone = Vec::new();
        for &queue in &self.queues {
            match WatcherRow::read(conn, queue)? {
                Some(watcher) => read.push(watcher),
                None => gone.push(queue),
            }
        }
        self.queues.retain(|q| !gone.contains(q));
        let now = crate::page::now();
        sort(&mut read, self.sort.0, self.sort.1, now, &|w| {
            Some(self.simple_status(w, now).0)
        });
        let changed = read != self.watchers || !gone.is_empty();
        self.watchers = read;
        Ok(changed)
    }

    pub fn watcher(&self, queue: i64) -> Option<&WatcherRow> {
        self.watchers.iter().find(|w| w.queue == queue)
    }

    /// What the list says of a watcher just added or entered again, for a
    /// while (`GetWatcherSimpleStatus`), else its own status.
    pub fn simple_status(&self, watcher: &WatcherRow, now: i64) -> (SimpleStatus, String) {
        let said = |list: &[(i64, i64)]| {
            list.iter()
                .any(|&(q, at)| q == watcher.queue && now <= at + SAID_FOR)
        };
        if said(&self.added) {
            (SimpleStatus::Working, "just added".into())
        } else if said(&self.already) {
            (SimpleStatus::Working, "already watching".into())
        } else {
            watcher.simple_status(now)
        }
    }

    /// The list's rows, in its order.
    pub fn rows(&self, now: i64) -> Vec<[String; 6]> {
        self.watchers
            .iter()
            .map(|w| {
                w.row(
                    self.state.highlighted == Some(w.queue),
                    &self.simple_status(w, now).1,
                    &self.settings,
                    self.short_summary,
                    now,
                )
            })
            .collect()
    }
}

/// The list's columns (`COLUMN_LIST_WATCHERS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Subject,
    Files,
    Checker,
    Status,
    Items,
    Added,
}

impl Column {
    pub const ALL: [Column; 6] = [
        Column::Subject,
        Column::Files,
        Column::Checker,
        Column::Status,
        Column::Items,
        Column::Added,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Column::Subject => "subject",
            Column::Files => "files status",
            Column::Checker => "checking status",
            Column::Status => "status",
            Column::Items => "items",
            Column::Added => "added",
        }
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// `TimestampToPrettyTimeDelta` with its `no_prefix` (no "in " before a
/// time to come).
fn until(timestamp: i64, now: i64) -> String {
    let text = timestamp_to_pretty_time_delta(timestamp, now, " ago");
    text.strip_prefix("in ").map_or(text.clone(), str::to_owned)
}

impl WatcherRow {
    /// The watcher on `queue`, if it is one.
    pub fn read(conn: &Connection, queue: i64) -> hydrus_store::Result<Option<Self>> {
        let Some(row) = queues::queue(conn, queue)? else {
            return Ok(None);
        };
        let Some(state) = hydrus_store::watchers::watcher_state(&row) else {
            return Ok(None);
        };
        Ok(Some(Self {
            queue,
            state,
            files: queues::file_seed_counts(conn, queue)?,
            checks: queues::gallery_seed_counts(conn, queue)?,
            files_paused: row.files_paused,
            live: live::live(conn, &[queue])?
                .remove(&queue)
                .unwrap_or_default(),
        }))
    }

    /// Its thread's title (`GetSubject`).
    pub fn subject(&self) -> &str {
        if self.state.subject.is_empty() {
            "unknown subject"
        } else {
            &self.state.subject
        }
    }

    /// The thread 404'd or files stopped appearing (`IsDead`).
    pub fn dead(&self) -> bool {
        self.state.status != CheckerStatus::Ok
    }

    pub fn files_finished(&self) -> bool {
        !has_work(&self.files)
    }

    /// Whether the daemon is downloading for it now.
    pub fn working(&self) -> bool {
        self.live.file_job.is_some() || self.live.gallery_job.is_some()
    }

    /// Whether it still has files to get and isn't paused
    /// (`CurrentlyWorking`, which closing its page asks about).
    pub fn importing(&self) -> bool {
        !self.files_finished() && !self.files_paused
    }

    /// When it checks next, if it has a time (none before its first check).
    fn next_check(&self) -> Option<i64> {
        (self.state.next_check_time != 0).then_some(self.state.next_check_time)
    }

    /// Why it isn't working, while it can't (a server's error, say),
    /// and when it will again.
    fn held_off(&self, now: i64) -> Option<String> {
        let s = &self.state;
        (now <= s.no_work_until).then(|| match self.next_check() {
            None => format!(
                "{} - working again {}",
                s.no_work_until_reason,
                timestamp_to_pretty_time_delta(s.no_work_until, now, " ago")
            ),
            Some(next) => format!(
                "{} - next check {}",
                s.no_work_until_reason,
                timestamp_to_pretty_time_delta(s.no_work_until.max(next), now, " ago")
            ),
        })
    }

    /// What it is up to (`WatcherImport.GetSimpleStatus`).
    pub fn simple_status(&self, now: i64) -> (SimpleStatus, String) {
        let s = &self.state;
        let next = self.next_check();
        if let Some(text) = self.held_off(now) {
            return (SimpleStatus::Deferred, text);
        }
        let check_passed = next.is_none_or(|n| now > n);
        let checker_go = check_passed && !s.checking_paused;
        let files_go = !self.files_finished() && !self.files_paused;
        let fixed = |status: SimpleStatus| (status, status.text().to_owned());
        if checker_go || files_go {
            fixed(if self.working() {
                SimpleStatus::Working
            } else {
                SimpleStatus::Pending
            })
        } else if s.status == CheckerStatus::NotFound {
            (SimpleStatus::Done, "404".into())
        } else if s.status == CheckerStatus::Dead {
            (SimpleStatus::Done, "DEAD".into())
        } else if s.checking_paused {
            fixed(if self.working() {
                SimpleStatus::Pausing
            } else {
                SimpleStatus::Paused
            })
        } else if check_passed {
            fixed(SimpleStatus::Pending)
        } else {
            (SimpleStatus::Deferred, until(next.unwrap_or(0), now))
        }
    }

    /// Its row in the page's list (`_ConvertDataToDisplayTuple`): the
    /// subject (starred if shown), its files' pause, its checking's pause
    /// (or stop, dead), its status, its short file log status and when it
    /// was added.
    pub fn row(
        &self,
        highlighted: bool,
        status: &str,
        settings: &DownloaderPageSettings,
        short_summary: (bool, bool),
        now: i64,
    ) -> [String; 6] {
        let checking = if self.dead() {
            settings.stop_character.clone()
        } else if self.state.checking_paused {
            settings.pause_character.clone()
        } else {
            String::new()
        };
        [
            if highlighted {
                format!("* {}", self.subject())
            } else {
                self.subject().to_owned()
            },
            if self.files_paused {
                settings.pause_character.clone()
            } else {
                String::new()
            },
            checking,
            status.to_owned(),
            queues::file_log_short_status(&self.files, short_summary.0, short_summary.1),
            hydrus_core::time::timestamp_to_pretty_time_delta_minutes(
                self.state.created,
                now,
                " ago",
            ),
        ]
    }

    /// The highlighted watcher box's files line (`GetStatus`): what its
    /// file work is doing.
    pub fn files_line(&self, now: i64) -> String {
        self.held_off(now).unwrap_or_else(|| {
            live_line(&self.live.files_status, self.files_paused, self.working())
        })
    }

    /// The box's checker line (`WatcherReviewPanel._UpdateStatus`): what
    /// its checking is doing, or when it checks next.
    pub fn checker_line(&self, now: i64) -> String {
        let s = &self.state;
        let line = self.held_off(now).unwrap_or_else(|| {
            live_line(&self.live.gallery_status, s.checking_paused, self.working())
        });
        match self.next_check() {
            _ if s.checking_paused || !line.is_empty() => line,
            None => line,
            Some(next) if now > next => "checking imminently".into(),
            Some(next) => format!(
                "next check {}",
                if next == now {
                    "now".into()
                } else {
                    format!("in {}", pretty_time_delta(next - now, false))
                }
            ),
        }
    }
}

/// How fast files were appearing at the watcher's last check, as its box
/// says ("at last check, found 5 files in previous 1 day").
pub fn velocity(conn: &Connection, watcher: &WatcherRow) -> hydrus_store::Result<String> {
    let seeds = queues::file_seeds(conn, watcher.queue)?;
    Ok(watcher.state.checker.pretty_current_velocity(
        &hydrus_store::watchers::seed_times(&seeds),
        watcher.state.last_check_time,
    ))
}

/// Sort watchers by a column, as the reference's list sorts
/// (`_ConvertDataToSortTuple`, stable); `status` gives the list's own
/// status for one, if it says something else.
pub fn sort(
    watchers: &mut [WatcherRow],
    column: Column,
    ascending: bool,
    now: i64,
    status: &dyn Fn(&WatcherRow) -> Option<SimpleStatus>,
) {
    let checking_rank = |w: &WatcherRow| {
        if w.dead() {
            -1
        } else {
            i8::from(!w.state.checking_paused)
        }
    };
    let status_key = |w: &WatcherRow| {
        let simple = status(w).unwrap_or_else(|| w.simple_status(now).0);
        let then = match w.state.status {
            CheckerStatus::Ok => w.state.next_check_time,
            CheckerStatus::Dead => 1,
            CheckerStatus::NotFound => 2,
        };
        (simple, then)
    };
    watchers.sort_by(|a, b| {
        let order = match column {
            Column::Subject => a.subject().cmp(b.subject()),
            Column::Files => a.files_paused.cmp(&b.files_paused),
            Column::Checker => checking_rank(a).cmp(&checking_rank(b)),
            Column::Status => status_key(a).cmp(&status_key(b)),
            Column::Items => {
                let progress = |w: &WatcherRow| {
                    let (done, total) = queues::file_log_value_range(&w.files);
                    (total, done)
                };
                progress(a).cmp(&progress(b))
            }
            Column::Added => a.state.created.cmp(&b.state.created),
        };
        if ascending { order } else { order.reverse() }
    });
}

/// The page's progress, for its tab (`GetValueRange`): its unfinished
/// watchers' files done and in all.
pub fn value_range(watchers: &[WatcherRow]) -> (usize, usize) {
    watchers
        .iter()
        .map(|w| queues::file_log_value_range(&w.files))
        .filter(|(done, all)| done != all)
        .fold((0, 0), |(v, r), (done, all)| (v + done, r + all))
}

/// Why closing a watcher page needs asking about (`CheckAbleToClose`):
/// watchers still importing, or (if `confirm_non_empty`) any files held.
pub fn close_veto(watchers: &[WatcherRow], confirm_non_empty: bool) -> Option<String> {
    let working = watchers.iter().filter(|w| w.importing()).count();
    if working > 0 {
        return Some(format!(
            "{} watchers are still importing.",
            hydrus_core::numbers::human_int(working as u64)
        ));
    }
    let held: usize = watchers
        .iter()
        .map(|w| w.files.values().sum::<usize>())
        .sum();
    (confirm_non_empty && held > 0).then(|| {
        format!(
            "This is a watcher page holding {} import objects.",
            hydrus_core::numbers::human_int(held as u64)
        )
    })
}

/// The page's totals over its watchers' file logs (`_UpdateImportStatus`):
/// "3 watchers - 12/40" and the full status, or "waiting for new watchers"
/// with none. (The reference words its dead watchers, "2 DEAD - ", and
/// then leaves that out.)
pub fn totals(watchers: &[WatcherRow]) -> (String, String) {
    if watchers.is_empty() {
        return ("waiting for new watchers".into(), String::new());
    }
    let mut total = StatusCounts::new();
    for watcher in watchers {
        for (status, n) in &watcher.files {
            *total.entry(*status).or_default() += n;
        }
    }
    let (done, all) = queues::file_log_value_range(&total);
    (
        format!(
            "{} watchers - {}",
            hydrus_core::numbers::human_int(watchers.len() as u64),
            queues::value_range_text(done, all)
        ),
        queues::file_log_status(&total),
    )
}

#[cfg(test)]
mod tests {
    use hydrus_core::subscriptions::CheckerOptions;
    use hydrus_store::queues::SeedStatus;

    use super::*;

    const NOW: i64 = 100_000;

    fn watcher(files: &[(SeedStatus, usize)]) -> WatcherRow {
        WatcherRow {
            queue: 1,
            state: WatcherState::new(
                "https://boards.example/thread/1",
                CheckerOptions::default(),
                NOW - 600,
            ),
            files: files.iter().copied().collect(),
            checks: StatusCounts::new(),
            files_paused: false,
            live: QueueLive::default(),
        }
    }

    fn downloading() -> QueueLive {
        QueueLive {
            gallery_job: Some(live::JobLive::default()),
            ..QueueLive::default()
        }
    }

    #[test]
    fn a_watchers_status_as_the_reference_reckons_it() {
        use SeedStatus::{SuccessfulAndNew as New, Unknown};
        let status = |w: &WatcherRow| w.simple_status(NOW);
        // never checked: pending, or working while it downloads
        let mut w = watcher(&[]);
        assert_eq!(status(&w), (SimpleStatus::Pending, "pending".into()));
        w.live = downloading();
        assert_eq!(status(&w), (SimpleStatus::Working, "working".into()));
        // checked, the next check to come: when (no "in")
        let mut w = watcher(&[(New, 3)]);
        w.state.last_check_time = NOW - 60;
        w.state.next_check_time = NOW + 300;
        assert_eq!(status(&w), (SimpleStatus::Deferred, "5 minutes".into()));
        // ... unless it has files to get
        w.files.insert(Unknown, 1);
        assert_eq!(status(&w).0, SimpleStatus::Pending);
        w.files.remove(&Unknown);
        // checking paused: paused, or pausing while it still downloads
        w.state.checking_paused = true;
        assert_eq!(status(&w), (SimpleStatus::Paused, String::new()));
        w.live = downloading();
        assert_eq!(
            status(&w),
            (SimpleStatus::Pausing, "pausing\u{2026}".into())
        );
        w.live = QueueLive::default();
        // dead or gone (their checking paused, as the daemon leaves them)
        w.state.status = CheckerStatus::Dead;
        assert_eq!(status(&w), (SimpleStatus::Done, "DEAD".into()));
        w.state.status = CheckerStatus::NotFound;
        assert_eq!(status(&w), (SimpleStatus::Done, "404".into()));
        // held off: why, and until when
        let mut w = watcher(&[(New, 3)]);
        w.state.next_check_time = NOW + 60;
        w.state.no_work_until = NOW + 600;
        w.state.no_work_until_reason = "server busy".into();
        assert_eq!(
            status(&w),
            (
                SimpleStatus::Deferred,
                "server busy - next check in 10 minutes".into()
            )
        );
        w.state.next_check_time = 0;
        assert_eq!(status(&w).1, "server busy - working again in 10 minutes");
    }

    #[test]
    fn a_watchers_row_and_box_as_the_reference_shows_them() {
        use SeedStatus::{SuccessfulAndNew as New, Unknown};
        let settings = DownloaderPageSettings::default();
        let mut w = watcher(&[(New, 2), (Unknown, 1)]);
        w.state.subject = "a thread".into();
        let row = w.row(true, "pending", &settings, (true, true), NOW);
        assert_eq!(row[0], "* a thread");
        assert_eq!((row[1].as_str(), row[2].as_str()), ("", ""));
        assert_eq!(row[3], "pending");
        assert_eq!(row[4], "2/3 - 2N");
        assert_eq!(row[5], "10 minutes ago");
        // its files paused, its checking paused, then dead
        w.files_paused = true;
        w.state.checking_paused = true;
        let row = w.row(false, "", &settings, (true, true), NOW);
        assert_eq!(row[0], "a thread");
        assert_eq!(row[1], settings.pause_character);
        assert_eq!(row[2], settings.pause_character);
        w.state.status = CheckerStatus::Dead;
        assert_eq!(
            w.row(false, "", &settings, (true, true), NOW)[2],
            settings.stop_character
        );
        // (no subject yet)
        w.state.subject.clear();
        assert_eq!(w.subject(), "unknown subject");

        // the box's checker line: when it checks next, or what it does
        let mut w = watcher(&[]);
        w.state.next_check_time = NOW + 300;
        assert_eq!(w.checker_line(NOW), "next check in 5 minutes");
        assert_eq!(w.checker_line(NOW + 300), "next check now");
        assert_eq!(w.checker_line(NOW + 301), "checking imminently");
        w.live.gallery_status = "checking".into();
        assert_eq!(w.checker_line(NOW), "checking");
        w.live.gallery_status.clear();
        w.state.checking_paused = true;
        assert_eq!(w.checker_line(NOW), "paused");
        w.files_paused = true;
        assert_eq!(w.files_line(NOW), "paused");
    }

    #[test]
    fn a_watcher_pages_totals_and_close_question() {
        use SeedStatus::{SuccessfulAndNew as New, Unknown};
        assert_eq!(
            totals(&[]),
            ("waiting for new watchers".into(), String::new())
        );
        let done = watcher(&[(New, 2)]);
        let mut going = watcher(&[(New, 1), (Unknown, 2)]);
        going.state.status = CheckerStatus::Dead;
        let (top, bottom) = totals(&[done.clone(), going.clone()]);
        // (the dead aren't counted, as in the reference)
        assert_eq!(top, "2 watchers - 3/5");
        assert_eq!(bottom, "3 successful");
        assert_eq!(value_range(&[done.clone(), going.clone()]), (1, 3));
        assert_eq!(
            close_veto(&[done.clone(), going.clone()], true).as_deref(),
            Some("1 watchers are still importing.")
        );
        going.files_paused = true;
        assert_eq!(
            close_veto(&[done.clone(), going.clone()], true).as_deref(),
            Some("This is a watcher page holding 5 import objects.")
        );
        assert_eq!(close_veto(&[done, going], false), None);
    }

    #[test]
    fn a_watcher_just_added_or_entered_again_says_so_for_a_while() {
        let w = watcher(&[]);
        let mut view = WatcherView {
            page_key: hydrus_core::pages::PageKey([1; 32]),
            page_name: "watcher".into(),
            queues: vec![1],
            watchers: vec![w.clone()],
            state: hydrus_core::pages::DownloaderPageState::default(),
            sort: (Column::Status, true),
            selected: None,
            added: vec![(1, NOW)],
            already: Vec::new(),
            settings: DownloaderPageSettings::default(),
            short_summary: (true, true),
        };
        assert_eq!(view.simple_status(&w, NOW + SAID_FOR).1, "just added");
        assert_eq!(view.simple_status(&w, NOW + SAID_FOR + 1).1, "pending");
        view.added.clear();
        view.already.push((1, NOW));
        assert_eq!(view.simple_status(&w, NOW).1, "already watching");
    }
}
