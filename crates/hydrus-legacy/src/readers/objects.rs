//! Serialised objects (`json_dumps`, `json_dumps_named`,
//! `json_dumps_hashed`, `json_dict`), the legacy YAML `options`, and the
//! file maintenance queue.
//!
//! The `json_dumps*` tables store an object's type and version in columns
//! and only its `info` in the `dump` column. The stored text is kept
//! verbatim ([`StoredObject::dump`]) so objects that are not decoded can be
//! carried over byte-for-byte; [`StoredObject::parse`] gives the generic
//! tree.

use hydrus_core::{HashId, Sha256, TimestampMs};
use rusqlite::OptionalExtension;
use rusqlite::types::ValueRef;

use super::column;
use crate::db::LegacyDb;
use crate::error::{LegacyError, Result};
use crate::objects::{
    ClientApiManager, ClientOptions, FavouriteSearchManager, LegacyOptions, TagDisplayManager,
};
use crate::pyjson::PyJson;
use crate::rows::{Paging, Rows};
use crate::serialisable::{SerialisableError, SerialisableObject, SerialisableType};

/// A singleton object from `json_dumps` (one per type).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub kind: SerialisableType,
    pub version: u32,
    /// The `info` JSON exactly as stored.
    pub dump: String,
}

/// A named object from `json_dumps_named`: subscriptions, GUI sessions,
/// shortcut sets, duplicates auto-resolution rules and the like. Some types
/// keep a few timestamped backups under the same name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredNamedObject {
    pub kind: SerialisableType,
    pub name: String,
    pub version: u32,
    pub timestamp: TimestampMs,
    pub dump: String,
}

/// A content-addressed object from `json_dumps_hashed` (GUI session page
/// data, referenced by hash from sessions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredHashedObject {
    pub hash: Sha256,
    pub kind: SerialisableType,
    pub version: u32,
    pub dump: String,
}

/// A queued file maintenance job (`external_caches.file_maintenance_jobs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaintenanceJob {
    pub hash_id: HashId,
    /// A `ClientFilesMaintenance.REGENERATE_FILE_DATA_JOB_*` code.
    pub job_type: i64,
    /// Seconds since the epoch before which the job should not run.
    pub time_can_start: Option<i64>,
}

impl StoredObject {
    pub fn parse(&self) -> Result<SerialisableObject, SerialisableError> {
        SerialisableObject::from_stored(self.kind, None, self.version, &self.dump)
    }
}

impl StoredNamedObject {
    pub fn parse(&self) -> Result<SerialisableObject, SerialisableError> {
        SerialisableObject::from_stored(
            self.kind,
            Some(self.name.clone()),
            self.version,
            &self.dump,
        )
    }
}

impl StoredHashedObject {
    pub fn parse(&self) -> Result<SerialisableObject, SerialisableError> {
        SerialisableObject::from_stored(self.kind, None, self.version, &self.dump)
    }
}

/// Read a `dump` column: text, or a UTF-8 blob (the reference binds bytes).
fn dump_column(row: &rusqlite::Row<'_>, index: usize, table: &str) -> Result<String> {
    let bytes = match row.get_ref(index)? {
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => bytes,
        other => {
            return Err(LegacyError::bad_value(
                table,
                format!("dump is {:?}, not text", other.data_type()),
            ));
        }
    };
    String::from_utf8(bytes.to_vec())
        .map_err(|e| LegacyError::bad_value(table, format!("dump is not utf-8: {e}")))
}

fn kind_column(row: &rusqlite::Row<'_>, index: usize, table: &str) -> Result<SerialisableType> {
    let code: i64 = column(row, index, table)?;
    u16::try_from(code)
        .map(SerialisableType)
        .map_err(|_| LegacyError::bad_value(table, format!("dump type {code}")))
}

fn version_column(row: &rusqlite::Row<'_>, index: usize, table: &str) -> Result<u32> {
    let version: i64 = column(row, index, table)?;
    u32::try_from(version).map_err(|_| LegacyError::bad_value(table, format!("version {version}")))
}

