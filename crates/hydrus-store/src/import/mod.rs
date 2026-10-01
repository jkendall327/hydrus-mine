//! One-time import of a reference (hydrus v688) database.
//!
//! The source is attached **read-only** and never modified. Primary data is
//! copied with bulk `INSERT … SELECT` statements, preserving every interned
//! id, into a brand new database file written without a journal (it's a
//! scratch file until the import succeeds, when it is renamed into place).
//! Derived data is then rebuilt from scratch, and every copied table's row
//! count is checked against its source.
//!
//! Service settings and serialised objects need the reference's serialisation
//! format decoded: [`decode_input`] does that with `hydrus-legacy`'s typed
//! decoders, and [`import_legacy`] puts the two halves together.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_core::{ServiceId, ServiceKey, ServiceType};

use crate::error::{Result, StoreError};
use crate::network::{self, Cookie, CustomHeader, NetworkContext};
use crate::services::{self, ServiceKind};
use crate::{queues, schema, subscriptions};

mod decode;

pub use decode::auto_resolution_rule;

pub use decode::decode_input;
pub use decode::stored_duplicates_page;

/// Reference schema version this importer understands.
pub const SUPPORTED_REFERENCE_VERSION: u32 = 688;

/// What the caller decoded from the source's serialised objects.
#[derive(Debug, Default, Clone)]
pub struct ImportInput {
    /// Settings for services whose type carries settings (ratings, the
    /// Client API, repositories). Keyed by the source's service id.
    pub service_kinds: HashMap<ServiceId, ServiceKind>,
    /// Client API access keys.
    pub api_permissions: Vec<ApiPermissionsRow>,
    /// Typed settings to store under `settings`, as JSON.
    pub settings: BTreeMap<String, serde_json::Value>,
    /// Custom HTTP headers; `None` gives the store the defaults.
    pub custom_headers: Option<Vec<(NetworkContext, CustomHeader)>>,
    /// Unexpired cookies, by session.
    pub cookies: Vec<(NetworkContext, Cookie)>,
    /// Subscriptions; their queries' histories are copied during the import.
    pub subscriptions: Vec<SubscriptionInput>,
    /// Import folders, with the files each has seen.
    pub import_folders: Vec<ImportFolderInput>,
    /// Downloader pages open in the session the reference opens with.
    pub downloader_pages: Vec<DownloaderPageInput>,
    /// The session the reference opens with, as a tree of pages.
    pub session: Option<SessionInput>,
    /// Duplicates auto-resolution rules, by the reference's rule id (their
    /// pair statuses are copied during the import).
    pub auto_resolution_rules: Vec<(i64, crate::duplicates::auto::Rule)>,
    /// Each network context's bandwidth usage so far (the rules are a
    /// setting).
    pub bandwidth_usage: Vec<(NetworkContext, hydrus_core::bandwidth::Tracker)>,
    /// Things that could not be converted (they are still kept verbatim).
    pub warnings: Vec<String>,
}

/// An import folder to import.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportFolderInput {
    pub name: String,
    pub settings: hydrus_parse::folders::ImportFolderSettings,
    pub options: hydrus_core::import_options::ImportOptionsSlice,
    pub paused: bool,
    pub file_seeds: Vec<crate::queues::FileSeed>,
}

/// A subscription to import.
#[derive(Debug, Clone, PartialEq)]
pub struct SubscriptionInput {
    pub name: String,
    pub settings: SubscriptionSettings,
    /// Each query's state, with the name its history is stored under in the
    /// source's `json_dumps_named`.
    pub queries: Vec<(String, QueryState)>,
}

/// A downloader page open in the session the reference opens with. Its
/// work carries over as queues named after the page: a URL page's one
/// queue, a gallery page's searches or a watcher page's watchers.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloaderPageInput {
    pub name: String,
    pub queues: Vec<PageQueueInput>,
}

/// A session's tree of pages.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionInput {
    pub name: String,
    pub pages: Vec<PageInput>,
}

/// A page of a session, or a notebook of pages.
#[derive(Debug, Clone, PartialEq)]
pub struct PageInput {
    pub name: String,
    pub content: PageInputContent,
    /// The files the page showed, in order.
    pub hashes: Vec<hydrus_core::Sha256>,
}

/// What a page is: [`hydrus_core::pages::PageContent`], with a downloader
/// page's queues still to be made.
#[derive(Debug, Clone, PartialEq)]
pub enum PageInputContent {
    Pages(Vec<PageInput>),
    Search {
        search: hydrus_core::search::context::FileSearchContext,
        synchronised: bool,
        sort: Option<hydrus_core::pages::PageSort>,
    },
    /// Showing the queues of `downloader_pages[index]`.
    Downloader {
        kind: hydrus_core::pages::DownloaderKind,
        index: usize,
        sort: Option<hydrus_core::pages::PageSort>,
    },
    Duplicates {
        duplicates: hydrus_core::pages::DuplicatesPage,
        sort: Option<hydrus_core::pages::PageSort>,
    },
    Other {
        page_type: i64,
        stored: Option<serde_json::Value>,
        sort: Option<hydrus_core::pages::PageSort>,
    },
}

/// One of a downloader page's queues.
#[derive(Debug, Clone, PartialEq)]
pub struct PageQueueInput {
    pub options: hydrus_core::import_options::ImportOptionsSlice,
    pub files_paused: bool,
    pub gallery_paused: bool,
    /// Seconds (`None` for a URL page, which doesn't keep one).
    pub created: Option<i64>,
    pub state: PageQueueState,
    pub file_seeds: Vec<hydrus_legacy::objects::subscriptions::LegacyFileSeed>,
    pub gallery_seeds: Vec<hydrus_legacy::objects::subscriptions::LegacyGallerySeed>,
}

/// What kind of queue, with its state.
#[derive(Debug, Clone, PartialEq)]
pub enum PageQueueState {
    Urls,
    Gallery(hydrus_core::gallery::GallerySearch),
    /// Its next check is timed when imported, from its files, as the
    /// reference times it when the page starts.
    Watcher(hydrus_core::watchers::WatcherState),
}

/// One Client API access key, as stored natively.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiPermissionsRow {
    pub access_key: Vec<u8>,
    pub name: String,
    pub permits_everything: bool,
    pub permissions: serde_json::Value,
    pub search_tag_filter: Option<serde_json::Value>,
}

/// What an import did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ImportReport {
    /// Rows copied per destination table.
    pub rows: BTreeMap<String, u64>,
    pub services: usize,
    pub tag_services: usize,
    /// Settings that could not be converted and were only kept verbatim.
    pub warnings: Vec<String>,
}

/// Import the reference install whose database directory is `source_dir`
/// into a new native database at `dest`, which must not exist.
pub fn import_legacy(source_dir: &Path, dest: &Path) -> Result<ImportReport> {
    let input = {
        // see attach_read_only: don't leave WAL sidecars behind in the source
        let mode = if source_is_open(source_dir) {
            hydrus_legacy::OpenMode::Shared
        } else {
            hydrus_legacy::OpenMode::Immutable
        };
        let options = hydrus_legacy::OpenOptions {
            mode,
            ..hydrus_legacy::OpenOptions::default()
        };
        let db = hydrus_legacy::LegacyDb::open_with(source_dir, options)?;
        decode_input(&db)?
    };
    import(source_dir, dest, &input)
}

/// Whether any of the source database files has a WAL file, i.e. it is open
/// in the reference client or wasn't closed cleanly.
fn source_is_open(source_dir: &Path) -> bool {
    hydrus_legacy::db::DATABASE_FILES
        .iter()
        .any(|(_, file)| has_wal(&source_dir.join(file)))
}

fn has_wal(path: &Path) -> bool {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    Path::new(&wal).exists()
}

/// Import the reference database in `source_dir` into a new native database
/// at `dest`, which must not exist, with already-decoded settings.
pub fn import(source_dir: &Path, dest: &Path, input: &ImportInput) -> Result<ImportReport> {
    if dest.exists() {
        return Err(StoreError::Invalid(format!(
            "{} already exists",
            dest.display()
        )));
    }
    let scratch = scratch_path(dest);
    let _ = std::fs::remove_file(&scratch);
    let result = import_into(source_dir, &scratch, input);
    match result {
        Ok(mut report) => {
            std::fs::rename(&scratch, dest)?;
            copy_mpv_conf(source_dir, dest, &mut report);
            Ok(report)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&scratch);
            Err(e)
        }
    }
}

/// The reference's mpv settings (`mpv.conf` in its database directory),
/// which the desktop client's player loads from the store's directory.
fn copy_mpv_conf(source_dir: &Path, dest: &Path, report: &mut ImportReport) {
    let source = source_dir.join("mpv.conf");
    let Some(target) = dest.parent().map(|dir| dir.join("mpv.conf")) else {
        return;
    };
    if !source.exists() || target.exists() {
        return;
    }
    if let Err(e) = std::fs::copy(&source, &target) {
        report.warnings.push(format!(
            "mpv.conf could not be copied, so video plays with hydrus's default mpv settings: {e}"
        ));
    }
}

fn scratch_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".importing");
    dest.with_file_name(name)
}

/// Attach a source file read-only without creating anything next to it.
///
/// A cleanly closed WAL database has no `-wal` file; attaching it read-only
/// would make SQLite create `-wal`/`-shm` sidecars, so it is attached as
/// `immutable` instead (and [`SourceFingerprint`] checks afterwards that it
/// really didn't change). If a `-wal` exists, the reference client is probably
/// running; its sidecars already exist and a normal read-only attach reads a
/// consistent snapshot alongside it.
fn attach_read_only(conn: &Connection, path: &Path, alias: &str) -> Result<()> {
    if !path.exists() {
        return Err(StoreError::Invalid(format!(
            "{} does not exist",
            path.display()
        )));
    }
    let mode = if has_wal(path) {
        "mode=ro"
    } else {
        "mode=ro&immutable=1"
    };
    let uri = format!("file:{}?{mode}", path.display());
    conn.execute(&format!("ATTACH DATABASE ?1 AS {alias}"), [uri])?;
    Ok(())
}

