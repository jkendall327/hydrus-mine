//! Popup messages: jobs shown to the user, with a title, text, progress
//! gauges, files and buttons to pause or cancel them, as the reference's
//! `JobStatus` and its popup queue (`JobStatusPopupQueue`) keep them. The
//! daemon and the Client API add them; the client shows them, and the
//! user dismisses them. Oldest first; a dismissed one leaves the queue.

use rusqlite::{Connection, OptionalExtension as _, params};
use serde::{Deserialize, Serialize};

use hydrus_core::Sha256;

use crate::error::Result;

/// How many popups the client shows at once (the reference's
/// `_max_messages_to_display`): the oldest; others wait.
pub const IN_VIEW: usize = 10;

/// One popup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub key: [u8; 32],
    /// Seconds since the epoch.
    pub creation_time: f64,
    pub pausable: bool,
    pub cancellable: bool,
    pub cancelled: bool,
    pub paused: bool,
    pub done: bool,
    pub dismissed: bool,
    /// When a "finish and dismiss in n seconds" takes effect (seconds).
    pub dismiss_at: Option<i64>,
    pub status_title: Option<String>,
    pub status_text_1: Option<String>,
    pub status_text_2: Option<String>,
    /// Gauges' value and range.
    pub popup_gauge_1: Option<(i64, i64)>,
    pub popup_gauge_2: Option<(i64, i64)>,
    pub api_data: Option<serde_json::Value>,
    /// Its files may join those of another popup with the same label.
    pub attached_files_mergable: bool,
    /// Attached files and their label.
    pub files: Option<(Vec<Sha256>, Option<String>)>,
    /// An error's traceback, to show or copy.
    #[serde(default)]
    pub traceback: Option<String>,
    #[serde(default)]
    pub had_error: bool,
    /// The download it is doing, if any (its network job).
    #[serde(default)]
    pub network_job: Option<crate::live::JobLive>,
    #[serde(default)]
    pub popup_clipboard: Option<(String, String)>,
    #[serde(default)]
    pub popup_yes_no_question: Option<([u8; 32], String)>,
    #[serde(default)]
    pub user_callable_label: Option<String>,
    #[serde(default)]
    pub action_owner: Option<[u8; 32]>,
}

impl Job {
    /// A job that can be paused or cancelled is ongoing; any other is done
    /// from the start. Made at `now` (seconds).
    pub fn new(pausable: bool, cancellable: bool, now: f64) -> Self {
        Self {
            key: rand::random(),
            creation_time: now,
            pausable,
            cancellable,
            cancelled: false,
            paused: false,
            done: !(pausable || cancellable),
            dismissed: false,
            dismiss_at: None,
            status_title: None,
            status_text_1: None,
            status_text_2: None,
            popup_gauge_1: None,
            popup_gauge_2: None,
            api_data: None,
            attached_files_mergable: false,
            files: None,
            traceback: None,
            had_error: false,
            network_job: None,
            popup_clipboard: None,
            popup_yes_no_question: None,
            user_callable_label: None,
            action_owner: None,
        }
    }

    /// A message: a popup done from the start, with this text
    /// (`HydrusData.ShowText`).
    pub fn text(text: impl Into<String>, now: f64) -> Self {
        let mut job = Self::new(false, false, now);
        job.status_text_1 = Some(text.into());
        job
    }

    pub fn finish(&mut self) {
        self.done = true;
        self.paused = false;
        self.pausable = false;
        self.cancellable = false;
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.finish();
    }

    /// Pause or resume, if it can be.
    pub fn pause_play(&mut self) {
        if self.pausable {
            self.paused = !self.paused;
        }
    }

    /// Finish, and dismiss now or `seconds` after `now`.
    pub fn finish_and_dismiss(&mut self, seconds: Option<i64>, now: i64) {
        self.finish();
        match seconds {
            None => self.dismissed = true,
            Some(s) => self.dismiss_at = Some(now + s),
        }
    }

    /// Whether it has been dismissed by `now` (seconds).
    pub fn is_dismissed(&self, now: i64) -> bool {
        self.dismissed || self.dismiss_at.is_some_and(|at| now > at)
    }

