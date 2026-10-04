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

/// Shared tag-autocomplete tabs: the children result cap and service-specific most-used tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TagAutocompleteTabs {
    pub children_limit: Option<usize>,
    pub most_used: std::collections::BTreeMap<String, Vec<String>>,
}
impl Default for TagAutocompleteTabs {
    fn default() -> Self {
        Self {
            children_limit: Some(40),
            most_used: std::collections::BTreeMap::new(),
        }
    }
}
impl Setting for TagAutocompleteTabs {
    const KEY: &'static str = "tag_autocomplete_tabs";
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

/// Main-window identity and the optional client-exit confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuiSettings {
    pub application_display_name: String,
    pub confirm_exit: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            application_display_name: "hydrus client".into(),
            confirm_exit: false,
        }
    }
}

impl Setting for GuiSettings {
    const KEY: &'static str = "gui_settings";
}

/// The options window’s opening page and search placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OptionsPreferences {
    pub remember_panel: bool,
    pub last_panel: String,
    pub search_at_top: bool,
}

impl Default for OptionsPreferences {
    fn default() -> Self {
        Self {
            remember_panel: true,
            last_panel: "gui".into(),
            search_at_top: true,
        }
    }
}

impl Setting for OptionsPreferences {
    const KEY: &'static str = "options_preferences";
}

/// Prompt after the chooser creates a notebook (`rename_page_of_pages_on_pick_new`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct NotebookCreationSettings {
    pub rename_new_notebooks: bool,
}

impl Setting for NotebookCreationSettings {
    const KEY: &'static str = "gui_notebook_creation";
}

/// `default_new_page_goes`, in the reference choice order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub enum PageInsertion {
    FarLeft,
    LeftOfCurrent,
    RightOfCurrent,
    #[default]
    FarRight,
}

impl PageInsertion {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::FarLeft),
            1 => Some(Self::LeftOfCurrent),
            2 => Some(Self::RightOfCurrent),
            3 => Some(Self::FarRight),
            _ => None,
        }
    }

    pub fn index(self, current: Option<usize>, count: usize) -> usize {
        let Some(current) = current else {
            return 0;
        };
        match self {
            Self::FarLeft => 0,
            Self::LeftOfCurrent => current.min(count),
            Self::RightOfCurrent => (current + 1).min(count),
            Self::FarRight => count,
        }
    }
}

impl Setting for PageInsertion {
    const KEY: &'static str = "gui_page_insertion";
}

/// Whether import-options editors hide inappropriate options for each caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ImportOptionsUiSettings {
    pub simple: bool,
}
impl Default for ImportOptionsUiSettings {
    fn default() -> Self {
        Self { simple: true }
    }
}
impl Setting for ImportOptionsUiSettings {
    const KEY: &'static str = "import_options_ui";
}

/// Startup and periodic last-session saving, as GUI Sessions edits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuiSessionSettings {
    pub startup: Option<String>,
    pub autosave_minutes: u16,
    pub only_during_idle: bool,
    pub warn_large_session: bool,
}

impl Default for GuiSessionSettings {
    fn default() -> Self {
        Self {
            startup: Some(crate::sessions::LAST_SESSION.into()),
            autosave_minutes: 5,
            only_during_idle: false,
            warn_large_session: true,
        }
    }
}

impl Setting for GuiSessionSettings {
    const KEY: &'static str = "gui_sessions";
}

/// Idle eligibility from the reference's user-action and mouse timers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GuiIdleSettings {
    pub enabled: bool,
    pub user_seconds: Option<u64>,
    pub mouse_seconds: Option<u64>,
    pub api_seconds: Option<u64>,
}
impl Default for GuiIdleSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            user_seconds: Some(1800),
            mouse_seconds: Some(600),
            api_seconds: None,
        }
    }
}
impl Setting for GuiIdleSettings {
    const KEY: &'static str = "gui_idle";
}

/// Which recognised URL types the desktop watches for in changed clipboard text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ClipboardUrls {
    pub watchers: bool,
    pub other_recognised: bool,
}

impl Setting for ClipboardUrls {
    const KEY: &'static str = "clipboard_urls";
}