/// Sizes and modification times of the source files, to prove they didn't
/// change during an import.
#[derive(Debug, PartialEq, Eq)]
struct SourceFingerprint(Vec<(PathBuf, u64, Option<std::time::SystemTime>)>);

impl SourceFingerprint {
    fn take(source_dir: &Path) -> Result<Self> {
        let mut out = Vec::new();
        for name in ["client.db", "client.master.db", "client.mappings.db"] {
            let path = source_dir.join(name);
            let meta = std::fs::metadata(&path)?;
            out.push((path, meta.len(), meta.modified().ok()));
        }
        Ok(Self(out))
    }
}

fn import_into(source_dir: &Path, scratch: &Path, input: &ImportInput) -> Result<ImportReport> {
    let mut conn = Connection::open_with_flags(
        scratch,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.execute_batch(
        "PRAGMA journal_mode = OFF;
         PRAGMA synchronous = OFF;
         PRAGMA main.locking_mode = EXCLUSIVE;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -1048576;",
    )?;
    rusqlite::vtab::array::load_module(&conn)?;
    schema::migrate(&mut conn)?;

    attach_read_only(&conn, &source_dir.join("client.db"), "src")?;
    attach_read_only(&conn, &source_dir.join("client.master.db"), "src_master")?;
    attach_read_only(
        &conn,
        &source_dir.join("client.mappings.db"),
        "src_mappings",
    )?;

    let version: u32 = conn.query_row("SELECT version FROM src.version", [], |r| r.get(0))?;
    if version != SUPPORTED_REFERENCE_VERSION {
        return Err(StoreError::Invalid(format!(
            "this database is from hydrus v{version}; update it to v{SUPPORTED_REFERENCE_VERSION} with the Python \
             client first, then import"
        )));
    }

    let fingerprint = SourceFingerprint::take(source_dir)?;
    let mut report = ImportReport {
        warnings: input.warnings.clone(),
        ..ImportReport::default()
    };
    let tx = conn.transaction()?;
    let mut copier = Copier {
        conn: &tx,
        report: &mut report,
    };

    let services = copier.services(input)?;
    copier.master()?;
    copier.files(&services)?;
    copier.file_metadata()?;
    copier.tag_services(&services)?;
    copier.duplicates()?;
    copier.storage(source_dir)?;
    copier.settings(input)?;
    copier.network(input)?;
    copier.subscriptions(input)?;
    let page_queues = copier.downloader_pages(input)?;
    copier.session(input, &page_queues)?;
    copier.import_folders(input)?;
    copier.auto_resolution(input)?;
    copier.derived()?;
    tx.commit()?;

    conn.execute_batch("DETACH src; DETACH src_master; DETACH src_mappings;")?;
    if SourceFingerprint::take(source_dir)? != fingerprint {
        return Err(StoreError::Invalid(
            "the source database changed while it was being imported; close hydrus and try again"
                .into(),
        ));
    }
    conn.execute_batch("ANALYZE; PRAGMA main.locking_mode = NORMAL; PRAGMA journal_mode = WAL;")?;
    Ok(report)
}

/// A source file service with materialised membership tables.
struct SourceFileService {
    id: ServiceId,
}

struct SourceServices {
    file_services: Vec<SourceFileService>,
    tag_services: Vec<ServiceId>,
}

/// A file storage location in the source.
struct SourceLocation {
    id: i64,
    location: String,
    weight: Option<i64>,
    max_bytes: Option<i64>,
    thumbnail_override: bool,
}

struct Copier<'a> {
    conn: &'a Connection,
    report: &'a mut ImportReport,
}

