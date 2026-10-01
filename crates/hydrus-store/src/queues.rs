//! Import queues: URL lists, gallery searches, watchers and subscription
//! queries, each with its *file seeds* (a URL or path to import, with what
//! is known about it so far) and *gallery seeds* (a page of results to
//! read), as the reference's file seed caches and gallery logs keep them.
//!
//! Seeds are rows, so a queue of thousands of entries changes one row at a
//! time rather than rewriting a serialised list.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};

use hydrus_core::import_options::ImportOptionsSlice;

use crate::error::{Result, StoreError};

/// What kind of importer a queue belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QueueKind {
    /// A list of URLs (the reference's "urls downloader" page).
    Urls,
    Gallery,
    Watcher,
    Subscription,
    /// An import folder (its settings are the queue's extra).
    ImportFolder,
}

impl QueueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            QueueKind::Urls => "urls",
            QueueKind::Gallery => "gallery",
            QueueKind::Watcher => "watcher",
            QueueKind::Subscription => "subscription",
            QueueKind::ImportFolder => "import_folder",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "urls" => QueueKind::Urls,
            "gallery" => QueueKind::Gallery,
            "watcher" => QueueKind::Watcher,
            "subscription" => QueueKind::Subscription,
            "import_folder" => QueueKind::ImportFolder,
            _ => return None,
        })
    }
}

/// An import queue.
#[derive(Debug, Clone, PartialEq)]
pub struct Queue {
    pub id: i64,
    pub kind: QueueKind,
    pub name: String,
    pub page_key: Option<Vec<u8>>,
    pub created: i64,
    pub files_paused: bool,
    pub gallery_paused: bool,
    /// This importer's own import options.
    pub options: ImportOptionsSlice,
    /// What else its kind keeps.
    pub extra: serde_json::Value,
}

/// `CC.STATUS_*` for a seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SeedStatus {
    Unknown = 0,
    SuccessfulAndNew = 1,
    SuccessfulButRedundant = 2,
    Deleted = 3,
    Error = 4,
    Vetoed = 7,
    Skipped = 8,
    SuccessfulAndChildFiles = 9,
}

impl SeedStatus {
    pub fn code(self) -> i64 {
        self as i64
    }

    pub fn from_code(code: i64) -> Option<Self> {
        Some(match code {
            // the reference no longer uses "new" (5) and "paused" (6)
            0 | 5 | 6 => SeedStatus::Unknown,
            1 => SeedStatus::SuccessfulAndNew,
            2 => SeedStatus::SuccessfulButRedundant,
            3 => SeedStatus::Deleted,
            4 => SeedStatus::Error,
            7 => SeedStatus::Vetoed,
            8 => SeedStatus::Skipped,
            9 => SeedStatus::SuccessfulAndChildFiles,
            _ => return None,
        })
    }

    pub fn is_successful(self) -> bool {
        matches!(
            self,
            SeedStatus::SuccessfulAndNew
                | SeedStatus::SuccessfulButRedundant
                | SeedStatus::SuccessfulAndChildFiles
        )
    }
}

/// What a file seed is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SeedType {
    Path = 0,
    Url = 1,
}

/// What a file seed has gathered on its way (parsed tags and notes, URLs to
/// associate, hashes to check, headers to send).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSeedMeta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_headers: Vec<(String, String)>,
    /// Tags from outside (the Client API's), filtered like parsed tags.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub external_filterable_tags: BTreeSet<String>,
    /// Tags from outside added as they are: `(service key hex, tags)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_additional_tags: Vec<(String, BTreeSet<String>)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub primary_urls: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub source_urls: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<String>,
    /// `(name, text)`, a note per name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<(String, String)>,
    /// `(hash type, hex)`, the first of each type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hashes: Vec<(String, String)>,
    /// A Cloudflare cache's `Last-Modified` too far from the source time to
    /// be it (seconds), kept as the file's cloudflare.com time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloudflare_last_modified: Option<i64>,
}

impl FileSeedMeta {
    pub fn hash(&self, hash_type: &str) -> Option<&str> {
        self.hashes
            .iter()
            .find(|(t, _)| t == hash_type)
            .map(|(_, h)| h.as_str())
    }

    pub fn set_hash(&mut self, hash_type: &str, hex: String) {
        match self.hashes.iter_mut().find(|(t, _)| t == hash_type) {
            Some(entry) => entry.1 = hex,
            None => self.hashes.push((hash_type.to_owned(), hex)),
        }
    }