impl ClipboardUrls {
    /// Whether the desktop should read clipboard text at all.
    pub fn enabled(self) -> bool {
        self.watchers || self.other_recognised
    }
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

/// Advanced mode (`advanced_mode`): the reference's menus and dialogs
/// offer more with it on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct AdvancedMode(pub bool);

impl Setting for AdvancedMode {
    const KEY: &'static str = "advanced_mode";
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
    /// Prefix clipboard hashes with their type (`prefix_hash_when_copying`).
    pub prefix_hash_when_copying: bool,
}

impl Default for FileHandlingSettings {
    fn default() -> Self {
        Self {
            comic_book_detection: true,
            transparency_strictness: 2,
            do_not_chmod: false,
            prefix_hash_when_copying: false,
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

/// How the thumbnail grid spaces its thumbnails: each is drawn with a
/// border this many pixels wide, and this many pixels of margin around it
/// (`thumbnail_border`, `thumbnail_margin`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ThumbnailLayout {
    pub border: u32,
    pub margin: u32,
}

impl Default for ThumbnailLayout {
    fn default() -> Self {
        Self {
            border: 1,
            margin: 2,
        }
    }
}

impl Setting for ThumbnailLayout {
    const KEY: &'static str = "thumbnail_layout";
}

impl Setting for hydrus_core::tag_summary::TagSummaries {
    const KEY: &'static str = "tag_summaries";
}

/// New search pages' tag domain (`default_tag_service_search_page`), and
/// the file domain a search moves to when it is set to every tag service
/// while searching all known files (`default_local_location_context`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SearchDefaults {
    pub tag_service: hydrus_core::ServiceKey,
    pub local_location: hydrus_core::search::context::LocationContext,
}

impl Default for SearchDefaults {
    /// All known tags, and "my files".
    fn default() -> Self {
        use hydrus_core::service::builtin_keys;
        Self {
            tag_service: hydrus_core::ServiceKey::new(builtin_keys::COMBINED_TAG.to_vec()),
            local_location: hydrus_core::search::context::LocationContext::single(
                hydrus_core::ServiceKey::new(builtin_keys::MY_FILES.to_vec()),
            ),
        }
    }
}

impl SearchDefaults {
    /// Match GetDefaultLocalLocationContext: discard missing domains, then
    /// use all local file domains if none remain.
    pub fn resolved_local_location(
        &self,
        services: &crate::services::ServiceRegistry,
    ) -> hydrus_core::search::context::LocationContext {
        use hydrus_core::search::context::LocationContext;
        let location = LocationContext::new(
            self.local_location
                .current()
                .iter()
                .filter(|key| services.by_key(key).is_ok())
                .cloned(),
            self.local_location
                .deleted()
                .iter()
                .filter(|key| services.by_key(key).is_ok())
                .cloned(),
        );
        if location.current().is_empty() && location.deleted().is_empty() {
            LocationContext::single(hydrus_core::ServiceKey::new(
                hydrus_core::service::builtin_keys::COMBINED_LOCAL_FILE_DOMAINS.to_vec(),
            ))
        } else {
            location
        }
    }
}

impl Setting for SearchDefaults {
    const KEY: &'static str = "search_defaults";
}

/// Read autocomplete and the initial state of a newly created search page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FileSearchSettings {
    pub search_immediately: bool,
    pub show_system_everything: bool,
    pub active_predicate_rows: u32,
    pub autocomplete_rows: u32,
    pub float_autocomplete: bool,
    pub implicit_limit: Option<u64>,
    pub refresh_limited_sort: bool,
}

impl Default for FileSearchSettings {
    fn default() -> Self {
        Self {
            search_immediately: true,
            show_system_everything: true,
            active_predicate_rows: 6,
            autocomplete_rows: 22,
            float_autocomplete: true,
            implicit_limit: None,
            refresh_limited_sort: true,
        }
    }
}

impl Setting for FileSearchSettings {
    const KEY: &'static str = "file_search";
}

/// Native media canvas presentation (`media playback` and `media viewer`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewerCanvasSettings {
    pub recenter_on_resize: bool,
    pub transparency_checkerboard: bool,
    pub transparency_greenscreen: bool,
    pub seek_height: u32,
    pub seek_hidden_height: Option<u32>,
    pub seek_nub_width: u32,
}
impl Default for ViewerCanvasSettings {
    fn default() -> Self {
        Self {
            recenter_on_resize: true,
            transparency_checkerboard: false,
            transparency_greenscreen: false,
            seek_height: 20,
            seek_hidden_height: Some(5),
            seek_nub_width: 10,
        }
    }
}
impl Setting for ViewerCanvasSettings {
    const KEY: &'static str = "viewer_canvas";
}

