//! The database schema and its forward-only migrations.
//!
//! `PRAGMA user_version` records how many migrations have been applied. A
//! database written by a newer build (higher version than we know) is refused
//! rather than risk misreading it.
//!
//! Per-service tables (mappings) are created by [`create_tag_service_tables`]
//! when a tag service is created, not by migrations.

use std::fmt::Write as _;

use rusqlite::Connection;

use crate::error::{Result, StoreError};
use hydrus_core::ServiceId;

/// Each entry upgrades the schema by one version. Never edit an entry once it
/// has shipped; append a new one.
const MIGRATIONS: &[&str] = &[V1, V2, V3, V4];

/// The schema version this build writes.
pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Subscriptions and their queries (`subscriptions.rs`); each query's
/// history is an import queue.
const V4: &str = r"
CREATE TABLE subscriptions (
    subscription_id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    -- SubscriptionSettings (JSON)
    settings TEXT NOT NULL
) STRICT;

CREATE TABLE subscription_queries (
    -- the import queue holding the query's history
    queue_id INTEGER PRIMARY KEY,
    subscription_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    -- QueryState (JSON)
    state TEXT NOT NULL
) STRICT;

CREATE INDEX subscription_queries_by_subscription ON subscription_queries (subscription_id, position);
";

/// Import queues: URL lists, gallery searches, watchers and subscription
/// queries, with their file and gallery seeds as rows (`queues.rs`).
const V3: &str = r"
CREATE TABLE import_queues (
    queue_id INTEGER PRIMARY KEY,
    -- 'urls', 'gallery', 'watcher' or 'subscription'
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    -- a page key the Client API can name it by
    page_key BLOB,
    created INTEGER NOT NULL,
    files_paused INTEGER NOT NULL DEFAULT 0,
    gallery_paused INTEGER NOT NULL DEFAULT 0,
    -- this importer's own import options (JSON)
    options TEXT NOT NULL DEFAULT '{}',
    -- what else its kind keeps (JSON)
    extra TEXT NOT NULL DEFAULT '{}'
) STRICT;

CREATE INDEX import_queues_by_name ON import_queues (kind, name);

CREATE TABLE file_seeds (
    seed_id INTEGER PRIMARY KEY,
    queue_id INTEGER NOT NULL,
    -- order within the queue (a child is inserted after its parent)
    position REAL NOT NULL,
    -- 0: a path, 1: a URL
    seed_type INTEGER NOT NULL,
    data TEXT NOT NULL,
    -- what makes two seeds the same (the URL normalised for storage)
    data_for_comparison TEXT NOT NULL,
    created INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    source_time INTEGER,
    -- CC.STATUS_*
    status INTEGER NOT NULL,
    note TEXT NOT NULL,
    referral_url TEXT,
    -- request headers, tags, notes, URLs and hashes gathered so far (JSON)
    metadata TEXT NOT NULL,
    UNIQUE (queue_id, seed_type, data_for_comparison)
) STRICT;

CREATE INDEX file_seeds_by_status ON file_seeds (queue_id, status, position);

CREATE TABLE gallery_seeds (
    seed_id INTEGER PRIMARY KEY,
    queue_id INTEGER NOT NULL,
    position REAL NOT NULL,
    url TEXT NOT NULL,
    can_generate_more_pages INTEGER NOT NULL,
    created INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    status INTEGER NOT NULL,
    note TEXT NOT NULL,
    referral_url TEXT,
    -- request headers, tags to pass on, run token (JSON)
    metadata TEXT NOT NULL
) STRICT;

CREATE INDEX gallery_seeds_by_status ON gallery_seeds (queue_id, status, position);
";

/// Network sessions' cookies and custom HTTP headers (`network.rs`).
const V2: &str = r"
-- cookies of each network session (a session per registrable domain)
CREATE TABLE network_cookies (
    session_type INTEGER NOT NULL,
    session_key TEXT NOT NULL,
    domain TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    value TEXT,
    expires INTEGER,
    secure INTEGER NOT NULL,
    -- other attributes, as JSON [[name, value or null], ...]
    rest TEXT NOT NULL,
    UNIQUE (session_type, session_key, domain, path, name)
) STRICT;

