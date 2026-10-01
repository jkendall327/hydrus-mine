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

/// The client's global pause switches (the reference's "network > pause"
/// menu): each stops a kind of downloading until it is switched off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct Pauses {
    /// Subscriptions (`pause_subs_sync`).
    pub subscriptions: bool,
    /// Every new request (`pause_all_new_network_traffic`); subscriptions
    /// wait too.
    pub network_traffic: bool,
    /// Every downloader queue: URL queues, gallery searches and watchers
    /// (`pause_all_paged_importers`).
    pub paged_importers: bool,
    /// The queues' file downloads (`pause_all_file_queues`).
    pub file_queues: bool,
    /// Gallery pages: gallery searches and URL queues' gallery URLs
    /// (`pause_all_gallery_searches`).
    pub gallery_searches: bool,
    /// Watchers' thread checks (`pause_all_watcher_checkers`).
    pub watcher_checkers: bool,
}

impl Setting for Pauses {
    const KEY: &'static str = "pauses";
}

impl Pauses {
    /// The reference's option names, with this field.
    pub fn by_option_name(&mut self) -> [(&'static str, &mut bool); 6] {
        [
            ("pause_subs_sync", &mut self.subscriptions),
            ("pause_all_new_network_traffic", &mut self.network_traffic),
            ("pause_all_paged_importers", &mut self.paged_importers),
            ("pause_all_file_queues", &mut self.file_queues),
            ("pause_all_gallery_searches", &mut self.gallery_searches),
            ("pause_all_watcher_checkers", &mut self.watcher_checkers),
        ]
    }

    /// Whether subscriptions may run.
    pub fn subscriptions_run(self) -> bool {
        !(self.subscriptions || self.network_traffic)
    }

    /// Whether queues may download files.
    pub fn files_run(self) -> bool {
        !(self.paged_importers || self.file_queues)
    }

    /// Whether queues may read gallery pages.
    pub fn galleries_run(self) -> bool {
        !(self.paged_importers || self.gallery_searches)
    }

    /// Whether watchers may check their threads.
    pub fn watchers_run(self) -> bool {
        !(self.paged_importers || self.watcher_checkers)
    }
}

impl Setting for FolderSettings {
    const KEY: &'static str = "folders";
}

/// How files are read and written. As in the reference, these hold for the
/// whole process, set when an importer is made for the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileHandlingSettings {
    /// Tell comic book archives from other zips
    /// (`allow_comic_book_archive_detection`).
    pub comic_book_detection: bool,
    /// What counts as transparency (`file_has_transparency_strictness`):
    /// 0, an alpha channel; 1, one that isn't all clear or all opaque; 2,
    /// one a human might notice.
    pub transparency_strictness: u8,
    /// Leave files' permissions alone (`do_not_do_chmod_mode`).
    pub do_not_chmod: bool,
}

impl Default for FileHandlingSettings {
    fn default() -> Self {
        Self {
            comic_book_detection: true,
            transparency_strictness: 2,
            do_not_chmod: false,
        }
    }
}

impl Setting for FileHandlingSettings {
    const KEY: &'static str = "file_handling";
}

impl Setting for hydrus_core::pages::SortSettings {
    const KEY: &'static str = "sorts";
}

/// How the GUI opens pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PageSettings {
    /// Open files in a new duplicate filter page on all my files, not the
    /// page's domain (`open_files_to_duplicate_filter_uses_all_my_files`).
    pub duplicate_filter_uses_all_my_files: bool,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            duplicate_filter_uses_all_my_files: true,
        }
    }
}

impl Setting for PageSettings {
    const KEY: &'static str = "pages";
}

/// Export folders.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct ExportFolders(pub Vec<hydrus_parse::folders::ExportFolder>);

impl Setting for ExportFolders {
    const KEY: &'static str = "export_folders";
}

/// How exported files are named (`export_phrase` and the export character
/// limits; `always_apply_ntfs_export_filename_rules`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ExportSettings {
    /// The phrase new export folders start with.
    pub phrase: String,
    /// The longest whole path (none: the platform's).
    pub path_character_limit: Option<i64>,
    /// The longest directory name (none: the platform's).
    pub dirname_character_limit: Option<i64>,
    pub filename_character_limit: i64,
    /// Name files for Windows even on other systems.
    pub always_apply_ntfs_rules: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            phrase: "{hash}".into(),
            path_character_limit: None,
            dirname_character_limit: None,
            filename_character_limit: 220,
            always_apply_ntfs_rules: false,
        }
    }
}

impl Setting for ExportSettings {
    const KEY: &'static str = "export";
}

impl Setting for hydrus_core::url::UrlClassSettings {
    const KEY: &'static str = "url_classes";
}

/// Import option defaults.
impl Setting for hydrus_core::import_options::ImportOptionsManager {
    const KEY: &'static str = "import_options";
}

/// Saved searches, in the reference's stored order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
pub struct FavouriteSearches(pub Vec<hydrus_core::pages::FavouriteSearch>);

impl Setting for FavouriteSearches {
    const KEY: &'static str = "favourite_searches";
}

/// The desktop client's lock password.
impl Setting for hydrus_core::lock::LockPassword {
    const KEY: &'static str = "lock_password";
}

/// How tags are shown in the GUI.
impl Setting for hydrus_core::tag_presentation::TagPresentation {
    const KEY: &'static str = "tag_presentation";
}

/// The tag lists' colours by namespace.
impl Setting for hydrus_core::tag_presentation::NamespaceColours {
    const KEY: &'static str = "namespace_colours";
}

/// How the media viewer shows and zooms files.
impl Setting for hydrus_core::media_viewer::MediaViewerSettings {
    const KEY: &'static str = "media_viewer";
}

/// How a file's info lines read.
impl Setting for hydrus_core::media_viewer::InfoLineSettings {
    const KEY: &'static str = "info_lines";
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
