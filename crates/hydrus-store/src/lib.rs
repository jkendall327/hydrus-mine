//! The native hydrus-rs database.
//!
//! See `docs/rust/STORE.md` for the schema design and consistency rules.

pub mod autocomplete;
pub mod bandwidth;
pub mod conn;
pub mod content;
pub mod counts;
pub mod delete_lock;
pub mod display;
pub mod domains;
pub mod duplicates;
pub mod error;
pub mod file_maintenance;
pub mod gallery;
pub mod import;
pub mod import_folders;
pub mod legacy;
pub mod live;
pub mod maintenance;
pub mod master;
pub mod media;
pub mod network;
pub mod network_runtime;
pub mod paths;
pub mod pending;
pub mod popups;
pub mod queues;
pub mod schema;
pub mod services;
pub mod services_management;
pub mod session_backups;
pub mod sessions;
pub mod settings;
pub mod similar;
pub mod stats;
pub mod storage;
pub mod store;
pub mod subscriptions;
pub mod synth;
pub mod tag_display;
pub mod tag_display_config;
pub mod tag_migration;
pub mod text;
pub mod transfer;
pub mod trash;
pub mod urls;
pub mod watchers;

pub use conn::{Db, Paused, WriteCtx};
pub use error::{Result, StoreError};
pub use store::{Snapshot, Store};

pub mod api_permissions;
