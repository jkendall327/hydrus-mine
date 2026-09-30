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
use crate::objects::subscriptions::{LegacySubscription, QueryLog};
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

    /// The import options manager: import option defaults (type 144).
    pub fn import_options_manager(
        &self,
    ) -> Result<Option<hydrus_core::import_options::ImportOptionsManager>> {
        self.singleton(
            SerialisableType(144),
            crate::objects::import_options::manager,
        )
    }

    /// The network domain manager's GUGs and page parsers (type 53).
    pub fn downloaders(&self) -> Result<Option<hydrus_parse::Downloaders>> {
        self.singleton(SerialisableType(53), crate::objects::parsers::downloaders)
    }

    /// The network domain manager's custom HTTP headers (type 53).
    pub fn custom_headers(&self) -> Result<Option<Vec<crate::objects::domain::CustomHeader>>> {
        self.singleton(SerialisableType(53), crate::objects::domain::custom_headers)
    }

    /// The latest object of each name of one type in `json_dumps_named`.
    fn latest_named(&self, kind: SerialisableType) -> Result<Vec<StoredNamedObject>> {
        let mut latest: std::collections::BTreeMap<String, StoredNamedObject> =
            std::collections::BTreeMap::new();
        for row in self.json_dumps_named()? {
            let row = row?;
            if row.kind != kind {
                continue;
            }
            if latest
                .get(&row.name)
                .is_none_or(|kept| kept.timestamp <= row.timestamp)
            {
                latest.insert(row.name.clone(), row);
            }
        }
        Ok(latest.into_values().collect())
    }

    /// Every network session with its cookies (named objects of type 96,
    /// the latest of each name).
    pub fn network_sessions(&self) -> Result<Vec<crate::objects::NetworkSession>> {
        self.latest_named(SerialisableType(96))?
            .into_iter()
            .map(|row| {
                let location = format!("json_dumps_named session {:?}", row.name);
                let object = row
                    .parse()
                    .map_err(|e| LegacyError::serialisable(&location, e))?;
                crate::objects::NetworkSession::from_object(&object)
                    .map_err(|e| LegacyError::serialisable(&location, e))
            })
            .collect()
    }

    /// Every subscription (named objects of type 88, the latest of each
    /// name), without their queries' histories (see [`Self::query_log`]).
    /// Each is decoded on its own, so one that can't be read doesn't stop
    /// the others.
    pub fn subscriptions(&self) -> Result<Vec<(String, Result<LegacySubscription>)>> {
        Ok(self
            .latest_named(SerialisableType(88))?
            .into_iter()
            .map(|row| {
                let location = format!("json_dumps_named subscription {:?}", row.name);
                let decoded = row
                    .parse()
                    .and_then(|object| crate::objects::subscriptions::subscription(&object))
                    .map_err(|e| LegacyError::serialisable(&location, e));
                (row.name, decoded)
            })
            .collect())
    }

    /// Every export folder (type 16), decoded; a folder that cannot be
    /// decoded is an error in its slot.
    pub fn export_folders(
        &self,
    ) -> Result<
        Vec<(
            String,
            Result<crate::objects::export_folders::LegacyExportFolder>,
        )>,
    > {
        Ok(self
            .latest_named(SerialisableType(16))?
            .into_iter()
            .map(|row| {
                let location = format!("json_dumps_named export folder {:?}", row.name);
                let decoded = row
                    .parse()
                    .and_then(|object| crate::objects::export_folders::export_folder(&object))
                    .map_err(|e| LegacyError::serialisable(&location, e));
                (row.name, decoded)
            })
            .collect())
    }

    /// Every import folder (type 19), decoded; a folder that cannot be
    /// decoded is an error in its slot.
    pub fn import_folders(
        &self,
    ) -> Result<
        Vec<(
            String,
            Result<crate::objects::import_folders::LegacyImportFolder>,
        )>,
    > {
        Ok(self
            .latest_named(SerialisableType(19))?
            .into_iter()
            .map(|row| {
                let location = format!("json_dumps_named import folder {:?}", row.name);
                let decoded = row
                    .parse()
                    .and_then(|object| crate::objects::import_folders::import_folder(&object))
                    .map_err(|e| LegacyError::serialisable(&location, e));
                (row.name, decoded)
            })
            .collect())
    }

    /// Every duplicates auto-resolution rule (type 128), decoded; a rule
    /// that cannot be decoded is an error in its slot.
    pub fn auto_resolution_rules(
        &self,
    ) -> Result<
        Vec<(
            String,
            Result<crate::objects::auto_resolution::AutoResolutionRule>,
        )>,
    > {
        Ok(self
            .latest_named(SerialisableType::DUPLICATES_AUTO_RESOLUTION_RULE)?
            .into_iter()
            .map(|row| {
                let location = format!("json_dumps_named auto-resolution rule {:?}", row.name);
                let decoded = row
                    .parse()
                    .and_then(|object| {
                        crate::objects::auto_resolution::AutoResolutionRule::from_object(&object)
                    })
                    .map_err(|e| LegacyError::serialisable(&location, e));
                (row.name, decoded)
            })
            .collect())
    }

    /// A subscription query's history (type 86) by the name in its
    /// [`QueryHeader`](crate::objects::subscriptions::QueryHeader); `None`
    /// if it is missing (the reference then starts the query afresh).
    pub fn query_log(&self, name: &str) -> Result<Option<QueryLog>> {
        let table = self.table("main", "json_dumps_named")?;
        let row = self
            .connection()
            .query_row(
                &format!(
                    "SELECT version, dump FROM {table} WHERE dump_type = 86 AND dump_name = ? \
                     ORDER BY timestamp_ms DESC LIMIT 1"
                ),
                [name],
                |row| {
                    Ok((
                        version_column(row, 0, "json_dumps_named"),
                        dump_column(row, 1, "json_dumps_named"),
                    ))
                },
            )
            .optional()?;
        let Some((version, dump)) = row else {
            return Ok(None);
        };
        let location = format!("json_dumps_named query log {name:?}");
        let object = SerialisableObject::from_stored(
            SerialisableType(86),
            Some(name.to_owned()),
            version?,
            &dump?,
        )
        .map_err(|e| LegacyError::serialisable(&location, e))?;
        crate::objects::subscriptions::query_log(&object)
            .map(Some)
            .map_err(|e| LegacyError::serialisable(&location, e))
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
