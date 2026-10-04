//! The last accepted conversion, shared across all converter editors.
use crate::{
    Result, StoreError, legacy,
    settings::{self, Setting},
};
use hydrus_core::url::strings::Conversion;
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A child acceptance updates this preference even if its parent is canceled.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LastStringConversion(pub Option<Conversion>);
impl Setting for LastStringConversion {
    const KEY: &'static str = "last_used_string_conversion_step";
}

/// Native edits take precedence over a preserved reference preference.
pub fn load(conn: &Connection) -> Result<LastStringConversion> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?)",
        [LastStringConversion::KEY],
        |row| row.get(0),
    )?;
    if exists {
        return settings::get(conn);
    }
    let Some((version, info)) = legacy::singleton(conn, 22)? else {
        return Ok(LastStringConversion::default());
    };
    let object =
        SerialisableObject::from_stored(SerialisableType::CLIENT_OPTIONS, None, version, &info)
            .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    options
        .last_used_string_conversion()
        .map(LastStringConversion)
        .map_err(|error| StoreError::Corrupt(error.to_string()))
}
