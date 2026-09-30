//! The native hydrus-rs database.
//!
//! See `docs/rust/STORE.md` for the schema design and consistency rules.

pub mod autocomplete;
pub mod conn;
pub mod content;
pub mod counts;
pub mod display;
pub mod duplicates;
pub mod error;
pub mod import;
pub mod import_folders;
pub mod maintenance;
pub mod master;
pub mod media;
pub mod network;
pub mod queues;
pub mod schema;
pub mod services;
pub mod settings;
pub mod similar;
pub mod stats;
pub mod storage;
pub mod store;
pub mod subscriptions;
pub mod synth;
pub mod text;
pub mod transfer;
pub mod urls;

pub use conn::{Db, Paused, WriteCtx};
pub use error::{Result, StoreError};
pub use store::{Snapshot, Store};
