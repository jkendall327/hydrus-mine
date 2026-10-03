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

use hydrus_core::HashId;
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
    /// Files imported from disk (the reference's "import" page,
    /// `HDDImport`; its [`LocalImport`] settings are the queue's extra).
    LocalImport,
    /// A simple downloader (`SimpleDownloaderImport`): pages parsed by a
    /// formula for files to download, its [`SimpleDownloader`] jobs the
    /// queue's extra, the pages done its gallery seeds.
    SimpleDownloader,
}

impl QueueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            QueueKind::Urls => "urls",
            QueueKind::Gallery => "gallery",
            QueueKind::Watcher => "watcher",
            QueueKind::Subscription => "subscription",
            QueueKind::ImportFolder => "import_folder",
            QueueKind::LocalImport => "local_import",
            QueueKind::SimpleDownloader => "simple_downloader",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "urls" => QueueKind::Urls,
            "gallery" => QueueKind::Gallery,
            "watcher" => QueueKind::Watcher,
            "subscription" => QueueKind::Subscription,
            "import_folder" => QueueKind::ImportFolder,
            "local_import" => QueueKind::LocalImport,
            "simple_downloader" => QueueKind::SimpleDownloader,
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
    /// Its page was closed: it waits until the page is reopened (the
    /// reference's "page is closed"), or goes with it.
    pub page_closed: bool,
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

/// Seeds by status.
pub type StatusCounts = BTreeMap<SeedStatus, usize>;

fn count(counts: &StatusCounts, status: SeedStatus) -> usize {
    counts.get(&status).copied().unwrap_or(0)
}

fn human(n: usize) -> String {
    hydrus_core::numbers::human_int(n as u64)
}

/// `processed/total`, as the reference writes a value of a range
/// (`ValueRangeToPrettyString`).
pub fn value_range_text(value: usize, range: usize) -> String {
    hydrus_core::numbers::value_range(value as u64, range as u64)
}

/// How many of a file log's seeds are done, of how many
/// (`FileSeedCacheStatus.GetValueRange`): all but those not yet tried.
pub fn file_log_value_range(counts: &StatusCounts) -> (usize, usize) {
    let total: usize = counts.values().sum();
    (total - count(counts, SeedStatus::Unknown), total)
}

/// A file log's status in full, as the reference writes it
/// (`FileSeedCacheStatus.GetStatusText`): "5 successful (2 already in db),
/// 1 failed".
pub fn file_log_status(counts: &StatusCounts) -> String {
    let new = count(counts, SeedStatus::SuccessfulAndNew);
    let redundant = count(counts, SeedStatus::SuccessfulButRedundant);
    let mut parts = Vec::new();
    if new + redundant > 0 {
        let mut part = format!("{} successful", human(new + redundant));
        if new == 0 {
            part.push_str(" (all already in db)");
        } else if redundant > 0 {
            part.push_str(&format!(" ({} already in db)", human(redundant)));
        }
        parts.push(part);
    }
    for (status, what) in [
        (SeedStatus::Vetoed, "ignored"),
        (SeedStatus::Deleted, "previously deleted"),
        (SeedStatus::Error, "failed"),
        (SeedStatus::Skipped, "skipped"),
    ] {
        let n = count(counts, status);
        if n > 0 {
            parts.push(format!("{} {what}", human(n)));
        }
    }
    parts.join(", ")
}

/// A file log's status in short (`GetStatusText(simple = True)`): "6/10 -
/// 2Ign1F", with the new and the previously deleted as the options say.
pub fn file_log_short_status(counts: &StatusCounts, show_new: bool, show_deleted: bool) -> String {
    let (processed, total) = file_log_value_range(counts);
    if total == 0 {
        return String::new();
    }
    let mut text = if count(counts, SeedStatus::Unknown) > 0 {
        value_range_text(processed, total)
    } else {
        human(processed)
    };
    let new = count(counts, SeedStatus::SuccessfulAndNew);
    if show_new && new > 0 {
        text.push_str(&format!(" - {}N", human(new)));
    }
    let mut short = String::new();
    for (status, mark, shown) in [
        (SeedStatus::Vetoed, "Ign", true),
        (SeedStatus::Deleted, "D", show_deleted),
        (SeedStatus::Error, "F", true),
        (SeedStatus::Skipped, "S", true),
    ] {
        let n = count(counts, status);
        if shown && n > 0 {
            short.push_str(&format!("{}{mark}", human(n)));
        }
    }
    if !short.is_empty() {
        text.push_str(&format!(" - {short}"));
    }
    text
}

