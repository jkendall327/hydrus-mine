//! The native hydrus-rs database.
//!
//! See `docs/rust/STORE.md` for the schema design and consistency rules.

pub mod conn;
pub mod counts;
pub mod display;
pub mod error;
pub mod import;
pub mod master;
pub mod media;
pub mod schema;
pub mod services;
pub mod settings;
pub mod storage;
pub mod store;
pub mod text;

pub use conn::{Db, WriteCtx};
pub use error::{Result, StoreError};
pub use store::{Snapshot, Store};
