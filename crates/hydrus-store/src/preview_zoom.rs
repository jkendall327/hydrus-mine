//! Independent preview default zoom, with retained legacy fallback for old stores.
use crate::{Result, settings};
use hydrus_core::media_viewer::ZoomType;
use hydrus_legacy::objects::ClientOptions;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// The default selected by the next accepted preview or actual viewport resize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_zoom: ZoomType,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            default_zoom: ZoomType::DefaultForFiletype,
        }
    }
}
impl settings::Setting for Settings {
    const KEY: &'static str = "preview_default_zoom";
}
impl Settings {
    /// Decode the preview override without changing the full viewer policy.
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            default_zoom: options
                .integers
                .get("preview_default_zoom_type_override")
                .copied()
                .and_then(ZoomType::from_code)
                .unwrap_or(ZoomType::DefaultForFiletype),
        }
    }
}
/// Typed native values win over retained ClientOptions in pre-control stores.
pub fn load(conn: &Connection) -> Result<Settings> {
    use hydrus_legacy::serialisable::{SerialisableObject, SerialisableType};
    use settings::Setting as _;
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Settings::KEY],
        |row| row.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Settings::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|error| crate::error::StoreError::Corrupt(error.to_string()))?;
    let options = ClientOptions::from_object(&object)
        .map_err(|error| crate::error::StoreError::Corrupt(error.to_string()))?;
    Ok(Settings::from_legacy(&options))
}