impl Copier<'_> {
    fn table_exists(&self, schema: &str, table: &str) -> Result<bool> {
        let n: i64 = self.conn.query_row(
            &format!(
                "SELECT count(*) FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?"
            ),
            [table],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Run a bulk copy and record how many rows it produced.
    fn copy(&mut self, dest_table: &str, sql: &str) -> Result<u64> {
        let n = self.conn.execute(sql, [])? as u64;
        *self.report.rows.entry(dest_table.to_owned()).or_default() += n;
        Ok(n)
    }

    /// Like `copy`, and check the source had exactly that many rows.
    fn copy_checked(&mut self, dest_table: &str, source_table: &str, sql: &str) -> Result<()> {
        let expected: i64 =
            self.conn
                .query_row(&format!("SELECT count(*) FROM {source_table}"), [], |r| {
                    r.get(0)
                })?;
        let expected = expected as u64;
        let copied = self.copy(dest_table, sql)?;
        if copied != expected {
            return Err(StoreError::Corrupt(format!(
                "copied {copied} rows from {source_table} into {dest_table}, but the source has {expected}"
            )));
        }
        Ok(())
    }

    fn services(&mut self, input: &ImportInput) -> Result<SourceServices> {
        let mut stmt = self
            .conn
            .prepare("SELECT service_id, service_key, service_type, name FROM src.services")?;
        let rows: Vec<(ServiceId, ServiceKey, u8, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let mut out = SourceServices {
            file_services: Vec::new(),
            tag_services: Vec::new(),
        };
        for (id, key, type_code, name) in rows {
            let service_type = ServiceType::from_code(type_code).ok_or_else(|| {
                StoreError::Corrupt(format!("service {name} has unknown type {type_code}"))
            })?;
            let kind = match input.service_kinds.get(&id) {
                Some(kind) if kind.service_type() == service_type => kind.clone(),
                Some(kind) => {
                    return Err(StoreError::Invalid(format!(
                        "decoded settings for {name} are for a {}, but it is a {service_type}",
                        kind.service_type()
                    )));
                }
                None => settingless_kind(service_type).ok_or_else(|| {
                    StoreError::Invalid(format!(
                        "no decoded settings supplied for service \"{name}\" ({service_type})"
                    ))
                })?,
            };
            services::insert_with_id(self.conn, id, &key, &name, &kind)?;
            self.report.services += 1;
            if kind.has_mappings() {
                out.tag_services.push(id);
                self.report.tag_services += 1;
            }
            if self.table_exists("src", &format!("current_files_{id}"))? {
                out.file_services.push(SourceFileService { id });
            }
        }
        // keep AUTOINCREMENT from reusing ids of services deleted in the source
        self.conn.execute(
            "INSERT OR REPLACE INTO sqlite_sequence (name, seq) SELECT 'services', seq FROM src.sqlite_sequence WHERE name = 'services'",
            [],
        )?;
        Ok(out)
    }

    fn master(&mut self) -> Result<()> {
        let pairs = [
            (
                "hashes",
                "src_master.hashes",
                "INSERT INTO hashes (hash_id, sha256) SELECT hash_id, hash FROM src_master.hashes",
            ),
            (
                "hash_digests",
                "src_master.local_hashes",
                "INSERT INTO hash_digests (hash_id, md5, sha1, sha512) SELECT hash_id, md5, sha1, sha512 FROM src_master.local_hashes",
            ),
            (
                "namespaces",
                "src_master.namespaces",
                "INSERT INTO namespaces (namespace_id, namespace) SELECT namespace_id, namespace FROM src_master.namespaces",
            ),
            (
                "subtags",
                "src_master.subtags",
                "INSERT INTO subtags (subtag_id, subtag) SELECT subtag_id, subtag FROM src_master.subtags",
            ),
            (
                "tags",
                "src_master.tags",
                "INSERT INTO tags (tag_id, namespace_id, subtag_id) SELECT tag_id, namespace_id, subtag_id FROM src_master.tags",
            ),
            (
                "url_domains",
                "src_master.url_domains",
                "INSERT INTO url_domains (domain_id, domain) SELECT domain_id, domain FROM src_master.url_domains",
            ),
            (
                "urls",
                "src_master.urls",
                "INSERT INTO urls (url_id, domain_id, url) SELECT url_id, domain_id, url FROM src_master.urls",
            ),
            (
                "texts",
                "src_master.texts",
                "INSERT INTO texts (text_id, text) SELECT text_id, text FROM src_master.texts",
            ),
            (
                "labels",
                "src_master.labels",
                "INSERT INTO labels (label_id, label) SELECT label_id, label FROM src_master.labels",
            ),
            (
                "notes",
                "src_master.notes",
                "INSERT INTO notes (note_id, note) SELECT note_id, note FROM src_master.notes",
            ),
            (
                "perceptual_hashes",
                "src_master.shape_perceptual_hashes",
                "INSERT INTO perceptual_hashes (phash_id, phash) SELECT phash_id, phash FROM src_master.shape_perceptual_hashes",
            ),
        ];
        for (dest, source, sql) in pairs {
            self.copy_checked(dest, source, sql)?;
        }
        Ok(())
    }

    fn files(&mut self, services: &SourceServices) -> Result<()> {
        self.copy_checked(
            "files",
            "src.files_info",
            "INSERT INTO files (hash_id, size, mime, width, height, duration_ms, num_frames, has_audio, num_words,
                                forced_mime, file_modified_ms, pixel_hash, blurhash, flags)
             SELECT fi.hash_id, fi.size, fi.mime, fi.width, fi.height, fi.duration, fi.num_frames,
                    coalesce(fi.has_audio, 0), fi.num_words,
                    (SELECT forced_mime FROM src.files_info_forced_filetypes f WHERE f.hash_id = fi.hash_id),
                    (SELECT file_modified_timestamp_ms FROM src.file_modified_timestamps m WHERE m.hash_id = fi.hash_id),
                    (SELECT h.hash FROM src.pixel_hash_map p JOIN src_master.hashes h ON h.hash_id = p.pixel_hash_id
                     WHERE p.hash_id = fi.hash_id),
                    (SELECT blurhash FROM src_master.blurhashes b WHERE b.hash_id = fi.hash_id),
                    (EXISTS (SELECT 1 FROM src.has_exif x WHERE x.hash_id = fi.hash_id)) * 1
                  | (EXISTS (SELECT 1 FROM src.has_icc_profile x WHERE x.hash_id = fi.hash_id)) * 2
                  | (EXISTS (SELECT 1 FROM src.has_human_readable_embedded_metadata x WHERE x.hash_id = fi.hash_id)) * 4
                  | (EXISTS (SELECT 1 FROM src.has_transparency x WHERE x.hash_id = fi.hash_id)) * 8
                  | (EXISTS (SELECT 1 FROM src.has_xmp x WHERE x.hash_id = fi.hash_id)) * 16
                  | (EXISTS (SELECT 1 FROM src.has_iptc x WHERE x.hash_id = fi.hash_id)) * 32
                  | (EXISTS (SELECT 1 FROM src.has_software_source x WHERE x.hash_id = fi.hash_id)) * 64
             FROM src.files_info fi",
        )?;
        for service in &services.file_services {
            let sid = service.id;
            self.copy_checked(
                "file_domain_current",
                &format!("src.current_files_{sid}"),
                &format!(
                    "INSERT INTO file_domain_current (service_id, hash_id, added_ms)
                     SELECT {sid}, hash_id, timestamp_ms FROM src.current_files_{sid}"
                ),
            )?;
            self.copy_checked(
                "file_domain_deleted",
                &format!("src.deleted_files_{sid}"),
                &format!(
                    "INSERT INTO file_domain_deleted (service_id, hash_id, deleted_ms, original_added_ms)
                     SELECT {sid}, hash_id, timestamp_ms, original_timestamp_ms FROM src.deleted_files_{sid}"
                ),
            )?;
            self.copy_checked(
                "file_domain_pending",
                &format!("src.pending_files_{sid}"),
                &format!("INSERT INTO file_domain_pending (service_id, hash_id) SELECT {sid}, hash_id FROM src.pending_files_{sid}"),
            )?;
            self.copy_checked(
                "file_domain_petitioned",
                &format!("src.petitioned_files_{sid}"),
                &format!(
                    "INSERT INTO file_domain_petitioned (service_id, hash_id, reason_id)
                     SELECT {sid}, hash_id, reason_id FROM src.petitioned_files_{sid}"
                ),
            )?;
        }
        Ok(())
    }

    fn file_metadata(&mut self) -> Result<()> {
        let pairs = [
            (
                "file_perceptual_hashes",
                "src_master.shape_perceptual_hash_map",
                "INSERT INTO file_perceptual_hashes (hash_id, phash_id) SELECT hash_id, phash_id FROM src_master.shape_perceptual_hash_map",
            ),
            (
                "file_deletion_reasons",
                "src.local_file_deletion_reasons",
                "INSERT INTO file_deletion_reasons (hash_id, reason_id) SELECT hash_id, reason_id FROM src.local_file_deletion_reasons",
            ),
            ("file_inbox", "src.file_inbox", "INSERT INTO file_inbox (hash_id) SELECT hash_id FROM src.file_inbox"),
            (
                "file_archived",
                "src.archive_timestamps",
                "INSERT INTO file_archived (hash_id, archived_ms) SELECT hash_id, archived_timestamp_ms FROM src.archive_timestamps",
            ),
            (
                "file_domain_modified",
                "src.file_domain_modified_timestamps",
                "INSERT INTO file_domain_modified (hash_id, domain_id, modified_ms)
                 SELECT hash_id, domain_id, file_modified_timestamp_ms FROM src.file_domain_modified_timestamps",
            ),
            (
                "file_viewing_stats",
                "src.file_viewing_stats",
                "INSERT INTO file_viewing_stats (hash_id, canvas_type, views, viewtime_ms, last_viewed_ms)
                 SELECT hash_id, canvas_type, views, viewtime_ms, last_viewed_timestamp_ms FROM src.file_viewing_stats",
            ),
            ("file_urls", "src.url_map", "INSERT INTO file_urls (hash_id, url_id) SELECT hash_id, url_id FROM src.url_map"),
            (
                "file_notes",
                "src.file_notes",
                "INSERT INTO file_notes (hash_id, label_id, note_id) SELECT hash_id, name_id, note_id FROM src.file_notes",
            ),
            (
                "ratings",
                "src.local_ratings",
                "INSERT INTO ratings (service_id, hash_id, rating) SELECT service_id, hash_id, rating FROM src.local_ratings",
            ),
            (
                "ratings_incdec",
                "src.local_incdec_ratings",
                "INSERT INTO ratings_incdec (service_id, hash_id, rating) SELECT service_id, hash_id, rating FROM src.local_incdec_ratings",
            ),
            (
                "similar_search_status",
                "src.shape_search_cache",
                // the reference marks a reset search with -1; here that is NULL, "not searched"
                "INSERT INTO similar_search_status (hash_id, searched_distance) SELECT hash_id, CASE WHEN searched_distance < 0 THEN NULL ELSE searched_distance END FROM src.shape_search_cache",
            ),
            (
                "recent_tags",
                "src.recent_tags",
                "INSERT INTO recent_tags (service_id, tag_id, used_ms) SELECT service_id, tag_id, timestamp_ms FROM src.recent_tags",
            ),
        ];
        for (dest, source, sql) in pairs {
            self.copy_checked(dest, source, sql)?;
        }
        Ok(())
    }

    fn tag_services(&mut self, services: &SourceServices) -> Result<()> {
        for &sid in &services.tag_services {
            let t = schema::MappingTables::new(sid);
            for (status, dest) in [
                ("current", &t.current),
                ("deleted", &t.deleted),
                ("pending", &t.pending),
            ] {
                self.copy_checked(
                    dest,
                    &format!("src_mappings.{status}_mappings_{sid}"),
                    &format!("INSERT INTO {dest} (tag_id, hash_id) SELECT tag_id, hash_id FROM src_mappings.{status}_mappings_{sid}"),
                )?;
            }
            self.copy_checked(
                &t.petitioned,
                &format!("src_mappings.petitioned_mappings_{sid}"),
                &format!(
                    "INSERT INTO {} (tag_id, hash_id, reason_id) SELECT tag_id, hash_id, reason_id FROM src_mappings.petitioned_mappings_{sid}",
                    t.petitioned
                ),
            )?;

            for (status_code, status) in [
                (0, "current"),
                (1, "pending"),
                (2, "deleted"),
                (3, "petitioned"),
            ] {
                let reason = if matches!(status, "pending" | "petitioned") {
                    "reason_id"
                } else {
                    "NULL"
                };
                self.copy_checked(
                    "tag_siblings",
                    &format!("src.{status}_tag_siblings_{sid}"),
                    &format!(
                        "INSERT INTO tag_siblings (service_id, status, bad_tag_id, good_tag_id, reason_id)
                         SELECT {sid}, {status_code}, bad_tag_id, good_tag_id, {reason} FROM src.{status}_tag_siblings_{sid}"
                    ),
                )?;
                self.copy_checked(
                    "tag_parents",
                    &format!("src.{status}_tag_parents_{sid}"),
                    &format!(
                        "INSERT INTO tag_parents (service_id, status, child_tag_id, parent_tag_id, reason_id)
                         SELECT {sid}, {status_code}, child_tag_id, parent_tag_id, {reason} FROM src.{status}_tag_parents_{sid}"
                    ),
                )?;
            }
        }
        for (kind, table) in [
            (0, "tag_sibling_application"),
            (1, "tag_parent_application"),
        ] {
            self.copy_checked(
                "tag_display_application",
                &format!("src.{table}"),
                &format!(
                    "INSERT INTO tag_display_application (display_service_id, kind, position, source_service_id)
                     SELECT master_service_id, {kind}, service_index, application_service_id FROM src.{table}"
                ),
            )?;
        }
        Ok(())
    }

    fn duplicates(&mut self) -> Result<()> {
        let pairs = [
            (
                "dup_groups",
                "src.duplicate_files",
                "INSERT INTO dup_groups (group_id, king_hash_id) SELECT media_id, king_hash_id FROM src.duplicate_files",
            ),
            (
                "dup_group_members",
                "src.duplicate_file_members",
                "INSERT INTO dup_group_members (group_id, hash_id) SELECT media_id, hash_id FROM src.duplicate_file_members",
            ),
            (
                "alt_groups",
                "src.alternate_file_groups",
                "INSERT INTO alt_groups (alt_group_id) SELECT alternates_group_id FROM src.alternate_file_groups",
            ),
            (
                "alt_group_members",
                "src.alternate_file_group_members",
                "INSERT INTO alt_group_members (alt_group_id, group_id) SELECT alternates_group_id, media_id FROM src.alternate_file_group_members",
            ),
            (
                "alt_confirmed_pairs",
                "src.confirmed_alternate_pairs",
                "INSERT INTO alt_confirmed_pairs (smaller_group_id, larger_group_id) SELECT smaller_media_id, larger_media_id FROM src.confirmed_alternate_pairs",
            ),
            (
                "false_positive_pairs",
                "src.duplicate_false_positives",
                "INSERT INTO false_positive_pairs (smaller_alt_group_id, larger_alt_group_id)
                 SELECT smaller_alternates_group_id, larger_alternates_group_id FROM src.duplicate_false_positives",
            ),
            (
                "potential_pairs",
                "src.potential_duplicate_pairs",
                "INSERT INTO potential_pairs (smaller_group_id, larger_group_id, distance)
                 SELECT smaller_media_id, larger_media_id, distance FROM src.potential_duplicate_pairs",
            ),
        ];
        for (dest, source, sql) in pairs {
            self.copy_checked(dest, source, sql)?;
        }
        Ok(())
    }

    /// File storage locations. Relative paths are relative to the source db
    /// dir; they're made absolute so the new database finds the same files.
    fn storage(&mut self, source_dir: &Path) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT c.location_id, c.location, i.weight, i.max_num_bytes,
                    EXISTS (SELECT 1 FROM src.ideal_thumbnail_override_location t WHERE t.location_id = c.location_id)
             FROM src.current_client_files_locations c
             LEFT JOIN src.ideal_client_files_locations i USING (location_id)",
        )?;
        let rows: Vec<SourceLocation> = stmt
            .query_map([], |r| {
                Ok(SourceLocation {
                    id: r.get(0)?,
                    location: r.get(1)?,
                    weight: r.get(2)?,
                    max_bytes: r.get(3)?,
                    thumbnail_override: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        for SourceLocation {
            id,
            location,
            weight,
            max_bytes,
            thumbnail_override,
        } in rows
        {
            let path = Path::new(&location);
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                source_dir.join(path)
            };
            self.conn.execute(
                "INSERT INTO storage_locations (location_id, path, ideal_weight, max_bytes, is_thumbnail_override)
                 VALUES (?, ?, ?, ?, ?)",
                params![id, absolute.to_string_lossy(), weight, max_bytes, thumbnail_override],
            )?;
            *self
                .report
                .rows
                .entry("storage_locations".into())
                .or_default() += 1;
        }
        self.copy_checked(
            "storage_subfolders",
            "src.client_files_subfolders",
            "INSERT INTO storage_subfolders (prefix, location_id) SELECT prefix, location_id FROM src.client_files_subfolders",
        )?;
        Ok(())
    }

    fn network(&mut self, input: &ImportInput) -> Result<()> {
        match &input.custom_headers {
            None => network::create_defaults(self.conn)?,
            Some(headers) => {
                for (context, h) in headers {
                    network::set_header(
                        self.conn,
                        context,
                        &h.name,
                        Some(&h.value),
                        Some(h.approval),
                        Some(&h.reason),
                    )?;
                }
                *self
                    .report
                    .rows
                    .entry("network_headers".into())
                    .or_default() += headers.len() as u64;
            }
        }
        for (session, cookie) in &input.cookies {
            network::set_cookie(self.conn, session, cookie)?;
        }
        *self
            .report
            .rows
            .entry("network_cookies".into())
            .or_default() += input.cookies.len() as u64;
        crate::bandwidth::save_usage(self.conn, &input.bandwidth_usage)?;
        *self
            .report
            .rows
            .entry("bandwidth_usage".into())
            .or_default() += input.bandwidth_usage.len() as u64;
        Ok(())
    }

    fn settings(&mut self, input: &ImportInput) -> Result<()> {
        for p in &input.api_permissions {
            self.conn.execute(
                "INSERT INTO api_permissions (access_key, name, permits_everything, permissions, search_tag_filter)
                 VALUES (?, ?, ?, ?, ?)",
                params![
                    p.access_key,
                    p.name,
                    p.permits_everything,
                    p.permissions.to_string(),
                    p.search_tag_filter.as_ref().map(ToString::to_string)
                ],
            )?;
        }
        *self
            .report
            .rows
            .entry("api_permissions".into())
            .or_default() += input.api_permissions.len() as u64;
        for (key, value) in &input.settings {
            self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?, ?)",
                params![key, value.to_string()],
            )?;
        }
        // Every reference serialised object is also kept verbatim, whether or
        // not it has been converted, so nothing is ever lost.
        self.copy(
            "legacy_objects",
            "INSERT INTO legacy_objects (source, type_id, name, version, timestamp_ms, dump)
             SELECT 'json_dumps', dump_type, '', version, 0, CAST(dump AS TEXT) FROM src.json_dumps",
        )?;
        self.copy(
            "legacy_objects",
            "INSERT INTO legacy_objects (source, type_id, name, version, timestamp_ms, dump)
             SELECT 'json_dumps_named', dump_type, dump_name, version, timestamp_ms, CAST(dump AS TEXT) FROM src.json_dumps_named",
        )?;
        self.copy(
            "legacy_objects",
            "INSERT INTO legacy_objects (source, type_id, name, version, timestamp_ms, dump)
             SELECT 'json_dumps_hashed', dump_type, hex(hash), version, 0, CAST(dump AS TEXT) FROM src.json_dumps_hashed",
        )?;
        self.copy(
            "legacy_objects",
            "INSERT INTO legacy_objects (source, type_id, name, version, timestamp_ms, dump)
             SELECT 'json_dict', 0, name, 0, 0, CAST(dump AS TEXT) FROM src.json_dict",
        )?;
        self.copy(
            "legacy_objects",
            "INSERT INTO legacy_objects (source, type_id, name, version, timestamp_ms, dump)
             SELECT 'options', 0, 'options', 0, 0, options FROM src.options",
        )?;
        Ok(())
    }

    /// Subscriptions with their queries, each query's history decoded and
    /// copied one at a time.
    fn subscriptions(&mut self, input: &ImportInput) -> Result<()> {
        let url_classes = input
            .settings
            .get(<hydrus_core::url::UrlClassSettings as crate::settings::Setting>::KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(hydrus_core::url::UrlClasses::new);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut log_stmt = self.conn.prepare(
            "SELECT version, CAST(dump AS TEXT) FROM src.json_dumps_named
             WHERE dump_type = 86 AND dump_name = ? ORDER BY timestamp_ms DESC LIMIT 1",
        )?;
        let mut counts = [0u64; 4];
        for s in &input.subscriptions {
            let Some(id) = subscriptions::create_subscription(self.conn, &s.name, &s.settings)?
            else {
                continue;
            };
            counts[0] += 1;
            for (log_name, state) in &s.queries {
                let queue = subscriptions::add_query(self.conn, id, state, now)?;
                counts[1] += 1;
                let stored: Option<(i64, String)> = log_stmt
                    .query_row([log_name], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?;
                let Some((version, dump)) = stored else {
                    self.report.warnings.push(format!(
                        "The history of query \"{}\" of subscription \"{}\" is missing; it will start afresh",
                        state.query_text, s.name
                    ));
                    continue;
                };
                let log = hydrus_legacy::serialisable::SerialisableObject::from_stored(
                    hydrus_legacy::serialisable::SerialisableType(86),
                    Some(log_name.clone()),
                    u32::try_from(version).unwrap_or(u32::MAX),
                    &dump,
                )
                .and_then(|object| hydrus_legacy::objects::subscriptions::query_log(&object));
                let log = match log {
                    Ok(log) => log,
                    Err(e) => {
                        self.report.warnings.push(format!(
                            "The history of query \"{}\" of subscription \"{}\" could not be read, so it \
                             will start afresh (the original is kept): {e}",
                            state.query_text, s.name
                        ));
                        continue;
                    }
                };
                let warnings = &mut self.report.warnings;
                let files: Vec<_> = log
                    .file_seeds
                    .iter()
                    .filter_map(|f| decode::file_seed(f, url_classes.as_ref(), warnings))
                    .collect();
                let galleries: Vec<_> = log
                    .gallery_seeds
                    .iter()
                    .map(|g| decode::gallery_seed(g, warnings))
                    .collect();
                counts[2] += queues::restore_file_seeds(self.conn, queue, &files)? as u64;
                queues::restore_gallery_seeds(self.conn, queue, &galleries)?;
                counts[3] += galleries.len() as u64;
            }
        }
        for (table, n) in [
            "subscriptions",
            "subscription_queries",
            "file_seeds",
            "gallery_seeds",
        ]
        .into_iter()
        .zip(counts)
        {
            *self.report.rows.entry(table.into()).or_default() += n;
        }
        Ok(())
    }

    /// Downloader pages' queues, with their files and gallery pages; each
    /// page's queue ids.
    fn downloader_pages(&mut self, input: &ImportInput) -> Result<Vec<Vec<i64>>> {
        let url_classes = input
            .settings
            .get(<hydrus_core::url::UrlClassSettings as crate::settings::Setting>::KEY)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .map(hydrus_core::url::UrlClasses::new);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut counts = [0u64; 3];
        let mut page_queues = Vec::new();
        for page in &input.downloader_pages {
            let mut ids = Vec::new();
            for q in &page.queues {
                let kind = match q.state {
                    PageQueueState::Urls => queues::QueueKind::Urls,
                    PageQueueState::Gallery(_) => queues::QueueKind::Gallery,
                    PageQueueState::Watcher(_) => queues::QueueKind::Watcher,
                };
                let id = queues::create_queue(
                    self.conn,
                    kind,
                    &page.name,
                    None,
                    &q.options,
                    q.created.unwrap_or(now),
                )?;
                queues::set_paused(self.conn, id, Some(q.files_paused), Some(q.gallery_paused))?;
                let warnings = &mut self.report.warnings;
                let files: Vec<_> = q
                    .file_seeds
                    .iter()
                    .filter_map(|f| decode::file_seed(f, url_classes.as_ref(), warnings))
                    .collect();
                let galleries: Vec<_> = q
                    .gallery_seeds
                    .iter()
                    .map(|g| decode::gallery_seed(g, warnings))
                    .collect();
                counts[1] += queues::restore_file_seeds(self.conn, id, &files)? as u64;
                queues::restore_gallery_seeds(self.conn, id, &galleries)?;
                counts[2] += galleries.len() as u64;
                let extra = match &q.state {
                    PageQueueState::Urls => None,
                    PageQueueState::Gallery(search) => Some(serde_json::to_value(search)),
                    PageQueueState::Watcher(state) => {
                        let mut state = state.clone();
                        let times: Vec<_> = files
                            .iter()
                            .map(|s| hydrus_core::subscriptions::SeedTime {
                                source_time: s.source_time,
                                created: s.created,
                            })
                            .collect();
                        state.update_next_check_time(&times, now);
                        Some(serde_json::to_value(state))
                    }
                };
                if let Some(extra) = extra {
                    let extra = extra.expect("plain data serialises");
                    queues::set_queue_extra(self.conn, id, &extra)?;
                }
                counts[0] += 1;
                ids.push(id);
            }
            page_queues.push(ids);
        }
        for (table, n) in ["import_queues", "file_seeds", "gallery_seeds"]
            .into_iter()
            .zip(counts)
        {
            *self.report.rows.entry(table.into()).or_default() += n;
        }
        Ok(page_queues)
    }

    /// The session's pages, saved as the one the GUI opens with, and the
    /// files each showed.
    fn session(&mut self, input: &ImportInput, page_queues: &[Vec<i64>]) -> Result<()> {
        use hydrus_core::pages::{Page, PageContent, PageKey, Session};

        fn convert(
            page: &PageInput,
            page_queues: &[Vec<i64>],
            files: &mut Vec<(PageKey, Vec<hydrus_core::Sha256>)>,
        ) -> Page {
            let key = PageKey::random();
            if !page.hashes.is_empty() {
                files.push((key, page.hashes.clone()));
            }
            let content = match &page.content {
                PageInputContent::Pages(pages) => PageContent::Pages(
                    pages
                        .iter()
                        .map(|p| convert(p, page_queues, files))
                        .collect(),
                ),
                PageInputContent::Search {
                    search,
                    synchronised,
                    sort,
                } => PageContent::Search {
                    search: search.clone(),
                    synchronised: *synchronised,
                    sort: sort.clone(),
                },
                PageInputContent::Downloader { kind, index, sort } => PageContent::Downloader {
                    kind: *kind,
                    queues: page_queues.get(*index).cloned().unwrap_or_default(),
                    sort: sort.clone(),
                },
                PageInputContent::Duplicates { duplicates, sort } => PageContent::Duplicates {
                    duplicates: duplicates.clone(),
                    sort: sort.clone(),
                },
                PageInputContent::Other {
                    page_type,
                    stored,
                    sort,
                } => PageContent::Other {
                    page_type: *page_type,
                    stored: stored.clone(),
                    sort: sort.clone(),
                },
            };
            Page {
                key,
                name: page.name.clone(),
                content,
            }
        }

        let Some(session) = &input.session else {
            return Ok(());
        };
        let mut files = Vec::new();
        let pages = session
            .pages
            .iter()
            .map(|p| convert(p, page_queues, &mut files))
            .collect();
        let all: Vec<hydrus_core::Sha256> =
            files.iter().flat_map(|(_, h)| h.iter().copied()).collect();
        let ids = crate::master::hash_ids(self.conn, &all)?;
        for (key, hashes) in &files {
            let hash_ids: Vec<hydrus_core::HashId> =
                hashes.iter().filter_map(|h| ids.get(h).copied()).collect();
            crate::sessions::set_page_files(self.conn, key, &hash_ids)?;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        // what the reference opened with is what we open with
        let session = Session {
            name: crate::sessions::LAST_SESSION.to_owned(),
            pages,
        };
        crate::sessions::save(self.conn, &session, now)?;
        *self.report.rows.entry("sessions".into()).or_default() += 1;
        *self.report.rows.entry("page_files".into()).or_default() += files.len() as u64;
        Ok(())
    }

    /// Import folders, with the files each has seen (so nothing is
    /// imported twice).
    fn import_folders(&mut self, input: &ImportInput) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let (mut folders, mut seeds) = (0u64, 0u64);
        for f in &input.import_folders {
            let Some(id) = crate::import_folders::create_import_folder(
                self.conn,
                &f.name,
                &f.settings,
                &f.options,
                f.paused,
                now,
            )?
            else {
                continue;
            };
            folders += 1;
            seeds += queues::restore_file_seeds(self.conn, id, &f.file_seeds)? as u64;
        }
        *self.report.rows.entry("import_folders".into()).or_default() += folders;
        *self.report.rows.entry("file_seeds".into()).or_default() += seeds;
        Ok(())
    }

    /// Duplicates auto-resolution rules, with each pair's status for each
    /// rule and the pairs each actioned (the reference keeps them in
    /// per-rule tables; a missing table is an empty one).
    fn auto_resolution(&mut self, input: &ImportInput) -> Result<()> {
        use crate::duplicates::auto::{self, PairStatus};
        let src_has = |conn: &Connection, table: &str| -> Result<bool> {
            Ok(conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM src.sqlite_master WHERE type = 'table' AND name = ?)",
                [table],
                |r| r.get(0),
            )?)
        };
        let (mut rules, mut pairs, mut actioned) = (0u64, 0u64, 0u64);
        for (id, rule) in &input.auto_resolution_rules {
            if auto::add_rule(self.conn, rule, Some(*id))?.is_none() {
                self.report.warnings.push(format!(
                    "Two duplicates auto-resolution rules are called \"{}\"; only the first was kept",
                    rule.name
                ));
                continue;
            }
            rules += 1;
            let queue = |status: PairStatus| match status {
                PairStatus::Denied => format!("duplicate_files_auto_resolution_declined_{id}"),
                PairStatus::ReadyToAction => {
                    format!("duplicate_files_auto_resolution_pending_actions_{id}")
                }
                other => format!(
                    "duplicate_files_auto_resolution_pair_decisions_{id}_{}",
                    other.code()
                ),
            };
            for status in PairStatus::ALL {
                if status == PairStatus::Actioned {
                    continue;
                }
                let table = queue(status);
                if !src_has(self.conn, &table)? {
                    continue;
                }
                let (a, b, when) = match status {
                    PairStatus::ReadyToAction => ("hash_id_a", "hash_id_b", "NULL"),
                    PairStatus::Denied => ("NULL", "NULL", "timestamp_ms"),
                    _ => ("NULL", "NULL", "NULL"),
                };
                pairs += self.conn.execute(
                    &format!(
                        "INSERT OR IGNORE INTO dup_auto_pairs
                            (rule_id, smaller_group_id, larger_group_id, status, hash_id_a, hash_id_b, timestamp_ms)
                         SELECT ?, smaller_media_id, larger_media_id, ?, {a}, {b}, {when} FROM src.{table}"
                    ),
                    params![id, status.code()],
                )? as u64;
            }
            let table = format!("duplicate_files_auto_resolution_actioned_{id}");
            if src_has(self.conn, &table)? {
                actioned += self.conn.execute(
                    &format!(
                        "INSERT INTO dup_auto_actioned (rule_id, hash_id_a, hash_id_b, duplicate_type, timestamp_ms)
                         SELECT ?, hash_id_a, hash_id_b, duplicate_type, timestamp_ms FROM src.{table}"
                    ),
                    [id],
                )? as u64;
            }
        }
        for (table, n) in [
            ("dup_auto_rules", rules),
            ("dup_auto_pairs", pairs),
            ("dup_auto_actioned", actioned),
        ] {
            *self.report.rows.entry(table.into()).or_default() += n;
        }
        Ok(())
    }

    fn derived(&mut self) -> Result<()> {
        crate::maintenance::rebuild_caches(self.conn)
    }
}

