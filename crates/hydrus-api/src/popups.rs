//! Popup messages (`/manage_popups/*`): jobs a tool shows the user, with a
//! title, text, progress gauges and files, as the reference's `JobStatus`
//! and popup queue keep them.
//!
//! Without a GUI every popup is "in view", and a dismissed popup leaves the
//! queue as soon as it is dismissed (the reference's GUI clears dismissed
//! popups on its next refresh).

use parking_lot::Mutex;
use serde_json::{Map, Value as Json, json};

use hydrus_core::Sha256;

/// One popup.
#[derive(Debug, Clone)]
pub struct Job {
    pub key: [u8; 32],
    /// Seconds since the epoch.
    pub creation_time: f64,
    pub pausable: bool,
    pub cancellable: bool,
    pub cancelled: bool,
    pub paused: bool,
    pub done: bool,
    dismissed: bool,
    /// When a "finish and dismiss in n seconds" takes effect (seconds).
    dismiss_at: Option<i64>,
    pub status_title: Option<String>,
    pub status_text_1: Option<String>,
    pub status_text_2: Option<String>,
    pub popup_gauge_1: Option<Json>,
    pub popup_gauge_2: Option<Json>,
    pub api_data: Option<Json>,
    pub attached_files_mergable: bool,
    /// Attached files and their label.
    pub files: Option<(Vec<Sha256>, Option<String>)>,
}

fn now_seconds() -> f64 {
    hydrus_core::time::TimestampMs::now().millis() as f64 / 1000.0
}

impl Job {
    /// A job that can be paused or cancelled is ongoing; any other is done
    /// from the start.
    pub fn new(pausable: bool, cancellable: bool) -> Self {
        Self {
            key: rand::random(),
            creation_time: now_seconds(),
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
        }
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

    /// Finish, and dismiss now or after `seconds`.
    pub fn finish_and_dismiss(&mut self, seconds: Option<i64>) {
        self.finish();
        match seconds {
            None => self.dismissed = true,
            Some(s) => self.dismiss_at = Some(now_seconds().floor() as i64 + s),
        }
    }

    fn is_dismissed(&self) -> bool {
        self.dismissed
            || self
                .dismiss_at
                .is_some_and(|at| (now_seconds().floor() as i64) > at)
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

    /// `JobStatus.ToString`.
    fn nice_string(&self) -> String {
        [&self.status_title, &self.status_text_1, &self.status_text_2]
            .into_iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// As the Client API reports it (`JobStatusToDict`).
    pub fn to_json(&self) -> Json {
        let mut out = Map::new();
        let mut put = |k: &str, v: Json| {
            if !v.is_null() {
                out.insert(k.to_owned(), v);
            }
        };
        put("key", json!(hex::encode(self.key)));
        put("creation_time", json!(self.creation_time));
        put("status_title", json!(self.status_title));
        put("status_text_1", json!(self.status_text_1));
        put("status_text_2", json!(self.status_text_2));
        put("had_error", json!(false));
        put("is_cancellable", json!(self.cancellable));
        put("is_cancelled", json!(self.cancelled));
        put("is_done", json!(self.done));
        put("is_pausable", json!(self.pausable));
        put("is_paused", json!(self.paused));
        put("nice_string", json!(self.nice_string()));
        put(
            "popup_gauge_1",
            self.popup_gauge_1.clone().unwrap_or(Json::Null),
        );
        put(
            "popup_gauge_2",
            self.popup_gauge_2.clone().unwrap_or(Json::Null),
        );
        if self.attached_files_mergable {
            put("attached_files_mergable", json!(true));
        }
        put("api_data", self.api_data.clone().unwrap_or(Json::Null));
        if let Some((hashes, label)) = &self.files {
            put(
                "files",
                json!({
                    "hashes": hashes.iter().map(Sha256::to_hex).collect::<Vec<_>>(),
                    "label": label,
                }),
            );
        }
        Json::Object(out)
    }
}

/// The popup queue, oldest first.
#[derive(Debug, Default)]
pub struct Popups {
    jobs: Mutex<Vec<Job>>,
}

impl Popups {
    pub fn add(&self, job: Job) {
        self.jobs.lock().push(job);
    }

    fn clear_dismissed(jobs: &mut Vec<Job>) {
        jobs.retain(|j| !j.is_dismissed());
    }

    /// Every popup not dismissed.
    pub fn all(&self) -> Vec<Job> {
        let mut jobs = self.jobs.lock();
        Self::clear_dismissed(&mut jobs);
        jobs.clone()
    }

    /// Change a popup, returning it as changed; `None` if there is no such
    /// popup (or it was dismissed).
    pub fn update<R>(&self, key: &[u8], f: impl FnOnce(&mut Job) -> R) -> Option<R> {
        let mut jobs = self.jobs.lock();
        Self::clear_dismissed(&mut jobs);
        jobs.iter_mut().find(|j| j.key.as_slice() == key).map(f)
    }
}