    pub fn add_hash_if_new(&mut self, hash_type: &str, hex: String) {
        if self.hash(hash_type).is_none() {
            self.hashes.push((hash_type.to_owned(), hex));
        }
    }

    pub fn set_note(&mut self, name: &str, text: String) {
        match self.notes.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 = text,
            None => self.notes.push((name.to_owned(), text)),
        }
    }

    pub fn add_request_headers(&mut self, headers: &[(String, String)]) {
        for (name, value) in headers {
            match self.request_headers.iter_mut().find(|(n, _)| n == name) {
                Some(entry) => entry.1.clone_from(value),
                None => self.request_headers.push((name.clone(), value.clone())),
            }
        }
    }
}

/// A file seed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSeed {
    pub id: i64,
    pub queue_id: i64,
    pub seed_type: SeedType,
    /// The URL (normalised for requests) or path.
    pub data: String,
    /// What makes two seeds the same (the URL normalised for storage).
    pub data_for_comparison: String,
    pub created: i64,
    pub modified: i64,
    /// When the site says the file was posted.
    pub source_time: Option<i64>,
    pub status: SeedStatus,
    pub note: String,
    pub referral_url: Option<String>,
    pub meta: FileSeedMeta,
}

/// A file seed to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFileSeed {
    pub seed_type: SeedType,
    pub data: String,
    pub data_for_comparison: String,
    pub source_time: Option<i64>,
    pub referral_url: Option<String>,
    pub meta: FileSeedMeta,
}

/// What a gallery seed passes on to what it finds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GallerySeedMeta {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub request_headers: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub external_filterable_tags: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_additional_tags: Vec<(String, BTreeSet<String>)>,
    /// Which run of a search this page belongs to (hex).
    #[serde(default)]
    pub run_token: String,
    #[serde(default)]
    pub force_next_page_url_generation: bool,
}

/// A gallery seed: a page of results to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GallerySeed {
    pub id: i64,
    pub queue_id: i64,
    pub url: String,
    pub can_generate_more_pages: bool,
    pub created: i64,
    pub modified: i64,
    pub status: SeedStatus,
    pub note: String,
    pub referral_url: Option<String>,
    pub meta: GallerySeedMeta,
}

/// A gallery seed to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewGallerySeed {
    pub url: String,
    pub can_generate_more_pages: bool,
    pub referral_url: Option<String>,
    pub meta: GallerySeedMeta,
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("plain data serialises")
}

fn parse<T: for<'de> Deserialize<'de>>(text: &str, what: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| StoreError::Corrupt(format!("{what}: {e}")))
}

fn status(code: i64) -> Result<SeedStatus> {
    SeedStatus::from_code(code)
        .ok_or_else(|| StoreError::Corrupt(format!("unknown seed status {code}")))
}

// queues -----------------------------------------------------------------------

const QUEUE_COLUMNS: &str =
    "queue_id, kind, name, page_key, created, files_paused, gallery_paused, options, extra";

fn queue_from_row(row: &Row<'_>) -> rusqlite::Result<(Queue, String, String)> {
    let kind: String = row.get(1)?;
    Ok((
        Queue {
            id: row.get(0)?,
            kind: QueueKind::parse(&kind).unwrap_or(QueueKind::Urls),
            name: row.get(2)?,
            page_key: row.get(3)?,
            created: row.get(4)?,
            files_paused: row.get(5)?,
            gallery_paused: row.get(6)?,
            options: ImportOptionsSlice::default(),
            extra: serde_json::Value::Null,
        },
        row.get(7)?,
        row.get(8)?,
    ))
}

fn finish_queue((mut queue, options, extra): (Queue, String, String)) -> Result<Queue> {
    queue.options = parse(&options, "queue import options")?;
    queue.extra = parse(&extra, "queue settings")?;
    Ok(queue)
}

/// Make a queue.
/// Tell whichever process runs the queues (the daemon) that a queue was
/// made or changed (seeds added, paused or resumed, deleted), so it looks
/// at it now rather than when it next would.
pub fn nudge(conn: &Connection, queue: i64) -> Result<()> {
    conn.execute("INSERT INTO queue_nudges (queue_id) VALUES (?)", [queue])?;
    Ok(())
}