/// The kind of a service type that has no settings, if it has none.
fn settingless_kind(service_type: ServiceType) -> Option<ServiceKind> {
    Some(match service_type {
        ServiceType::LocalTag => ServiceKind::LocalTags,
        ServiceType::LocalFileDomain => ServiceKind::LocalFiles,
        ServiceType::LocalFileUpdateDomain => ServiceKind::LocalUpdates,
        ServiceType::LocalFileTrashDomain => ServiceKind::Trash,
        ServiceType::CombinedLocalFileDomains => ServiceKind::CombinedLocalMedia,
        ServiceType::HydrusLocalFileStorage => ServiceKind::LocalFileStorage,
        ServiceType::CombinedDeletedFile => ServiceKind::CombinedDeleted,
        ServiceType::CombinedFile => ServiceKind::AllKnownFiles,
        ServiceType::CombinedTag => ServiceKind::AllKnownTags,
        ServiceType::LocalNotes => ServiceKind::LocalNotes,
        _ => return None,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::services::{
        LikeRatingConfig, NumericalRatingConfig, PenBrush, RatingColours, RatingDisplay, Rgb,
        ServerConfig, StarAppearance, StarShape,
    };
    use hydrus_testkit::legacy_fixture;

    pub(crate) fn import_basic() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
        let source = legacy_fixture("basic");
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        import_legacy(source.path(), &dest).unwrap();
        (source, dest_dir, dest)
    }

    #[test]
    fn the_global_pause_switches_come_across() {
        let source = legacy_fixture("basic");
        let input = decode_input(&hydrus_legacy::LegacyDb::open(source.path()).unwrap()).unwrap();
        let pauses: crate::settings::Pauses =
            serde_json::from_value(input.settings["pauses"].clone()).unwrap();
        // the fixture's client had all new network traffic paused
        assert_eq!(
            pauses,
            crate::settings::Pauses {
                network_traffic: true,
                ..crate::settings::Pauses::default()
            }
        );
        assert!(!pauses.subscriptions_run());
        assert!(pauses.files_run() && pauses.galleries_run() && pauses.watchers_run());
    }

    #[test]
    fn import_options_that_run_programs_are_reported() {
        use hydrus_core::import_options::{ExternalProgramsOptions, ImportOptionsSlice};
        let runs = ImportOptionsSlice {
            external_programs: Some(ExternalProgramsOptions {
                stored: Some("[...]".into()),
            }),
            ..ImportOptionsSlice::default()
        };
        let source = legacy_fixture("basic");
        let mut input =
            decode_input(&hydrus_legacy::LegacyDb::open(source.path()).unwrap()).unwrap();
        assert!(
            decode::external_program_users(&input).is_empty(),
            "none in the fixture"
        );
        input.import_folders.push(ImportFolderInput {
            name: "inbox".into(),
            settings: hydrus_parse::folders::ImportFolderSettings::default(),
            options: runs,
            paused: false,
            file_seeds: Vec::new(),
        });
        assert_eq!(
            decode::external_program_users(&input),
            ["import folder \"inbox\""]
        );
    }

    #[test]
    fn the_network_options_come_across() {
        let source = legacy_fixture("basic");
        let input = decode_input(&hydrus_legacy::LegacyDb::open(source.path()).unwrap()).unwrap();
        let network: crate::network::NetworkSettings =
            serde_json::from_value(input.settings["network"].clone()).unwrap();
        // the fixture's client kept hydrus's defaults
        assert_eq!(network, crate::network::NetworkSettings::default());
    }

    #[test]
    fn the_trash_limits_come_across() {
        let source = legacy_fixture("basic");
        let input = decode_input(&hydrus_legacy::LegacyDb::open(source.path()).unwrap()).unwrap();
        let trash: crate::trash::TrashSettings =
            serde_json::from_value(input.settings["trash"].clone()).unwrap();
        assert_eq!(trash, crate::trash::TrashSettings::default());
    }

    #[test]
    fn a_default_install_keeps_the_default_bandwidth_rules() {
        let source = legacy_fixture("basic");
        let input = decode_input(&hydrus_legacy::LegacyDb::open(source.path()).unwrap()).unwrap();
        let stored: crate::bandwidth::BandwidthSettings =
            serde_json::from_value(input.settings["bandwidth"].clone()).unwrap();
        let sorted = |mut v: Vec<(NetworkContext, hydrus_core::bandwidth::Rules)>| {
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v
        };
        let defaults = crate::bandwidth::BandwidthSettings::default();
        assert_eq!(sorted(stored.rules.clone()), sorted(defaults.rules.clone()));
        assert_eq!(
            stored,
            BandwidthSettingsOrdered::with_rules(defaults, stored.rules.clone())
        );
        // a fresh install has used no bandwidth yet
        assert!(input.bandwidth_usage.is_empty());
    }

    /// `settings` with `rules` in place of its own (to compare the rest).
    struct BandwidthSettingsOrdered;

    impl BandwidthSettingsOrdered {
        fn with_rules(
            mut settings: crate::bandwidth::BandwidthSettings,
            rules: Vec<(NetworkContext, hydrus_core::bandwidth::Rules)>,
        ) -> crate::bandwidth::BandwidthSettings {
            settings.rules = rules;
            settings
        }
    }

    #[test]
    fn decodes_the_basic_fixture_settings() {
        let source = legacy_fixture("basic");
        let input = decode_input(&hydrus_legacy::LegacyDb::open(source.path()).unwrap()).unwrap();
        let colours = |like: [u8; 3], dislike: [u8; 3]| RatingColours {
            like: PenBrush {
                pen: Rgb([0, 0, 0]),
                brush: Rgb(like),
            },
            dislike: PenBrush {
                pen: Rgb([0, 0, 0]),
                brush: Rgb(dislike),
            },
            ..RatingColours::default()
        };
        let display = |colours| RatingDisplay {
            colours,
            ..RatingDisplay::default()
        };
        let mut kinds: Vec<_> = input.service_kinds.into_iter().collect();
        kinds.sort_by_key(|(id, _)| *id);
        assert_eq!(
            kinds,
            [
                (
                    ServiceId(12),
                    ServiceKind::RatingLike(LikeRatingConfig {
                        display: display(colours([240, 240, 65], [200, 80, 120])),
                        appearance: StarAppearance::Shape(StarShape::FAT_STAR),
                    })
                ),
                (
                    ServiceId(13),
                    ServiceKind::ClientApi(ServerConfig {
                        port: Some(45901),
                        ..ServerConfig::default()
                    })
                ),
                (
                    ServiceId(16),
                    ServiceKind::RatingNumerical(NumericalRatingConfig {
                        display: display(RatingColours::default()),
                        appearance: StarAppearance::Shape(StarShape::CIRCLE),
                        num_stars: 5,
                        allow_zero: true,
                        custom_pad: 4,
                        show_fraction_beside_stars: 0,
                    })
                ),
                (
                    ServiceId(17),
                    ServiceKind::RatingIncDec(display(RatingColours::default()))
                ),
            ]
        );
        let keys: Vec<_> = input
            .api_permissions
            .iter()
            .map(|p| {
                (
                    hex::encode(&p.access_key),
                    p.name.as_str(),
                    p.permits_everything,
                    p.permissions.to_string(),
                    p.search_tag_filter.as_ref().map(ToString::to_string),
                )
            })
            .collect();
        assert_eq!(
            keys,
            [
                (
                    "0123456789abcdef".repeat(4),
                    "oracle",
                    true,
                    "[]".to_owned(),
                    None
                ),
                (
                    "fedcba9876543210".repeat(4),
                    "oracle restricted",
                    false,
                    "[3]".to_owned(),
                    Some(
                        r#"{"rules":{"":"blacklist",":":"blacklist","safe":"whitelist"}}"#
                            .to_owned()
                    )
                ),
            ]
        );
        assert_eq!(
            input.settings["thumbnails"],
            serde_json::json!({"bounding_width": 150, "bounding_height": 125, "scale": "down_only", "dpr_percent": 100, "video_percentage_in": 35})
        );
        // the reference's defaults
        assert_eq!(
            input.settings["delete_lock"],
            serde_json::json!({
                "archived": false,
                "reinbox_after_archive_delete": false,
                "reinbox_after_duplicate_filter": false,
                "reinbox_in_auto_resolution": false
            })
        );
        assert_eq!(
            input.settings["file_handling"],
            serde_json::json!({"comic_book_detection": true, "transparency_strictness": 2, "do_not_chmod": false})
        );
        // recorded, counting the media viewer and the Client API
        assert_eq!(
            input.settings["file_viewing_statistics"],
            serde_json::json!({"active": true, "interesting_canvases": [0, 4]})
        );
    }

    #[test]
    fn imports_the_basic_fixture() {
        let (source, _dest_dir, dest) = import_basic();
        let conn = Connection::open(&dest).unwrap();
        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT count(*) FROM services"), 17);
        assert_eq!(count("SELECT count(*) FROM files"), 36);
        assert_eq!(count("SELECT count(*) FROM mappings_9_current"), 129);
        assert_eq!(
            count("SELECT count(*) FROM tag_parents WHERE service_id = 9"),
            4
        );
        assert_eq!(
            count("SELECT count(*) FROM legacy_objects WHERE source = 'json_dumps'"),
            11
        );
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        let location: String = conn
            .query_row("SELECT path FROM storage_locations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(Path::new(&location), source.path().join("client_files"));
        // nothing was created next to the source
        let mut names: Vec<String> = std::fs::read_dir(source.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "client.caches.db",
                "client.db",
                "client.mappings.db",
                "client.master.db",
                "client_files"
            ]
        );
    }

    #[test]
    fn refuses_to_overwrite_or_import_the_wrong_version() {
        let (source, _dest_dir, dest) = import_basic();
        assert!(import_legacy(source.path(), &dest).is_err());
        let wrong = legacy_fixture("basic");
        let c = Connection::open(wrong.path().join("client.db")).unwrap();
        c.execute("UPDATE version SET version = 600", []).unwrap();
        drop(c);
        let err = import_legacy(wrong.path(), &dest.with_file_name("other.db")).unwrap_err();
        assert!(err.to_string().contains("600"), "{err}");
        assert!(!dest.with_file_name("other.db.importing").exists());
    }

    /// Compare every count the native store derived with the reference's own
    /// autocomplete caches. Returns how many rows were compared.
    fn compare_counts_with_reference(source: &Path, dest: &Path) -> usize {
        let conn = Connection::open(dest).unwrap();
        conn.execute(
            "ATTACH DATABASE ?1 AS ref",
            [format!(
                "file:{}?mode=ro&immutable=1",
                source.join("client.caches.db").display()
            )],
        )
        .unwrap();
        let rows = |sql: &str| -> Vec<(i64, i64, i64)> {
            conn.prepare(sql)
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        let ids = |sql: &str| -> Vec<i64> {
            conn.prepare(sql)
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        let exists = |table: &str| -> bool {
            conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM ref.sqlite_master WHERE name = ?1)",
                [table],
                |r| r.get(0),
            )
            .unwrap()
        };
        let tag_services = ids(&format!(
            "SELECT service_id FROM services WHERE service_type IN ({}, {})",
            ServiceType::LocalTag.code(),
            ServiceType::TagRepository.code()
        ));
        let all_known_files = ids(&format!(
            "SELECT service_id FROM services WHERE service_type = {}",
            ServiceType::CombinedFile.code()
        ))[0];
        let file_services = ids("SELECT service_id FROM services");
        let mut compared = 0;
        for tag_service in tag_services {
            for (display, ours, theirs) in [
                (false, "cache_tag_counts", "ac_cache"),
                (true, "cache_display_counts", "display_ac_cache"),
            ] {
                let expected = rows(&format!(
                    "SELECT tag_id, current_count, pending_count FROM ref.combined_files_{theirs}_{tag_service}
                     WHERE current_count > 0 OR pending_count > 0 ORDER BY tag_id"
                ));
                let actual = rows(&format!(
                    "SELECT tag_id, current, pending FROM {ours}_{tag_service} WHERE domain_id = {all_known_files} ORDER BY tag_id"
                ));
                assert_eq!(
                    actual, expected,
                    "all known files, service {tag_service}, display {display}"
                );
                compared += expected.len();
                for &file_service in &file_services {
                    let table = format!("specific_{theirs}_{file_service}_{tag_service}");
                    if !exists(&table) {
                        continue;
                    }
                    let expected = rows(&format!(
                        "SELECT tag_id, current_count, pending_count FROM ref.{table}
                         WHERE current_count > 0 OR pending_count > 0 ORDER BY tag_id"
                    ));
                    let actual = rows(&format!(
                        "SELECT tag_id, current, pending FROM {ours}_{tag_service} WHERE domain_id = {file_service} ORDER BY tag_id"
                    ));
                    assert_eq!(
                        actual, expected,
                        "domain {file_service}, service {tag_service}, display {display}"
                    );
                    compared += expected.len();
                }
            }
        }
        compared
    }

    #[test]
    fn rebuilt_counts_match_the_reference_caches() {
        let (source, _dest_dir, dest) = import_basic();
        let compared = compare_counts_with_reference(source.path(), &dest);
        assert!(compared > 300, "only compared {compared} rows");
    }

    /// The same check on any reference install, e.g. one made by
    /// `oracle/make_bench_db.py`:
    /// `HYDRUS_REFERENCE_DB=/path/to/db cargo test -p hydrus-store --release -- --ignored any_reference`
    #[test]
    #[ignore = "needs HYDRUS_REFERENCE_DB"]
    fn any_reference_install_counts_match() {
        let source =
            PathBuf::from(std::env::var_os("HYDRUS_REFERENCE_DB").expect("HYDRUS_REFERENCE_DB"));
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        import_legacy(&source, &dest).unwrap();
        let compared = compare_counts_with_reference(&source, &dest);
        eprintln!("compared {compared} count rows");
        assert!(compared > 0);
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;
    use crate::network::{self, NetworkContext};

    /// A session container holding one of the reference's pickled jars
    /// (`oracle/fixtures/cookie_jars.json`) comes across without its
    /// expired cookies, as the reference drops them when it loads a session.
    #[test]
    fn imports_session_cookies() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let jars = hydrus_testkit::fixture_json("cookie_jars.json");
        let case = jars["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| {
                c["what"] == "jar"
                    && c["protocol"] == 4
                    && c["cookies"].as_array().unwrap().len() > 2
            })
            .unwrap();
        let dump = serde_json::json!([[47, 2, [2, "example.com"]], case["pickle"]]).to_string();
        {
            let conn = Connection::open(source.path().join("client.db")).unwrap();
            conn.execute(
                "INSERT INTO json_dumps_named (dump_type, dump_name, version, timestamp_ms, dump) VALUES (96, 'example.com', 2, 1, ?)",
                [dump],
            )
            .unwrap();
        }
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        import_legacy(source.path(), &dest).unwrap();
        let conn = Connection::open(&dest).unwrap();
        let cookies = network::cookies(&conn, &NetworkContext::domain("example.com")).unwrap();
        let names: Vec<&str> = cookies.iter().map(|c| c.name.as_str()).collect();
        // "empty" (expires 0) and "neg" (-5) had expired
        assert_eq!(names, ["sid", "pref", "unicode", "big"]);
        assert_eq!(cookies[0].rest, [("HttpOnly".to_owned(), None)]);
        assert!(cookies[0].secure);
        assert_eq!(cookies[1].expires, None);
    }

    fn file_seed_facts(f: &crate::queues::FileSeed) -> serde_json::Value {
        let mut notes = f.meta.notes.clone();
        notes.sort();
        let mut hashes = f.meta.hashes.clone();
        hashes.sort();
        let mut headers = f.meta.request_headers.clone();
        headers.sort();
        serde_json::json!({
            "type": f.seed_type as i64,
            "data": f.data,
            "comparison": f.data_for_comparison,
            "created": f.created,
            "modified": f.modified,
            "source_time": f.source_time,
            "status": f.status.code(),
            "note": f.note,
            "referral": f.referral_url,
            "headers": headers,
            "filterable": f.meta.external_filterable_tags,
            "additional": f.meta.external_additional_tags.iter().cloned().collect::<BTreeMap<_, _>>(),
            "primary": f.meta.primary_urls,
            "source": f.meta.source_urls,
            "tags": f.meta.tags,
            "notes": notes,
            "hashes": hashes,
        })
    }

    fn gallery_seed_facts(g: &crate::queues::GallerySeed) -> serde_json::Value {
        let mut headers = g.meta.request_headers.clone();
        headers.sort();
        serde_json::json!({
            "url": g.url,
            "can_generate_more_pages": g.can_generate_more_pages,
            "created": g.created,
            "modified": g.modified,
            "status": g.status.code(),
            "note": g.note,
            "referral": g.referral_url,
            "headers": headers,
            "filterable": g.meta.external_filterable_tags,
            "additional": g.meta.external_additional_tags.iter().cloned().collect::<BTreeMap<_, _>>(),
        })
    }

    /// The page data hashes of a session tree (as the oracle records it), in
    /// order.
    fn page_hashes(node: &serde_json::Value, out: &mut Vec<String>) {
        match node.get("pages") {
            Some(pages) => {
                for page in pages.as_array().unwrap() {
                    page_hashes(page, out);
                }
            }
            None => out.push(node["page_data_hash"].as_str().unwrap().to_owned()),
        }
    }

    /// A session made by the reference (`oracle/dump_gui_sessions.py`)
    /// planted in a reference database as its last session (over an older
    /// save of it): each downloader page's work comes over as queues named
    /// after the page, in the session's order, with their files and gallery
    /// pages.
    #[test]
    fn imports_downloader_pages_from_the_last_session() {
        use crate::queues::QueueKind;
        let source = hydrus_testkit::legacy_fixture("basic");
        let recorded = hydrus_testkit::fixture_json("gui_sessions.json");
        let sessions = recorded["sessions"].as_array().unwrap();
        let queue_count = |session: &serde_json::Value| -> usize {
            session["facts"]["pages"]
                .as_object()
                .unwrap()
                .values()
                .map(|p| {
                    let v = &p["page"]["variables"];
                    usize::from(v.get("urls_import").is_some())
                        + v.get("multiple_gallery_import")
                            .map_or(0, |m| m["gallery_imports"].as_array().unwrap().len())
                        + v.get("multiple_watcher_import").map_or(0, |m| {
                            m["watchers"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|w| w["url"] != "")
                                .count()
                        })
                })
                .sum()
        };
        // the session with the most work, over an older save of another
        let (chosen, older) = {
            let mut by_work: Vec<&serde_json::Value> = sessions.iter().collect();
            by_work.sort_by_key(|s| std::cmp::Reverse(queue_count(s)));
            (by_work[0], by_work[1])
        };
        assert!(queue_count(chosen) >= 3);
        plant_last_session(source.path(), &[older, chosen]);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        let report = import_legacy(source.path(), &dest).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);

        // what the reference had, in the session's order
        let mut hashes = Vec::new();
        page_hashes(&chosen["facts"]["tree"], &mut hashes);
        let mut expected = Vec::new();
        for hash in &hashes {
            let page = &chosen["facts"]["pages"][hash]["page"];
            let name = page["name"].clone();
            let variables = &page["variables"];
            if let Some(u) = variables.get("urls_import") {
                expected.push((
                    QueueKind::Urls,
                    name.clone(),
                    u.clone(),
                    u["paused"].clone(),
                    u["paused"].clone(),
                ));
            }
            if let Some(m) = variables.get("multiple_gallery_import") {
                for g in m["gallery_imports"].as_array().unwrap() {
                    expected.push((
                        QueueKind::Gallery,
                        name.clone(),
                        g.clone(),
                        g["files_paused"].clone(),
                        g["gallery_paused"].clone(),
                    ));
                }
            }
            if let Some(m) = variables.get("multiple_watcher_import") {
                for w in m["watchers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|w| w["url"] != "")
                {
                    expected.push((
                        QueueKind::Watcher,
                        name.clone(),
                        w.clone(),
                        w["files_paused"].clone(),
                        false.into(),
                    ));
                }
            }
        }

        let conn = Connection::open(&dest).unwrap();
        let imported: Vec<_> = crate::queues::queues(&conn, None)
            .unwrap()
            .into_iter()
            .filter(|q| {
                matches!(
                    q.kind,
                    QueueKind::Urls | QueueKind::Gallery | QueueKind::Watcher
                )
            })
            .collect();
        assert_eq!(imported.len(), expected.len());
        let options = |value: &serde_json::Value| {
            hydrus_legacy::objects::import_options::slice(
                &hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
                    &value.to_string(),
                )
                .unwrap(),
            )
            .unwrap()
        };
        for (queue, (kind, name, facts, files_paused, gallery_paused)) in
            imported.iter().zip(&expected)
        {
            assert_eq!(queue.kind, *kind);
            assert_eq!(queue.name, *name);
            assert_eq!(queue.files_paused, *files_paused);
            assert_eq!(queue.gallery_paused, *gallery_paused);
            assert_eq!(queue.options, options(&facts["import_options"]));
            let files: Vec<_> = crate::queues::file_seeds(&conn, queue.id)
                .unwrap()
                .iter()
                .map(file_seed_facts)
                .collect();
            let galleries: Vec<_> = crate::queues::gallery_seeds(&conn, queue.id)
                .unwrap()
                .iter()
                .map(gallery_seed_facts)
                .collect();
            assert_eq!(serde_json::json!(files), facts["file_seeds"]);
            assert_eq!(serde_json::json!(galleries), facts["gallery_seeds"]);
            match kind {
                QueueKind::Gallery => {
                    assert_eq!(queue.created, facts["created"]);
                    assert_eq!(
                        queue.extra,
                        serde_json::json!({
                            "query": facts["query"],
                            "source_name": facts["source_name"],
                            "file_limit": facts["file_limit"],
                            "num_new_urls_found": facts["num_new_urls_found"],
                            "num_urls_found": facts["num_urls_found"],
                        })
                    );
                }
                QueueKind::Watcher => {
                    let state: hydrus_core::watchers::WatcherState =
                        serde_json::from_value(queue.extra.clone()).unwrap();
                    assert_eq!(state.url, facts["url"]);
                    assert_eq!(state.subject, facts["subject"]);
                    assert_eq!(state.last_check_time, facts["last_check_time"]);
                    assert_eq!(state.no_work_until, facts["no_work_until"]);
                    assert_eq!(state.no_work_until_reason, facts["no_work_until_reason"]);
                    assert_eq!(state.created, facts["created"]);
                    assert_eq!(
                        serde_json::json!(state.external_filterable_tags),
                        facts["filterable"]
                    );
                    assert_eq!(
                        serde_json::json!(
                            state
                                .external_additional_tags
                                .iter()
                                .cloned()
                                .collect::<BTreeMap<_, _>>()
                        ),
                        facts["additional"]
                    );
                    // timed when imported, as the reference times it when the page starts
                    assert!(state.next_check_time > 0);
                    // a thread that was dead or gone stays paused
                    if facts["checking_status"] != 0 {
                        assert!(state.checking_paused);
                    }
                }
                _ => assert_eq!(queue.extra, serde_json::json!({})),
            }
        }

        // the session's tree, as the GUI will open it: each page with its
        // search or queues, and the files it showed
        let session = crate::sessions::load(&conn, crate::sessions::LAST_SESSION)
            .unwrap()
            .unwrap();
        let mut queue_ids = imported.iter().map(|q| q.id);
        check_pages(
            &conn,
            &session.pages,
            &chosen["facts"]["tree"]["pages"],
            &chosen["facts"]["pages"],
            &mut queue_ids,
        );
        assert!(queue_ids.next().is_none(), "every queue is on a page");
        let pages = session.all_pages();
        for kind in [7, 9, 10] {
            assert!(
                pages.iter().any(|p| p.content.page_type() == kind),
                "{kind}"
            );
        }
    }

    /// Plant sessions recorded by the reference as a reference database's
    /// "last session", each saved after the one before (so the last one is
    /// the one it opens with), with the files their pages show.
    fn plant_last_session(source: &Path, sessions: &[&serde_json::Value]) {
        let conn = Connection::open(source.join("client.db")).unwrap();
        // (the fixture's client saved its own last session when it closed)
        let latest: i64 = conn
            .query_row(
                "SELECT coalesce(max(timestamp_ms), 0) FROM json_dumps_named WHERE dump_type = 104",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let master = Connection::open(source.join("client.master.db")).unwrap();
        for (session, timestamp) in sessions.iter().zip(latest + 1..) {
            for page in session["facts"]["pages"].as_object().unwrap().values() {
                for hash in page["hashes"].as_array().unwrap() {
                    master
                        .execute(
                            "INSERT OR IGNORE INTO hashes (hash) VALUES (?)",
                            [hex::decode(hash.as_str().unwrap()).unwrap()],
                        )
                        .unwrap();
                }
            }
            let container = &session["container"];
            conn.execute(
                "INSERT INTO json_dumps_named (dump_type, dump_name, version, timestamp_ms, dump) VALUES (104, 'last session', ?, ?, ?)",
                params![container[2].as_i64(), timestamp, container[3].to_string()],
            )
            .unwrap();
            for (hash, stored) in session["page_data"].as_object().unwrap() {
                conn.execute(
                    "INSERT OR IGNORE INTO json_dumps_hashed (hash, dump_type, version, dump) VALUES (?, ?, ?, ?)",
                    params![
                        hex::decode(hash).unwrap(),
                        stored[0].as_i64(),
                        stored[1].as_i64(),
                        stored[2].to_string()
                    ],
                )
                .unwrap();
            }
        }
    }

    /// The reference's mpv settings come over beside the store.
    #[test]
    fn copies_the_mpv_conf() {
        let source = hydrus_testkit::legacy_fixture("basic");
        std::fs::write(source.path().join("mpv.conf"), "loop-file=no\n").unwrap();
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        let report = import_legacy(source.path(), &dest).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
        assert_eq!(
            std::fs::read_to_string(dest_dir.path().join("mpv.conf")).unwrap(),
            "loop-file=no\n"
        );
    }

    /// A session with a search page comes over with its search, sort and
    /// files, beside its downloader pages.
    #[test]
    fn imports_search_pages_from_the_last_session() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let recorded = hydrus_testkit::fixture_json("gui_sessions.json");
        let chosen = recorded["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| {
                s["facts"]["pages"]
                    .as_object()
                    .unwrap()
                    .values()
                    .any(|p| p["page"]["type"] == 6)
            })
            .unwrap();
        plant_last_session(source.path(), &[chosen]);
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        let report = import_legacy(source.path(), &dest).unwrap();
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);

        let conn = Connection::open(&dest).unwrap();
        let session = crate::sessions::load(&conn, crate::sessions::LAST_SESSION)
            .unwrap()
            .unwrap();
        let queues: Vec<i64> = crate::queues::queues(&conn, None)
            .unwrap()
            .into_iter()
            .map(|q| q.id)
            .collect();
        let mut queue_ids = queues.into_iter();
        check_pages(
            &conn,
            &session.pages,
            &chosen["facts"]["tree"]["pages"],
            &chosen["facts"]["pages"],
            &mut queue_ids,
        );
        assert!(
            session
                .all_pages()
                .iter()
                .any(|p| p.content.page_type() == 6)
        );
    }

    /// Our pages against the reference's session tree and pages' facts.
    fn check_pages(
        conn: &Connection,
        ours: &[hydrus_core::pages::Page],
        expected: &serde_json::Value,
        facts: &serde_json::Value,
        queues: &mut impl Iterator<Item = i64>,
    ) {
        use hydrus_core::pages::PageContent;
        let expected = expected.as_array().unwrap();
        assert_eq!(ours.len(), expected.len());
        for (page, node) in ours.iter().zip(expected) {
            if let Some(children) = node.get("pages") {
                assert_eq!(page.name, node["name"]);
                let PageContent::Pages(ours) = &page.content else {
                    panic!("{page:?} is not a notebook");
                };
                check_pages(conn, ours, children, facts, queues);
                continue;
            }
            let fact = &facts[node["page_data_hash"].as_str().unwrap()];
            let p = &fact["page"];
            assert_eq!(page.name, p["name"]);
            assert_eq!(page.content.page_type(), p["type"]);
            match &page.content {
                PageContent::Search {
                    search,
                    synchronised,
                    ..
                } => {
                    let variables = &p["variables"];
                    assert_eq!(*synchronised, variables["synchronised"]);
                    let stored = hydrus_legacy::objects::FileSearchContext::from_object(
                        &hydrus_legacy::serialisable::SerialisableObject::from_tuple_str(
                            &variables["file_search_context"].to_string(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                    assert_eq!(
                        *search,
                        decode::file_search_for_tests(&stored),
                        "{}",
                        page.name
                    );
                }
                PageContent::Downloader { queues: ids, .. } => {
                    for id in ids {
                        assert_eq!(Some(*id), queues.next());
                    }
                }
                other => panic!("{other:?}"),
            }
            assert!(page.content.sort().is_some());
            let files: Vec<String> = crate::sessions::page_files(conn, &page.key)
                .unwrap()
                .into_iter()
                .map(|id| crate::master::hash(conn, id).unwrap().unwrap().to_hex())
                .collect();
            assert_eq!(serde_json::json!(files), fact["hashes"], "{}", page.name);
        }
    }

    /// Subscriptions made by the reference (`oracle/dump_subscriptions.py`)
    /// planted in a reference database: each comes over with its settings,
    /// queries and every query's history in order.
    #[test]
    fn imports_subscriptions_with_their_histories() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let recorded = hydrus_testkit::fixture_json("subscriptions.json");
        let cases = recorded["subscriptions"].as_array().unwrap();
        let missing_log = cases
            .iter()
            .flat_map(|c| c["logs"].as_array().unwrap())
            .next()
            .unwrap()["facts"]["name"]
            .clone();
        {
            let conn = Connection::open(source.path().join("client.db")).unwrap();
            let mut insert = conn
                .prepare("INSERT INTO json_dumps_named (dump_type, dump_name, version, timestamp_ms, dump) VALUES (?, ?, ?, ?, ?)")
                .unwrap();
            for case in cases {
                let stored = &case["stored"];
                // an older save of the same name, which must lose
                insert
                    .execute(params![88, stored[1].as_str(), 4, 1, "[]"])
                    .unwrap();
                insert
                    .execute(params![
                        88,
                        stored[1].as_str(),
                        stored[2].as_i64(),
                        2,
                        stored[3].to_string()
                    ])
                    .unwrap();
                for log in case["logs"].as_array().unwrap() {
                    let stored = &log["stored"];
                    if stored[1] == missing_log {
                        continue;
                    }
                    insert
                        .execute(params![
                            86,
                            stored[1].as_str(),
                            stored[2].as_i64(),
                            5,
                            stored[3].to_string()
                        ])
                        .unwrap();
                }
            }
        }
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        let report = import_legacy(source.path(), &dest).unwrap();
        assert_eq!(report.rows["subscriptions"], cases.len() as u64);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("is missing; it will start afresh")),
            "{:?}",
            report.warnings
        );

        let conn = Connection::open(&dest).unwrap();
        let imported = subscriptions::subscriptions(&conn).unwrap();
        assert_eq!(imported.len(), cases.len());
        for case in cases {
            let facts = &case["facts"];
            let s = subscriptions::find_subscription(&conn, facts["name"].as_str().unwrap())
                .unwrap()
                .unwrap();
            let settings = &s.settings;
            let c = &settings.checker;
            assert_eq!(
                serde_json::json!({
                    "gug_key": settings.gug_key,
                    "gug_name": settings.gug_name,
                    "checker": [facts["checker"][0], c.never_faster_than, c.never_slower_than, [c.death_file_velocity.0, c.death_file_velocity.1]],
                    "initial_file_limit": settings.initial_file_limit,
                    "periodic_file_limit": settings.periodic_file_limit,
                    "this_is_a_random_sample": settings.this_is_a_random_sample,
                    "paused": settings.paused,
                    "no_work_until": settings.no_work_until,
                    "no_work_until_reason": settings.no_work_until_reason,
                    "presentation": [
                        settings.show_a_popup_while_working,
                        settings.publish_files_to_popup_button,
                        settings.publish_files_to_page,
                        settings.publish_label_override,
                        settings.merge_query_publish_events,
                    ],
                }),
                serde_json::json!({
                    "gug_key": facts["gug_key"],
                    "gug_name": facts["gug_name"],
                    "checker": facts["checker"],
                    "initial_file_limit": facts["initial_file_limit"],
                    "periodic_file_limit": facts["periodic_file_limit"],
                    "this_is_a_random_sample": facts["this_is_a_random_sample"],
                    "paused": facts["paused"],
                    "no_work_until": facts["no_work_until"],
                    "no_work_until_reason": facts["no_work_until_reason"],
                    "presentation": facts["presentation"],
                })
            );
            let queries = subscriptions::queries(&conn, s.id).unwrap();
            let expected_queries = facts["queries"].as_array().unwrap();
            assert_eq!(queries.len(), expected_queries.len());
            for ((query, expected), log) in queries
                .iter()
                .zip(expected_queries)
                .zip(case["logs"].as_array().unwrap())
            {
                let state = &query.state;
                assert_eq!(state.query_text, expected["query_text"].as_str().unwrap());
                assert_eq!(
                    state.display_name.as_deref(),
                    expected["display_name"].as_str()
                );
                assert_eq!(state.check_now, expected["check_now"]);
                assert_eq!(state.last_check_time, expected["last_check_time"]);
                assert_eq!(state.next_check_time, expected["next_check_time"]);
                assert_eq!(state.paused, expected["paused"]);
                assert_eq!(state.dead, expected["checker_status"] == 1);
                assert_eq!(
                    state.file_seed_compaction_number,
                    expected["file_seed_compaction_number"]
                );
                assert_eq!(
                    state.gallery_seed_compaction_number,
                    expected["gallery_seed_compaction_number"]
                );
                let files: Vec<_> = crate::queues::file_seeds(&conn, query.queue_id)
                    .unwrap()
                    .iter()
                    .map(file_seed_facts)
                    .collect();
                let galleries: Vec<_> = crate::queues::gallery_seeds(&conn, query.queue_id)
                    .unwrap()
                    .iter()
                    .map(gallery_seed_facts)
                    .collect();
                if log["facts"]["name"] == missing_log {
                    assert!(files.is_empty() && galleries.is_empty());
                } else {
                    assert_eq!(serde_json::json!(files), log["facts"]["file_seeds"]);
                    assert_eq!(serde_json::json!(galleries), log["facts"]["gallery_seeds"]);
                }
            }
        }
    }
}