impl LegacyDb {
    /// Every singleton object (`json_dumps`), by type.
    pub fn json_dumps(&self) -> Result<Vec<StoredObject>> {
        let table = self.table("main", "json_dumps")?;
        let mut statement = self.connection().prepare(&format!(
            "SELECT dump_type, version, dump FROM {table} ORDER BY dump_type"
        ))?;
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(StoredObject {
                kind: kind_column(row, 0, "json_dumps")?,
                version: version_column(row, 1, "json_dumps")?,
                dump: dump_column(row, 2, "json_dumps")?,
            });
        }
        Ok(out)
    }

    /// The singleton object of one type, if stored.
    pub fn json_dump(&self, kind: SerialisableType) -> Result<Option<StoredObject>> {
        let table = self.table("main", "json_dumps")?;
        let mut statement = self.connection().prepare(&format!(
            "SELECT version, dump FROM {table} WHERE dump_type = ?1"
        ))?;
        let mut rows = statement.query([i64::from(kind.code())])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(StoredObject {
            kind,
            version: version_column(row, 0, "json_dumps")?,
            dump: dump_column(row, 1, "json_dumps")?,
        }))
    }

    /// Every named object (`json_dumps_named`), streamed: GUI sessions can be
    /// large.
    pub fn json_dumps_named(&self) -> Result<Rows<'_, StoredNamedObject>> {
        let table = self.table("main", "json_dumps_named")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["rowid"],
                columns: &["dump_type", "dump_name", "version", "timestamp_ms", "dump"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "json_dumps_named";
                Ok(StoredNamedObject {
                    kind: kind_column(row, 1, T)?,
                    name: column(row, 2, T)?,
                    version: version_column(row, 3, T)?,
                    timestamp: column(row, 4, T)?,
                    dump: dump_column(row, 5, T)?,
                })
            }),
        ))
    }

    /// Every hashed object (`json_dumps_hashed`), streamed.
    pub fn json_dumps_hashed(&self) -> Result<Rows<'_, StoredHashedObject>> {
        let table = self.table("main", "json_dumps_hashed")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["rowid"],
                columns: &["hash", "dump_type", "version", "dump"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "json_dumps_hashed";
                Ok(StoredHashedObject {
                    hash: column(row, 1, T)?,
                    kind: kind_column(row, 2, T)?,
                    version: version_column(row, 3, T)?,
                    dump: dump_column(row, 4, T)?,
                })
            }),
        ))
    }

    /// Plain JSON values by name (`json_dict`). Unused by v688 itself but
    /// kept by old databases.
    pub fn json_dict(&self) -> Result<Vec<(String, PyJson)>> {
        let table = self.table("main", "json_dict")?;
        let mut statement = self
            .connection()
            .prepare(&format!("SELECT name, dump FROM {table} ORDER BY name"))?;
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let name: String = column(row, 0, "json_dict")?;
            let dump = dump_column(row, 1, "json_dict")?;
            let value = PyJson::parse(&dump)
                .map_err(|e| LegacyError::serialisable(format!("json_dict {name:?}"), e.into()))?;
            out.push((name, value));
        }
        Ok(out)
    }

    fn singleton<T>(
        &self,
        kind: SerialisableType,
        decode: fn(&SerialisableObject) -> Result<T, SerialisableError>,
    ) -> Result<Option<T>> {
        let Some(stored) = self.json_dump(kind)? else {
            return Ok(None);
        };
        let location = format!("json_dumps {kind}");
        let object = stored
            .parse()
            .map_err(|e| LegacyError::serialisable(&location, e))?;
        decode(&object)
            .map(Some)
            .map_err(|e| LegacyError::serialisable(&location, e))
    }

    /// The client options (type 22).
    pub fn client_options(&self) -> Result<Option<ClientOptions>> {
        self.singleton(SerialisableType::CLIENT_OPTIONS, ClientOptions::from_object)
    }

    /// Client API access keys and permissions (type 75).
    pub fn client_api_manager(&self) -> Result<Option<ClientApiManager>> {
        self.singleton(
            SerialisableType::CLIENT_API_MANAGER,
            ClientApiManager::from_object,
        )
    }

    /// The URL classes and parser links of the network domain manager
    /// (type 53).
    pub fn url_class_settings(&self) -> Result<Option<hydrus_core::url::UrlClassSettings>> {
        self.singleton(
            SerialisableType(53),
            crate::objects::domain::url_class_settings,
        )
    }

    /// Tag display filters and autocomplete options (type 79).
    pub fn tag_display_manager(&self) -> Result<Option<TagDisplayManager>> {
        self.singleton(
            SerialisableType::TAG_DISPLAY_MANAGER,
            TagDisplayManager::from_object,
        )
    }

    /// Favourite searches (type 81).
    pub fn favourite_search_manager(&self) -> Result<Option<FavouriteSearchManager>> {
        self.singleton(
            SerialisableType::FAVOURITE_SEARCH_MANAGER,
            FavouriteSearchManager::from_object,
        )
    }

    /// The legacy YAML options, with defaults for missing keys. The raw YAML
    /// is available from [`LegacyDb::legacy_options_yaml`].
    pub fn legacy_options(&self) -> Result<LegacyOptions> {
        LegacyOptions::parse(self.legacy_options_yaml()?.as_deref()).map_err(LegacyError::Yaml)
    }

    /// The legacy `options` row exactly as stored, if there is one.
    pub fn legacy_options_yaml(&self) -> Result<Option<String>> {
        let table = self.table("main", "options")?;
        let text: Option<Option<String>> = self
            .connection()
            .query_row(&format!("SELECT options FROM {table}"), [], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(text.flatten())
    }

    /// Queued file maintenance jobs. This is a work queue in the caches
    /// database, not user data: an importer may skip it and re-derive what
    /// it needs, losing only work that had not been done yet.
    pub fn file_maintenance_jobs(&self) -> Result<Rows<'_, MaintenanceJob>> {
        let table = self.table("external_caches", "file_maintenance_jobs")?;
        Ok(Rows::new(
            self,
            &Paging {
                table: &table,
                keys: &["hash_id", "job_type"],
                columns: &["time_can_start"],
                join: "",
            },
            Box::new(|row| {
                const T: &str = "file_maintenance_jobs";
                Ok(MaintenanceJob {
                    hash_id: column(row, 0, T)?,
                    job_type: column(row, 1, T)?,
                    time_can_start: column(row, 2, T)?,
                })
            }),
        ))
    }
}