/// What a surviving original page and main window do when a viewer closes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewerClosingSettings {
    pub reselect_page: bool,
    pub select_exit_media: bool,
    pub activate_focusing: bool,
    pub activate_always: bool,
}
impl Default for ViewerClosingSettings {
    fn default() -> Self {
        Self {
            reselect_page: false,
            select_exit_media: true,
            activate_focusing: false,
            activate_always: false,
        }
    }
}
impl Setting for ViewerClosingSettings {
    const KEY: &'static str = "viewer_closing";
}

/// Whether mouseover panels require the native viewer's active window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewerFocusSettings {
    pub seek_requires_focus: bool,
    pub hovers_require_focus: bool,
}
impl Default for ViewerFocusSettings {
    fn default() -> Self {
        Self {
            seek_requires_focus: true,
            hovers_require_focus: true,
        }
    }
}
impl Setting for ViewerFocusSettings {
    const KEY: &'static str = "viewer_focus";
}

/// Pointer panning and cursor visibility during native viewer drags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewerPointerSettings {
    pub disallow_duration_drag: bool,
    pub hide_during_drag: bool,
}
impl Default for ViewerPointerSettings {
    fn default() -> Self {
        Self {
            disallow_duration_drag: false,
            hide_during_drag: !cfg!(target_os = "macos"),
        }
    }
}
impl Setting for ViewerPointerSettings {
    const KEY: &'static str = "viewer_pointer";
}

/// Pop-in hover panels and the passive bottom-right index in the media viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewerHoverSettings {
    pub tags: bool,
    pub ratings: bool,
    pub notes: bool,
    pub index_background: bool,
}
impl Default for ViewerHoverSettings {
    fn default() -> Self {
        Self {
            tags: true,
            ratings: true,
            notes: true,
            index_background: true,
        }
    }
}
impl Setting for ViewerHoverSettings {
    const KEY: &'static str = "viewer_hovers";
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

/// The system predicates last added from the system predicate editors.
impl Setting for hydrus_core::search::recent::RecentPredicates {
    const KEY: &'static str = "recent_predicates";
}

/// How ratings are drawn over thumbnails.
impl Setting for hydrus_core::thumbnail::ThumbnailRatingSettings {
    const KEY: &'static str = "thumbnail_ratings";
}

/// How a file's info lines read.
impl Setting for hydrus_core::media_viewer::InfoLineSettings {
    const KEY: &'static str = "info_lines";
}

/// Where the main window and the media viewer open, and how big.
impl Setting for hydrus_core::windows::WindowSettings {
    const KEY: &'static str = "windows";
}

/// The volume and mute the media viewer plays at.
impl Setting for hydrus_core::media_viewer::AudioSettings {
    const KEY: &'static str = "audio";
}

/// The media viewer's slideshows.
impl Setting for hydrus_core::media_viewer::SlideshowSettings {
    const KEY: &'static str = "slideshow";
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

impl Setting for hydrus_core::pages::PageNameSettings {
    const KEY: &'static str = "page_names";
}

impl Setting for hydrus_core::pages::DownloaderPageSettings {
    const KEY: &'static str = "downloader_pages";
}

/// The simple downloader's parsing formulae (the reference's options'
/// `simple_downloader_formulae`) and the one last chosen
/// (`favourite_simple_downloader_formula`), which new pages start on.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct SimpleDownloaderFormulae {
    pub formulae: Vec<hydrus_parse::simple::SimpleFormula>,
    pub favourite: String,
}

impl Default for SimpleDownloaderFormulae {
    /// A new reference client's.
    fn default() -> Self {
        Self {
            formulae: hydrus_legacy::objects::parsers::default_simple_formulae()
                .unwrap_or_default(),
            favourite: "all files linked by images in page".to_owned(),
        }
    }
}

impl Setting for SimpleDownloaderFormulae {
    const KEY: &'static str = "simple_downloader_formulae";
}

/// What the daemon (`hydrus serve`) running on the store last said of its
/// Client API, for the desktop client to show: and which process it was.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ClientApiStatus {
    /// The daemon's process id.
    pub pid: u32,
    pub state: ClientApiState,
}

/// Whether the daemon's Client API is running.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
pub enum ClientApiState {
    /// Not said yet.
    #[default]
    Starting,
    /// Off, as its service has no port (as the reference leaves it).
    Off,
    /// Listening at this address.
    Listening(String),
    /// It couldn't start, as the reference says it (`Could not start
    /// "client api": ...`); the rest of the daemon runs on.
    Failed(String),
}

impl Setting for ClientApiStatus {
    const KEY: &'static str = "client_api_status";
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