/// Whether any queue has been nudged (a cheap read, before taking them).
pub fn any_nudged(conn: &Connection) -> Result<bool> {
    Ok(
        conn.query_row("SELECT EXISTS (SELECT 1 FROM queue_nudges)", [], |r| {
            r.get(0)
        })?,
    )
}

/// The queues nudged since last time, each once, taken off the list.
pub fn take_nudges(conn: &Connection) -> Result<Vec<i64>> {
    let nudged: Vec<i64> = conn
        .prepare("SELECT DISTINCT queue_id FROM queue_nudges ORDER BY queue_id")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    conn.execute("DELETE FROM queue_nudges", [])?;
    Ok(nudged)
}

pub fn create_queue(
    conn: &Connection,
    kind: QueueKind,
    name: &str,
    page_key: Option<&[u8]>,
    options: &ImportOptionsSlice,
    now: i64,
) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO import_queues (kind, name, page_key, created, options, extra) VALUES (?, ?, ?, ?, ?, '{}')",
    )?
    .execute(params![kind.as_str(), name, page_key, now, json(options)])?;
    Ok(conn.last_insert_rowid())
}

pub fn queue(conn: &Connection, id: i64) -> Result<Option<Queue>> {
    conn.prepare_cached(&format!(
        "SELECT {QUEUE_COLUMNS} FROM import_queues WHERE queue_id = ?"
    ))?
    .query_row([id], queue_from_row)
    .optional()?
    .map(finish_queue)
    .transpose()
}

/// Every queue (of one kind), oldest first.
pub fn queues(conn: &Connection, kind: Option<QueueKind>) -> Result<Vec<Queue>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {QUEUE_COLUMNS} FROM import_queues WHERE ?1 IS NULL OR kind = ?1 ORDER BY queue_id"
    ))?;
    let rows = stmt
        .query_map([kind.map(QueueKind::as_str)], queue_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(finish_queue).collect()
}

/// A queue by its page key, else the first of its kind with this name.
pub fn find_queue(
    conn: &Connection,
    kind: QueueKind,
    name: Option<&str>,
    page_key: Option<&[u8]>,
) -> Result<Option<Queue>> {
    let all = queues(conn, Some(kind))?;
    if let Some(key) = page_key {
        return Ok(all.into_iter().find(|q| q.page_key.as_deref() == Some(key)));
    }
    Ok(all.into_iter().find(|q| Some(q.name.as_str()) == name))
}

pub fn set_queue_options(conn: &Connection, id: i64, options: &ImportOptionsSlice) -> Result<()> {
    conn.prepare_cached("UPDATE import_queues SET options = ? WHERE queue_id = ?")?
        .execute(params![json(options), id])?;
    Ok(())
}

pub fn set_queue_extra(conn: &Connection, id: i64, extra: &serde_json::Value) -> Result<()> {
    conn.prepare_cached("UPDATE import_queues SET extra = ? WHERE queue_id = ?")?
        .execute(params![extra.to_string(), id])?;
    Ok(())
}

pub fn set_paused(
    conn: &Connection,
    id: i64,
    files: Option<bool>,
    gallery: Option<bool>,
) -> Result<()> {
    if let Some(files) = files {
        conn.prepare_cached("UPDATE import_queues SET files_paused = ? WHERE queue_id = ?")?
            .execute(params![files, id])?;
    }
    if let Some(gallery) = gallery {
        conn.prepare_cached("UPDATE import_queues SET gallery_paused = ? WHERE queue_id = ?")?
            .execute(params![gallery, id])?;
    }
    Ok(())
}

pub fn rename_queue(conn: &Connection, id: i64, name: &str) -> Result<()> {
    conn.prepare_cached("UPDATE import_queues SET name = ? WHERE queue_id = ?")?
        .execute(params![name, id])?;
    Ok(())
}

/// Delete a queue and its seeds.
pub fn delete_queue(conn: &Connection, id: i64) -> Result<()> {
    conn.prepare_cached("DELETE FROM file_seeds WHERE queue_id = ?")?
        .execute([id])?;
    conn.prepare_cached("DELETE FROM gallery_seeds WHERE queue_id = ?")?
        .execute([id])?;
    conn.prepare_cached("DELETE FROM import_queues WHERE queue_id = ?")?
        .execute([id])?;
    Ok(())
}

/// Drop file seeds by id (history compaction).
pub fn remove_file_seeds_by_id(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM file_seeds WHERE seed_id = ?")?;
    for id in ids {
        stmt.execute([id])?;
    }
    Ok(())
}

