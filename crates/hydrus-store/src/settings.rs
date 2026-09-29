//! Typed settings, stored as JSON under a key in the `settings` table.
//!
//! Each setting is a serde type with a fixed key. Missing keys (a new
//! database, or one imported before a setting existed) read as the default.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use hydrus_core::thumbnail::ThumbnailSettings;

use crate::error::Result;

/// A setting stored under a fixed key.
pub trait Setting: Serialize + DeserializeOwned + Default {
    const KEY: &'static str;
}

impl Setting for ThumbnailSettings {
    const KEY: &'static str = "thumbnails";
}

/// Favourite tags offered by autocomplete.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct FavouriteTags(pub Vec<String>);

impl Setting for FavouriteTags {
    const KEY: &'static str = "favourite_tags";
}

pub fn get<S: Setting>(conn: &Connection) -> Result<S> {
    let value: Option<String> = conn
        .prepare_cached("SELECT value FROM settings WHERE key = ?")?
        .query_row([S::KEY], |r| r.get(0))
        .optional()?;
    Ok(match value {
        Some(json) => serde_json::from_str(&json)?,
        None => S::default(),
    })
}

pub fn set<S: Setting>(conn: &Connection, value: &S) -> Result<()> {
    conn.prepare_cached("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")?
        .execute(params![S::KEY, serde_json::to_string(value)?])?;
    Ok(())
}