    /// Attach files (deduplicated), or detach them if there are none.
    pub fn set_files(&mut self, hashes: Vec<Sha256>, label: Option<String>) {
        if hashes.is_empty() {
            self.files = None;
            return;
        }
        let mut unique: Vec<Sha256> = Vec::with_capacity(hashes.len());
        for hash in hashes {
            if !unique.contains(&hash) {
                unique.push(hash);
            }
        }
        self.files = Some((unique, label));
    }

    /// `JobStatus.ToString`: its title, texts and traceback.
    pub fn nice_string(&self) -> String {
        [
            &self.status_title,
            &self.status_text_1,
            &self.status_text_2,
            &self.traceback,
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
    }
}

fn load(json: &str) -> Result<Job> {
    serde_json::from_str(json)
        .map_err(|e| crate::error::StoreError::Invalid(format!("stored popup: {e}")))
}

/// Add a popup to the queue's end, as the reference's popup manager does
/// (`AddMessage`): unless its files may merge, and a popup whose files may
/// has the same label, which takes its files instead.
pub fn add(conn: &Connection, job: &Job, now: i64) -> Result<()> {
    if job.attached_files_mergable
        && let Some((hashes, label)) = &job.files
    {
        for mut existing in all(conn, now)? {
            if !existing.attached_files_mergable {
                continue;
            }
            let Some((existing_hashes, existing_label)) = existing.files.clone() else {
                continue;
            };
            if existing_label == *label {
                let mut merged = existing_hashes;
                merged.extend(hashes.iter().copied());
                existing.set_files(merged, existing_label);
                put(conn, &existing)?;
                return Ok(());
            }
        }
    }
    conn.prepare_cached("INSERT INTO popups (key, job) VALUES (?1, ?2)")?
        .execute(params![job.key.as_slice(), serde_json::to_string(job)?])?;
    Ok(())
}

fn put(conn: &Connection, job: &Job) -> Result<()> {
    conn.prepare_cached("UPDATE popups SET job = ?2 WHERE key = ?1")?
        .execute(params![job.key.as_slice(), serde_json::to_string(job)?])?;
    Ok(())
}

/// Every popup not dismissed by `now`, oldest first.
pub fn all(conn: &Connection, now: i64) -> Result<Vec<Job>> {
    let mut stmt = conn.prepare_cached("SELECT job FROM popups ORDER BY seq")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        let job = load(&row?)?;
        if !job.is_dismissed(now) {
            out.push(job);
        }
    }
    Ok(out)
}

/// A popup, unless there is no such popup or it was dismissed by `now`.
pub fn get(conn: &Connection, key: &[u8], now: i64) -> Result<Option<Job>> {
    let json: Option<String> = conn
        .prepare_cached("SELECT job FROM popups WHERE key = ?1")?
        .query_row([key], |r| r.get(0))
        .optional()?;
    Ok(match json {
        Some(json) => Some(load(&json)?).filter(|job| !job.is_dismissed(now)),
        None => None,
    })
}

/// Change a popup, keeping it as changed (or, dismissed, removing it);
/// `None` if there is no such popup (or it was dismissed by `now`).
pub fn update<R>(
    conn: &Connection,
    key: &[u8],
    now: i64,
    f: impl FnOnce(&mut Job) -> R,
) -> Result<Option<R>> {
    let json: Option<String> = conn
        .prepare_cached("SELECT job FROM popups WHERE key = ?1")?
        .query_row([key], |r| r.get(0))
        .optional()?;
    let Some(json) = json else {
        return Ok(None);
    };
    let mut job = load(&json)?;
    if job.is_dismissed(now) {
        clear_dismissed(conn, now)?;
        return Ok(None);
    }
    let result = f(&mut job);
    if job.is_dismissed(now) {
        conn.prepare_cached("DELETE FROM popups WHERE key = ?1")?
            .execute([key])?;
    } else {
        put(conn, &job)?;
    }
    Ok(Some(result))
}

/// Forget the popups dismissed by `now`.
pub fn clear_dismissed(conn: &Connection, now: i64) -> Result<()> {
    let mut stmt = conn.prepare_cached("SELECT key, job FROM popups")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut gone = Vec::new();
    for row in rows {
        let (key, json) = row?;
        if load(&json)?.is_dismissed(now) {
            gone.push(key);
        }
    }
    for key in gone {
        conn.prepare_cached("DELETE FROM popups WHERE key = ?1")?
            .execute([key])?;
    }
    Ok(())
}