/// Drop gallery seeds by id (history compaction).
pub fn remove_gallery_seeds_by_id(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare_cached("DELETE FROM gallery_seeds WHERE seed_id = ?")?;
    for id in ids {
        stmt.execute([id])?;
    }
    Ok(())
}

// file seeds -------------------------------------------------------------------

const FILE_SEED_COLUMNS: &str = "seed_id, queue_id, seed_type, data, data_for_comparison, created, \
     modified, source_time, status, note, referral_url, metadata";

fn file_seed_from_row(row: &Row<'_>) -> rusqlite::Result<(FileSeed, i64, String)> {
    let seed_type: i64 = row.get(2)?;
    Ok((
        FileSeed {
            id: row.get(0)?,
            queue_id: row.get(1)?,
            seed_type: if seed_type == 0 {
                SeedType::Path
            } else {
                SeedType::Url
            },
            data: row.get(3)?,
            data_for_comparison: row.get(4)?,
            created: row.get(5)?,
            modified: row.get(6)?,
            source_time: row.get(7)?,
            status: SeedStatus::Unknown,
            note: row.get(9)?,
            referral_url: row.get(10)?,
            meta: FileSeedMeta::default(),
        },
        row.get(8)?,
        row.get(11)?,
    ))
}

fn finish_file_seed((mut seed, code, meta): (FileSeed, i64, String)) -> Result<FileSeed> {
    seed.status = status(code)?;
    seed.meta = parse(&meta, "file seed metadata")?;
    Ok(seed)
}

fn has_file_seed(
    conn: &Connection,
    queue: i64,
    seed: &NewFileSeed,
) -> Result<Option<(i64, SeedStatus)>> {
    conn.prepare_cached(
        "SELECT seed_id, status FROM file_seeds WHERE queue_id = ? AND seed_type = ? AND data_for_comparison = ?",
    )?
    .query_row(
        params![queue, seed.seed_type as i64, seed.data_for_comparison],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    )
    .optional()?
    .map(|(id, code)| Ok((id, status(code)?)))
    .transpose()
}

fn insert_file_seed(
    conn: &Connection,
    queue: i64,
    position: f64,
    seed: &NewFileSeed,
    now: i64,
) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO file_seeds (queue_id, position, seed_type, data, data_for_comparison, created, modified, source_time, status, note, referral_url, metadata)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, '', ?, ?)",
    )?
    .execute(params![
        queue,
        position,
        seed.seed_type as i64,
        seed.data,
        seed.data_for_comparison,
        now,
        now,
        seed.source_time,
        seed.referral_url,
        json(&seed.meta)
    ])?;
    Ok(())
}

fn last_position(conn: &Connection, table: &str, queue: i64) -> Result<f64> {
    Ok(conn
        .prepare_cached(&format!(
            "SELECT MAX(position) FROM {table} WHERE queue_id = ?"
        ))?
        .query_row([queue], |r| r.get::<_, Option<f64>>(0))?
        .unwrap_or(0.0))
}

/// Add seeds to the end of a queue, skipping ones it already has (a seed
/// that failed with an error is set to be tried again when
/// `dupe_try_again`); how many were added or reset.
pub fn add_file_seeds(
    conn: &Connection,
    queue: i64,
    seeds: &[NewFileSeed],
    dupe_try_again: bool,
    now: i64,
) -> Result<usize> {
    let mut position = last_position(conn, "file_seeds", queue)?;
    let mut changed = 0;
    for seed in seeds {
        if let Some((id, existing)) = has_file_seed(conn, queue, seed)? {
            if dupe_try_again && existing == SeedStatus::Error {
                conn.prepare_cached(
                    "UPDATE file_seeds SET status = 0, note = '', modified = ? WHERE seed_id = ?",
                )?
                .execute(params![now, id])?;
                changed += 1;
            }
            continue;
        }
        position += 1.0;
        insert_file_seed(conn, queue, position, seed, now)?;
        changed += 1;
    }
    Ok(changed)
}

