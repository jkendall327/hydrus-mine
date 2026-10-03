//! What the daemon's queues are doing now, for the pages that show them:
//! each queue's status lines and its current downloads (the live state the
//! reference's importer sidebars read from their importers), which `hydrus
//! serve` keeps here as it works and the client reads; and downloads
//! another process asks the daemon to cancel.

use std::collections::HashMap;

use hydrus_core::numbers::human_bytes;
use rusqlite::Connection;

use crate::error::Result;

/// A queue's live state.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueueLive {
    /// What its file work is doing (the reference's `_files_status`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub files_status: String,
    /// What its gallery work is doing (`_gallery_status`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub gallery_status: String,
    /// The file it is downloading, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_job: Option<JobLive>,
    /// The gallery page it is downloading, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery_job: Option<JobLive>,
}

/// The daemon's network use, for the main window's status bar: when its
/// network engine started, what it has read since and in the last second,
/// and when this was said.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DaemonLive {
    pub started: i64,
    pub bytes: u64,
    pub speed: u64,
    pub at: i64,
}

impl crate::settings::Setting for DaemonLive {
    const KEY: &'static str = "daemon_live";
}

/// A download in progress (the reference's `NetworkJob.GetStatus`).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JobLive {
    /// The URL it is fetching, or last fetched.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    pub status: String,
    /// Bytes read in the last second.
    pub speed: u64,
    pub bytes_read: u64,
    pub bytes_to_read: Option<u64>,
    pub done: bool,
    /// It failed or was cancelled.
    pub error: bool,
}

impl JobLive {
    /// Its line under the importer's log.
    pub fn line(&self) -> JobLine {
        network_job_line(
            &self.status,
            self.speed,
            Some(self.bytes_read),
            self.bytes_to_read,
            self.error,
            self.done,
        )
    }
}

/// Which of a queue's downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    File,
    Gallery,
}

impl JobKind {
    fn code(self) -> i64 {
        match self {
            JobKind::File => 0,
            JobKind::Gallery => 1,
        }
    }

    fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(JobKind::File),
            1 => Some(JobKind::Gallery),
            _ => None,
        }
    }
}

/// Forget every queue's live state and asked-for cancels (the daemon
/// starting or stopping).
pub fn clear(conn: &Connection) -> Result<()> {
    conn.execute_batch("DELETE FROM queue_live; DELETE FROM queue_job_cancels;")?;
    Ok(())
}

/// Keep these queues' live state: each one's new state, or `None` for one
/// no longer running.
pub fn publish(conn: &Connection, changes: &[(i64, Option<QueueLive>)]) -> Result<()> {
    let mut upsert = conn.prepare_cached(
        "INSERT INTO queue_live (queue_id, live) VALUES (?, ?)
         ON CONFLICT (queue_id) DO UPDATE SET live = excluded.live",
    )?;
    let mut delete = conn.prepare_cached("DELETE FROM queue_live WHERE queue_id = ?")?;
    for (queue, live) in changes {
        match live {
            Some(live) => {
                let json = serde_json::to_string(live).expect("plain data serialises");
                upsert.execute(rusqlite::params![queue, json])?;
            }
            None => {
                delete.execute([queue])?;
            }
        }
    }
    Ok(())
}

/// What changed between the live state last published and `now` (the
/// running queues' state), for [`publish`]; `last` becomes `now`.
pub fn changes(
    last: &mut HashMap<i64, QueueLive>,
    now: Vec<(i64, QueueLive)>,
) -> Vec<(i64, Option<QueueLive>)> {
    let now: HashMap<i64, QueueLive> = now.into_iter().collect();
    let mut changed: Vec<(i64, Option<QueueLive>)> = now
        .iter()
        .filter(|(queue, live)| last.get(queue) != Some(live))
        .map(|(&queue, live)| (queue, Some(live.clone())))
        .chain(
            last.keys()
                .filter(|queue| !now.contains_key(queue))
                .map(|&queue| (queue, None)),
        )
        .collect();
    changed.sort_by_key(|(queue, _)| *queue);
    *last = now;
    changed
}

/// These queues' live state, for those that have any.
pub fn live(conn: &Connection, queues: &[i64]) -> Result<HashMap<i64, QueueLive>> {
    let mut out = HashMap::new();
    let mut statement = conn.prepare_cached("SELECT live FROM queue_live WHERE queue_id = ?")?;
    for &queue in queues {
        let json: Option<String> = statement
            .query_row([queue], |r| r.get(0))
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(e),
            })?;
        if let Some(live) = json.and_then(|j| serde_json::from_str(&j).ok()) {
            out.insert(queue, live);
        }
    }
    Ok(out)
}

/// Ask the daemon to cancel a queue's current download (the reference's
/// network job control's cancel button).
pub fn cancel(conn: &Connection, queue: i64, kind: JobKind) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO queue_job_cancels (queue_id, kind) VALUES (?, ?)",
        [queue, kind.code()],
    )?;
    crate::queues::nudge(conn, queue)
}

/// The cancels asked for, once each.
pub fn take_cancels(conn: &Connection) -> Result<Vec<(i64, JobKind)>> {
    let asked: Vec<(i64, i64)> = conn
        .prepare("SELECT queue_id, kind FROM queue_job_cancels ORDER BY queue_id, kind")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    conn.execute("DELETE FROM queue_job_cancels", [])?;
    Ok(asked
        .into_iter()
        .filter_map(|(queue, kind)| Some((queue, JobKind::from_code(kind)?)))
        .collect())
}