/// Forget the popups for work that isn't done: the daemon starting, the
/// work they were showing has stopped (the reference's popups go with the
/// client). Messages and finished work stay to be read.
pub fn forget_unfinished(conn: &Connection, now: i64) -> Result<()> {
    for mut job in all(conn, now)? {
        if !job.done {
            conn.prepare_cached("DELETE FROM popups WHERE key = ?1")?
                .execute([job.key.as_slice()])?;
        } else if job.action_owner.take().is_some() {
            job.popup_yes_no_question = None;
            job.user_callable_label = None;
            put(conn, &job)?;
        }
    }
    conn.execute("DELETE FROM popup_action_requests", [])?;
    conn.execute("DELETE FROM popup_action_owners", [])?;
    clear_dismissed(conn, now)
}

/// Dismiss every popup that is done (the popup manager's "dismiss all",
/// `DeleteAllPossible`).
pub fn dismiss_all_done(conn: &Connection, now: i64) -> Result<()> {
    for job in all(conn, now)? {
        if job.done {
            conn.prepare_cached("DELETE FROM popups WHERE key = ?1")?
                .execute([job.key.as_slice()])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::schema::migrate(&mut conn).unwrap();
        conn
    }

    #[test]
    fn popups_queue_oldest_first_until_dismissed() {
        let conn = conn();
        let first = Job::text("first", 100.0);
        let mut second = Job::new(true, true, 101.0);
        second.status_title = Some("working".into());
        add(&conn, &first, 100).unwrap();
        add(&conn, &second, 101).unwrap();
        let keys: Vec<[u8; 32]> = all(&conn, 102).unwrap().iter().map(|j| j.key).collect();
        assert_eq!(keys, [first.key, second.key]);
        // an ongoing job pauses, and isn't dismissed by "dismiss all"
        update(&conn, &second.key, 102, Job::pause_play).unwrap();
        assert!(all(&conn, 102).unwrap()[1].paused);
        dismiss_all_done(&conn, 102).unwrap();
        assert_eq!(all(&conn, 102).unwrap().len(), 1);
        // (and is forgotten when the daemon starts again, unlike a message)
        let unfinished = conn.unchecked_transaction().unwrap();
        let message = Job::text("read me", 102.0);
        add(&unfinished, &message, 102).unwrap();
        forget_unfinished(&unfinished, 102).unwrap();
        let keys: Vec<[u8; 32]> = all(&unfinished, 102)
            .unwrap()
            .iter()
            .map(|j| j.key)
            .collect();
        assert_eq!(keys, [message.key]);
        unfinished.rollback().unwrap();
        // finished, it dismisses in two seconds
        update(&conn, &second.key, 102, |j| {
            j.finish_and_dismiss(Some(2), 102);
        })
        .unwrap();
        assert_eq!(all(&conn, 104).unwrap().len(), 1);
        assert!(all(&conn, 105).unwrap().is_empty());
        assert_eq!(update(&conn, &second.key, 105, |_| ()).unwrap(), None);
        let left: i64 = conn
            .query_row("SELECT count(*) FROM popups", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "a dismissed popup leaves the queue");
    }

    #[test]
    fn mergable_files_with_the_same_label_join_one_popup() {
        let conn = conn();
        let hash = |n: u8| Sha256([n; 32]);
        let files = |hashes: Vec<Sha256>, label: &str| {
            let mut job = Job::text("downloaded", 0.0);
            job.attached_files_mergable = true;
            job.set_files(hashes, Some(label.into()));
            job
        };
        add(&conn, &files(vec![hash(1), hash(2)], "sub a"), 0).unwrap();
        add(&conn, &files(vec![hash(2), hash(3)], "sub a"), 0).unwrap();
        add(&conn, &files(vec![hash(4)], "sub b"), 0).unwrap();
        let jobs = all(&conn, 0).unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(
            jobs[0].files,
            Some((vec![hash(1), hash(2), hash(3)], Some("sub a".into())))
        );
        // (not mergable: a popup of its own)
        let mut own = files(vec![hash(5)], "sub a");
        own.attached_files_mergable = false;
        add(&conn, &own, 0).unwrap();
        assert_eq!(all(&conn, 0).unwrap().len(), 3);
    }
}