/// Append seeds exactly as they were (a migrated history), skipping any the
/// queue already has; how many were added.
pub fn restore_file_seeds(conn: &Connection, queue: i64, seeds: &[FileSeed]) -> Result<usize> {
    let mut position = last_position(conn, "file_seeds", queue)?;
    let mut stmt = conn.prepare_cached(
        "INSERT OR IGNORE INTO file_seeds (queue_id, position, seed_type, data, data_for_comparison, created, modified, source_time, status, note, referral_url, metadata)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;
    let mut added = 0;
    for seed in seeds {
        position += 1.0;
        added += stmt.execute(params![
            queue,
            position,
            seed.seed_type as i64,
            seed.data,
            seed.data_for_comparison,
            seed.created,
            seed.modified,
            seed.source_time,
            seed.status.code(),
            seed.note,
            seed.referral_url,
            json(&seed.meta)
        ])?;
    }
    Ok(added)
}

/// Insert seeds straight after another (a post's files after the post),
/// skipping ones the queue already has; how many were added.
pub fn insert_file_seeds_after(
    conn: &Connection,
    after: &FileSeed,
    seeds: &[NewFileSeed],
    now: i64,
) -> Result<usize> {
    let start: f64 = conn
        .prepare_cached("SELECT position FROM file_seeds WHERE seed_id = ?")?
        .query_row([after.id], |r| r.get(0))?;
    let next: Option<f64> = conn
        .prepare_cached("SELECT MIN(position) FROM file_seeds WHERE queue_id = ? AND position > ?")?
        .query_row(params![after.queue_id, start], |r| r.get(0))?;
    let mut fresh = Vec::new();
    let mut seen = BTreeSet::new();
    for seed in seeds {
        if !seen.insert((seed.seed_type as i64, seed.data_for_comparison.clone())) {
            continue;
        }
        if has_file_seed(conn, after.queue_id, seed)?.is_none() {
            fresh.push(seed);
        }
    }
    let end = next.unwrap_or(start + fresh.len() as f64 + 1.0);
    let step = (end - start) / (fresh.len() as f64 + 1.0);
    for (i, seed) in fresh.iter().enumerate() {
        insert_file_seed(
            conn,
            after.queue_id,
            start + step * (i as f64 + 1.0),
            seed,
            now,
        )?;
    }
    Ok(fresh.len())
}

pub fn file_seed(conn: &Connection, id: i64) -> Result<Option<FileSeed>> {
    conn.prepare_cached(&format!(
        "SELECT {FILE_SEED_COLUMNS} FROM file_seeds WHERE seed_id = ?"
    ))?
    .query_row([id], file_seed_from_row)
    .optional()?
    .map(finish_file_seed)
    .transpose()
}

/// The first seed still to be worked on.
pub fn next_file_seed(conn: &Connection, queue: i64) -> Result<Option<FileSeed>> {
    conn.prepare_cached(&format!(
        "SELECT {FILE_SEED_COLUMNS} FROM file_seeds WHERE queue_id = ? AND status = 0 ORDER BY position LIMIT 1"
    ))?
    .query_row([queue], file_seed_from_row)
    .optional()?
    .map(finish_file_seed)
    .transpose()
}

/// A queue's seeds in order.
pub fn file_seeds(conn: &Connection, queue: i64) -> Result<Vec<FileSeed>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {FILE_SEED_COLUMNS} FROM file_seeds WHERE queue_id = ? ORDER BY position"
    ))?;
    let rows = stmt
        .query_map([queue], file_seed_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(finish_file_seed).collect()
}

/// Save a seed's progress.
pub fn update_file_seed(conn: &Connection, seed: &FileSeed) -> Result<()> {
    conn.prepare_cached(
        "UPDATE file_seeds SET data = ?, modified = ?, source_time = ?, status = ?, note = ?, referral_url = ?, metadata = ?
         WHERE seed_id = ?",
    )?
    .execute(params![
        seed.data,
        seed.modified,
        seed.source_time,
        seed.status.code(),
        seed.note,
        seed.referral_url,
        json(&seed.meta),
        seed.id
    ])?;
    Ok(())
}