/// A search (gallery) log's status, and how many of its pages are done of
/// how many (`GenerateGallerySeedLogStatus`): "1 successful, 2 pending".
pub fn search_log_status(counts: &StatusCounts) -> (String, (usize, usize)) {
    let mut parts = Vec::new();
    for (status, what) in [
        (SeedStatus::SuccessfulAndNew, "successful"),
        (SeedStatus::Vetoed, "ignored"),
        (SeedStatus::Error, "failed"),
        (SeedStatus::Skipped, "skipped"),
        (SeedStatus::Unknown, "pending"),
    ] {
        let n = count(counts, status);
        if n > 0 {
            parts.push(format!("{} {what}", human(n)));
        }
    }
    let total: usize = counts.values().sum();
    let unknown = count(counts, SeedStatus::Unknown);
    (parts.join(", "), (total - unknown, total))
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

const QUEUE_COLUMNS: &str = "queue_id, kind, name, page_key, created, files_paused, gallery_paused, \
     options, extra, page_closed";

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
            page_closed: row.get(9)?,
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

/// The nudge that wakes the subscriptions daemon rather than a queue (no
/// queue has this id): menu bar's network > pause > "nudge subscriptions
/// awake".
pub const SUBSCRIPTIONS_NUDGE: i64 = 0;

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

/// Give queue `to` a copy of `from`'s file and gallery seeds (a
/// duplicated subscription's query, its log copied as the reference's
/// export and import copies it).
pub fn copy_seeds(conn: &Connection, from: i64, to: i64) -> Result<()> {
    conn.prepare_cached(
        "INSERT INTO file_seeds (queue_id, position, seed_type, data, data_for_comparison, created, modified, source_time, status, note, referral_url, metadata)
         SELECT ?2, position, seed_type, data, data_for_comparison, created, modified, source_time, status, note, referral_url, metadata
         FROM file_seeds WHERE queue_id = ?1 ORDER BY position",
    )?
    .execute(params![from, to])?;
    conn.prepare_cached(
        "INSERT INTO gallery_seeds (queue_id, position, url, can_generate_more_pages, created, modified, status, note, referral_url, metadata)
         SELECT ?2, position, url, can_generate_more_pages, created, modified, status, note, referral_url, metadata
         FROM gallery_seeds WHERE queue_id = ?1 ORDER BY position",
    )?
    .execute(params![from, to])?;
    Ok(())
}

/// Make a queue.
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

/// A local import's settings (`HDDImport`'s), kept as its queue's extra.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalImport {
    /// Delete each file (to the recycle bin, if the options say) once it
    /// is imported or found already in the database, and its sidecars.
    pub delete_after_success: bool,
    /// The sidecar routers each imported file's metadata is read with.
    pub routers: Vec<hydrus_parse::sidecar::Router>,
}

impl LocalImport {
    /// A queue's, if it is a local import.
    pub fn of(queue: &Queue) -> Option<Self> {
        (queue.kind == QueueKind::LocalImport)
            .then(|| serde_json::from_value(queue.extra.clone()).unwrap_or_default())
    }
}

/// A page a simple downloader is to parse, with the formula it was given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleJob {
    pub url: String,
    pub formula: hydrus_parse::simple::SimpleFormula,
}

/// A simple downloader's state, kept as its queue's extra: the formula
/// chosen for new jobs, and the pages waiting to be parsed, in order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimpleDownloader {
    pub formula_name: String,
    pub pending: Vec<SimpleJob>,
}

impl SimpleDownloader {
    /// A queue's, if it is a simple downloader.
    pub fn of(queue: &Queue) -> Option<Self> {
        (queue.kind == QueueKind::SimpleDownloader)
            .then(|| serde_json::from_value(queue.extra.clone()).unwrap_or_default())
    }
}

/// Make a simple downloader (`SimpleDownloaderImport.__init__`) on this
/// formula, with no jobs yet; its id.
pub fn create_simple_downloader(
    conn: &Connection,
    page_key: Option<&[u8]>,
    options: &ImportOptionsSlice,
    state: &SimpleDownloader,
    now: i64,
) -> Result<i64> {
    let id = create_queue(
        conn,
        QueueKind::SimpleDownloader,
        "simple downloader",
        page_key,
        options,
        now,
    )?;
    set_queue_extra(conn, id, &serde_json::to_value(state)?)?;
    Ok(id)
}