/// A download's line under an importer's log, as the reference's
/// `NetworkJobControl` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLine {
    /// The status's first line.
    pub left: String,
    /// How much it has read (of how much), and how fast.
    pub right: String,
    /// The gauge's value and range.
    pub gauge: (u64, u64),
    pub can_cancel: bool,
}

impl Default for JobLine {
    /// No download: blank, the gauge empty.
    fn default() -> Self {
        Self {
            left: String::new(),
            right: String::new(),
            gauge: (0, 1),
            can_cancel: false,
        }
    }
}

impl JobLine {
    /// How full the gauge is, 0 to 1.
    pub fn fraction(&self) -> f32 {
        let (value, range) = self.gauge;
        (value as f64 / range.max(1) as f64) as f32
    }
}

/// `NetworkJobControl._Update` for a job saying this: its status's first
/// line; what it has read (of the total, when that differs) and its speed,
/// unless it has read nothing or failed; and the gauge, as hydrus's
/// `Gauge` sets it (a range over 1000 scaled to 1000; an unknown one
/// empty, its "pulse" being switched off).
pub fn network_job_line(
    status: &str,
    speed: u64,
    bytes_read: Option<u64>,
    bytes_to_read: Option<u64>,
    has_error: bool,
    is_done: bool,
) -> JobLine {
    let mut right = String::new();
    if let Some(read) = bytes_read
        && read > 0
        && !has_error
    {
        match bytes_to_read {
            Some(total) if total != read => {
                right = format!("{}/{}", human_bytes(read), human_bytes(total));
            }
            _ => right = human_bytes(read),
        }
        // (a quick download just says its size)
        if Some(speed) != bytes_to_read {
            right.push_str(&format!(" {}/s", human_bytes(speed)));
        }
    }
    JobLine {
        left: first_line(status).to_owned(),
        right,
        gauge: gauge(bytes_read, bytes_to_read),
        can_cancel: !is_done,
    }
}

/// `Gauge.SetRange(range)` then `SetValue(value)`, from new.
fn gauge(value: Option<u64>, range: Option<u64>) -> (u64, u64) {
    let (maximum, actual_range) = match range {
        None | Some(0) => return (0, 1),
        Some(r) if r > 1000 => (1000, Some(r)),
        Some(r) => (r, None),
    };
    let Some(value) = value else {
        return (0, 1);
    };
    let value = match actual_range {
        Some(actual) => ((1000.0 * (value as f64 / actual as f64)).trunc() as u64).min(1000),
        None => value,
    };
    (value.min(maximum), maximum)
}

/// The text up to its first line break, as Python's `splitlines` breaks
/// lines (`HydrusText.GetFirstLine`).
fn first_line(text: &str) -> &str {
    let end = text
        .find([
            '\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}',
        ])
        .unwrap_or(text.len());
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::configure(&conn).unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn live_state_is_kept_replaced_and_cleared() {
        let conn = conn();
        let working = QueueLive {
            files_status: "working".into(),
            file_job: Some(JobLive {
                url: String::new(),
                status: "downloading\u{2026}".into(),
                speed: 300,
                bytes_read: 600,
                bytes_to_read: Some(1200),
                done: false,
                error: false,
            }),
            ..QueueLive::default()
        };
        publish(
            &conn,
            &[(1, Some(working.clone())), (2, Some(QueueLive::default()))],
        )
        .unwrap();
        let got = live(&conn, &[1, 2, 3]).unwrap();
        assert_eq!(got.len(), 2, "3 has none");
        assert_eq!(got[&1], working);
        // replaced, and gone when it stops
        publish(&conn, &[(1, Some(QueueLive::default())), (2, None)]).unwrap();
        let got = live(&conn, &[1, 2]).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[&1], QueueLive::default());
        clear(&conn).unwrap();
        assert!(live(&conn, &[1]).unwrap().is_empty());
    }

    #[test]
    fn only_changes_are_published() {
        let mut last = HashMap::new();
        let working = QueueLive {
            files_status: "working".into(),
            ..QueueLive::default()
        };
        let idle = QueueLive::default();
        assert_eq!(
            changes(&mut last, vec![(1, working.clone()), (2, idle.clone())]),
            vec![(1, Some(working.clone())), (2, Some(idle.clone()))]
        );
        assert!(changes(&mut last, vec![(2, idle.clone()), (1, working.clone())]).is_empty());
        assert_eq!(
            changes(&mut last, vec![(1, idle.clone())]),
            vec![(1, Some(idle.clone())), (2, None)],
            "1 changed, 2 stopped"
        );
        assert_eq!(changes(&mut last, Vec::new()), vec![(1, None)]);
        assert!(last.is_empty());
    }

    #[test]
    fn cancels_are_taken_once_and_nudge_the_queue() {
        let conn = conn();
        cancel(&conn, 4, JobKind::File).unwrap();
        cancel(&conn, 4, JobKind::File).unwrap();
        cancel(&conn, 2, JobKind::Gallery).unwrap();
        assert_eq!(crate::queues::take_nudges(&conn).unwrap(), vec![2, 4]);
        assert_eq!(
            take_cancels(&conn).unwrap(),
            vec![(2, JobKind::Gallery), (4, JobKind::File)]
        );
        assert!(take_cancels(&conn).unwrap().is_empty());
    }

    #[test]
    fn a_status_shows_its_first_line() {
        assert_eq!(first_line(""), "");
        assert_eq!(first_line("one"), "one");
        assert_eq!(first_line("Error: 404\nnot found"), "Error: 404");
        assert_eq!(first_line("a\r\nb"), "a");
        assert_eq!(first_line("a\u{2028}b"), "a");
        assert_eq!(first_line("\nb"), "");
    }
}