/// How many of a queue's seeds have each status.
pub fn file_seed_counts(conn: &Connection, queue: i64) -> Result<BTreeMap<SeedStatus, usize>> {
    let mut stmt = conn.prepare_cached(
        "SELECT status, COUNT(*) FROM file_seeds WHERE queue_id = ? GROUP BY status",
    )?;
    let rows = stmt
        .query_map([queue], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut counts = BTreeMap::new();
    for (code, n) in rows {
        *counts.entry(status(code)?).or_default() += n as usize;
    }
    Ok(counts)
}

/// Set seeds with these statuses to be tried again; how many.
pub fn retry_file_seeds(
    conn: &Connection,
    queue: i64,
    statuses: &[SeedStatus],
    now: i64,
) -> Result<usize> {
    let mut n = 0;
    for s in statuses {
        n += conn
            .prepare_cached("UPDATE file_seeds SET status = 0, note = '', modified = ? WHERE queue_id = ? AND status = ?")?
            .execute(params![now, queue, s.code()])?;
    }
    Ok(n)
}

/// Remove seeds with these statuses; how many.
pub fn remove_file_seeds(conn: &Connection, queue: i64, statuses: &[SeedStatus]) -> Result<usize> {
    let mut n = 0;
    for s in statuses {
        n += conn
            .prepare_cached("DELETE FROM file_seeds WHERE queue_id = ? AND status = ?")?
            .execute(params![queue, s.code()])?;
    }
    Ok(n)
}

// gallery seeds ----------------------------------------------------------------

const GALLERY_SEED_COLUMNS: &str = "seed_id, queue_id, url, can_generate_more_pages, created, modified, status, note, referral_url, metadata";

fn gallery_seed_from_row(row: &Row<'_>) -> rusqlite::Result<(GallerySeed, i64, String)> {
    Ok((
        GallerySeed {
            id: row.get(0)?,
            queue_id: row.get(1)?,
            url: row.get(2)?,
            can_generate_more_pages: row.get(3)?,
            created: row.get(4)?,
            modified: row.get(5)?,
            status: SeedStatus::Unknown,
            note: row.get(7)?,
            referral_url: row.get(8)?,
            meta: GallerySeedMeta::default(),
        },
        row.get(6)?,
        row.get(9)?,
    ))
}

fn finish_gallery_seed((mut seed, code, meta): (GallerySeed, i64, String)) -> Result<GallerySeed> {
    seed.status = status(code)?;
    seed.meta = parse(&meta, "gallery seed metadata")?;
    Ok(seed)
}

/// Add gallery seeds (after `after`, or at the end), skipping a URL the
/// queue already has in the same run; how many were added.
pub fn add_gallery_seeds(
    conn: &Connection,
    queue: i64,
    seeds: &[NewGallerySeed],
    after: Option<&GallerySeed>,
    now: i64,
) -> Result<usize> {
    let existing: BTreeSet<(String, String)> = gallery_seeds(conn, queue)?
        .into_iter()
        .map(|s| (s.url, s.meta.run_token))
        .collect();
    let mut seen = BTreeSet::new();
    let fresh: Vec<&NewGallerySeed> = seeds
        .iter()
        .filter(|s| seen.insert(s.url.clone()))
        .filter(|s| !existing.contains(&(s.url.clone(), s.meta.run_token.clone())))
        .collect();
    let (start, end) = if let Some(after) = after {
        let start: f64 = conn
            .prepare_cached("SELECT position FROM gallery_seeds WHERE seed_id = ?")?
            .query_row([after.id], |r| r.get(0))?;
        let next: Option<f64> = conn
            .prepare_cached(
                "SELECT MIN(position) FROM gallery_seeds WHERE queue_id = ? AND position > ?",
            )?
            .query_row(params![queue, start], |r| r.get(0))?;
        (start, next.unwrap_or(start + fresh.len() as f64 + 1.0))
    } else {
        let start = last_position(conn, "gallery_seeds", queue)?;
        (start, start + fresh.len() as f64 + 1.0)
    };
    let step = (end - start) / (fresh.len() as f64 + 1.0);
    for (i, seed) in fresh.iter().enumerate() {
        conn.prepare_cached(
            "INSERT INTO gallery_seeds (queue_id, position, url, can_generate_more_pages, created, modified, status, note, referral_url, metadata)
             VALUES (?, ?, ?, ?, ?, ?, 0, '', ?, ?)",
        )?
        .execute(params![
            queue,
            start + step * (i as f64 + 1.0),
            seed.url,
            seed.can_generate_more_pages,
            now,
            now,
            seed.referral_url,
            json(&seed.meta)
        ])?;
    }
    Ok(fresh.len())
}

/// Append gallery seeds exactly as they were (a migrated history).
pub fn restore_gallery_seeds(conn: &Connection, queue: i64, seeds: &[GallerySeed]) -> Result<()> {
    let mut position = last_position(conn, "gallery_seeds", queue)?;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO gallery_seeds (queue_id, position, url, can_generate_more_pages, created, modified, status, note, referral_url, metadata)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )?;
    for seed in seeds {
        position += 1.0;
        stmt.execute(params![
            queue,
            position,
            seed.url,
            seed.can_generate_more_pages,
            seed.created,
            seed.modified,
            seed.status.code(),
            seed.note,
            seed.referral_url,
            json(&seed.meta)
        ])?;
    }
    Ok(())
}

