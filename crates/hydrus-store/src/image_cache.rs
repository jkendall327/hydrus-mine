//! Decoded-image cache policy, with retained legacy ClientOptions fallback.
use crate::{
    Result, StoreError,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;

/// Full-resolution raster bytes, idle seconds and strict single-image admission percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Policy {
    pub bytes: u64,
    pub timeout: u64,
    pub percentage: u64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            bytes: 1024 * 1024 * 1024,
            timeout: 600,
            percentage: 25,
        }
    }
}
impl Setting for Policy {
    const KEY: &'static str = "image_cache";
}
impl Policy {
    /// Load raw stored values; Options applies the actual Qt control normalization.
    pub fn from_legacy(options: &ClientOptions) -> Self {
        let defaults = Self::default();
        let integer = |name, default| {
            options
                .integers
                .get(name)
                .map_or(default, |value| (*value).max(0) as u64)
        };
        Self {
            bytes: integer("image_cache_size", defaults.bytes),
            timeout: integer("image_cache_timeout", defaults.timeout),
            percentage: integer("image_cache_storage_limit_percentage", defaults.percentage),
        }
    }
    /// Merge edited fields, preserving concurrent updates when an untouched
    /// control merely normalized an imported value to its displayed bounds.
    pub fn save_changed(
        self,
        conn: &Connection,
        before: Self,
        displayed_before: Self,
    ) -> Result<()> {
        if self == before {
            return Ok(());
        }
        let mut latest = load(conn)?;
        for (after, before, displayed, field) in [
            (
                self.bytes,
                before.bytes,
                displayed_before.bytes,
                &mut latest.bytes,
            ),
            (
                self.timeout,
                before.timeout,
                displayed_before.timeout,
                &mut latest.timeout,
            ),
            (
                self.percentage,
                before.percentage,
                displayed_before.percentage,
                &mut latest.percentage,
            ),
        ] {
            if after != before && (after != displayed || *field == before) {
                *field = after;
            }
        }
        settings::set(conn, &latest)
    }
}
/// Native preferences win; an existing imported store keeps its original policy.
pub fn load(conn: &Connection) -> Result<Policy> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Policy::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Policy::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| StoreError::Corrupt(error.to_string()))?;
    Ok(Policy::from_legacy(&options))
}
