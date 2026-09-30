//! Read-only, typed access to reference (hydrus v688) client databases.
//!
//! This crate is the source side of the one-time importer (ADR-1): it reads
//! a reference database directory and presents its primary data as typed
//! values, without ever writing to it. It knows nothing about the native
//! store.
//!
//! # Layers
//!
//! * [`LegacyDb`] ([`db`]) opens the four database files read-only, checks
//!   the version is exactly 688, and attaches them as the reference does.
//! * [`readers`] add a typed reader per table of primary data. Big tables
//!   (hashes, tags, mappings, file domains, ...) are streamed with [`Rows`]
//!   in primary-key order and bounded memory; small configuration tables are
//!   returned whole.
//! * [`serialisable`] is a lossless, generic model of the reference's
//!   versioned JSON objects, and [`pyjson`] reads and re-emits their JSON
//!   byte-for-byte, so objects nobody decodes yet can be carried across
//!   verbatim (ADR-7).
//! * [`objects`] has typed decoders for the objects the importer and the
//!   Client API need: service settings, Client API permissions, tag filters,
//!   tag display settings, favourite searches and the client options.
//! * [`paths`] computes where each file and thumbnail is on disk.
//!
//! # Example
//!
//! ```no_run
//! use hydrus_core::ContentStatus;
//! use hydrus_legacy::LegacyDb;
//!
//! # fn main() -> hydrus_legacy::Result<()> {
//! let db = LegacyDb::open("/path/to/hydrus/db")?;
//! let _snapshot = db.snapshot()?; // one consistent view for the whole import
//! for service in db.services()? {
//!     if service.service_type.is_real_tag_service() {
//!         for mapping in db.mappings(service.id, ContentStatus::Current)? {
//!             let mapping = mapping?;
//!             // copy mapping.tag_id, mapping.hash_id ...
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # What is not read
//!
//! Derived data is never imported (ADR-1; the native store rebuilds its
//! own), so there are no readers for the caches database
//! (`client.caches.db`: autocomplete counts, display-tag caches, full-text
//! indexes, local hash/tag lookup caches) or for the derived tables in
//! `client.db`: `service_info` (cached counts), `shape_vptree` and
//! `shape_maintenance_branch_regen` (the similar-files search tree),
//! `shape_search_cache_numbers`, `duplicates_files_auto_resolution_rule_count_cache`
//! and the auto-resolution search queues, and the maintenance bookkeeping
//! tables (`analyze_timestamps`, `vacuum_timestamps`,
//! `last_shutdown_work_time`, `deferred_delete_tables`). The one queue in
//! the caches database, `file_maintenance_jobs`, has a reader so an
//! importer can choose to carry pending work over.

pub mod db;
pub mod error;
pub mod objects;
pub mod paths;
pub mod pickle;
pub use hydrus_core::pyjson;
pub mod readers;
pub mod rows;
pub mod serialisable;

pub use db::{LegacyDb, OpenMode, OpenOptions, ServiceInfo, Snapshot};
pub use error::{LegacyError, Result};
pub use rows::Rows;