/// Change a simple downloader's state (`PendJob`, `SetPendingJobs`,
/// `SetFormulaName`...): `change` is given it as stored now.
pub fn update_simple_downloader(
    conn: &Connection,
    id: i64,
    change: impl FnOnce(&mut SimpleDownloader),
) -> Result<()> {
    let Some(queue) = queue(conn, id)? else {
        return Ok(());
    };
    let Some(mut state) = SimpleDownloader::of(&queue) else {
        return Ok(());
    };
    change(&mut state);
    set_queue_extra(conn, id, &serde_json::to_value(state)?)
}

/// Tags for some paths: by path, `(tag service key hex, tags)`.
pub type PathTags = BTreeMap<String, Vec<(String, BTreeSet<String>)>>;

/// Make a local import of `paths`, each a path seed with its modified time
/// (seconds) as its source time, in order (`HDDImport.__init__`); its id.
/// `tags` are tags for some paths, by tag service, added to their files as
/// they are (`paths_to_additional_service_keys_to_tags`).
pub fn create_local_import(
    conn: &Connection,
    page_key: Option<&[u8]>,
    options: &ImportOptionsSlice,
    paths: &[(String, Option<i64>)],
    tags: &PathTags,
    settings: LocalImport,
    now: i64,
) -> Result<i64> {
    let id = create_queue(
        conn,
        QueueKind::LocalImport,
        "import",
        page_key,
        options,
        now,
    )?;
    set_queue_extra(conn, id, &serde_json::to_value(settings)?)?;
    let seeds: Vec<NewFileSeed> = paths
        .iter()
        .map(|(path, modified)| NewFileSeed {
            seed_type: SeedType::Path,
            data: path.clone(),
            data_for_comparison: path.clone(),
            source_time: *modified,
            referral_url: None,
            meta: FileSeedMeta {
                external_additional_tags: tags.get(path).cloned().unwrap_or_default(),
                ..FileSeedMeta::default()
            },
        })
        .collect();
    add_file_seeds(conn, id, &seeds, false, now)?;
    Ok(id)
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

/// The queues a page made (those with its key), oldest first.
pub fn queues_with_page_key(conn: &Connection, page_key: &[u8]) -> Result<Vec<Queue>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {QUEUE_COLUMNS} FROM import_queues WHERE page_key = ? ORDER BY queue_id"
    ))?;
    let rows = stmt
        .query_map([page_key], queue_from_row)?
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
/// Close or reopen a queue's page (see [`Queue::page_closed`]).
pub fn set_page_closed(conn: &Connection, id: i64, closed: bool) -> Result<()> {
    conn.execute(
        "UPDATE import_queues SET page_closed = ? WHERE queue_id = ?",
        params![closed, id],
    )?;
    nudge(conn, id)
}

/// Delete the queues of pages closed and not reopened (as the reference's
/// closed pages go once the client closes): how many there were.
pub fn delete_closed_queues(conn: &Connection) -> Result<usize> {
    let closed: Vec<i64> = conn
        .prepare("SELECT queue_id FROM import_queues WHERE page_closed = 1")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for &id in &closed {
        delete_queue(conn, id)?;
    }
    Ok(closed.len())
}

/// Hand URLs typed into a URL queue's page to whoever runs the queues (the
/// daemon), to add as the reference adds them (`PendURLs`).
pub fn request_urls(conn: &Connection, queue: i64, urls: &[String]) -> Result<()> {
    conn.execute(
        "INSERT INTO queue_url_requests (queue_id, urls) VALUES (?, ?)",
        params![queue, urls.join("\n")],
    )?;
    nudge(conn, queue)
}

