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
//! format decoded; that is the caller's job (see the `hydrus-legacy` crate),
//! so it's passed in as [`ImportInput`]. This module only knows SQL.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, params};

use hydrus_core::{ServiceId, ServiceKey, ServiceType, SubtagId};

use crate::error::{Result, StoreError};
use crate::services::{self, ServiceKind};
use crate::{counts, master, schema};

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
}

/// Import the reference database in `source_dir` into a new native database
/// at `dest`, which must not exist.
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
        Ok(report) => {
            std::fs::rename(&scratch, dest)?;
            Ok(report)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&scratch);
            Err(e)
        }
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
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    let mode = if Path::new(&wal).exists() {
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
    let mut report = ImportReport::default();
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
                "INSERT INTO similar_search_status (hash_id, searched_distance) SELECT hash_id, searched_distance FROM src.shape_search_cache",
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

    fn derived(&mut self) -> Result<()> {
        // autocomplete word index and integer values for every subtag
        let mut stmt = self.conn.prepare("SELECT subtag_id, subtag FROM subtags")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, SubtagId>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, subtag) = row?;
            master::index_subtag(self.conn, id, &subtag)?;
        }
        counts::rebuild_all(self.conn)?;
        Ok(())
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
        LikeRatingConfig, NumericalRatingConfig, RatingDisplay, ServerConfig, StarShape,
    };

    /// Unpack a reference fixture database into a temp dir.
    pub(crate) fn legacy_fixture(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let tarball = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../oracle/fixtures/legacy_db/{name}.tar.gz"));
        let file = std::fs::File::open(&tarball).unwrap();
        tar::Archive::new(flate2::read::GzDecoder::new(file))
            .unpack(dir.path())
            .unwrap();
        dir
    }

    /// Settings for the basic fixture's configurable services, as the legacy
    /// decoder will produce them.
    pub(crate) fn basic_input() -> ImportInput {
        let mut input = ImportInput::default();
        let like = LikeRatingConfig {
            display: RatingDisplay::default(),
            shape: StarShape::FAT_STAR,
            rating_svg: None,
        };
        let stars = NumericalRatingConfig {
            display: RatingDisplay::default(),
            shape: StarShape::CIRCLE,
            rating_svg: None,
            num_stars: 5,
            allow_zero: true,
            custom_pad: 4,
            show_fraction_beside_stars: 0,
        };
        input
            .service_kinds
            .insert(ServiceId(12), ServiceKind::RatingLike(like));
        input.service_kinds.insert(
            ServiceId(13),
            ServiceKind::ClientApi(ServerConfig {
                port: Some(45901),
                ..ServerConfig::default()
            }),
        );
        input
            .service_kinds
            .insert(ServiceId(16), ServiceKind::RatingNumerical(stars));
        input.service_kinds.insert(
            ServiceId(17),
            ServiceKind::RatingIncDec(RatingDisplay::default()),
        );
        input
    }

    pub(crate) fn import_basic() -> (tempfile::TempDir, tempfile::TempDir, PathBuf) {
        let source = legacy_fixture("basic");
        let dest_dir = tempfile::tempdir().unwrap();
        let dest = dest_dir.path().join("hydrus.db");
        import(source.path(), &dest, &basic_input()).unwrap();
        (source, dest_dir, dest)
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
        assert!(import(source.path(), &dest, &basic_input()).is_err());
        let wrong = legacy_fixture("basic");
        let c = Connection::open(wrong.path().join("client.db")).unwrap();
        c.execute("UPDATE version SET version = 600", []).unwrap();
        drop(c);
        let err = import(
            wrong.path(),
            &dest.with_file_name("other.db"),
            &basic_input(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("v600"), "{err}");
        assert!(!dest.with_file_name("other.db.importing").exists());
    }

    #[test]
    fn rebuilt_counts_match_the_reference_caches() {
        let (source, _dest_dir, dest) = import_basic();
        let conn = Connection::open(&dest).unwrap();
        conn.execute(
            "ATTACH DATABASE ?1 AS ref",
            [format!(
                "file:{}?mode=ro",
                source.path().join("client.caches.db").display()
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
        let mut compared = 0;
        for tag_service in [9, 10, 14] {
            for (display, ours, theirs) in [
                (false, "cache_tag_counts", "ac_cache"),
                (true, "cache_display_counts", "display_ac_cache"),
            ] {
                // all known files
                let expected = rows(&format!(
                    "SELECT tag_id, current_count, pending_count FROM ref.combined_files_{theirs}_{tag_service}
                     WHERE current_count > 0 OR pending_count > 0 ORDER BY tag_id"
                ));
                let actual = rows(&format!(
                    "SELECT tag_id, current, pending FROM {ours}_{tag_service} WHERE domain_id = 2 ORDER BY tag_id"
                ));
                assert_eq!(
                    actual, expected,
                    "all known files, service {tag_service}, display {display}"
                );
                compared += expected.len();
                for file_service in [3, 4, 5, 6, 7, 8, 15] {
                    let expected = rows(&format!(
                        "SELECT tag_id, current_count, pending_count FROM ref.specific_{theirs}_{file_service}_{tag_service}
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
        assert!(compared > 300, "only compared {compared} rows");
    }
}