-- custom HTTP headers per network context (rowid keeps their order)
CREATE TABLE network_headers (
    context_type INTEGER NOT NULL,
    context_key TEXT NOT NULL,
    name TEXT NOT NULL,
    value TEXT NOT NULL,
    approval INTEGER NOT NULL,
    reason TEXT NOT NULL,
    UNIQUE (context_type, context_key, name)
) STRICT;
";

const V1: &str = r"
-- master data -----------------------------------------------------------

CREATE TABLE hashes (
    hash_id INTEGER PRIMARY KEY,
    sha256 BLOB NOT NULL UNIQUE
) STRICT;

CREATE TABLE hash_digests (
    hash_id INTEGER PRIMARY KEY,
    md5 BLOB,
    sha1 BLOB,
    sha512 BLOB
) STRICT;
CREATE INDEX hash_digests_md5 ON hash_digests (md5);
CREATE INDEX hash_digests_sha1 ON hash_digests (sha1);
CREATE INDEX hash_digests_sha512 ON hash_digests (sha512);

CREATE TABLE namespaces (
    namespace_id INTEGER PRIMARY KEY,
    namespace TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE subtags (
    subtag_id INTEGER PRIMARY KEY,
    subtag TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE tags (
    tag_id INTEGER PRIMARY KEY,
    namespace_id INTEGER NOT NULL,
    subtag_id INTEGER NOT NULL,
    UNIQUE (namespace_id, subtag_id)
) STRICT;
CREATE INDEX tags_subtag ON tags (subtag_id);

CREATE TABLE url_domains (
    domain_id INTEGER PRIMARY KEY,
    domain TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE urls (
    url_id INTEGER PRIMARY KEY,
    domain_id INTEGER NOT NULL,
    url TEXT NOT NULL UNIQUE
) STRICT;
CREATE INDEX urls_domain ON urls (domain_id);

-- free text: petition and deletion reasons
CREATE TABLE texts (
    text_id INTEGER PRIMARY KEY,
    text TEXT NOT NULL UNIQUE
) STRICT;

-- note names
CREATE TABLE labels (
    label_id INTEGER PRIMARY KEY,
    label TEXT NOT NULL UNIQUE
) STRICT;

-- note bodies
CREATE TABLE notes (
    note_id INTEGER PRIMARY KEY,
    note TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE perceptual_hashes (
    phash_id INTEGER PRIMARY KEY,
    phash BLOB NOT NULL UNIQUE
) STRICT;

-- services ---------------------------------------------------------------

CREATE TABLE services (
    service_id INTEGER PRIMARY KEY AUTOINCREMENT,
    service_key BLOB NOT NULL UNIQUE,
    service_type INTEGER NOT NULL,
    name TEXT NOT NULL,
    config TEXT NOT NULL
) STRICT;

-- files --------------------------------------------------------------------

CREATE TABLE files (
    hash_id INTEGER PRIMARY KEY,
    size INTEGER NOT NULL,
    mime INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    duration_ms INTEGER,
    num_frames INTEGER,
    has_audio INTEGER NOT NULL DEFAULT 0,
    num_words INTEGER,
    forced_mime INTEGER,
    file_modified_ms INTEGER,
    pixel_hash BLOB,
    blurhash TEXT,
    flags INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX files_size ON files (size);
CREATE INDEX files_mime ON files (mime);
CREATE INDEX files_width ON files (width);
CREATE INDEX files_height ON files (height);
CREATE INDEX files_duration ON files (duration_ms);
CREATE INDEX files_num_frames ON files (num_frames);
CREATE INDEX files_file_modified ON files (file_modified_ms);
CREATE INDEX files_pixel_hash ON files (pixel_hash) WHERE pixel_hash IS NOT NULL;
CREATE INDEX files_forced_mime ON files (forced_mime) WHERE forced_mime IS NOT NULL;
-- rare content flags get partial indexes; see FileFlags
CREATE INDEX files_flag_exif ON files (hash_id) WHERE flags & 1;
CREATE INDEX files_flag_icc ON files (hash_id) WHERE flags & 2;
CREATE INDEX files_flag_hrm ON files (hash_id) WHERE flags & 4;
CREATE INDEX files_flag_transparency ON files (hash_id) WHERE flags & 8;
CREATE INDEX files_flag_xmp ON files (hash_id) WHERE flags & 16;
CREATE INDEX files_flag_iptc ON files (hash_id) WHERE flags & 32;
CREATE INDEX files_flag_software_source ON files (hash_id) WHERE flags & 64;

CREATE TABLE file_perceptual_hashes (
    hash_id INTEGER NOT NULL,
    phash_id INTEGER NOT NULL,
    PRIMARY KEY (hash_id, phash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_perceptual_hashes_phash ON file_perceptual_hashes (phash_id);

CREATE TABLE file_domain_current (
    service_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL,
    added_ms INTEGER,
    PRIMARY KEY (service_id, hash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_domain_current_hash ON file_domain_current (hash_id);
CREATE INDEX file_domain_current_added ON file_domain_current (service_id, added_ms);

CREATE TABLE file_domain_deleted (
    service_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL,
    deleted_ms INTEGER,
    original_added_ms INTEGER,
    PRIMARY KEY (service_id, hash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_domain_deleted_hash ON file_domain_deleted (hash_id);
CREATE INDEX file_domain_deleted_time ON file_domain_deleted (service_id, deleted_ms);

CREATE TABLE file_domain_pending (
    service_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL,
    PRIMARY KEY (service_id, hash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_domain_pending_hash ON file_domain_pending (hash_id);

CREATE TABLE file_domain_petitioned (
    service_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL,
    reason_id INTEGER,
    PRIMARY KEY (service_id, hash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_domain_petitioned_hash ON file_domain_petitioned (hash_id);

CREATE TABLE file_deletion_reasons (
    hash_id INTEGER PRIMARY KEY,
    reason_id INTEGER NOT NULL
) STRICT;

-- files that left local storage; a maintenance job deletes them from disk
-- (never inline in the write, so a rolled-back or undone delete loses nothing)
CREATE TABLE deferred_physical_deletes (
    hash_id INTEGER PRIMARY KEY,
    queued_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE file_inbox (
    hash_id INTEGER PRIMARY KEY
) STRICT;

CREATE TABLE file_archived (
    hash_id INTEGER PRIMARY KEY,
    archived_ms INTEGER
) STRICT;
CREATE INDEX file_archived_time ON file_archived (archived_ms);

CREATE TABLE file_domain_modified (
    hash_id INTEGER NOT NULL,
    domain_id INTEGER NOT NULL,
    modified_ms INTEGER NOT NULL,
    PRIMARY KEY (hash_id, domain_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_domain_modified_time ON file_domain_modified (modified_ms);

CREATE TABLE file_viewing_stats (
    hash_id INTEGER NOT NULL,
    canvas_type INTEGER NOT NULL,
    views INTEGER NOT NULL,
    viewtime_ms INTEGER NOT NULL,
    last_viewed_ms INTEGER,
    PRIMARY KEY (hash_id, canvas_type)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_viewing_stats_views ON file_viewing_stats (views);
CREATE INDEX file_viewing_stats_viewtime ON file_viewing_stats (viewtime_ms);
CREATE INDEX file_viewing_stats_last_viewed ON file_viewing_stats (last_viewed_ms);

CREATE TABLE file_urls (
    hash_id INTEGER NOT NULL,
    url_id INTEGER NOT NULL,
    PRIMARY KEY (hash_id, url_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_urls_url ON file_urls (url_id);

CREATE TABLE file_notes (
    hash_id INTEGER NOT NULL,
    label_id INTEGER NOT NULL,
    note_id INTEGER NOT NULL,
    PRIMARY KEY (hash_id, label_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX file_notes_label ON file_notes (label_id);
CREATE INDEX file_notes_note ON file_notes (note_id);

-- like/dislike (0.0 or 1.0) and numerical (fraction of max stars) ratings
CREATE TABLE ratings (
    service_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL,
    rating REAL NOT NULL,
    PRIMARY KEY (service_id, hash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX ratings_hash ON ratings (hash_id);
CREATE INDEX ratings_value ON ratings (service_id, rating);

CREATE TABLE ratings_incdec (
    service_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL,
    rating INTEGER NOT NULL,
    PRIMARY KEY (service_id, hash_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX ratings_incdec_hash ON ratings_incdec (hash_id);
CREATE INDEX ratings_incdec_value ON ratings_incdec (service_id, rating);

-- tag relations ------------------------------------------------------------

-- status: 0 current, 1 pending, 2 deleted, 3 petitioned (ContentStatus)
CREATE TABLE tag_siblings (
    service_id INTEGER NOT NULL,
    status INTEGER NOT NULL,
    bad_tag_id INTEGER NOT NULL,
    good_tag_id INTEGER NOT NULL,
    reason_id INTEGER,
    PRIMARY KEY (service_id, status, bad_tag_id, good_tag_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX tag_siblings_good ON tag_siblings (good_tag_id);
CREATE INDEX tag_siblings_bad ON tag_siblings (bad_tag_id);

CREATE TABLE tag_parents (
    service_id INTEGER NOT NULL,
    status INTEGER NOT NULL,
    child_tag_id INTEGER NOT NULL,
    parent_tag_id INTEGER NOT NULL,
    reason_id INTEGER,
    PRIMARY KEY (service_id, status, child_tag_id, parent_tag_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX tag_parents_child ON tag_parents (child_tag_id);
CREATE INDEX tag_parents_parent ON tag_parents (parent_tag_id);

-- kind: 0 siblings, 1 parents. Lower position = higher priority.
CREATE TABLE tag_display_application (
    display_service_id INTEGER NOT NULL,
    kind INTEGER NOT NULL,
    position INTEGER NOT NULL,
    source_service_id INTEGER NOT NULL,
    PRIMARY KEY (display_service_id, kind, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE recent_tags (
    service_id INTEGER NOT NULL,
    tag_id INTEGER NOT NULL,
    used_ms INTEGER NOT NULL,
    PRIMARY KEY (service_id, tag_id)
) STRICT, WITHOUT ROWID;

-- duplicates ---------------------------------------------------------------

-- a duplicate group: files that are the same image, with a best 'king'
CREATE TABLE dup_groups (
    group_id INTEGER PRIMARY KEY,
    king_hash_id INTEGER NOT NULL UNIQUE
) STRICT;

CREATE TABLE dup_group_members (
    group_id INTEGER NOT NULL,
    hash_id INTEGER NOT NULL UNIQUE,
    PRIMARY KEY (group_id, hash_id)
) STRICT, WITHOUT ROWID;

-- an alternates group: duplicate groups that are related but not duplicates
CREATE TABLE alt_groups (
    alt_group_id INTEGER PRIMARY KEY
) STRICT;

CREATE TABLE alt_group_members (
    alt_group_id INTEGER NOT NULL,
    group_id INTEGER NOT NULL UNIQUE,
    PRIMARY KEY (alt_group_id, group_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE alt_confirmed_pairs (
    smaller_group_id INTEGER NOT NULL,
    larger_group_id INTEGER NOT NULL,
    PRIMARY KEY (smaller_group_id, larger_group_id)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX alt_confirmed_pairs_larger ON alt_confirmed_pairs (larger_group_id, smaller_group_id);

CREATE TABLE false_positive_pairs (
    smaller_alt_group_id INTEGER NOT NULL,
    larger_alt_group_id INTEGER NOT NULL,
    PRIMARY KEY (smaller_alt_group_id, larger_alt_group_id)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX false_positive_pairs_larger ON false_positive_pairs (larger_alt_group_id, smaller_alt_group_id);

CREATE TABLE potential_pairs (
    smaller_group_id INTEGER NOT NULL,
    larger_group_id INTEGER NOT NULL,
    distance INTEGER NOT NULL,
    PRIMARY KEY (smaller_group_id, larger_group_id)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX potential_pairs_larger ON potential_pairs (larger_group_id, smaller_group_id);

-- how far each file has been searched for similar files (NULL: not yet)
CREATE TABLE similar_search_status (
    hash_id INTEGER PRIMARY KEY,
    searched_distance INTEGER
) STRICT;

-- file storage ------------------------------------------------------------

CREATE TABLE storage_locations (
    location_id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    ideal_weight INTEGER,
    max_bytes INTEGER,
    is_thumbnail_override INTEGER NOT NULL DEFAULT 0
) STRICT;

-- prefix is 'f' or 't' followed by hex, e.g. 'f3a' at granularity 2
CREATE TABLE storage_subfolders (
    prefix TEXT NOT NULL,
    location_id INTEGER NOT NULL,
    PRIMARY KEY (prefix, location_id)
) STRICT, WITHOUT ROWID;

-- settings and objects ----------------------------------------------------

-- typed settings, one serde type per key
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE TABLE api_permissions (
    access_key BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    permits_everything INTEGER NOT NULL,
    permissions TEXT NOT NULL,
    search_tag_filter TEXT
) STRICT;

-- reference-format serialised objects not (yet) converted to native types,
-- kept verbatim so nothing is lost; see ARCHITECTURE.md ADR-7
CREATE TABLE legacy_objects (
    source TEXT NOT NULL,
    type_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    version INTEGER NOT NULL,
    timestamp_ms INTEGER NOT NULL,
    dump TEXT NOT NULL,
    PRIMARY KEY (source, type_id, name, timestamp_ms)
) STRICT, WITHOUT ROWID;

-- derived ------------------------------------------------------------------

-- words of each subtag's searchable form, for autocomplete prefix search;
-- tokenised in Rust (see text.rs) to match the reference exactly
CREATE TABLE cache_subtag_words (
    word TEXT NOT NULL,
    subtag_id INTEGER NOT NULL,
    PRIMARY KEY (word, subtag_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX cache_subtag_words_subtag ON cache_subtag_words (subtag_id);

-- subtags whose searchable form differs from the subtag itself, keyed by
-- that form, so a search for 'blue eyes' also finds 'blue_eyes'
CREATE TABLE cache_searchable_subtags (
    searchable TEXT NOT NULL,
    subtag_id INTEGER NOT NULL,
    PRIMARY KEY (searchable, subtag_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX cache_searchable_subtags_subtag ON cache_searchable_subtags (subtag_id);

-- integer-valued subtags, for 'system:tag as number'
CREATE TABLE cache_integer_subtags (
    subtag_id INTEGER PRIMARY KEY,
    value INTEGER NOT NULL
) STRICT;
CREATE INDEX cache_integer_subtags_value ON cache_integer_subtags (value);

CREATE VIRTUAL TABLE cache_note_fts USING fts5 (
    note,
    content = '',
    contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 0'
);
";

/// Open-time connection setup shared by the writer and readers.
pub(crate) fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = FULL;
         PRAGMA foreign_keys = OFF;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -262144;
         PRAGMA mmap_size = 268435456;
         PRAGMA busy_timeout = 30000;",
    )?;
    rusqlite::vtab::array::load_module(conn)?;
    Ok(())
}

/// Bring the schema up to [`SCHEMA_VERSION`], applying each missing migration
/// in its own transaction.
pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let version = index as u32 + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied schema migration");
    }
    Ok(())
}

/// Names of one tag service's mapping tables.
#[derive(Debug, Clone)]
pub struct MappingTables {
    pub current: String,
    pub deleted: String,
    pub pending: String,
    pub petitioned: String,
    pub counts: String,
    pub display_counts: String,
}

impl MappingTables {
    pub fn new(service: ServiceId) -> Self {
        let sid = service.get();
        Self {
            current: format!("mappings_{sid}_current"),
            deleted: format!("mappings_{sid}_deleted"),
            pending: format!("mappings_{sid}_pending"),
            petitioned: format!("mappings_{sid}_petitioned"),
            counts: format!("cache_tag_counts_{sid}"),
            display_counts: format!("cache_display_counts_{sid}"),
        }
    }

    /// The table for a mapping status.
    pub fn for_status(&self, status: hydrus_core::ContentStatus) -> &str {
        use hydrus_core::ContentStatus;
        match status {
            ContentStatus::Current => &self.current,
            ContentStatus::Deleted => &self.deleted,
            ContentStatus::Pending => &self.pending,
            ContentStatus::Petitioned => &self.petitioned,
        }
    }
}

/// Create the per-service tables for a new tag service.
pub(crate) fn create_tag_service_tables(conn: &Connection, service: ServiceId) -> Result<()> {
    let t = MappingTables::new(service);
    let mut sql = String::new();
    for table in [&t.current, &t.deleted, &t.pending] {
        write!(
            sql,
            "CREATE TABLE {table} (tag_id INTEGER NOT NULL, hash_id INTEGER NOT NULL, \
             PRIMARY KEY (tag_id, hash_id)) STRICT, WITHOUT ROWID;
             CREATE UNIQUE INDEX {table}_hash ON {table} (hash_id, tag_id);\n"
        )
        .expect("writing to a String cannot fail");
    }
    let petitioned = &t.petitioned;
    write!(
            sql,
        "CREATE TABLE {petitioned} (tag_id INTEGER NOT NULL, hash_id INTEGER NOT NULL, reason_id INTEGER, \
         PRIMARY KEY (tag_id, hash_id)) STRICT, WITHOUT ROWID;
         CREATE UNIQUE INDEX {petitioned}_hash ON {petitioned} (hash_id, tag_id);\n"
    )
    .expect("writing to a String cannot fail");
    for table in [&t.counts, &t.display_counts] {
        write!(
            sql,
            "CREATE TABLE {table} (domain_id INTEGER NOT NULL, tag_id INTEGER NOT NULL, \
             current INTEGER NOT NULL, pending INTEGER NOT NULL, \
             PRIMARY KEY (domain_id, tag_id)) STRICT, WITHOUT ROWID;
             CREATE INDEX {table}_tag ON {table} (tag_id);\n"
        )
        .expect("writing to a String cannot fail");
    }
    conn.execute_batch(&sql)?;
    Ok(())
}

/// Drop a tag service's tables.
pub(crate) fn drop_tag_service_tables(conn: &Connection, service: ServiceId) -> Result<()> {
    let t = MappingTables::new(service);
    for table in [
        &t.current,
        &t.deleted,
        &t.pending,
        &t.petitioned,
        &t.counts,
        &t.display_counts,
    ] {
        conn.execute_batch(&format!("DROP TABLE IF EXISTS {table};"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_cleanly_and_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        migrate(&mut conn).unwrap();
        let v: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        migrate(&mut conn).unwrap();
    }

    #[test]
    fn refuses_newer_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            migrate(&mut conn),
            Err(StoreError::SchemaTooNew { .. })
        ));
    }

    #[test]
    fn tag_service_tables_round_trip() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        create_tag_service_tables(&conn, ServiceId(7)).unwrap();
        conn.execute("INSERT INTO mappings_7_current VALUES (1, 2)", [])
            .unwrap();
        drop_tag_service_tables(&conn, ServiceId(7)).unwrap();
        assert!(
            conn.execute("INSERT INTO mappings_7_current VALUES (1, 2)", [])
                .is_err()
        );
    }
}