/// The URLs handed to a queue (see [`request_urls`]), in order, taken.
pub fn take_url_requests(conn: &Connection, queue: i64) -> Result<Vec<String>> {
    let lines: Vec<String> = conn
        .prepare("SELECT urls FROM queue_url_requests WHERE queue_id = ? ORDER BY request_id")?
        .query_map([queue], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    conn.execute("DELETE FROM queue_url_requests WHERE queue_id = ?", [queue])?;
    Ok(lines
        .iter()
        .flat_map(|urls| urls.lines().map(str::to_owned))
        .collect())
}

/// The files a queue's page shows, in the queue's order: those its seeds
/// imported, or found already in the database (`FileSeed.ShouldPresent`,
/// with the reference's default presentation, all files).
pub fn presented_files(conn: &Connection, queue: i64) -> Result<Vec<HashId>> {
    let mut stmt = conn.prepare_cached(
        "SELECT metadata FROM file_seeds WHERE queue_id = ? AND status IN (1, 2, 9)
         ORDER BY position",
    )?;
    let metas: Vec<String> = stmt
        .query_map([queue], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut hashes = Vec::new();
    for meta in metas {
        let meta: FileSeedMeta = parse(&meta, "file seed metadata")?;
        if let Some(hash) = meta
            .hashes
            .iter()
            .find(|(kind, _)| kind == "sha256")
            .and_then(|(_, hex)| hex.parse::<hydrus_core::Sha256>().ok())
        {
            hashes.push(hash);
        }
    }
    let ids = crate::master::hash_ids(conn, &hashes)?;
    let mut seen = std::collections::HashSet::new();
    Ok(hashes
        .iter()
        .filter_map(|h| ids.get(h).copied())
        .filter(|id| seen.insert(*id))
        .collect())
}

/// Seeds of a queue's search log (gallery pages) by status.
pub fn gallery_seed_counts(conn: &Connection, queue: i64) -> Result<StatusCounts> {
    let mut stmt = conn.prepare_cached(
        "SELECT status, COUNT(*) FROM gallery_seeds WHERE queue_id = ? GROUP BY status",
    )?;
    let rows = stmt.query_map([queue], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
    let mut counts = StatusCounts::new();
    for row in rows {
        let (code, n) = row?;
        *counts.entry(status(code)?).or_default() += usize::try_from(n).unwrap_or(0);
    }
    Ok(counts)
}

pub fn delete_queue(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM queue_url_requests WHERE queue_id = ?", [id])?;
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

/// The files a queue presents as `options` say, or (none given) as its own
/// presentation options do (`GetPresentedHashes`): those its seeds
/// imported or found, new or in the inbox as the options want, and in
/// their location; in the queue's order, each once.
pub fn presented_files_as(
    conn: &Connection,
    queue_id: i64,
    options: Option<&hydrus_core::import_options::PresentationOptions>,
) -> Result<Vec<HashId>> {
    let own = if options.is_none() {
        queue(conn, queue_id)?
            .and_then(|q| q.options.presentation)
            .unwrap_or_default()
    } else {
        hydrus_core::import_options::PresentationOptions::default()
    };
    let options = options.unwrap_or(&own);
    let mut found: Vec<(hydrus_core::Sha256, bool)> = Vec::new();
    for seed in file_seeds(conn, queue_id)? {
        if !seed.status.is_successful() {
            continue;
        }
        if let Some(hash) = seed
            .meta
            .hash("sha256")
            .and_then(|h| h.parse::<hydrus_core::Sha256>().ok())
        {
            found.push((hash, seed.status == SeedStatus::SuccessfulAndNew));
        }
    }
    let hashes: Vec<hydrus_core::Sha256> = found.iter().map(|(h, _)| *h).collect();
    let ids = crate::master::hash_ids(conn, &hashes)?;
    let all: Vec<HashId> = ids.values().copied().collect();
    let inbox = crate::media::inboxed(conn, &all)?;
    let mut located = std::collections::HashSet::new();
    for key in &options.location {
        let Ok(key) = hex::decode(key) else {
            continue;
        };
        let service: Option<hydrus_core::ServiceId> = conn
            .query_row(
                "SELECT service_id FROM services WHERE service_key = ?",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(service) = service {
            located.extend(crate::media::current_in(conn, service, &all)?);
        }
    }
    let mut seen = std::collections::HashSet::new();
    Ok(found
        .iter()
        .filter_map(|(hash, new)| {
            let id = *ids.get(hash)?;
            (located.contains(&id)
                && options.presents(*new, || inbox.contains(&id))
                && seen.insert(id))
            .then_some(id)
        })
        .collect())
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

/// Set seeds to a status, their notes cleared (`SetStatus`): set back to
/// unknown, their hashes are forgotten too, so they import afresh.
pub fn set_file_seed_statuses(
    conn: &Connection,
    ids: &[i64],
    status: SeedStatus,
    now: i64,
) -> Result<()> {
    for &id in ids {
        let Some(mut seed) = file_seed(conn, id)? else {
            continue;
        };
        seed.status = status;
        seed.note.clear();
        seed.modified = now;
        if status == SeedStatus::Unknown {
            seed.meta.hashes.clear();
        }
        update_file_seed(conn, &seed)?;
    }
    Ok(())
}

/// Reverse the order a queue's seeds are worked in (`ReverseFileSeedCache`).
pub fn reverse_file_seeds(conn: &Connection, queue: i64) -> Result<()> {
    conn.prepare_cached("UPDATE file_seeds SET position = -position WHERE queue_id = ?")?
        .execute([queue])?;
    Ok(())
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
        crate::schema::configure(&conn).unwrap();
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
    fn a_pages_queue_waits_while_closed_and_goes_with_it() {
        let conn = conn();
        let opts = ImportOptionsSlice::default();
        let kept = create_queue(&conn, QueueKind::Urls, "kept", None, &opts, 0).unwrap();
        let closed = create_queue(&conn, QueueKind::Urls, "closed", None, &opts, 0).unwrap();
        add_file_seeds(&conn, closed, &[seed("https://a.example/1")], false, 0).unwrap();
        request_urls(&conn, closed, &["https://a.example/2".into()]).unwrap();
        set_page_closed(&conn, closed, true).unwrap();
        // (nudged, for the daemon to stop its work now)
        assert_eq!(take_nudges(&conn).unwrap(), [closed]);
        assert!(queue(&conn, closed).unwrap().unwrap().page_closed);
        assert!(!queue(&conn, kept).unwrap().unwrap().page_closed);
        // reopened, it runs again
        set_page_closed(&conn, closed, false).unwrap();
        assert!(!queue(&conn, closed).unwrap().unwrap().page_closed);
        // closed for good: it and everything of it go
        set_page_closed(&conn, closed, true).unwrap();
        assert_eq!(delete_closed_queues(&conn).unwrap(), 1);
        assert!(queue(&conn, closed).unwrap().is_none());
        assert!(file_seeds(&conn, closed).unwrap().is_empty());
        assert!(take_url_requests(&conn, closed).unwrap().is_empty());
        assert!(queue(&conn, kept).unwrap().is_some());
    }

    #[test]
    fn urls_typed_into_a_page_are_handed_over_once_in_order() {
        let conn = conn();
        let opts = ImportOptionsSlice::default();
        let a = create_queue(&conn, QueueKind::Urls, "a", None, &opts, 0).unwrap();
        let b = create_queue(&conn, QueueKind::Urls, "b", None, &opts, 0).unwrap();
        request_urls(
            &conn,
            a,
            &["https://x.example/1".into(), "https://x.example/2".into()],
        )
        .unwrap();
        request_urls(&conn, b, &["https://y.example/1".into()]).unwrap();
        request_urls(&conn, a, &["https://x.example/3".into()]).unwrap();
        assert_eq!(take_nudges(&conn).unwrap(), [a, b]);
        assert_eq!(
            take_url_requests(&conn, a).unwrap(),
            [
                "https://x.example/1",
                "https://x.example/2",
                "https://x.example/3"
            ]
        );
        assert!(take_url_requests(&conn, a).unwrap().is_empty());
        assert_eq!(
            take_url_requests(&conn, b).unwrap(),
            ["https://y.example/1"]
        );
    }

    #[test]
    fn a_queues_page_shows_the_files_its_seeds_brought_in_order() {
        let conn = conn();
        let opts = ImportOptionsSlice::default();
        let q = create_queue(&conn, QueueKind::Urls, "q", None, &opts, 0).unwrap();
        let hash = |n: u8| hydrus_core::Sha256::from_slice(&[n; 32]).unwrap();
        let ids: Vec<HashId> = (1..=3)
            .map(|n| crate::master::intern_hash(&conn, &hash(n)).unwrap())
            .collect();
        let urls = [
            "https://a.example/1",
            "https://a.example/2",
            "https://a.example/3",
            "https://a.example/4",
            "https://a.example/5",
        ];
        let seeds: Vec<NewFileSeed> = urls.iter().map(|u| seed(u)).collect();
        add_file_seeds(&conn, q, &seeds, false, 0).unwrap();
        let mut stored = file_seeds(&conn, q).unwrap();
        // new, failed (with a hash), already in db (the first file again),
        // already in db, and not tried yet
        for (seed, (status, file)) in stored.iter_mut().zip([
            (SeedStatus::SuccessfulAndNew, Some(2)),
            (SeedStatus::Error, Some(3)),
            (SeedStatus::SuccessfulButRedundant, Some(2)),
            (SeedStatus::SuccessfulButRedundant, Some(1)),
            (SeedStatus::Unknown, None),
        ]) {
            seed.status = status;
            if let Some(n) = file {
                seed.meta.set_hash("sha256", hash(n).to_hex());
            }
            update_file_seed(&conn, seed).unwrap();
        }
        assert_eq!(presented_files(&conn, q).unwrap(), [ids[1], ids[0]]);
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
