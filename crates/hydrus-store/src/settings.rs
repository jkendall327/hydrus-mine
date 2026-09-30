//! Typed settings, stored as JSON under a key in the `settings` table.
//!
//! Each setting is a serde type with a fixed key. Missing keys (a new
//! database, or one imported before a setting existed) read as the default.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use hydrus_core::CanvasType;
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

/// File viewing statistics: whether they are recorded, and which viewers'
/// statistics count as "views" and "view time" when a search or sort does
/// not name viewers (the reference's `file_viewing_statistics_active` and
/// `file_viewing_stats_interesting_canvas_types` options).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileViewingStatistics {
    pub active: bool,
    pub interesting_canvases: Vec<CanvasType>,
}

impl Default for FileViewingStatistics {
    /// The reference's defaults: on, counting the media viewer and the Client API.
    fn default() -> Self {
        Self {
            active: true,
            interesting_canvases: vec![CanvasType::MediaViewer, CanvasType::ClientApi],
        }
    }
}

impl Setting for FileViewingStatistics {
    const KEY: &'static str = "file_viewing_statistics";
}

/// Import and export folders, and deleting files outside the store
/// (`pause_import_folders_sync`, `pause_export_folders_sync`,
/// `copy_import_files_to_temp_dir`, and the old options'
/// `delete_to_recycle_bin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FolderSettings {
    pub pause_import_folders: bool,
    pub pause_export_folders: bool,
    /// Import a copy of each file, so the original is never held open.
    pub copy_import_files_to_temp_dir: bool,
    /// Deleted files go to the recycle bin (or are deleted for good if
    /// that fails), rather than straight away.
    pub delete_to_recycle_bin: bool,
}

impl Default for FolderSettings {
    fn default() -> Self {
        Self {
            pause_import_folders: false,
            pause_export_folders: false,
            copy_import_files_to_temp_dir: true,
            delete_to_recycle_bin: true,
        }
    }
}

impl Setting for FolderSettings {
    const KEY: &'static str = "folders";
}

impl Setting for hydrus_core::url::UrlClassSettings {
    const KEY: &'static str = "url_classes";
}

/// Import option defaults.
impl Setting for hydrus_core::import_options::ImportOptionsManager {
    const KEY: &'static str = "import_options";
}

/// GUGs and page parsers.
impl Setting for hydrus_core::subscriptions::GalleryDefaults {
    const KEY: &'static str = "gallery_defaults";
}

impl Setting for hydrus_core::subscriptions::CheckerDefaults {
    const KEY: &'static str = "checker_defaults";
}

impl Setting for hydrus_parse::Downloaders {
    const KEY: &'static str = "downloaders";
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