pub fn gallery_seeds(conn: &Connection, queue: i64) -> Result<Vec<GallerySeed>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {GALLERY_SEED_COLUMNS} FROM gallery_seeds WHERE queue_id = ? ORDER BY position"
    ))?;
    let rows = stmt
        .query_map([queue], gallery_seed_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter().map(finish_gallery_seed).collect()
}

pub fn next_gallery_seed(conn: &Connection, queue: i64) -> Result<Option<GallerySeed>> {
    conn.prepare_cached(&format!(
        "SELECT {GALLERY_SEED_COLUMNS} FROM gallery_seeds WHERE queue_id = ? AND status = 0 ORDER BY position LIMIT 1"
    ))?
    .query_row([queue], gallery_seed_from_row)
    .optional()?
    .map(finish_gallery_seed)
    .transpose()
}

pub fn update_gallery_seed(conn: &Connection, seed: &GallerySeed) -> Result<()> {
    conn.prepare_cached(
        "UPDATE gallery_seeds SET modified = ?, status = ?, note = ?, referral_url = ?, can_generate_more_pages = ?, metadata = ?
         WHERE seed_id = ?",
    )?
    .execute(params![
        seed.modified,
        seed.status.code(),
        seed.note,
        seed.referral_url,
        seed.can_generate_more_pages,
        json(&seed.meta),
        seed.id
    ])?;
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

    fn seed(url: &str) -> NewFileSeed {
        NewFileSeed {
            seed_type: SeedType::Url,
            data: url.into(),
            data_for_comparison: url.into(),
            source_time: None,
            referral_url: None,
            meta: FileSeedMeta::default(),
        }
    }

    #[test]
    fn nudges_are_taken_once_each() {
        let conn = conn();
        assert!(!any_nudged(&conn).unwrap());
        for queue in [3, 1, 3] {
            nudge(&conn, queue).unwrap();
        }
        assert!(any_nudged(&conn).unwrap());
        assert_eq!(take_nudges(&conn).unwrap(), [1, 3]);
        assert!(!any_nudged(&conn).unwrap());
        assert!(take_nudges(&conn).unwrap().is_empty());
    }

    #[test]
    fn seeds_dedupe_order_and_retry() {
        let conn = conn();
        let q = create_queue(
            &conn,
            QueueKind::Urls,
            "downloader",
            None,
            &ImportOptionsSlice::default(),
            1,
        )
        .unwrap();
        assert_eq!(
            add_file_seeds(&conn, q, &[seed("a"), seed("b"), seed("a")], true, 1).unwrap(),
            2
        );
        let first = next_file_seed(&conn, q).unwrap().unwrap();
        assert_eq!(first.data, "a");
        assert_eq!(
            insert_file_seeds_after(&conn, &first, &[seed("a1"), seed("a2"), seed("b")], 2)
                .unwrap(),
            2
        );
        let order: Vec<String> = file_seeds(&conn, q)
            .unwrap()
            .into_iter()
            .map(|s| s.data)
            .collect();
        assert_eq!(order, ["a", "a1", "a2", "b"]);
        let mut done = first.clone();
        done.status = SeedStatus::Error;
        done.note = "boom".into();
        update_file_seed(&conn, &done).unwrap();
        assert_eq!(next_file_seed(&conn, q).unwrap().unwrap().data, "a1");
        // adding it again retries the error
        assert_eq!(add_file_seeds(&conn, q, &[seed("a")], true, 3).unwrap(), 1);
        assert_eq!(next_file_seed(&conn, q).unwrap().unwrap().data, "a");
        assert_eq!(file_seed_counts(&conn, q).unwrap()[&SeedStatus::Unknown], 4);
        assert!(
            find_queue(&conn, QueueKind::Urls, Some("downloader"), None)
                .unwrap()
                .is_some()
        );
        delete_queue(&conn, q).unwrap();
        assert!(file_seeds(&conn, q).unwrap().is_empty());
    }
}
