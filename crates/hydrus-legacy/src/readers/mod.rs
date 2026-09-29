//! Typed readers for every table of primary data.
//!
//! Each reader is a method on [`crate::LegacyDb`]. Anything that can be
//! large returns a streaming [`crate::Rows`] iterator in primary-key order;
//! small configuration tables return a `Vec`.

mod duplicates;
mod files;
mod master;
mod metadata;
mod objects;
mod repositories;
mod services;
mod storage;
mod tags;

pub use duplicates::{
    ActionedPair, AlternatesGroupId, DeclinedPair, MediaId, PotentialDuplicatePair,
};
pub use files::{CurrentFile, DeletedFile, FileInfo, FileProperty, ViewingStats};
pub use master::{LocalHashes, TagDefinition, UrlDefinition};
pub use metadata::{FileNote, IncDecRating, Rating, RecentTag};
pub use objects::{MaintenanceJob, StoredHashedObject, StoredNamedObject, StoredObject};
pub use repositories::{ServiceDirectory, UpdateProcessed};
pub use services::Service;
pub use storage::{FileStorageConfig, IdealLocation, StorageLocationId};
pub use tags::{Mapping, TagPair};

use hydrus_core::{Mime, TimestampMs};
use rusqlite::Row;
use rusqlite::types::FromSql;

use crate::error::{LegacyError, Result};

/// Read a column, reporting the table and column on failure.
pub(crate) fn column<T: FromSql>(row: &Row<'_>, index: usize, table: &str) -> Result<T> {
    row.get(index).map_err(|e| {
        let name = row.as_ref().column_name(index).unwrap_or("?");
        LegacyError::bad_value(table, format!("column {name}: {e}"))
    })
}

/// Read a file type code.
pub(crate) fn mime_column(row: &Row<'_>, index: usize, table: &str) -> Result<Mime> {
    let code: i64 = column(row, index, table)?;
    u8::try_from(code)
        .ok()
        .and_then(Mime::from_code)
        .ok_or_else(|| LegacyError::bad_value(table, format!("unknown file type {code}")))
}

/// Read a nullable millisecond timestamp.
pub(crate) fn timestamp_column(
    row: &Row<'_>,
    index: usize,
    table: &str,
) -> Result<Option<TimestampMs>> {
    column(row, index, table)
}
