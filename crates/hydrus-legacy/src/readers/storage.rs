//! Where the client keeps its files: storage locations, the prefix
//! subfolders in each, and the user's ideal distribution.

use std::collections::BTreeMap;

use super::column;
use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};

/// A row id of `current_client_files_locations`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StorageLocationId(pub i64);

/// Where the user would like files to be, and how much of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdealLocation {
    pub location: StorageLocationId,
    /// Relative share of the files.
    pub weight: i64,
    /// A cap on the bytes stored there, if any.
    pub max_num_bytes: Option<i64>,
}

/// The file storage configuration.
///
/// Files are spread over *prefix subfolders*: `f` (files) or `t`
/// (thumbnails) followed by the first `granularity` hex characters of the
/// sha256, e.g. `f3a` at granularity 2. Each prefix lives in one (rarely,
/// mid-migration, more than one) storage location. See [`crate::paths`] for
/// turning this into file paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStorageConfig {
    /// Hex characters per prefix (2 by default, meaning 256 file and 256
    /// thumbnail subfolders).
    pub granularity: usize,
    /// Storage locations as stored: *portable* paths, relative to the
    /// database directory when inside it, absolute otherwise.
    pub locations: BTreeMap<StorageLocationId, String>,
    /// `(prefix, location)` in stored order.
    pub subfolders: Vec<(String, StorageLocationId)>,
    pub ideal_locations: Vec<IdealLocation>,
    /// A separate home for thumbnails, if the user set one.
    pub ideal_thumbnail_override: Option<StorageLocationId>,
}

impl LegacyDb {
    /// Read the file storage configuration.
    pub fn file_storage(&self) -> Result<FileStorageConfig> {
        let connection = self.connection();
        let granularity_table = self.table("main", "current_storage_granularity")?;
        let granularity: i64 = connection.query_row(
            &format!("SELECT granularity FROM {granularity_table}"),
            [],
            |row| row.get(0),
        )?;
        let granularity = usize::try_from(granularity)
            .ok()
            .filter(|g| (1..=6).contains(g))
            .ok_or_else(|| {
                LegacyError::bad_value(
                    "current_storage_granularity",
                    format!("granularity {granularity}"),
                )
            })?;

        let table = self.table("main", "current_client_files_locations")?;
        let mut statement = connection.prepare(&format!(
            "SELECT location_id, location FROM {table} ORDER BY location_id"
        ))?;
        let mut locations = BTreeMap::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let id: i64 = column(row, 0, "current_client_files_locations")?;
            locations.insert(
                StorageLocationId(id),
                column(row, 1, "current_client_files_locations")?,
            );
        }

        let known = |id: i64, table: &str| -> Result<StorageLocationId> {
            let id = StorageLocationId(id);
            if locations.contains_key(&id) {
                Ok(id)
            } else {
                Err(LegacyError::bad_value(
                    table,
                    format!("unknown location id {}", id.0),
                ))
            }
        };

        let table = self.table("main", "client_files_subfolders")?;
        let mut statement = connection.prepare(&format!(
            "SELECT prefix, location_id FROM {table} ORDER BY prefix, location_id"
        ))?;
        let mut subfolders = Vec::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let prefix: String = column(row, 0, "client_files_subfolders")?;
            let valid = prefix.len() == granularity + 1
                && (prefix.starts_with('f') || prefix.starts_with('t'))
                && prefix[1..]
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            if !valid {
                return Err(LegacyError::bad_value(
                    "client_files_subfolders",
                    format!("prefix {prefix:?} does not match granularity {granularity}"),
                ));
            }
            let location = known(
                column(row, 1, "client_files_subfolders")?,
                "client_files_subfolders",
            )?;
            subfolders.push((prefix, location));
        }

        let table = self.table("main", "ideal_client_files_locations")?;
        let mut statement = connection.prepare(&format!(
            "SELECT location_id, weight, max_num_bytes FROM {table} ORDER BY location_id"
        ))?;
        let mut ideal_locations = Vec::new();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            const T: &str = "ideal_client_files_locations";
            ideal_locations.push(IdealLocation {
                location: known(column(row, 0, T)?, T)?,
                weight: column(row, 1, T)?,
                max_num_bytes: column(row, 2, T)?,
            });
        }

        let table = self.table("main", "ideal_thumbnail_override_location")?;
        let override_id: Option<i64> = connection
            .query_row(&format!("SELECT location_id FROM {table}"), [], |row| {
                row.get(0)
            })
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let ideal_thumbnail_override = override_id
            .map(|id| known(id, "ideal_thumbnail_override_location"))
            .transpose()?;

        Ok(FileStorageConfig {
            granularity,
            locations,
            subfolders,
            ideal_locations,
            ideal_thumbnail_override,
        })
    }
}
