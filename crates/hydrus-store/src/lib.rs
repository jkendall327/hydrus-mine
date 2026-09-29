//! The native hydrus-rs database.
//!
//! See `docs/rust/STORE.md` for the schema design and consistency rules.

pub mod conn;
pub mod display;
pub mod error;
pub mod master;
pub mod schema;
pub mod services;
pub mod text;

pub use conn::{Db, WriteCtx};
pub use error::{Result, StoreError};
