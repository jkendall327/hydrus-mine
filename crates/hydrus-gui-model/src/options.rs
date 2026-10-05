//! The options window (file > options): the reference's options dialog
//! (`ManageOptionsPanel`), its pages listed as it lists them (by name,
//! "advanced" last), each with the options hydrus-rs honours, in their
//! boxes and labelled as the reference's are (checked against the
//! reference's dialog, recorded by `oracle/record_options_dialog.py`).
//! Changes wait until "apply", which writes them to the store together.

use std::rc::Rc;

use hydrus_core::media_viewer::{
    InfoLineSettings, MediaViewerSettings, SlideshowSettings, ZoomCentre, ZoomType,
};
use hydrus_core::pages::{
    DownloaderPageSettings, FileCountDisplay, PageCollect, PageNameSettings, PageSort, SortSettings,
};
use hydrus_core::subscriptions::{CheckerDefaults, CheckerOptions, GalleryDefaults};
use hydrus_core::tag_presentation::TagPresentation;
use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
use hydrus_core::thumbnail::{ThumbnailRatingSettings, ThumbnailScale, ThumbnailSettings};
use hydrus_core::url::UrlClassSettings;
use hydrus_core::windows::WindowSettings;
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::command_palette::{CommandPaletteSettings, Provider};
use hydrus_store::delete_lock::DeleteLock;
use hydrus_store::duplicates::DuplicateFilterSettings;
use hydrus_store::duplicates::auto::AutoResolutionSettings;
use hydrus_store::file_maintenance::FileMaintenanceSettings;
use hydrus_store::network::NetworkSettings;
use hydrus_store::regex_favourites::RegexFavourites;
use hydrus_store::session_backups::SessionBackupSettings;
use hydrus_store::sessions::NotebookSettings;
use hydrus_store::settings::{
    AdvancedMode, ExportSettings, FavouriteTags, FileHandlingSettings, FileSearchSettings,
    FileViewingStatistics, FolderSettings, GuiSettings, NotebookCreationSettings,
    OptionsPreferences, PageSettings, SearchDefaults, TagAutocompleteTabs, ThumbnailLayout,
    ViewerBackgroundSettings, ViewerCanvasSettings, ViewerClosingSettings, ViewerCursorSettings,
    ViewerFocusSettings, ViewerHoverSettings, ViewerPlaybackSettings, ViewerPointerSettings,
    ViewerTagScrollSettings,
};
use hydrus_store::similar::SimilarFilesSettings;
use hydrus_store::tag_editing::TagEditingSettings;
use hydrus_store::trash::TrashSettings;
use rusqlite::Connection;

#[path = "options_popup_width.rs"]
mod popup_width;

fn normalise_idle_timeout(seconds: Option<u64>) -> Option<u64> {
    seconds.map(|seconds| (seconds / 60).clamp(1, 1000) * 60)
}

macro_rules! settings {
    (@save $conn:ident, $after:ident, $before:ident, page_layout) => {
        hydrus_store::page_layout::save_changed($conn, &$after.page_layout, &$before.page_layout)?;
    };
    (@save $conn:ident, $after:ident, $before:ident, popup_width) => {
        hydrus_store::popup_width::save_changed($conn, &$after.popup_width, &$before.popup_width)?;
    };
    (@save $conn:ident, $after:ident, $before:ident, related_tags) => {
        if $after.related_tags.weights != $before.related_tags.weights {
            let mut current: hydrus_store::related_tags::Settings = hydrus_store::settings::get($conn)?;
            current.weights.clone_from(&$after.related_tags.weights);
            hydrus_store::settings::set($conn, &current)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, tag_autocomplete_tabs) => {
        if $after.tag_autocomplete_tabs != $before.tag_autocomplete_tabs {
            let mut current: TagAutocompleteTabs = hydrus_store::settings::get($conn)?;
            if $after.tag_autocomplete_tabs.children_limit != $before.tag_autocomplete_tabs.children_limit {
                current.children_limit = $after.tag_autocomplete_tabs.children_limit;
            }
            for (key, tags) in &$after.tag_autocomplete_tabs.most_used {
                if $before.tag_autocomplete_tabs.most_used.get(key) != Some(tags) {
                    current.most_used.insert(key.clone(), tags.clone());
                }
            }
            hydrus_store::settings::set($conn, &current)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, thumbnail_preview_selection) => {
        if $after.thumbnail_preview_selection != $before.thumbnail_preview_selection {
            $after.thumbnail_preview_selection.save_changed($conn, &$before.thumbnail_preview_selection)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, windows) => {
        if $after.windows != $before.windows {
            let mut windows: WindowSettings = hydrus_store::settings::get($conn)?;
            let before_frames = $before.windows.frames();
            for (name,frame) in $after.windows.frames() {
                if before_frames.get(&name) != Some(&frame) { windows.set_frame(&name,frame); }
            }
            if $after.windows.save_media_viewer_on_close != $before.windows.save_media_viewer_on_close {
                windows.save_media_viewer_on_close = $after.windows.save_media_viewer_on_close;
            }
            hydrus_store::settings::set($conn,&windows)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, thumbnail_cache) => {
        if $after.thumbnail_cache != $before.thumbnail_cache {
            let mut latest:hydrus_store::settings::ThumbnailCacheSettings=hydrus_store::settings::get($conn)?;
            if $after.thumbnail_cache.bytes!=$before.thumbnail_cache.bytes {latest.bytes=$after.thumbnail_cache.bytes;}
            if $after.thumbnail_cache.timeout!=$before.thumbnail_cache.timeout {latest.timeout=$after.thumbnail_cache.timeout;}
            hydrus_store::settings::set($conn,&latest)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, file_view_removal) => {
        if $after.file_view_removal != $before.file_view_removal {
            let mut latest: hydrus_store::settings::FileViewRemoval = hydrus_store::settings::get($conn)?;
            if $after.file_view_removal.filtered != $before.file_view_removal.filtered { latest.filtered = $after.file_view_removal.filtered; }
            if $after.file_view_removal.skipped != $before.file_view_removal.skipped { latest.skipped = $after.file_view_removal.skipped; }
            if $after.file_view_removal.trashed != $before.file_view_removal.trashed { latest.trashed = $after.file_view_removal.trashed; }
            if $after.file_view_removal.moved != $before.file_view_removal.moved { latest.moved = $after.file_view_removal.moved; }
            hydrus_store::settings::set($conn, &latest)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, local_transfer) => {
        if $after.local_transfer != $before.local_transfer {
            let mut latest: hydrus_store::settings::LocalTransferPreferences = hydrus_store::settings::get($conn)?;
            if $after.local_transfer.copy != $before.local_transfer.copy { latest.copy = $after.local_transfer.copy; }
            if $after.local_transfer.move_files != $before.local_transfer.move_files { latest.move_files = $after.local_transfer.move_files; }
            hydrus_store::settings::set($conn, &latest)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, deletion) => {
        if $after.deletion != $before.deletion {
            let mut deletion = $after.deletion.clone();
            let current: hydrus_store::settings::DeletionPreferences = hydrus_store::settings::get($conn)?;
            if deletion.last_action == $before.deletion.last_action {deletion.last_action = current.last_action;}
            if deletion.last_reason == $before.deletion.last_reason {deletion.last_reason = current.last_reason;}
            hydrus_store::settings::set($conn, &deletion)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, viewer_eye_menu) => {
        if $after.viewer_eye_menu != $before.viewer_eye_menu {
            let mut eye = $after.viewer_eye_menu.clone();
            let current: hydrus_store::settings::ViewerEyeMenuSettings = hydrus_store::settings::get($conn)?;
            // These defaults belong to live eye-menu actions, not this Options page.
            eye.start_on_top = current.start_on_top;
            eye.start_on_top_while_playing = current.start_on_top_while_playing;
            eye.start_frameless = current.start_frameless;
            hydrus_store::settings::set($conn, &eye)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, note_preferences) => {
        if $after.note_preferences != $before.note_preferences {
            let mut latest: hydrus_store::settings::NotePreferences = hydrus_store::settings::get($conn)?;
            if $after.note_preferences.copy_all != $before.note_preferences.copy_all {
                latest.copy_all = $after.note_preferences.copy_all;
            }
            if $after.note_preferences.copy_json != $before.note_preferences.copy_json {
                latest.copy_json = $after.note_preferences.copy_json;
            }
            if $after.note_preferences.start_at_end != $before.note_preferences.start_at_end {
                latest.start_at_end = $after.note_preferences.start_at_end;
            }
            if $after.note_preferences.hover_text_only != $before.note_preferences.hover_text_only {
                latest.hover_text_only = $after.note_preferences.hover_text_only;
            }
            hydrus_store::settings::set($conn, &latest)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, duplicate_colours) => {
        if $after.duplicate_colours != $before.duplicate_colours {
            let mut latest: hydrus_store::settings::DuplicateColourSettings = hydrus_store::settings::get($conn)?;
            if $after.duplicate_colours.intensity_a != $before.duplicate_colours.intensity_a {
                latest.intensity_a = $after.duplicate_colours.intensity_a;
            }
            if $after.duplicate_colours.intensity_b != $before.duplicate_colours.intensity_b {
                latest.intensity_b = $after.duplicate_colours.intensity_b;
            }
            if $after.duplicate_colours.checkerboard != $before.duplicate_colours.checkerboard {
                latest.checkerboard = $after.duplicate_colours.checkerboard;
            }
            hydrus_store::settings::set($conn, &latest)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, gui_idle) => {
        if $after.gui_idle != $before.gui_idle {
            let mut latest: hydrus_store::settings::GuiIdleSettings = hydrus_store::settings::get($conn)?;
            if $after.gui_idle.enabled != $before.gui_idle.enabled {
                latest.enabled = $after.gui_idle.enabled;
            }
            for (field, value, before) in [
                (&mut latest.user_seconds, $after.gui_idle.user_seconds, $before.gui_idle.user_seconds),
                (&mut latest.mouse_seconds, $after.gui_idle.mouse_seconds, $before.gui_idle.mouse_seconds),
                (&mut latest.api_seconds, $after.gui_idle.api_seconds, $before.gui_idle.api_seconds),
            ] {
                // An unchanged Qt control still normalises imported seconds.
                // Such an implicit edit must not overwrite a newer live value.
                if value != before
                    && (value != normalise_idle_timeout(before) || *field == before)
                {
                    *field = value;
                }
            }
            hydrus_store::settings::set($conn, &latest)?;
        }
    };
    (@save $conn:ident, $after:ident, $before:ident, $field:ident) => {
        if $after.$field != $before.$field {hydrus_store::settings::set($conn, &$after.$field)?;}
    };
    (@load $conn:ident, $ty:ty, $load:path) => { $load($conn) };
    (@load $conn:ident, $ty:ty) => { hydrus_store::settings::get::<$ty>($conn) };
    ($($field:ident: $ty:ty $(=> $load:path)?),* $(,)?) => {
        /// The settings the options window edits, as the store has them.
        #[derive(Debug, Clone, PartialEq)]
        pub struct Settings {
            $(pub $field: $ty,)*
        }

        impl Settings {
            pub fn load(conn: &Connection) -> hydrus_store::Result<Self> {
                Ok(Self {
                    $($field: settings!(@load conn, $ty $(, $load)?)?,)*
                })
            }

            /// Write those changed since `before`.
            pub fn save(&self, conn: &Connection, before: &Self) -> hydrus_store::Result<()> {
                $(
                    settings!(@save conn, self, before, $field);
                )*
                Ok(())
            }
        }
    };
}

settings! {
    shortcuts: hydrus_core::shortcuts::Settings,
    external_calls: hydrus_core::external_calls::Manager,
    open_externally: hydrus_core::open_externally::Routing,
    advanced: AdvancedMode,
    auto_resolution: AutoResolutionSettings,
    bandwidth: BandwidthSettings,
    checker_defaults: CheckerDefaults,
    command_palette: CommandPaletteSettings,
    delete_lock: DeleteLock,
    deletion: hydrus_store::settings::DeletionPreferences,
    local_transfer: hydrus_store::settings::LocalTransferPreferences,
    downloader_pages: DownloaderPageSettings,
    duplicate_filter: DuplicateFilterSettings,
    duplicate_colours: hydrus_store::settings::DuplicateColourSettings,
    export: ExportSettings,
    file_handling: FileHandlingSettings,
    file_view_removal: hydrus_store::settings::FileViewRemoval,
    thumbnail_cache: hydrus_store::settings::ThumbnailCacheSettings,
    file_maintenance: FileMaintenanceSettings,
    file_viewing: FileViewingStatistics,
    folders: FolderSettings,
    gallery: GalleryDefaults,
    gui: GuiSettings,
    gui_formatting: hydrus_store::settings::GuiFormatting,
    gui_sessions: hydrus_store::settings::GuiSessionSettings,
    gui_idle: hydrus_store::settings::GuiIdleSettings,
    info_line: InfoLineSettings,
    import_options: hydrus_core::import_options::ImportOptionsManager,
    import_options_ui: hydrus_store::settings::ImportOptionsUiSettings,
    import_work_slots: hydrus_store::settings::ImportWorkSlots,
    media_viewer: MediaViewerSettings,
    network: NetworkSettings,
    notebooks: NotebookSettings,
    notebook_creation: NotebookCreationSettings,
    page_insertion: hydrus_store::settings::PageInsertion,
    page_chooser: hydrus_store::settings::PageChooserSettings,
    page_navigation: hydrus_store::settings::PageNavigationSettings,
    tab_presentation: hydrus_store::settings::TabPresentationSettings,
    tab_drag: hydrus_store::settings::TabDragSettings,
    options_preferences: OptionsPreferences,
    page_names: PageNameSettings,
    page_settings: PageSettings,
    page_layout: hydrus_store::page_layout::PageLayout => hydrus_store::page_layout::load,
    popup_width: hydrus_store::popup_width::PopupWidth,
    regex_favourites: RegexFavourites => hydrus_store::regex_favourites::load,
    session_backups: SessionBackupSettings,
    search_defaults: SearchDefaults,
    file_search: FileSearchSettings,
    tag_editing: TagEditingSettings,
    tag_autocomplete_tabs: TagAutocompleteTabs,
    tag_suggestions: hydrus_store::settings::TagSuggestionSettings,
    related_tags: hydrus_store::related_tags::Settings,
    favourite_tags: FavouriteTags,
    similar_files: SimilarFilesSettings,
    slideshow: SlideshowSettings,
    sorts: SortSettings,
    tag_presentation: TagPresentation,
    namespace_colours: hydrus_core::tag_presentation::NamespaceColours,
    sibling_connector_colours: hydrus_core::tag_presentation::SiblingConnectorColours,
    tag_summaries: hydrus_core::tag_summary::TagSummaries,
    thumbnails: ThumbnailSettings,
    thumbnail_layout: ThumbnailLayout,
    thumbnail_navigation: hydrus_store::settings::ThumbnailNavigation,
    thumbnail_preview_selection: hydrus_store::thumbnail_preview_selection::Preferences,
    thumbnail_ratings: ThumbnailRatingSettings,
    rating_context_sizes: hydrus_store::settings::RatingContextSizes,
    note_preferences: hydrus_store::settings::NotePreferences,
    trash: TrashSettings,
    url_classes: UrlClassSettings,
    windows: WindowSettings,
    window_rescue: hydrus_store::settings::WindowRescueSettings,
    viewer_canvas: ViewerCanvasSettings,
    viewer_background: ViewerBackgroundSettings,
    viewer_hovers: ViewerHoverSettings,
    viewer_tag_scroll: ViewerTagScrollSettings,
    viewer_eye_menu: hydrus_store::settings::ViewerEyeMenuSettings,
    viewer_pointer: ViewerPointerSettings,
    viewer_focus: ViewerFocusSettings,
    viewer_closing: ViewerClosingSettings,
    viewer_cursor: ViewerCursorSettings,
    viewer_playback: ViewerPlaybackSettings,
}

/// An option's value as its control holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bytes {
        amount: i64,
        unit: usize,
    },
    Shortcuts(hydrus_core::shortcuts::Settings),
    Check(bool),
    Int(i64),
    /// A number, or none (the reference's `NoneableSpinCtrl`).
    Noneable(Option<i64>),
    /// A number with a fraction, as typed (read when applied).
    Float(String),
    /// The index of the item chosen.
    Choice(usize),
    SavedSession(Option<String>),
    GallerySource(Option<crate::gallery_source::KeyAndName>),
    Text(String),
    /// A plain LineEdit that distinguishes untouched legacy None from edited empty text.
    PlainNoneableText(Option<String>),
    /// Text, or none (the reference's `NoneableTextCtrl`); the text is
    /// kept while none, as its text box keeps it.
    NoneableText {
        none: bool,
        text: String,
    },
    /// A time, in seconds (the reference's `TimeDeltaWidget`).
    Duration(f64),
    /// A duration whose last numeric value remains staged while disabled.
    NoneableDuration {
        none: bool,
        seconds: f64,
    },
    /// Selected viewing canvases, in the reference checkbox-list order.
    Canvases(Vec<hydrus_core::CanvasType>),
    /// A number per a time in seconds (the reference's `VelocityCtrl`).
    Velocity(i64, f64),
    /// A file sort: its type and order (the reference's
    /// `MediaSortControl`).
    Sort(PageSort),
    /// How files collect (the reference's `MediaCollectControl`).
    Collect(PageCollect),
    /// A tag list's sort (the reference's `TagSortControl`).
    TagSort(TagSort),
    /// Checker options (the reference's `CheckerOptionsButton`).
    Checker(CheckerOptions),
    /// The editable regular expression/description pairs.
    RegexFavourites(RegexFavourites),
    /// Ordered advanced file-deletion reason suggestions.
    DeletionReasons(Vec<String>),
    NamespaceColours(crate::namespace_colours::Colours),
    FrameLocations(std::collections::BTreeMap<String, hydrus_core::windows::FrameLocation>),
    /// Registered external program calls, staged in the parent Options draft.
    ExternalCalls(hydrus_core::external_calls::Manager),
    OpenExternally(hydrus_core::open_externally::Routing),
    /// Shared favourite tags, staged until the parent options dialog applies.
    FavouriteTags(FavouriteTags),
    MostUsedTags(std::collections::BTreeMap<String, Vec<String>>),
    RelatedWeights(hydrus_store::related_tags::Weights),
    ImportOptions(crate::import_options_panel::Value),
    NamespaceSorts(Vec<PageSort>),
    TagBanner(hydrus_core::tag_summary::TagSummaryGenerator),
    ProviderOrder(Vec<Provider>),
    TagService(hydrus_core::ServiceKey),
    Location(hydrus_core::search::context::LocationContext),
}

/// What kind of control an option has.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Reference byte amount plus B/KB/MB/GB/TB multiplier.
    Bytes,
    Shortcuts,
    Check,
    Int {
        min: i64,
        max: i64,
    },
    /// The number shown while it is none is `default`.
    Noneable {
        none_phrase: &'static str,
        default: i64,
        min: i64,
        max: i64,
        unit: Option<&'static str>,
    },
    Float {
        min: f64,
        max: f64,
    },
    Choice(&'static [&'static str]),
    /// Named GUI sessions plus the blank-page startup choice.
    SavedSession,
    /// Media, preview and Client API viewing-statistic canvases.
    CanvasTicks,
    GallerySource,
    Text,
    /// An editable folder path with the shared native directory picker.
    Directory,
    NoneableText {
        none_phrase: &'static str,
    },
    /// A time shown as fields of these units, at least `min` seconds.
    Duration {
        units: &'static [Unit],
        min: f64,
    },
    /// A reference NoneableTimeDeltaWidget, including millisecond fields.
    NoneableDuration {
        units: &'static [Unit],
        min: f64,
        default: f64,
        none_phrase: &'static str,
    },
    /// A number in `number`'s range, `per` (the text between), then a time
    /// as a duration's.
    Velocity {
        number: (i64, i64),
        per: &'static str,
        units: &'static [Unit],
        min: f64,
    },
    /// A file sort, of the types a page's sort control offers.
    Sort,
    /// A collect, of the choices a page's collect control offers.
    Collect,
    /// A tag list's sort: its type, its order and its grouping.
    TagSort,
    /// Checker options: a "checker options" button opening their editor
    /// (`checker_options`).
    Checker,
    /// A button opening the transactional favourites list editor.
    RegexFavourites,
    /// Inline ordered advanced file-deletion reason queue.
    DeletionReasons,
    NamespaceColours,
    FrameLocations,
    /// Importable current file domains, edited in a child selector.
    LocalLocation,
    /// The detached registered external-call table.
    ExternalCalls,
    OpenExternally,
    /// A detached tag list editor sharing write autocomplete.
    FavouriteTags,
    MostUsedTags,
    RelatedWeights,
    /// The transactional manager page, including simple-mode presentation.
    ImportOptions,
    NamespaceSorts,
    TagBanner(crate::tag_banner::Target),
    /// Inline staged command-palette provider queue.
    ProviderOrder,
    /// Real tag services, optionally including all known tags.
    TagService {
        combined: bool,
    },
}

/// A tag sort's types, as the reference's control names them
/// (`sort_type_str_lookup`), in its order.
pub const TAG_SORT_TYPES: [(&str, TagSortType); 3] = [
    ("sort by tag", TagSortType::Tag),
    ("sort by subtag", TagSortType::Subtag),
    ("sort by count", TagSortType::Count),
];

/// A tag sort's orders for text (ascending first) and for counts (most
/// first), as the reference's control names them.
pub const TAG_SORT_TEXT_ORDERS: [&str; 2] = ["a-z", "z-a"];
pub const TAG_SORT_COUNT_ORDERS: [&str; 2] = ["most first", "fewest first"];

/// A tag sort's groupings (`group_by_str_lookup`), in its order.
pub const TAG_SORT_GROUPS: [(&str, TagGroupBy); 3] = [
    ("no grouping", TagGroupBy::Nothing),
    ("namespace (a-z)", TagGroupBy::NamespaceAz),
    ("namespace (user)", TagGroupBy::NamespaceUser),
];

/// The order choices for a tag sort's type, and which is chosen.
pub fn tag_sort_orders(sort: &TagSort) -> ([&'static str; 2], usize) {
    if sort.sort_type == TagSortType::Count {
        (TAG_SORT_COUNT_ORDERS, usize::from(sort.ascending))
    } else {
        (TAG_SORT_TEXT_ORDERS, usize::from(!sort.ascending))
    }
}

/// A tag sort with its type, order or grouping chosen (each by its place
/// among the control's choices): a type takes its order button's first
/// choice ("a-z", "most first"), as the reference's separate buttons
/// start.
pub fn tag_sort_chosen(sort: &TagSort, part: usize, index: usize) -> TagSort {
    let mut out = *sort;
    match part {
        0 => {
            if let Some((_, sort_type)) = TAG_SORT_TYPES.get(index) {
                out.sort_type = *sort_type;
                out.ascending = *sort_type != TagSortType::Count;
            }
        }
        1 => {
            out.ascending = if sort.sort_type == TagSortType::Count {
                index == 1
            } else {
                index == 0
            };
        }
        _ => {
            if let Some((_, group_by)) = TAG_SORT_GROUPS.get(index) {
                out.group_by = *group_by;
            }
        }
    }
    out
}

/// A field of a time's control, as the reference's `TimeDeltaWidget`
/// shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Days,
    Hours,
    Minutes,
    Seconds,
    Milliseconds,
}

impl Unit {
    /// The text after its field.
    pub fn label(self) -> &'static str {
        match self {
            Self::Days => "days",
            Self::Hours => "hours",
            Self::Minutes => "minutes",
            Self::Seconds => "seconds",
            Self::Milliseconds => "ms",
        }
    }

    /// Its name (`_show_<name>` in the reference).
    pub fn name(self) -> &'static str {
        match self {
            Self::Milliseconds => "milliseconds",
            unit => unit.label(),
        }
    }

    fn seconds(self) -> f64 {
        match self {
            Self::Days => 86400.0,
            Self::Hours => 3600.0,
            Self::Minutes => 60.0,
            Self::Seconds => 1.0,
            Self::Milliseconds => 0.001,
        }
    }

    /// Its field's largest value.
    pub fn max(self) -> i64 {
        match self {
            Self::Days => 3523,
            Self::Hours => 23,
            Self::Minutes | Self::Seconds => 59,
            Self::Milliseconds => 999,
        }
    }
}

/// A time's fields, as the reference's control sets them (`SetValue`):
/// each unit takes what it can of what the larger ones leave (held to its
/// field's largest value).
#[allow(clippy::cast_possible_truncation)] // (whole numbers, in range)
pub fn duration_fields(seconds: f64, units: &[Unit]) -> Vec<i64> {
    let mut left = seconds.max(0.0);
    units
        .iter()
        .map(|&unit| {
            let n = if unit == Unit::Milliseconds {
                (left * 1000.0).round()
            } else {
                // (a hair over, so 0.3 / 0.1 is 3)
                ((left + 1e-9) / unit.seconds()).floor()
            };
            left = (left - n * unit.seconds()).max(0.0);
            (n as i64).min(unit.max())
        })
        .collect()
}

/// Fields of a NoneableTimeDeltaWidget opened from persisted milliseconds.
/// Qt truncates its fractional millisecond remainder on SetValue; regular
/// duration editing retains its separately entered fields until Apply.
pub fn noneable_duration_fields(seconds: f64, units: &[Unit]) -> Vec<i64> {
    let mut remaining = seconds.max(0.0);
    units
        .iter()
        .map(|&unit| {
            let number = if unit == Unit::Milliseconds {
                (remaining * 1000.0) as i64
            } else {
                let number = (remaining / unit.seconds()).floor() as i64;
                remaining %= unit.seconds();
                number
            };
            number.min(unit.max())
        })
        .collect()
}

/// The time these fields say.
pub fn duration_seconds(fields: &[i64], units: &[Unit]) -> f64 {
    units
        .iter()
        .zip(fields)
        .map(|(unit, &n)| n as f64 * unit.seconds())
        .sum()
}

type Get = Rc<dyn Fn(&Settings) -> Value>;
/// Set the value, or say why it wasn't set.
type Set = Rc<dyn Fn(&mut Settings, &Value) -> Result<(), String>>;

/// An option: its label (the reference's, before its control), control,
/// and where its value is kept.
#[derive(Clone)]
pub struct Opt {
    pub label: &'static str,
    pub kind: Kind,
    pub get: Get,
    pub set: Set,
    pub enabled: fn(&Settings) -> bool,
}

impl std::fmt::Debug for Opt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Opt")
            .field("label", &self.label)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// What a page lays out: options, and titled boxes of them.
#[derive(Debug, Clone)]
pub enum Item {
    Opt(Opt),
    Box(&'static str, Vec<Item>),
}

#[derive(Debug, Clone)]
pub struct Page {
    pub name: &'static str,
    pub items: Vec<Item>,
}

impl Page {
    /// Its options, in order.
    pub fn options(&self) -> Vec<&Opt> {
        fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Opt>) {
            for item in items {
                match item {
                    Item::Opt(opt) => out.push(opt),
                    Item::Box(_, items) => walk(items, out),
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.items, &mut out);
        out
    }
}

fn wrong(label: &str) -> String {
    format!("{label}: not a value it can have")
}

fn opt(label: &'static str, kind: Kind, get: Get, set: Set) -> Item {
    Item::Opt(Opt {
        label,
        kind,
        get,
        set,
        enabled: |_| true,
    })
}

fn enabled(mut item: Item, predicate: fn(&Settings) -> bool) -> Item {
    if let Item::Opt(option) = &mut item {
        option.enabled = predicate;
    }
    item
}

/// Tag service choices in reference order: all known tags first when offered,
/// then local tags and repositories, each ordered by lowercase name.
pub fn tag_service_choices(
    store: &hydrus_store::Store,
    combined: bool,
) -> Vec<(hydrus_core::ServiceKey, String)> {
    use hydrus_core::service::{ServiceType, builtin_keys};
    let snapshot = store.snapshot();
    let mut choices = Vec::new();
    if combined && let Ok(service) = snapshot.services.builtin(builtin_keys::COMBINED_TAG) {
        choices.push((service.key.clone(), service.name.clone()));
    }
    for kind in [ServiceType::LocalTag, ServiceType::TagRepository] {
        let mut services = snapshot.services.of_type(kind).collect::<Vec<_>>();
        services.sort_by_key(|service| service.name.to_lowercase());
        choices.extend(
            services
                .into_iter()
                .map(|service| (service.key.clone(), service.name.clone())),
        );
    }
    choices
}

fn tag_service(
    label: &'static str,
    combined: bool,
    get: fn(&Settings) -> hydrus_core::ServiceKey,
    set: fn(&mut Settings, hydrus_core::ServiceKey),
    enabled: fn(&Settings) -> bool,
) -> Item {
    Item::Opt(Opt {
        label,
        kind: Kind::TagService { combined },
        get: Rc::new(move |settings| Value::TagService(get(settings))),
        set: Rc::new(move |settings, value| match value {
            Value::TagService(service) => {
                set(settings, service.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
        enabled,
    })
}

fn check(label: &'static str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool)) -> Item {
    opt(
        label,
        Kind::Check,
        Rc::new(move |s| Value::Check(get(s))),
        Rc::new(move |s, v| match v {
            Value::Check(b) => {
                set(s, *b);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn int(
    label: &'static str,
    (min, max): (i64, i64),
    get: fn(&Settings) -> i64,
    set: fn(&mut Settings, i64),
) -> Item {
    opt(
        label,
        Kind::Int { min, max },
        Rc::new(move |s| Value::Int(get(s))),
        Rc::new(move |s, v| match v {
            Value::Int(n) => {
                set(s, (*n).clamp(min, max));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// A noneable number's control: its none phrase, the number it shows when
/// none, its range and unit.
#[derive(Clone, Copy)]
struct NoneableKind {
    none_phrase: &'static str,
    default: i64,
    range: (i64, i64),
    unit: Option<&'static str>,
}

const fn none(
    none_phrase: &'static str,
    default: i64,
    range: (i64, i64),
    unit: Option<&'static str>,
) -> NoneableKind {
    NoneableKind {
        none_phrase,
        default,
        range,
        unit,
    }
}

fn noneable(
    label: &'static str,
    kind: NoneableKind,
    get: fn(&Settings) -> Option<i64>,
    set: fn(&mut Settings, Option<i64>),
) -> Item {
    let (min, max) = kind.range;
    opt(
        label,
        Kind::Noneable {
            none_phrase: kind.none_phrase,
            default: kind.default,
            min,
            max,
            unit: kind.unit,
        },
        Rc::new(move |s| Value::Noneable(get(s))),
        Rc::new(move |s, v| match v {
            Value::Noneable(n) => {
                set(s, n.map(|n| n.clamp(min, max)));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// A float as the reference's spin boxes show it ("12.0").
fn float_text(f: f64) -> String {
    format!("{f:?}")
}

fn float(
    label: &'static str,
    (min, max): (f64, f64),
    get: fn(&Settings) -> f64,
    set: fn(&mut Settings, f64),
) -> Item {
    opt(
        label,
        Kind::Float { min, max },
        Rc::new(move |s| Value::Float(float_text(get(s)))),
        Rc::new(move |s, v| match v {
            Value::Float(text) => {
                let name = label.trim_end();
                let f: f64 = text
                    .trim()
                    .parse()
                    .map_err(|_| format!("{name} \"{text}\" is not a number"))?;
                if !(min..=max).contains(&f) {
                    return Err(format!("{name} must be from {min} to {max}"));
                }
                set(s, f);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn rating_size(
    label: &'static str,
    (min, max): (f64, f64),
    get: fn(&Settings) -> f64,
    set: fn(&mut Settings, f64),
) -> Item {
    opt(
        label,
        Kind::Float { min, max },
        Rc::new(move |s| Value::Float(float_text(get(s)))),
        Rc::new(move |s, v| match v {
            Value::Float(text) => {
                let value = text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| format!("{} \"{text}\" is not a number", label.trim_end()))?;
                set(
                    s,
                    crate::rating_sizes::round_hundredths(value.clamp(min, max)).clamp(min, max),
                );
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn choice(
    label: &'static str,
    items: &'static [&'static str],
    get: fn(&Settings) -> usize,
    set: fn(&mut Settings, usize),
) -> Item {
    opt(
        label,
        Kind::Choice(items),
        Rc::new(move |s| Value::Choice(get(s))),
        Rc::new(move |s, v| match v {
            Value::Choice(i) if *i < items.len() => {
                set(s, *i);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn sort(
    label: &'static str,
    get: fn(&Settings) -> PageSort,
    set: fn(&mut Settings, PageSort),
) -> Item {
    opt(
        label,
        Kind::Sort,
        Rc::new(move |s| Value::Sort(get(s))),
        Rc::new(move |s, v| match v {
            Value::Sort(sort) => {
                set(s, sort.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn collect(
    label: &'static str,
    get: fn(&Settings) -> PageCollect,
    set: fn(&mut Settings, PageCollect),
) -> Item {
    opt(
        label,
        Kind::Collect,
        Rc::new(move |s| Value::Collect(get(s))),
        Rc::new(move |s, v| match v {
            Value::Collect(collect) => {
                set(s, collect.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn checker(
    label: &'static str,
    get: fn(&Settings) -> CheckerOptions,
    set: fn(&mut Settings, CheckerOptions),
) -> Item {
    opt(
        label,
        Kind::Checker,
        Rc::new(move |s| Value::Checker(get(s))),
        Rc::new(move |s, v| match v {
            Value::Checker(options) => {
                set(s, options.clone());
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn tag_sort(
    label: &'static str,
    get: fn(&Settings) -> TagSort,
    set: fn(&mut Settings, TagSort),
) -> Item {
    opt(
        label,
        Kind::TagSort,
        Rc::new(move |s| Value::TagSort(get(s))),
        Rc::new(move |s, v| match v {
            Value::TagSort(sort) => {
                set(s, *sort);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn text(
    label: &'static str,
    get: fn(&Settings) -> String,
    set: fn(&mut Settings, &str) -> Result<(), String>,
) -> Item {
    opt(
        label,
        Kind::Text,
        Rc::new(move |s| Value::Text(get(s))),
        Rc::new(move |s, v| match v {
            Value::Text(t) => set(s, t),
            _ => Err(wrong(label)),
        }),
    )
}

fn noneable_text(
    label: &'static str,
    none_phrase: &'static str,
    get: fn(&Settings) -> Option<String>,
    set: fn(&mut Settings, Option<String>),
) -> Item {
    noneable_text_default(label, none_phrase, "", get, set)
}
fn noneable_text_default(
    label: &'static str,
    none_phrase: &'static str,
    default_text: &'static str,
    get: fn(&Settings) -> Option<String>,
    set: fn(&mut Settings, Option<String>),
) -> Item {
    opt(
        label,
        Kind::NoneableText { none_phrase },
        Rc::new(move |s| {
            let value = get(s);
            Value::NoneableText {
                none: value.is_none(),
                text: value.unwrap_or_else(|| default_text.to_owned()),
            }
        }),
        Rc::new(move |s, v| match v {
            Value::NoneableText { none, text } => {
                set(s, (!none).then(|| text.clone()));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// The units a time shows, and its least.
const fn time(units: &'static [Unit], min: f64) -> (&'static [Unit], f64) {
    (units, min)
}

fn duration(
    label: &'static str,
    (units, min): (&'static [Unit], f64),
    get: fn(&Settings) -> f64,
    set: fn(&mut Settings, f64),
) -> Item {
    opt(
        label,
        Kind::Duration { units, min },
        Rc::new(move |s| Value::Duration(get(s))),
        Rc::new(move |s, v| match v {
            // (less than its least is its least, as the reference's control
            // makes it)
            Value::Duration(d) => {
                set(s, d.max(min));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn noneable_duration(
    label: &'static str,
    units: &'static [Unit],
    min: f64,
    default: f64,
    none_phrase: &'static str,
    get: fn(&Settings) -> Option<u64>,
    set: fn(&mut Settings, Option<u64>),
) -> Item {
    opt(
        label,
        Kind::NoneableDuration {
            units,
            min,
            default,
            none_phrase,
        },
        Rc::new(move |s| {
            let value = get(s);
            Value::NoneableDuration {
                none: value.is_none(),
                seconds: value.map_or(default, |ms| ms as f64 / 1000.0),
            }
        }),
        Rc::new(move |s, v| match v {
            Value::NoneableDuration { none, seconds } => {
                // Reference MillisecondiseS truncates this float, including 1.001s.
                set(s, (!none).then_some((seconds.max(min) * 1000.0) as u64));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

fn velocity(
    label: &'static str,
    (number, per): ((i64, i64), &'static str),
    (units, min): (&'static [Unit], f64),
    get: fn(&Settings) -> (i64, f64),
    set: fn(&mut Settings, i64, f64),
) -> Item {
    opt(
        label,
        Kind::Velocity {
            number,
            per,
            units,
            min,
        },
        Rc::new(move |s| {
            let (n, seconds) = get(s);
            Value::Velocity(n, seconds)
        }),
        Rc::new(move |s, v| match v {
            Value::Velocity(n, seconds) => {
                set(s, (*n).clamp(number.0, number.1), seconds.max(min));
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// A duplicate comparison's score weight (-100 to 100).
fn score(label: &'static str, get: fn(&Settings) -> i32, set: fn(&mut Settings, i32)) -> Item {
    opt(
        label,
        Kind::Int {
            min: -100,
            max: 100,
        },
        Rc::new(move |s| Value::Int(i64::from(get(s)))),
        Rc::new(move |s, v| match v {
            Value::Int(n) => {
                set(s, (*n).clamp(-100, 100) as i32);
                Ok(())
            }
            _ => Err(wrong(label)),
        }),
    )
}

/// Whole seconds (as the store keeps them) from a time.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // (a time, rounded)
fn whole(seconds: f64) -> u64 {
    seconds.round().max(0.0) as u64
}

fn boxed(title: &'static str, items: Vec<Item>) -> Item {
    Item::Box(title, items)
}

/// Exact command-palette control order and bounds from the reference options panel.
fn command_palette_page() -> Page {
    Page {
        name: "command palette",
        items: vec![boxed(
            "command palette",
            vec![
                check(
                    "Initially show all page results:",
                    |s| s.command_palette.initially_show_pages,
                    |s, v| s.command_palette.initially_show_pages = v,
                ),
                check(
                    "Initially show page history results:",
                    |s| s.command_palette.initially_show_history,
                    |s, v| s.command_palette.initially_show_history = v,
                ),
                check(
                    "Initially show favourite search results:",
                    |s| s.command_palette.initially_show_favourites,
                    |s, v| s.command_palette.initially_show_favourites = v,
                ),
                int(
                    "Start searching when this many characters have been typed:",
                    (1, 64),
                    |s| i64::try_from(s.command_palette.threshold).unwrap_or(64),
                    |s, v| s.command_palette.threshold = usize::try_from(v).unwrap_or(1),
                ),
                noneable(
                    "Max page results to show:",
                    none("no limit", 10, (1, 1_000_000), None),
                    |s| {
                        s.command_palette
                            .page_limit
                            .map(|n| i64::try_from(n).unwrap_or(1_000_000))
                    },
                    |s, v| s.command_palette.page_limit = v.and_then(|n| usize::try_from(n).ok()),
                ),
                noneable(
                    "Max page history to show:",
                    none("no limit", 10, (1, 1_000_000), None),
                    |s| {
                        s.command_palette
                            .history_limit
                            .map(|n| i64::try_from(n).unwrap_or(1_000_000))
                    },
                    |s, v| {
                        s.command_palette.history_limit = v.and_then(|n| usize::try_from(n).ok());
                    },
                ),
                noneable(
                    "Max favourite searches to show:",
                    none("no limit", 10, (1, 1_000_000), None),
                    |s| {
                        s.command_palette
                            .favourite_limit
                            .map(|n| i64::try_from(n).unwrap_or(1_000_000))
                    },
                    |s, v| {
                        s.command_palette.favourite_limit = v.and_then(|n| usize::try_from(n).ok());
                    },
                ),
                check(
                    "Include \"page of pages\" page results:",
                    |s| s.command_palette.show_notebooks,
                    |s, v| s.command_palette.show_notebooks = v,
                ),
                check(
                    "Open favourite searches in a new page:",
                    |s| s.command_palette.favourites_new_page,
                    |s, v| s.command_palette.favourites_new_page = v,
                ),
                check(
                    "ADVANCED: Search main menubar:",
                    |s| s.command_palette.show_main_menu,
                    |s, v| s.command_palette.show_main_menu = v,
                ),
                check(
                    "ADVANCED: Search media menu:",
                    |s| s.command_palette.show_media_menu,
                    |s, v| s.command_palette.show_media_menu = v,
                ),
                boxed(
                    "search provider order",
                    vec![opt(
                        "You can re-order or remove search providers from the palette here. Any removed providers can be re-added.",
                        Kind::ProviderOrder,
                        Rc::new(|s| Value::ProviderOrder(s.command_palette.provider_order.clone())),
                        Rc::new(|s, v| match v {
                            Value::ProviderOrder(order) => {
                                s.command_palette.provider_order.clone_from(order);
                                Ok(())
                            }
                            _ => Err(wrong("search provider order")),
                        }),
                    )],
                ),
            ],
        )],
    }
}

/// The downloaders' waits after an error (`TimeDeltaButton`s of days to
/// seconds).
const ERROR_DELAY: &[Unit] = &[Unit::Days, Unit::Hours, Unit::Minutes, Unit::Seconds];

/// `CC.page_file_count_display_string_lookup`, in the order the reference
/// lists them.
const FILE_COUNTS: &[&str] = &[
    "for all pages",
    "for import pages",
    "for no pages",
    "for all pages, but only if greater than zero",
];

fn file_count_index(display: FileCountDisplay) -> usize {
    match display {
        FileCountDisplay::All => 0,
        FileCountDisplay::OnlyImporters => 1,
        FileCountDisplay::None => 2,
        FileCountDisplay::AllIfAny => 3,
    }
}

fn file_count_display(index: usize) -> FileCountDisplay {
    match index {
        0 => FileCountDisplay::All,
        1 => FileCountDisplay::OnlyImporters,
        2 => FileCountDisplay::None,
        _ => FileCountDisplay::AllIfAny,
    }
}

/// Numbers (slideshow durations, media zooms) as the reference writes them
/// ("1.0,5.0,10.0").
fn numbers_text(numbers: &[f64]) -> String {
    numbers
        .iter()
        .map(|d| float_text(*d))
        .collect::<Vec<_>>()
        .join(",")
}

/// The reference's parse of comma-separated numbers (slideshow durations,
/// media zooms: `UpdateOptions`): each a number, those above zero kept;
/// `what` says what couldn't be read.
fn parse_numbers(text: &str, what: &str) -> Result<Vec<f64>, String> {
    let numbers: Vec<f64> = text
        .split(',')
        .map(|part| part.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("Could not parse those {what}, so they were not saved!"))?;
    Ok(numbers.into_iter().filter(|d| *d > 0.0).collect())
}

/// `ClientGUICanvasMedia.ZOOM_CENTERPOINT_TYPES`, as the reference lists
/// them.
const ZOOM_CENTRES: &[&str] = &[
    "viewer center",
    "mouse (or viewer center if mouse outside)",
    "media center",
    "media top-left",
];
const ZOOM_CENTRE_ORDER: [ZoomCentre; 4] = [
    ZoomCentre::ViewerCentre,
    ZoomCentre::Mouse,
    ZoomCentre::MediaCentre,
    ZoomCentre::MediaTopLeft,
];

/// `MEDIA_VIEWER_ZOOM_TYPES`, as the reference lists them.
const ZOOM_TYPES: &[&str] = &[
    "default for filetype",
    "100% zoom",
    "canvas fit",
    "fill horizontally",
    "fill vertically",
    "canvas fill",
];
const ZOOM_TYPE_ORDER: [ZoomType; 6] = [
    ZoomType::DefaultForFiletype,
    ZoomType::Full,
    ZoomType::Canvas,
    ZoomType::FillX,
    ZoomType::FillY,
    ZoomType::FillAuto,
];

/// `HydrusImageHandling.thumbnail_scale_str_lookup`, as the reference lists
/// them.
const THUMBNAIL_SCALES: &[&str] = &["scale down only", "scale to fit", "scale to fill"];
const THUMBNAIL_SCALE_ORDER: [ThumbnailScale; 3] = [
    ThumbnailScale::DownOnly,
    ThumbnailScale::ToFit,
    ThumbnailScale::ToFill,
];

/// `has_transparency_strictness_string_lookup`, as the reference lists
/// them: the strictest (2) first.
const TRANSPARENCY: &[&str] = &[
    "it has a transparency channel that a human might recognise",
    "it has a transparency channel that is not completely transparent or opaque",
    "it has a transparency channel",
];

fn signed(n: Option<u64>) -> Option<i64> {
    n.map(|n| i64::try_from(n).unwrap_or(i64::MAX))
}

/// Choice order from GUISessionsPanel: blank first, ensure last session exists.
pub fn session_choices(store: &hydrus_store::Store) -> Vec<(Option<String>, String)> {
    let mut names = store
        .read(hydrus_store::sessions::names)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    if !names
        .iter()
        .any(|name| name == hydrus_store::sessions::LAST_SESSION)
    {
        names.insert(0, hydrus_store::sessions::LAST_SESSION.into());
    }
    let mut choices = vec![(None, "just a blank page".into())];
    choices.extend(names.into_iter().map(|name| (Some(name.clone()), name)));
    choices
}

fn unsigned(n: Option<i64>) -> Option<u64> {
    n.map(|n| u64::try_from(n).unwrap_or(0))
}

/// The pages, in the reference's order: sorted by name, then "advanced".
/// Some ranges are `settings`' (as the reference's are the options' when
/// its dialog opens).
#[allow(clippy::too_many_lines)] // (a table)
pub fn pages(settings: &Settings) -> Vec<Page> {
    // (the thumbnails' rating sizes go up to their width)
    let thumbnail_width = f64::from(settings.thumbnails.bounding_width);
    let advanced = settings.advanced.0;
    let timeout_range = if advanced { (1, 30 * 86400) } else { (3, 600) };
    let retry_range = if advanced { (1, 30 * 86400) } else { (3, 1800) };
    let error_delay_min = if advanced { 1.0 } else { 600.0 };
    let page = |name, items| Page { name, items };
    let mut pages = vec![
        page(
            "audio",
            vec![text(
                "Label for files with audio: ",
                |s| s.info_line.has_audio_label.clone(),
                |s, t| {
                    t.clone_into(&mut s.info_line.has_audio_label);
                    Ok(())
                },
            )],
        ),
        command_palette_page(),
        page(
            "connection",
            vec![
                boxed(
                    "general",
                    vec![
                        int(
                            "max connection attempts allowed per request: ",
                            (1, 10),
                            |s| i64::from(s.network.max_connection_attempts),
                            |s, v| s.network.max_connection_attempts = v as u32,
                        ),
                        int(
                            "max retries allowed per request: ",
                            (1, 10),
                            |s| i64::from(s.network.max_get_attempts),
                            |s, v| s.network.max_get_attempts = v as u32,
                        ),
                        int(
                            "network timeout (seconds): ",
                            timeout_range,
                            |s| s.network.network_timeout as i64,
                            |s, v| s.network.network_timeout = v as u64,
                        ),
                        int(
                            "connection error retry wait (seconds): ",
                            retry_range,
                            |s| s.network.connection_error_wait_time as i64,
                            |s, v| s.network.connection_error_wait_time = v as u64,
                        ),
                        int(
                            "serverside bandwidth retry wait (seconds): ",
                            retry_range,
                            |s| s.network.serverside_bandwidth_wait_time as i64,
                            |s, v| s.network.serverside_bandwidth_wait_time = v as u64,
                        ),
                        velocity(
                            "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ",
                            ((0, 100), "errors within"),
                            time(&[Unit::Hours, Unit::Minutes, Unit::Seconds], 30.0),
                            |s| {
                                (
                                    s.network.domain_error_number as i64,
                                    s.network.domain_error_window as f64,
                                )
                            },
                            |s, n, seconds| {
                                s.network.domain_error_number = n as usize;
                                s.network.domain_error_window = whole(seconds) as i64;
                            },
                        ),
                        int(
                            "max number of simultaneous active network jobs: ",
                            (1, if advanced { 1000 } else { 30 }),
                            |s| s.network.max_jobs as i64,
                            |s, v| s.network.max_jobs = v as usize,
                        ),
                        int(
                            "max number of simultaneous active network jobs per domain: ",
                            (1, if advanced { 100 } else { 5 }),
                            |s| s.network.max_jobs_per_domain as i64,
                            |s, v| s.network.max_jobs_per_domain = v as usize,
                        ),
                        check(
                            "DEBUG: do not verify regular https traffic:",
                            |s| !s.network.verify_https,
                            |s, v| s.network.verify_https = !v,
                        ),
                    ],
                ),
                boxed(
                    "proxy settings",
                    vec![
                        noneable_text(
                            "http: ",
                            "none",
                            |s| s.network.http_proxy.clone(),
                            |s, v| s.network.http_proxy = v,
                        ),
                        noneable_text(
                            "https: ",
                            "none",
                            |s| s.network.https_proxy.clone(),
                            |s, v| s.network.https_proxy = v,
                        ),
                        noneable_text(
                            "no_proxy: ",
                            "none",
                            |s| s.network.no_proxy.clone(),
                            |s, v| s.network.no_proxy = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "downloading",
            vec![
                boxed(
                    "gallery downloader",
                    vec![
                        opt(
                            "Default download source:",
                            Kind::GallerySource,
                            Rc::new(|s| Value::GallerySource(s.gallery.gug.clone())),
                            Rc::new(|s, v| match v {
                                Value::GallerySource(value) => {
                                    s.gallery.gug.clone_from(value);
                                    Ok(())
                                }
                                _ => Err("expected a gallery source".into()),
                            }),
                        ),
                        int(
                            "Additional fixed time (in seconds) to wait between gallery page fetches:",
                            (1, 3600),
                            |s| s.bandwidth.gallery_page_wait_pages,
                            |s, v| s.bandwidth.gallery_page_wait_pages = v,
                        ),
                        noneable(
                            "By default, stop searching once this many files are found:",
                            none("no limit", 2000, (1, 1_000_000), None),
                            |s| signed(s.gallery.file_limit),
                            |s, v| s.gallery.file_limit = unsigned(v),
                        ),
                        check(
                            "If new query entered and no current highlight, highlight the new query:",
                            |s| s.downloader_pages.highlight_new_query,
                            |s, v| s.downloader_pages.highlight_new_query = v,
                        ),
                        check(
                            "Force file downloads to occur quickly after Post URL fetches:",
                            |s| s.bandwidth.override_on_file_urls_from_posts,
                            |s, v| s.bandwidth.override_on_file_urls_from_posts = v,
                        ),
                    ],
                ),
                boxed(
                    "subscriptions",
                    vec![
                        int(
                            "Additional fixed time (in seconds) to wait between gallery page fetches:",
                            (1, 3600),
                            |s| s.bandwidth.gallery_page_wait_subscriptions,
                            |s, v| s.bandwidth.gallery_page_wait_subscriptions = v,
                        ),
                        int(
                            "Maximum number of subscriptions that can sync simultaneously:",
                            (1, 100),
                            |s| i64::from(s.network.max_simultaneous_subscriptions),
                            |s, value| {
                                s.network.max_simultaneous_subscriptions =
                                    u32::try_from(value).unwrap_or(1);
                            },
                        ),
                        noneable(
                            "If a subscription has this many failed file imports, stop and continue later:",
                            none("no limit", 5, (1, 1_000_000), Some("errors")),
                            |s| signed(s.network.subscription_file_error_cancel_threshold),
                            |s, value| {
                                s.network.subscription_file_error_cancel_threshold =
                                    unsigned(value);
                            },
                        ),
                        check(
                            "Sync subscriptions in random order:",
                            |s| s.network.process_subs_in_random_order,
                            |s, v| s.network.process_subs_in_random_order = v,
                        ),
                        checker(
                            "Default subscription checker options:",
                            |s| s.checker_defaults.subscriptions.clone(),
                            |s, v| s.checker_defaults.subscriptions = v,
                        ),
                    ],
                ),
                boxed(
                    "watchers",
                    vec![
                        int(
                            "Additional fixed time (in seconds) to wait between watcher checks:",
                            (1, 3600),
                            |s| s.bandwidth.watcher_page_wait,
                            |s, v| s.bandwidth.watcher_page_wait = v,
                        ),
                        check(
                            "If new watcher entered and no current highlight, highlight the new watcher:",
                            |s| s.downloader_pages.highlight_new_watcher,
                            |s, v| s.downloader_pages.highlight_new_watcher = v,
                        ),
                        checker(
                            "Default watcher checker options:",
                            |s| s.checker_defaults.watchers.clone(),
                            |s, v| s.checker_defaults.watchers = v,
                        ),
                    ],
                ),
                boxed(
                    "misc",
                    vec![
                        text(
                            "Pause character:",
                            |s| s.downloader_pages.pause_character.clone(),
                            |s, t| {
                                t.clone_into(&mut s.downloader_pages.pause_character);
                                Ok(())
                            },
                        ),
                        text(
                            "Stop character:",
                            |s| s.downloader_pages.stop_character.clone(),
                            |s, t| {
                                t.clone_into(&mut s.downloader_pages.stop_character);
                                Ok(())
                            },
                        ),
                        check(
                            "Show a 'N' (for 'new') count on short file import summaries:",
                            |s| s.page_names.short_summary_new,
                            |s, v| s.page_names.short_summary_new = v,
                        ),
                        check(
                            "Show a 'D' (for 'deleted') count on short file import summaries:",
                            |s| s.page_names.short_summary_deleted,
                            |s, v| s.page_names.short_summary_deleted = v,
                        ),
                        duration(
                            "Delay time on a gallery/watcher network error:",
                            time(ERROR_DELAY, error_delay_min),
                            |s| s.network.downloader_network_error_delay as f64,
                            |s, v| s.network.downloader_network_error_delay = whole(v),
                        ),
                        duration(
                            "Delay time on a subscription network error:",
                            time(ERROR_DELAY, error_delay_min),
                            |s| s.network.subscription_network_error_delay as f64,
                            |s, v| s.network.subscription_network_error_delay = whole(v) as i64,
                        ),
                        duration(
                            "Delay time on a subscription other error:",
                            time(ERROR_DELAY, error_delay_min),
                            |s| s.network.subscription_other_error_delay as f64,
                            |s, v| s.network.subscription_other_error_delay = whole(v) as i64,
                        ),
                        check(
                            "DEBUG: remove leading double-slashes from URL paths:",
                            |s| s.url_classes.collapse_leading_slashes,
                            |s, v| s.url_classes.collapse_leading_slashes = v,
                        ),
                        check(
                            "DEBUG: consider %20 the same as space in downloader query text inputs:",
                            |s| s.network.gug_percent_twenty_is_space,
                            |s, v| s.network.gug_percent_twenty_is_space = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "duplicates",
            vec![
                boxed(
                    "open in a new duplicates filter page",
                    vec![check(
                        "Set to \"combined local file domains\" when hitting \"Open files in a new duplicates filter page\":",
                        |s| s.page_settings.duplicate_filter_uses_all_my_files,
                        |s, v| s.page_settings.duplicate_filter_uses_all_my_files = v,
                    )],
                ),
                boxed(
                    "duplicate filter batches",
                    vec![
                        int(
                            "Max size of duplicate filter pair batches (in mixed mode):",
                            (5, 1024),
                            |s| i64::from(s.duplicate_filter.max_batch_size),
                            |s, v| s.duplicate_filter.max_batch_size = v as u32,
                        ),
                        noneable(
                            "Auto-commit completed batches of this size or smaller:",
                            none("no, always confirm", 1, (1, 50), None),
                            |s| s.duplicate_filter.auto_commit_batch_size.map(i64::from),
                            |s, v| s.duplicate_filter.auto_commit_batch_size = v.map(|n| n as u32),
                        ),
                    ],
                ),
                boxed(
                    "duplicate filter comparison score weights",
                    vec![
                        score(
                            "Score for jpeg with non-trivially higher jpeg quality:",
                            |s| s.duplicate_filter.scores.higher_jpeg_quality,
                            |s, v| s.duplicate_filter.scores.higher_jpeg_quality = v,
                        ),
                        score(
                            "Score for jpeg with significantly higher jpeg quality:",
                            |s| s.duplicate_filter.scores.much_higher_jpeg_quality,
                            |s, v| s.duplicate_filter.scores.much_higher_jpeg_quality = v,
                        ),
                        score(
                            "Score for file with non-trivially higher filesize:",
                            |s| s.duplicate_filter.scores.higher_filesize,
                            |s, v| s.duplicate_filter.scores.higher_filesize = v,
                        ),
                        score(
                            "Score for file with significantly higher filesize:",
                            |s| s.duplicate_filter.scores.much_higher_filesize,
                            |s, v| s.duplicate_filter.scores.much_higher_filesize = v,
                        ),
                        score(
                            "Score for file with higher resolution (as num pixels):",
                            |s| s.duplicate_filter.scores.higher_resolution,
                            |s, v| s.duplicate_filter.scores.higher_resolution = v,
                        ),
                        score(
                            "Score for file with significantly higher resolution (as num pixels):",
                            |s| s.duplicate_filter.scores.much_higher_resolution,
                            |s, v| s.duplicate_filter.scores.much_higher_resolution = v,
                        ),
                        score(
                            "Score for file with more tags:",
                            |s| s.duplicate_filter.scores.more_tags,
                            |s, v| s.duplicate_filter.scores.more_tags = v,
                        ),
                        score(
                            "Score for file with non-trivially earlier import time:",
                            |s| s.duplicate_filter.scores.older,
                            |s, v| s.duplicate_filter.scores.older = v,
                        ),
                        score(
                            "Score for file with 'nicer' resolution ratio:",
                            |s| s.duplicate_filter.scores.nicer_ratio,
                            |s, v| s.duplicate_filter.scores.nicer_ratio = v,
                        ),
                        score(
                            "Score for file with audio:",
                            |s| s.duplicate_filter.scores.has_audio,
                            |s, v| s.duplicate_filter.scores.has_audio = v,
                        ),
                    ],
                ),
                boxed(
                    "colours",
                    vec![
                        noneable(
                            "background light/dark switch intensity for A:",
                            none("do not change", 3, (1, 9), None),
                            |s| s.duplicate_colours.intensity_a.map(i64::from),
                            |s, v| s.duplicate_colours.intensity_a = v.map(|n| n as u8),
                        ),
                        noneable(
                            "background light/dark switch intensity for B:",
                            none("do not change", 3, (1, 9), None),
                            |s| s.duplicate_colours.intensity_b.map(i64::from),
                            |s, v| s.duplicate_colours.intensity_b = v.map(|n| n as u8),
                        ),
                        check(
                            "draw image transparency as checkerboard in the duplicate filter:",
                            |s| s.duplicate_colours.checkerboard,
                            |s, v| s.duplicate_colours.checkerboard = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "exporting",
            vec![
                boxed(
                    "all exports",
                    vec![
                        check(
                            "ADVANCED: Always apply NTFS filename rules to export filenames: ",
                            |s| s.export.always_apply_ntfs_rules,
                            |s, v| s.export.always_apply_ntfs_rules = v,
                        ),
                        noneable(
                            "ADVANCED: Export path length limit (characters/bytes): ",
                            none("let hydrus decide", 250, (96, 8192), None),
                            |s| s.export.path_character_limit,
                            |s, v| s.export.path_character_limit = v,
                        ),
                        noneable(
                            "ADVANCED: Export dirname length limit (characters/bytes): ",
                            none("let hydrus decide", 64, (16, 8192), None),
                            |s| s.export.dirname_character_limit,
                            |s, v| s.export.dirname_character_limit = v,
                        ),
                        int(
                            "ADVANCED: Export filename length limit (characters/bytes): ",
                            (16, 8192),
                            |s| s.export.filename_character_limit,
                            |s, v| s.export.filename_character_limit = v,
                        ),
                    ],
                ),
                boxed(
                    "export folder",
                    vec![opt(
                        "Default export directory: ",
                        Kind::Directory,
                        Rc::new(|s| {
                            Value::Text(s.export.default_directory.clone().unwrap_or_default())
                        }),
                        Rc::new(|s, value| match value {
                            Value::Text(path) => {
                                s.export.default_directory =
                                    (!path.trim().is_empty()).then(|| path.clone());
                                Ok(())
                            }
                            _ => Err(wrong("Default export directory: ")),
                        }),
                    )],
                ),
            ],
        ),
        page(
            "open externally",
            vec![opt(
                "open externally",
                Kind::OpenExternally,
                Rc::new(|s| Value::OpenExternally(s.open_externally.clone())),
                Rc::new(|s, value| match value {
                    Value::OpenExternally(routing) => {
                        s.open_externally = routing.clone();
                        Ok(())
                    }
                    _ => Err(wrong("open externally")),
                }),
            )],
        ),
        page(
            "external programs",
            vec![boxed(
                "external calls",
                vec![opt(
                    "external calls",
                    Kind::ExternalCalls,
                    Rc::new(|s| Value::ExternalCalls(s.external_calls.clone())),
                    Rc::new(|s, v| match v {
                        Value::ExternalCalls(calls) => {
                            s.external_calls = calls.clone();
                            Ok(())
                        }
                        _ => Err(wrong("external calls")),
                    }),
                )],
            )],
        ),
        page(
            "file search",
            vec![
                boxed(
                    "file search autocomplete",
                    vec![
                        int(
                            "Active Search Predicates list height:",
                            (1, 128),
                            |settings| i64::from(settings.file_search.active_predicate_rows),
                            |settings, value| {
                                settings.file_search.active_predicate_rows = value as u32;
                            },
                        ),
                        opt(
                            "Default/Fallback local file search location:",
                            Kind::LocalLocation,
                            Rc::new(|settings| {
                                Value::Location(settings.search_defaults.local_location.clone())
                            }),
                            Rc::new(|settings, value| match value {
                                Value::Location(location) => {
                                    settings.search_defaults.local_location = location.clone();
                                    Ok(())
                                }
                                _ => Err(wrong("Default/Fallback local file search location:")),
                            }),
                        ),
                        tag_service(
                            "Default tag service in search pages:",
                            true,
                            |settings| settings.search_defaults.tag_service.clone(),
                            |settings, service| settings.search_defaults.tag_service = service,
                            |_| true,
                        ),
                        check(
                            "Autocomplete dropdown floats over file search pages:",
                            |settings| settings.file_search.float_autocomplete,
                            |settings, value| settings.file_search.float_autocomplete = value,
                        ),
                        int(
                            "Autocomplete list height:",
                            (1, 128),
                            |settings| i64::from(settings.file_search.autocomplete_rows),
                            |settings, value| settings.file_search.autocomplete_rows = value as u32,
                        ),
                        check(
                            "Start new search pages in 'searching immediately':",
                            |settings| settings.file_search.search_immediately,
                            |settings, value| settings.file_search.search_immediately = value,
                        ),
                        check(
                            "Show system:everything:",
                            |settings| settings.file_search.show_system_everything,
                            |settings, value| settings.file_search.show_system_everything = value,
                        ),
                    ],
                ),
                boxed(
                    "file search",
                    vec![
                        noneable(
                            "Implicit system:limit for all searches: ",
                            none("no limit", 10_000, (1, 100_000_000), None),
                            |settings| {
                                settings
                                    .file_search
                                    .implicit_limit
                                    .map(|value| value as i64)
                            },
                            |settings, value| {
                                settings.file_search.implicit_limit =
                                    value.map(|value| value as u64);
                            },
                        ),
                        check(
                            "If explicit system:limit, then refresh search when file sort changes: ",
                            |settings| settings.file_search.refresh_limited_sort,
                            |settings, value| settings.file_search.refresh_limited_sort = value,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "file sort/collect",
            vec![boxed(
                "file sort",
                vec![
                    sort(
                        "Default file sort: ",
                        |s| s.sorts.default_sort.clone(),
                        |s, v| s.sorts.default_sort = v,
                    ),
                    sort(
                        "Secondary file sort (when primary gives two equal values): ",
                        |s| s.sorts.fallback_sort.clone(),
                        |s, v| s.sorts.fallback_sort = v,
                    ),
                    check(
                        "Update default file sort every time a new sort is manually chosen: ",
                        |s| s.sorts.save_page_sort_on_change,
                        |s, v| s.sorts.save_page_sort_on_change = v,
                    ),
                    collect(
                        "Default collect: ",
                        |s| s.sorts.default_collect.clone(),
                        |s, v| s.sorts.default_collect = v,
                    ),
                    boxed(
                        "namespace file sorting",
                        vec![opt(
                            "",
                            Kind::NamespaceSorts,
                            Rc::new(|settings| {
                                Value::NamespaceSorts(settings.sorts.namespace_sorts.clone())
                            }),
                            Rc::new(|settings, value| match value {
                                Value::NamespaceSorts(sorts) => {
                                    settings.sorts.namespace_sorts.clone_from(sorts);
                                    Ok(())
                                }
                                _ => Err(wrong("namespace file sorting")),
                            }),
                        )],
                    ),
                ],
            )],
        ),
        page(
            "file viewing statistics",
            vec![
                check(
                    "Enable file viewing statistics tracking?:",
                    |s| s.file_viewing.active,
                    |s, v| s.file_viewing.active = v,
                ),
                check(
                    "Enable file viewing statistics tracking in the archive/delete filter?:",
                    |s| s.file_viewing.archive_delete,
                    |s, v| s.file_viewing.archive_delete = v,
                ),
                check(
                    "Enable file viewing statistics tracking in the duplicate filter?:",
                    |s| s.file_viewing.duplicates,
                    |s, v| s.file_viewing.duplicates = v,
                ),
                noneable_duration(
                    "Min time to view on media viewer to count as a view:",
                    &[Unit::Minutes, Unit::Seconds, Unit::Milliseconds],
                    0.05,
                    2.0,
                    "count every view",
                    |s| s.file_viewing.media_min_ms,
                    |s, v| s.file_viewing.media_min_ms = v,
                ),
                noneable_duration(
                    "Cap any view on the media viewer to this maximum time:",
                    &[
                        Unit::Hours,
                        Unit::Minutes,
                        Unit::Seconds,
                        Unit::Milliseconds,
                    ],
                    1.0,
                    600.0,
                    "no limit",
                    |s| s.file_viewing.media_max_ms,
                    |s, v| s.file_viewing.media_max_ms = v,
                ),
                noneable_duration(
                    "Min time to view on preview viewer to count as a view:",
                    &[Unit::Minutes, Unit::Seconds, Unit::Milliseconds],
                    0.05,
                    5.0,
                    "count every view",
                    |s| s.file_viewing.preview_min_ms,
                    |s, v| s.file_viewing.preview_min_ms = v,
                ),
                noneable_duration(
                    "Cap any view on the preview viewer to this maximum time:",
                    &[
                        Unit::Hours,
                        Unit::Minutes,
                        Unit::Seconds,
                        Unit::Milliseconds,
                    ],
                    1.0,
                    60.0,
                    "no limit",
                    |s| s.file_viewing.preview_max_ms,
                    |s, v| s.file_viewing.preview_max_ms = v,
                ),
                choice(
                    "Show viewing stats on media right-click menus?:",
                    &[
                        "show a combined value, and stack the separate values a submenu",
                        "stack the separate values",
                    ],
                    |s| {
                        usize::from(
                            s.file_viewing.menu_display
                                == hydrus_store::settings::ViewingStatsMenuDisplay::Stacked,
                        )
                    },
                    |s, v| {
                        s.file_viewing.menu_display = if v == 1 {
                            hydrus_store::settings::ViewingStatsMenuDisplay::Stacked
                        } else {
                            hydrus_store::settings::ViewingStatsMenuDisplay::Combined
                        }
                    },
                ),
                opt(
                    "Which views to show?:",
                    Kind::CanvasTicks,
                    Rc::new(|s| Value::Canvases(s.file_viewing.interesting_canvases.clone())),
                    Rc::new(|s, v| match v {
                        Value::Canvases(canvases) => {
                            s.file_viewing.interesting_canvases.clone_from(canvases);
                            Ok(())
                        }
                        _ => Err(wrong("Which views to show?:")),
                    }),
                ),
            ],
        ),
        page(
            "files and trash",
            vec![
                check(
                    "Remove files from view when they are archive/delete filtered: ",
                    |s| s.file_view_removal.filtered,
                    |s, v| s.file_view_removal.filtered = v,
                ),
                enabled(
                    check(
                        "--even skipped files: ",
                        |s| s.file_view_removal.skipped,
                        |s, v| s.file_view_removal.skipped = v,
                    ),
                    |s| s.file_view_removal.filtered,
                ),
                check(
                    "Remove files from view when they are sent to the trash: ",
                    |s| s.file_view_removal.trashed,
                    |s, v| s.file_view_removal.trashed = v,
                ),
                check(
                    "Remove files from view when they are moved to another local file domain: ",
                    |s| s.file_view_removal.moved,
                    |s, v| s.file_view_removal.moved = v,
                ),
                check(
                    "When copying file hashes, prefix with booru-friendly hash type: ",
                    |s| s.file_handling.prefix_hash_when_copying,
                    |s, v| s.file_handling.prefix_hash_when_copying = v,
                ),
                check(
                    "Confirm sending files to trash: ",
                    |s| s.deletion.confirm_trash,
                    |s, v| s.deletion.confirm_trash = v,
                ),
                check(
                    "Confirm sending more than one file to archive or inbox: ",
                    |s| s.deletion.confirm_archive,
                    |s, v| s.deletion.confirm_archive = v,
                ),
                check(
                    "Confirm when copying files across local file domains: ",
                    |s| s.local_transfer.copy,
                    |s, v| s.local_transfer.copy = v,
                ),
                check(
                    "Confirm when moving files across local file domains: ",
                    |s| s.local_transfer.move_files,
                    |s, v| s.local_transfer.move_files = v,
                ),
                check(
                    "When physically deleting files or folders, send them to the OS's recycle bin: ",
                    |s| s.folders.delete_to_recycle_bin,
                    |s, v| s.folders.delete_to_recycle_bin = v,
                ),
                noneable(
                    "Number of hours a file will stay in the trash before being deleted: ",
                    none("no age limit", 72, (0, 8640), None),
                    |s| signed(s.trash.max_age_hours),
                    |s, v| s.trash.max_age_hours = unsigned(v),
                ),
                noneable(
                    "Maximum size of trash (MB): ",
                    none("no size limit", 2048, (0, 20480), None),
                    |s| signed(s.trash.max_size_mb),
                    |s, v| s.trash.max_size_mb = unsigned(v),
                ),
                check(
                    "TEST: Import local files directly from source, do not copy to temp dir beforehand.",
                    |s| !s.folders.copy_import_files_to_temp_dir,
                    |s, v| s.folders.copy_import_files_to_temp_dir = !v,
                ),
                check(
                    "ADVANCED: Do not do chmod when copying files",
                    |s| s.file_handling.do_not_chmod,
                    |s, v| s.file_handling.do_not_chmod = v,
                ),
                boxed(
                    "delete lock",
                    vec![
                        check(
                            "Do not permit archived files to be deleted from the trash: ",
                            |s| s.delete_lock.archived,
                            |s, v| s.delete_lock.archived = v,
                        ),
                        check(
                            "After archive/delete filter, ensure deletees are inboxed before delete: ",
                            |s| s.delete_lock.reinbox_after_archive_delete,
                            |s, v| s.delete_lock.reinbox_after_archive_delete = v,
                        ),
                        check(
                            "After duplicate filter, ensure deletees are inboxed before delete: ",
                            |s| s.delete_lock.reinbox_after_duplicate_filter,
                            |s, v| s.delete_lock.reinbox_after_duplicate_filter = v,
                        ),
                        check(
                            "In duplicates auto-resolution, ensure deletees are inboxed before delete: ",
                            |s| s.delete_lock.reinbox_in_auto_resolution,
                            |s, v| s.delete_lock.reinbox_in_auto_resolution = v,
                        ),
                    ],
                ),
                boxed(
                    "advanced file deletion and custom reasons",
                    vec![
                        check(
                            "Use the advanced file deletion dialog: ",
                            |s| s.deletion.advanced,
                            |s, v| s.deletion.advanced = v,
                        ),
                        enabled(
                            check(
                                "Remember the last action: ",
                                |s| s.deletion.remember_action,
                                |s, v| s.deletion.remember_action = v,
                            ),
                            |s| s.deletion.advanced,
                        ),
                        enabled(
                            check(
                                "Remember the last reason: ",
                                |s| s.deletion.remember_reason,
                                |s, v| s.deletion.remember_reason = v,
                            ),
                            |s| s.deletion.advanced,
                        ),
                        enabled(
                            opt(
                                "",
                                Kind::DeletionReasons,
                                Rc::new(|s| Value::DeletionReasons(s.deletion.reasons.clone())),
                                Rc::new(|s, v| match v {
                                    Value::DeletionReasons(reasons) => {
                                        s.deletion.reasons.clone_from(reasons);
                                        Ok(())
                                    }
                                    _ => Err(wrong("deletion reasons")),
                                }),
                            ),
                            |s| s.deletion.advanced,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "gui",
            vec![
                boxed(
                    "main window",
                    vec![
                        text(
                            "Application display name: ",
                            |s| s.gui.application_display_name.clone(),
                            |s, value| {
                                s.gui.application_display_name = if value.is_empty() {
                                    "hydrus client".into()
                                } else {
                                    value.into()
                                };
                                Ok(())
                            },
                        ),
                        check(
                            "Confirm client exit: ",
                            |s| s.gui.confirm_exit,
                            |s, value| s.gui.confirm_exit = value,
                        ),
                    ],
                ),
                boxed(
                    "misc",
                    vec![
                        check(
                            "Prefer ISO time (\"2018-03-01 12:40:23\") to \"5 days ago\": ",
                            |s| s.gui_formatting.iso,
                            |s, v| s.gui_formatting.iso = v,
                        ),
                        check(
                            "Remember last open options panel in this window: ",
                            |s| s.options_preferences.remember_panel,
                            |s, v| s.options_preferences.remember_panel = v,
                        ),
                        choice(
                            "Put the options search bar at the: ",
                            &["top of this window", "bottom of this window"],
                            |s| usize::from(!s.options_preferences.search_at_top),
                            |s, v| s.options_preferences.search_at_top = v == 0,
                        ),
                        int(
                            "EXPERIMENTAL: Bytes strings >1KB pseudo significant figures: ",
                            (1, 6),
                            |s| i64::from(s.gui_formatting.figures),
                            |s, v| s.gui_formatting.figures = u8::try_from(v).unwrap_or(3),
                        ),
                    ],
                ),
                boxed(
                    "frame locations",
                    vec![
                        check(
                            "BUGFIX: Disable off-screen window rescue: ",
                            |s| s.window_rescue.disabled,
                            |s, v| s.window_rescue.disabled = v,
                        ),
                        check(
                            "When rescuing, add top-left safety padding:",
                            |s| s.window_rescue.add_padding,
                            |s, v| s.window_rescue.add_padding = v,
                        ),
                        int(
                            "DEBUG: top-left padding to use (px): ",
                            (0, 100),
                            |s| i64::from(s.window_rescue.padding),
                            |s, v| s.window_rescue.padding = u8::try_from(v).unwrap_or_default(),
                        ),
                        check(
                            "Save media viewer window size and position on close: ",
                            |s| s.windows.save_media_viewer_on_close,
                            |s, v| s.windows.save_media_viewer_on_close = v,
                        ),
                        opt(
                            "",
                            Kind::FrameLocations,
                            Rc::new(|s| Value::FrameLocations(s.windows.frames())),
                            Rc::new(|s, v| match v {
                                Value::FrameLocations(frames) => {
                                    for (name, frame) in frames {
                                        s.windows.set_frame(name, frame.clone());
                                    }
                                    Ok(())
                                }
                                _ => Err(wrong("frame locations")),
                            }),
                        ),
                    ],
                ),
            ],
        ),
        page(
            "gui pages",
            vec![
                boxed(
                    "preview window",
                    vec![check(
                        "Hide the bottom-left preview window: ",
                        |s| s.page_layout.hide_preview,
                        |s, v| s.page_layout.hide_preview = v,
                    )],
                ),
                boxed(
                    "opening and closing",
                    vec![
                        choice(
                            "Put new page tabs on: ",
                            &[
                                "the far left",
                                "left of current page tab",
                                "right of current page tab",
                                "the far right",
                            ],
                            |s| match s.page_insertion {
                                hydrus_store::settings::PageInsertion::FarLeft => 0,
                                hydrus_store::settings::PageInsertion::LeftOfCurrent => 1,
                                hydrus_store::settings::PageInsertion::RightOfCurrent => 2,
                                hydrus_store::settings::PageInsertion::FarRight => 3,
                            },
                            |s, value| {
                                s.page_insertion =
                                    hydrus_store::settings::PageInsertion::from_code(
                                        i64::try_from(value).unwrap_or(3),
                                    )
                                    .unwrap_or_default();
                            },
                        ),
                        check(
                            "In new page chooser, show \"combined local file domains\" if appropriate:",
                            |s| s.page_chooser.show_combined,
                            |s, v| s.page_chooser.show_combined = v,
                        ),
                        check(
                            "  Put it at the top:",
                            |s| s.page_chooser.combined_at_top,
                            |s, v| s.page_chooser.combined_at_top = v,
                        ),
                        check(
                            "In new page chooser, show \"hydrus local file storage\":",
                            |s| s.page_chooser.show_storage,
                            |s, v| s.page_chooser.show_storage = v,
                        ),
                        check(
                            "  Put it at the top:",
                            |s| s.page_chooser.storage_at_top,
                            |s, v| s.page_chooser.storage_at_top = v,
                        ),
                        choice(
                            "When closing the current tab, move focus: ",
                            &[
                                "left of the closed page tab",
                                "right of the closed page tab",
                            ],
                            |s| usize::from(!s.notebooks.close_focus_left),
                            |s, value| s.notebooks.close_focus_left = value == 0,
                        ),
                        check(
                            "Confirm when closing any page: ",
                            |s| s.page_navigation.confirm_all_closes,
                            |s, v| s.page_navigation.confirm_all_closes = v,
                        ),
                        check(
                            "Confirm when closing a non-empty importer page: ",
                            |s| s.downloader_pages.confirm_non_empty_close,
                            |s, v| s.downloader_pages.confirm_non_empty_close = v,
                        ),
                    ],
                ),
                boxed(
                    "navigation and drag-and-drop",
                    vec![
                        choice(
                            "Notebook tab alignment: ",
                            &["top", "left", "right", "bottom"],
                            |s| usize::try_from(s.tab_presentation.alignment.code()).unwrap_or(0),
                            |s, value| {
                                s.tab_presentation.alignment =
                                    hydrus_store::settings::TabAlignment::from_code(
                                        i64::try_from(value).unwrap_or(0),
                                    )
                                    .unwrap_or_default();
                            },
                        ),
                        int(
                            "Maximum entries to show in page navigation history: ",
                            (1, 1000),
                            |s| i64::from(s.page_navigation.history_entries),
                            |s, v| {
                                s.page_navigation.history_entries = u16::try_from(v).unwrap_or(100);
                            },
                        ),
                        check(
                            "When switching to pages, move keyboard focus to any text input field: ",
                            |s| s.page_navigation.focus_search_on_change,
                            |s, v| s.page_navigation.focus_search_on_change = v,
                        ),
                        check(
                            "Selection chases dropped page after drag and drop: ",
                            |s| s.tab_drag.chase,
                            |s, v| s.tab_drag.chase = v,
                        ),
                        check(
                            "  With shift held down?: ",
                            |s| s.tab_drag.chase_shift,
                            |s, v| s.tab_drag.chase_shift = v,
                        ),
                        check(
                            "Navigate tabs during drag and drop: ",
                            |s| s.tab_drag.navigate,
                            |s, v| s.tab_drag.navigate = v,
                        ),
                        check(
                            "  With shift held down?: ",
                            |s| s.tab_drag.navigate_shift,
                            |s, v| s.tab_drag.navigate_shift = v,
                        ),
                        check(
                            "EXPERIMENTAL: Mouse wheel scrolls tab bar, not page selection: ",
                            |s| s.tab_drag.wheel_scroll,
                            |s, v| s.tab_drag.wheel_scroll = v,
                        ),
                        check(
                            "BUGFIX: Disable all page tab drag and drop: ",
                            |s| s.tab_drag.disabled,
                            |s, v| s.tab_drag.disabled = v,
                        ),
                        choice(
                            "EXPERIMENTAL: Show tab tree view: ",
                            &["disable", "left", "right"],
                            |s| usize::try_from(s.tab_presentation.tree_side()).unwrap_or(0),
                            |s, value| {
                                s.tab_presentation.tree_alignment = match value {
                                    1 => Some(hydrus_store::settings::TabAlignment::Left),
                                    2 => Some(hydrus_store::settings::TabAlignment::Right),
                                    _ => None,
                                };
                            },
                        ),
                        check(
                            "EXPERIMENTAL: Hide main page navigation tabs: ",
                            |s| s.tab_presentation.hide_navigation_tabs,
                            |s, value| s.tab_presentation.hide_navigation_tabs = value,
                        ),
                    ],
                ),
                boxed(
                    "page tab names",
                    vec![
                        int(
                            "Max characters to display in a page name: ",
                            (1, 256),
                            |s| s.page_names.max_chars as i64,
                            |s, v| s.page_names.max_chars = v as usize,
                        ),
                        check(
                            "When there are too many tabs to fit, '...' elide their names so they fit: ",
                            |s| s.tab_presentation.elide_names,
                            |s, value| s.tab_presentation.elide_names = value,
                        ),
                        choice(
                            "Show page file count after its name: ",
                            FILE_COUNTS,
                            |s| file_count_index(s.page_names.file_counts),
                            |s, i| s.page_names.file_counts = file_count_display(i),
                        ),
                        check(
                            "Show import page x/y progress after its name: ",
                            |s| s.page_names.import_progress,
                            |s, v| s.page_names.import_progress = v,
                        ),
                        check(
                            "Automatically prompt to rename new 'page of pages' after creation: ",
                            |s| s.notebook_creation.rename_new_notebooks,
                            |s, value| s.notebook_creation.rename_new_notebooks = value,
                        ),
                        check(
                            "  Also automatically prompt when sending some pages to one: ",
                            |s| s.notebooks.rename_sent_notebooks,
                            |s, value| s.notebooks.rename_sent_notebooks = value,
                        ),
                        check(
                            "Suffix 'page of pages' tab names with a decorator string: ",
                            |s| s.page_names.decorate_notebooks,
                            |s, v| s.page_names.decorate_notebooks = v,
                        ),
                        text(
                            "  Decorator string: ",
                            |s| s.page_names.notebook_decorator.clone(),
                            |s, t| {
                                t.clone_into(&mut s.page_names.notebook_decorator);
                                Ok(())
                            },
                        ),
                    ],
                ),
            ],
        ),
        page(
            "gui sessions",
            vec![boxed(
                "sessions",
                vec![
                    opt(
                        "Default session on startup: ",
                        Kind::SavedSession,
                        Rc::new(|s| Value::SavedSession(s.gui_sessions.startup.clone())),
                        Rc::new(|s, value| match value {
                            Value::SavedSession(name) => {
                                s.gui_sessions.startup.clone_from(name);
                                Ok(())
                            }
                            _ => Err(wrong("Default session on startup: ")),
                        }),
                    ),
                    int(
                        "If 'last session' above, autosave it how often (minutes)?",
                        (1, 1440),
                        |s| i64::from(s.gui_sessions.autosave_minutes),
                        |s, value| {
                            s.gui_sessions.autosave_minutes = u16::try_from(value).unwrap_or(5);
                        },
                    ),
                    check(
                        "If 'last session' above, only autosave during idle time?",
                        |s| s.gui_sessions.only_during_idle,
                        |s, value| s.gui_sessions.only_during_idle = value,
                    ),
                    int(
                        "Number of session backups to keep: ",
                        (1, 32),
                        |s| s.session_backups.keep as i64,
                        |s, value| s.session_backups.keep = value as usize,
                    ),
                    check(
                        "Show warning popup if session size exceeds 10,000,000: ",
                        |s| s.gui_sessions.warn_large_session,
                        |s, value| s.gui_sessions.warn_large_session = value,
                    ),
                ],
            )],
        ),
        page(
            "import options",
            vec![opt(
                "",
                Kind::ImportOptions,
                Rc::new(|settings| {
                    Value::ImportOptions(crate::import_options_panel::Value {
                        manager: settings.import_options.clone(),
                        ui: settings.import_options_ui.clone(),
                    })
                }),
                Rc::new(|settings, value| match value {
                    Value::ImportOptions(value) => {
                        settings.import_options = value.manager.clone();
                        settings.import_options_ui = value.ui.clone();
                        Ok(())
                    }
                    _ => Err(wrong("import options")),
                }),
            )],
        ),
        page(
            "importing",
            vec![
                boxed(
                    "filetypes",
                    vec![check(
                        "Inspect for .cbz properties when importing/rescanning .zip files:",
                        |s| s.file_handling.comic_book_detection,
                        |s, v| s.file_handling.comic_book_detection = v,
                    )],
                ),
                boxed(
                    "work slots",
                    vec![
                        int(
                            "Number of gallery downloader file queues that can import at the same time:",
                            (1, 500),
                            |s| s.import_work_slots.gallery_files,
                            |s, v| s.import_work_slots.gallery_files = v,
                        ),
                        int(
                            "Number of gallery downloader searches that can run at the same time:",
                            (1, 500),
                            |s| s.import_work_slots.gallery_search,
                            |s, v| s.import_work_slots.gallery_search = v,
                        ),
                        int(
                            "Number of watcher page file queues that can run at the same time:",
                            (1, 500),
                            |s| s.import_work_slots.watcher_files,
                            |s, v| s.import_work_slots.watcher_files = v,
                        ),
                        int(
                            "Number of watcher page checkers that can run at the same time:",
                            (1, 500),
                            |s| s.import_work_slots.watcher_check,
                            |s, v| s.import_work_slots.watcher_check = v,
                        ),
                        int(
                            "Number of other paged importer jobs that can run at the same time:",
                            (1, 500),
                            |s| s.import_work_slots.misc,
                            |s, v| s.import_work_slots.misc = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "maintenance and processing",
            vec![
                boxed(
                    "when to run high cpu jobs",
                    vec![boxed(
                        "idle",
                        vec![
                            enabled(
                                noneable(
                                    "Permit idle mode if no general browsing activity has occurred in the past: ",
                                    none("ignore normal browsing", 1, (1, 1000), Some("minutes")),
                                    |settings| {
                                        settings
                                            .gui_idle
                                            .user_seconds
                                            .map(|seconds| (seconds / 60).clamp(1, 1000) as i64)
                                    },
                                    |settings, value| {
                                        settings.gui_idle.user_seconds =
                                            value.map(|minutes| minutes as u64 * 60);
                                    },
                                ),
                                |settings| settings.gui_idle.enabled,
                            ),
                            enabled(
                                noneable(
                                    "Permit idle mode if your mouse cursor has not been moved in the past: ",
                                    none("ignore mouse movements", 1, (1, 1000), Some("minutes")),
                                    |settings| {
                                        settings
                                            .gui_idle
                                            .mouse_seconds
                                            .map(|seconds| (seconds / 60).clamp(1, 1000) as i64)
                                    },
                                    |settings, value| {
                                        settings.gui_idle.mouse_seconds =
                                            value.map(|minutes| minutes as u64 * 60);
                                    },
                                ),
                                |settings| settings.gui_idle.enabled,
                            ),
                            enabled(
                                noneable(
                                    "Permit idle mode if no Client API requests in the past: ",
                                    none("ignore client api", 1, (1, 1000), Some("minutes")),
                                    |settings| {
                                        settings
                                            .gui_idle
                                            .api_seconds
                                            .map(|seconds| (seconds / 60).clamp(1, 1000) as i64)
                                    },
                                    |settings, value| {
                                        settings.gui_idle.api_seconds =
                                            value.map(|minutes| minutes as u64 * 60);
                                    },
                                ),
                                |settings| settings.gui_idle.enabled,
                            ),
                        ],
                    )],
                ),
                boxed(
                    "file maintenance",
                    vec![
                        check(
                            "Run file maintenance during normal time: ",
                            |s| s.file_maintenance.during_active,
                            |s, v| s.file_maintenance.during_active = v,
                        ),
                        velocity(
                            "Normal throttle: ",
                            ((1, 1000), "heavy work units every"),
                            time(&[Unit::Minutes, Unit::Seconds], 1.0),
                            |s| {
                                (
                                    s.file_maintenance.active_files as i64,
                                    s.file_maintenance.active_seconds as f64,
                                )
                            },
                            |s, n, seconds| {
                                s.file_maintenance.active_files = n as u64;
                                s.file_maintenance.active_seconds = whole(seconds);
                            },
                        ),
                    ],
                ),
                boxed(
                    "potential duplicates search",
                    vec![
                        check(
                            "Search for potential duplicates in \"idle\" time: ",
                            |s| s.similar_files.during_idle,
                            |s, v| s.similar_files.during_idle = v,
                        ),
                        check(
                            "Search for potential duplicates in \"normal\" time: ",
                            |s| s.similar_files.during_active,
                            |s, v| s.similar_files.during_active = v,
                        ),
                    ],
                ),
                boxed(
                    "duplicates auto-resolution",
                    vec![
                        check(
                            "Work duplicates auto-resolution in \"normal\" time: ",
                            |s| s.auto_resolution.during_active,
                            |s, v| s.auto_resolution.during_active = v,
                        ),
                        duration(
                            "\"Normal\" ideal work packet time: ",
                            time(&[Unit::Seconds, Unit::Milliseconds], 0.1),
                            |s| f64::from(s.auto_resolution.work_time_ms_active) / 1000.0,
                            |s, v| {
                                s.auto_resolution.work_time_ms_active = whole(v * 1000.0) as u32;
                            },
                        ),
                        int(
                            "\"Normal\" rest time percentage: ",
                            (0, 100_000),
                            |s| i64::from(s.auto_resolution.rest_percentage_active),
                            |s, v| s.auto_resolution.rest_percentage_active = v as u32,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "media playback",
            vec![
                boxed(
                    "zoom and position",
                    vec![
                        choice(
                            "Centerpoint for media zooming:",
                            ZOOM_CENTRES,
                            |s| {
                                ZOOM_CENTRE_ORDER
                                    .iter()
                                    .position(|c| *c == s.media_viewer.zoom_centre)
                                    .unwrap_or(0)
                            },
                            |s, i| s.media_viewer.zoom_centre = ZOOM_CENTRE_ORDER[i],
                        ),
                        text(
                            "Media zooms:",
                            |s| numbers_text(&s.media_viewer.media_zooms),
                            |s, t| {
                                // (none above zero: left as they were)
                                let zooms = parse_numbers(t, "zooms")?;
                                if !zooms.is_empty() {
                                    s.media_viewer.media_zooms = zooms;
                                }
                                Ok(())
                            },
                        ),
                        choice(
                            "Media Viewer default zoom:",
                            ZOOM_TYPES,
                            |s| {
                                ZOOM_TYPE_ORDER
                                    .iter()
                                    .position(|t| *t == s.media_viewer.default_zoom_type)
                                    .unwrap_or(0)
                            },
                            |s, i| s.media_viewer.default_zoom_type = ZOOM_TYPE_ORDER[i],
                        ),
                        check(
                            "Re-center media on window resize:",
                            |settings| settings.viewer_canvas.recenter_on_resize,
                            |settings, value| settings.viewer_canvas.recenter_on_resize = value,
                        ),
                    ],
                ),
                boxed(
                    "transparency",
                    vec![
                        choice(
                            "Consider a file as \"having transparency\" when:",
                            TRANSPARENCY,
                            |s| {
                                2_usize.saturating_sub(usize::from(
                                    s.file_handling.transparency_strictness,
                                ))
                            },
                            |s, i| s.file_handling.transparency_strictness = 2 - i.min(2) as u8,
                        ),
                        check(
                            "Draw image transparency as checkerboard:",
                            |settings| settings.viewer_canvas.transparency_checkerboard,
                            |settings, value| {
                                settings.viewer_canvas.transparency_checkerboard = value;
                            },
                        ),
                        check(
                            "--Instead of checkerboard, use a bright greenscreen:",
                            |settings| settings.viewer_canvas.transparency_greenscreen,
                            |settings, value| {
                                settings.viewer_canvas.transparency_greenscreen = value;
                            },
                        ),
                    ],
                ),
                boxed(
                    "video/animations",
                    vec![check(
                        "Always Loop Animations:",
                        |s| s.viewer_playback.always_loop,
                        |s, value| s.viewer_playback.always_loop = value,
                    )],
                ),
            ],
        ),
        page(
            "media viewer",
            vec![
                boxed(
                    "mouse behaviour",
                    vec![
                        noneable(
                            "Time until mouse cursor autohides on media viewer:",
                            none("do not autohide", 700, (100, 100_000), Some("ms")),
                            |settings| settings.viewer_cursor.autohide_ms.map(i64::from),
                            |settings, value| {
                                settings.viewer_cursor.autohide_ms =
                                    value.map(|delay| delay as u32);
                            },
                        ),
                        check(
                            "Do not allow mouse media drag-panning when the media has duration:",
                            |settings| settings.viewer_pointer.disallow_duration_drag,
                            |settings, value| {
                                settings.viewer_pointer.disallow_duration_drag = value;
                            },
                        ),
                        check(
                            "Anchor mouse cursor during media viewer drags:",
                            |settings| settings.viewer_pointer.anchor_drag,
                            |settings, value| {
                                settings.viewer_pointer.anchor_drag = value;
                            },
                        ),
                        check(
                            "Hide mouse cursor during media viewer drags:",
                            |settings| settings.viewer_pointer.hide_during_drag,
                            |settings, value| settings.viewer_pointer.hide_during_drag = value,
                        ),
                        check(
                            "If set to anchor drags, undo on apparent touchscreen drag:",
                            |settings| settings.viewer_pointer.touch_unanchors,
                            |settings, value| {
                                settings.viewer_pointer.touch_unanchors = value;
                            },
                        ),
                    ],
                ),
                boxed(
                    "animation/audio seek bar",
                    vec![
                        int(
                            "Seek bar height:",
                            (1, 255),
                            |settings| i64::from(settings.viewer_canvas.seek_height),
                            |settings, value| settings.viewer_canvas.seek_height = value as u32,
                        ),
                        noneable(
                            "Seek bar height when mouse away:",
                            none("no, hide it completely", 5, (1, 255), Some("px")),
                            |settings| settings.viewer_canvas.seek_hidden_height.map(i64::from),
                            |settings, value| {
                                settings.viewer_canvas.seek_hidden_height =
                                    value.map(|height| height as u32);
                            },
                        ),
                        check(
                            "Seek bar full-height pop-in requires window focus:",
                            |settings| settings.viewer_focus.seek_requires_focus,
                            |settings, value| settings.viewer_focus.seek_requires_focus = value,
                        ),
                        int(
                            "Seek bar nub width:",
                            (1, 63),
                            |settings| i64::from(settings.viewer_canvas.seek_nub_width),
                            |settings, value| settings.viewer_canvas.seek_nub_width = value as u32,
                        ),
                    ],
                ),
                boxed(
                    "slideshows",
                    vec![
                        text(
                            "Slideshow durations:",
                            |s| numbers_text(&s.slideshow.durations),
                            |s, t| {
                                // (none above zero: left as they were)
                                let durations = parse_numbers(t, "slideshow durations")?;
                                if !durations.is_empty() {
                                    s.slideshow.durations = durations;
                                }
                                Ok(())
                            },
                        ),
                        check(
                            "Always play media once through before moving on:",
                            |s| s.slideshow.once_through,
                            |s, v| s.slideshow.once_through = v,
                        ),
                        noneable(
                            "Slideshow short-media skip seconds threshold:",
                            none("do not use", 10, (1, 86400), Some("s")),
                            |s| s.slideshow.short_loop_seconds,
                            |s, v| s.slideshow.short_loop_seconds = v,
                        ),
                        noneable(
                            "Slideshow short-media skip percentage threshold:",
                            none("do not use", 20, (1, 99), Some("%")),
                            |s| s.slideshow.short_loop_percentage,
                            |s, v| s.slideshow.short_loop_percentage = v,
                        ),
                        noneable(
                            "Slideshow shorter-media cutoff percentage threshold:",
                            none("do not use", 75, (1, 99), Some("%")),
                            |s| s.slideshow.short_cutoff_percentage,
                            |s, v| s.slideshow.short_cutoff_percentage = v,
                        ),
                        noneable(
                            "Slideshow long-media allowed delay percentage threshold:",
                            none("do not use", 50, (1, 500), Some("%")),
                            |s| s.slideshow.long_overspill_percentage,
                            |s, v| s.slideshow.long_overspill_percentage = v,
                        ),
                    ],
                ),
                boxed(
                    "closing focus",
                    vec![
                        check(
                            "When closing the media viewer, re-select original search page: ",
                            |settings| settings.viewer_closing.reselect_page,
                            |settings, value| settings.viewer_closing.reselect_page = value,
                        ),
                        check(
                            "When closing the media viewer, tell original search page to select exit media: ",
                            |settings| settings.viewer_closing.select_exit_media,
                            |settings, value| {
                                settings.viewer_closing.select_exit_media = value;
                            },
                        ),
                        check(
                            "ADVANCED: When closing the media viewer with the above focusing options, activate Main GUI: ",
                            |settings| settings.viewer_closing.activate_focusing,
                            |settings, value| {
                                settings.viewer_closing.activate_focusing = value;
                            },
                        ),
                        check(
                            "DEBUG: When closing the media viewer at any time, activate Main GUI: ",
                            |settings| settings.viewer_closing.activate_always,
                            |settings, value| settings.viewer_closing.activate_always = value,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "media viewer hovers",
            vec![
                boxed(
                    "background",
                    vec![
                        check(
                            "Draw tags (left) in the viewer background:",
                            |settings| settings.viewer_background.tags,
                            |settings, value| settings.viewer_background.tags = value,
                        ),
                        check(
                            "Draw file information (top) in the viewer background:",
                            |settings| settings.viewer_background.information,
                            |settings, value| settings.viewer_background.information = value,
                        ),
                        check(
                            "Draw ratings and locations (top-right) in the viewer background:",
                            |settings| settings.viewer_background.ratings,
                            |settings, value| settings.viewer_background.ratings = value,
                        ),
                        check(
                            "Draw notes (right) in the viewer background:",
                            |settings| settings.viewer_background.notes,
                            |settings, value| settings.viewer_background.notes = value,
                        ),
                        check(
                            "Draw index text (bottom-right) in the viewer background:",
                            |settings| settings.viewer_hovers.index_background,
                            |settings, value| settings.viewer_hovers.index_background = value,
                        ),
                    ],
                ),
                boxed(
                    "hover windows",
                    vec![
                        check(
                            "Hover window pop-in requires window focus:",
                            |settings| settings.viewer_focus.hovers_require_focus,
                            |settings, value| settings.viewer_focus.hovers_require_focus = value,
                        ),
                        check(
                            "Pop-in tags (left) hover window on mouseover:",
                            |settings| settings.viewer_hovers.tags,
                            |settings, value| settings.viewer_hovers.tags = value,
                        ),
                        check(
                            "Pop-in ratings and locations (top-right) hover window on mouseover:",
                            |settings| settings.viewer_hovers.ratings,
                            |settings, value| settings.viewer_hovers.ratings = value,
                        ),
                        check(
                            "Pop-in notes (right) hover window on mouseover:",
                            |settings| settings.viewer_hovers.notes,
                            |settings, value| settings.viewer_hovers.notes = value,
                        ),
                        choice(
                            "Allow a mouse wheel scroll over the taglist to propagate to the main canvas:",
                            &[
                                "never propagate",
                                "only propagate when list has no vertical scrollbar",
                                "only propagate if vertical scrollbar has not been used recently",
                                "propagate immediately after vertical scrollbar hits an end (Qt default)",
                            ],
                            |settings| usize::from(settings.viewer_tag_scroll.0.code()),
                            |settings, value| {
                                settings.viewer_tag_scroll.0 = u16::try_from(value)
                                    .ok()
                                    .and_then(
                                        hydrus_store::settings::TagWheelPropagation::from_code,
                                    )
                                    .unwrap_or_default();
                            },
                        ),
                    ],
                ),
                boxed(
                    "top hover button/menu controls",
                    vec![
                        choice(
                            "Zoom switch button switches between:",
                            &[
                                "100% and canvas fit",
                                "100% and canvas fit, and recenter media on switch",
                                "100% and canvas fit and canvas fill",
                                "100% and canvas fit and canvas fill, and recenter media on switch",
                            ],
                            |s| s.viewer_playback.zoom_switch.min(3),
                            |s, value| s.viewer_playback.zoom_switch = value,
                        ),
                        check(
                            "Collapse \"window\" submenu in 'view options' (eye menu):",
                            |s| s.viewer_eye_menu.collapse_window,
                            |s, v| s.viewer_eye_menu.collapse_window = v,
                        ),
                        check(
                            "Collapse \"hovers\" submenu in 'view options' (eye menu):",
                            |s| s.viewer_eye_menu.collapse_hovers,
                            |s, v| s.viewer_eye_menu.collapse_hovers = v,
                        ),
                        check(
                            "Collapse \"rendering\" submenu in 'view options' (eye menu):",
                            |s| s.viewer_eye_menu.collapse_rendering,
                            |s, v| s.viewer_eye_menu.collapse_rendering = v,
                        ),
                    ],
                ),
                boxed(
                    "top hover file summary",
                    vec![
                        check(
                            "Show archived status: ",
                            |s| s.info_line.archived_interesting,
                            |s, v| s.info_line.archived_interesting = v,
                        ),
                        check(
                            "Show archived time: ",
                            |s| s.info_line.archived_time_interesting,
                            |s, v| s.info_line.archived_time_interesting = v,
                        ),
                        check(
                            "Show file services: ",
                            |s| s.info_line.file_services_interesting,
                            |s, v| s.info_line.file_services_interesting = v,
                        ),
                        check(
                            "Show file service add times: ",
                            |s| s.info_line.file_services_import_times_interesting,
                            |s, v| s.info_line.file_services_import_times_interesting = v,
                        ),
                        check(
                            "Show file trash times: ",
                            |s| s.info_line.trash_time_interesting,
                            |s, v| s.info_line.trash_time_interesting = v,
                        ),
                        check(
                            "Show file trash reasons: ",
                            |s| s.info_line.trash_reason_interesting,
                            |s, v| s.info_line.trash_reason_interesting = v,
                        ),
                        check(
                            "Hide uninteresting modified times: ",
                            |s| s.info_line.hide_uninteresting_modified_time,
                            |s, v| s.info_line.hide_uninteresting_modified_time = v,
                        ),
                        check(
                            "Swap in common resolution labels:",
                            |s| s.info_line.nice_resolutions,
                            |s, v| s.info_line.nice_resolutions = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "notes",
            vec![
                check(
                    "Start editing notes with the text cursor at the end of the document: ",
                    |s| s.note_preferences.start_at_end,
                    |s, v| s.note_preferences.start_at_end = v,
                ),
                check(
                    "When middle-clicking a note hover, only copy the text: ",
                    |s| s.note_preferences.hover_text_only,
                    |s, v| s.note_preferences.hover_text_only = v,
                ),
            ],
        ),
        popup_width::page(),
        page(
            "ratings",
            vec![
                boxed(
                    "media viewer",
                    vec![
                        float(
                            "Media viewer like/dislike and numerical rating icon size:",
                            (1.0, 255.0),
                            |s| s.media_viewer.rating_icon_size,
                            |s, v| s.media_viewer.rating_icon_size = v,
                        ),
                        float(
                            "Media viewer inc/dec rating icon height:",
                            (2.0, 255.0),
                            |s| s.media_viewer.rating_incdec_height,
                            |s, v| s.media_viewer.rating_incdec_height = v,
                        ),
                    ],
                ),
                boxed(
                    "preview window",
                    vec![
                        rating_size(
                            "Preview window like/dislike and numerical rating icon size:",
                            (1.0, 255.0),
                            |s| s.rating_context_sizes.preview_icon_size,
                            |s, v| s.rating_context_sizes.preview_icon_size = v,
                        ),
                        rating_size(
                            "Preview window inc/dec rating icon height:",
                            (2.0, 255.0),
                            |s| s.rating_context_sizes.preview_incdec_height,
                            |s, v| s.rating_context_sizes.preview_incdec_height = v,
                        ),
                    ],
                ),
                boxed(
                    "thumbnails",
                    vec![
                        float(
                            "Thumbnail like/dislike and numerical rating icon size: ",
                            (1.0, thumbnail_width),
                            |s| s.thumbnail_ratings.icon_size,
                            |s, v| s.thumbnail_ratings.icon_size = v,
                        ),
                        float(
                            "Thumbnail inc/dec rating height: ",
                            (2.0, thumbnail_width),
                            |s| s.thumbnail_ratings.incdec_height,
                            |s, v| s.thumbnail_ratings.incdec_height = v,
                        ),
                        check(
                            "Give thumbnail ratings a flat background: ",
                            |s| s.thumbnail_ratings.background,
                            |s, v| s.thumbnail_ratings.background = v,
                        ),
                        check(
                            "Always draw thumbnail numerical ratings collapsed: ",
                            |s| s.thumbnail_ratings.numerical_collapsed,
                            |s, v| s.thumbnail_ratings.numerical_collapsed = v,
                        ),
                    ],
                ),
                boxed(
                    "dialogs",
                    vec![
                        rating_size(
                            "Dialogs like/dislike and numerical rating icon size:",
                            (6.0, 128.0),
                            |s| s.rating_context_sizes.dialog_icon_size,
                            |s, v| s.rating_context_sizes.dialog_icon_size = v,
                        ),
                        rating_size(
                            "Dialogs inc/dec rating height:",
                            (12.0, 128.0),
                            |s| s.rating_context_sizes.dialog_incdec_height,
                            |s, v| s.rating_context_sizes.dialog_incdec_height = v,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "regex favourites",
            vec![opt(
                "",
                Kind::RegexFavourites,
                Rc::new(|settings| Value::RegexFavourites(settings.regex_favourites.clone())),
                Rc::new(|settings, value| match value {
                    Value::RegexFavourites(favourites) => {
                        settings.regex_favourites = favourites.clone();
                        Ok(())
                    }
                    _ => Err(wrong("regex favourites")),
                }),
            )],
        ),
        page(
            "shortcuts",
            vec![opt(
                "shortcuts",
                Kind::Shortcuts,
                Rc::new(|s| Value::Shortcuts(s.shortcuts.clone())),
                Rc::new(|s, value| match value {
                    Value::Shortcuts(shortcuts) => {
                        s.shortcuts.clone_from(shortcuts);
                        Ok(())
                    }
                    _ => Err(wrong("shortcuts")),
                }),
            )],
        ),
        page(
            "speed and memory",
            vec![Item::Box(
                "thumbnail cache",
                vec![
                    opt(
                        "Memory reserved for thumbnail cache:",
                        Kind::Bytes,
                        Rc::new(|s| {
                            let (amount, unit) =
                                crate::thumbnail_cache::raw_separated(s.thumbnail_cache.bytes);
                            Value::Bytes { amount, unit }
                        }),
                        Rc::new(|s, v| {
                            if let Value::Bytes { amount, unit } = v {
                                s.thumbnail_cache.bytes =
                                    crate::thumbnail_cache::combined(*amount, *unit);
                                Ok(())
                            } else {
                                Err(wrong("thumbnail cache bytes"))
                            }
                        }),
                    ),
                    duration(
                        "Thumbnail cache timeout:",
                        time(&[Unit::Days, Unit::Hours, Unit::Minutes], 300.0),
                        |s| s.thumbnail_cache.timeout as f64,
                        |s, v| s.thumbnail_cache.timeout = v as u64,
                    ),
                ],
            )],
        ),
        page(
            "system",
            vec![boxed(
                "system sleep",
                vec![
                    check(
                        "Allow wake-from-system-sleep detection:",
                        |s| s.network.detect_sleep,
                        |s, v| s.network.detect_sleep = v,
                    ),
                    int(
                        "After a wake from system sleep, wait this many seconds before allowing new network access:",
                        (0, 60),
                        |s| s.network.wake_delay_period as i64,
                        |s, v| s.network.wake_delay_period = v as u64,
                    ),
                ],
            )],
        ),
        page(
            "tag autocomplete tabs",
            vec![
                boxed(
                    "children tags",
                    vec![noneable(
                        "How many tags to show in the children tab: ",
                        none("show all", 40, (1, 1_000_000), None),
                        |s| s.tag_autocomplete_tabs.children_limit.map(|n| n as i64),
                        |s, n| s.tag_autocomplete_tabs.children_limit = n.map(|n| n as usize),
                    )],
                ),
                boxed(
                    "favourite tags",
                    vec![opt(
                        "These tags will appear in every tag autocomplete results dropdown, under the 'favourites' tab.",
                        Kind::FavouriteTags,
                        Rc::new(|settings| Value::FavouriteTags(settings.favourite_tags.clone())),
                        Rc::new(|settings, value| match value {
                            Value::FavouriteTags(tags) => {
                                settings.favourite_tags = tags.clone();
                                Ok(())
                            }
                            _ => Err(wrong("favourite tags")),
                        }),
                    )],
                ),
            ],
        ),
        page(
            "tag editing",
            vec![
                boxed(
                    "tag dialogs",
                    vec![
                        check(
                            "Use listbook instead of tabbed notebook for tag service panels: ",
                            |settings| settings.tag_editing.use_listbook,
                            |settings, value| settings.tag_editing.use_listbook = value,
                        ),
                        check(
                            "Remember last used default tag service in manage tag dialogs: ",
                            |settings| settings.tag_editing.remember_service,
                            |settings, value| settings.tag_editing.remember_service = value,
                        ),
                        tag_service(
                            "Default tag service in tag dialogs: ",
                            false,
                            |settings| settings.tag_editing.default_service.clone(),
                            |settings, service| settings.tag_editing.default_service = service,
                            |settings| !settings.tag_editing.remember_service,
                        ),
                        check(
                            "Show parent info by default on edit/write taglists: ",
                            |settings| settings.tag_editing.tag_list_show_parents,
                            |settings, value| settings.tag_editing.tag_list_show_parents = value,
                        ),
                        check(
                            "Show parents expanded by default on edit/write taglists: ",
                            |settings| settings.tag_editing.tag_list_expand_parents,
                            |settings, value| settings.tag_editing.tag_list_expand_parents = value,
                        ),
                        check(
                            "Show sibling info by default on edit/write taglists: ",
                            |settings| settings.tag_editing.tag_list_show_siblings,
                            |settings, value| settings.tag_editing.tag_list_show_siblings = value,
                        ),
                    ],
                ),
                boxed(
                    "tag edit autocomplete",
                    vec![
                        check(
                            "By default, select the first tag result with actual count in write-autocomplete: ",
                            |s| s.tag_editing.select_first_with_count,
                            |s, v| s.tag_editing.select_first_with_count = v,
                        ),
                        check(
                            "When pasting multiline content into a write-autocomplete, skip the yes/no check: ",
                            |s| s.tag_editing.skip_multiline_paste_confirmation,
                            |s, v| s.tag_editing.skip_multiline_paste_confirmation = v,
                        ),
                        check(
                            "Show parent info by default on edit/write autocomplete taglists: ",
                            |s| s.tag_editing.autocomplete_show_parents,
                            |s, v| s.tag_editing.autocomplete_show_parents = v,
                        ),
                        check(
                            "Show parents expanded by default on edit/write autocomplete taglists: ",
                            |s| s.tag_editing.autocomplete_expand_parents,
                            |s, v| s.tag_editing.autocomplete_expand_parents = v,
                        ),
                        check(
                            "Show sibling info by default on edit/write autocomplete taglists: ",
                            |s| s.tag_editing.autocomplete_show_siblings,
                            |s, v| s.tag_editing.autocomplete_show_siblings = v,
                        ),
                        int(
                            "Autocomplete list height: ",
                            (1, 128),
                            |s| i64::from(s.tag_editing.autocomplete_list_height),
                            |s, v| s.tag_editing.autocomplete_list_height = v as u32,
                        ),
                    ],
                ),
            ],
        ),
        page(
            "tag presentation",
            vec![
                boxed(
                    "tag banners",
                    vec![
                        opt(
                            "On thumbnail top:",
                            Kind::TagBanner(crate::tag_banner::Target::ThumbnailTop),
                            Rc::new(|s| Value::TagBanner(s.tag_summaries.thumbnail_top.clone())),
                            Rc::new(|s, value| match value {
                                Value::TagBanner(value) => {
                                    s.tag_summaries.thumbnail_top.clone_from(value);
                                    Ok(())
                                }
                                _ => Err("not a tag banner".into()),
                            }),
                        ),
                        opt(
                            "On thumbnail bottom-right:",
                            Kind::TagBanner(crate::tag_banner::Target::ThumbnailBottomRight),
                            Rc::new(|s| {
                                Value::TagBanner(s.tag_summaries.thumbnail_bottom_right.clone())
                            }),
                            Rc::new(|s, value| match value {
                                Value::TagBanner(value) => {
                                    s.tag_summaries.thumbnail_bottom_right.clone_from(value);
                                    Ok(())
                                }
                                _ => Err("not a tag banner".into()),
                            }),
                        ),
                        opt(
                            "On media viewer top:",
                            Kind::TagBanner(crate::tag_banner::Target::MediaViewerTop),
                            Rc::new(|s| Value::TagBanner(s.tag_summaries.media_viewer_top.clone())),
                            Rc::new(|s, value| match value {
                                Value::TagBanner(value) => {
                                    s.tag_summaries.media_viewer_top.clone_from(value);
                                    Ok(())
                                }
                                _ => Err("not a tag banner".into()),
                            }),
                        ),
                    ],
                ),
                boxed(
                    "selection tags",
                    vec![noneable(
                        "Max number of thumbnails to compute tags for when none are selected: ",
                        none("no limit", 4096, (0, 10_000_000), None),
                        |s| s.tag_presentation.unselected_tag_limit.map(i64::from),
                        |s, n| s.tag_presentation.unselected_tag_limit = n.map(|n| n as u32),
                    )],
                ),
                boxed(
                    "namespace rendering",
                    vec![
                        check(
                            "Show namespaces: ",
                            |s| s.tag_presentation.show_namespaces,
                            |s, v| s.tag_presentation.show_namespaces = v,
                        ),
                        check(
                            "Show namespace if it is a number: ",
                            |s| s.tag_presentation.show_number_namespaces,
                            |s, v| s.tag_presentation.show_number_namespaces = v,
                        ),
                        check(
                            "Show namespace if subtag is a number: ",
                            |s| s.tag_presentation.show_subtag_number_namespaces,
                            |s, v| s.tag_presentation.show_subtag_number_namespaces = v,
                        ),
                        text(
                            "If shown, namespace connecting string: ",
                            |s| s.tag_presentation.namespace_connector.clone(),
                            |s, t| {
                                t.clone_into(&mut s.tag_presentation.namespace_connector);
                                Ok(())
                            },
                        ),
                    ],
                ),
                boxed(
                    "other rendering",
                    vec![
                        text(
                            "Sibling connecting string: ",
                            |s| s.tag_presentation.sibling_connector.clone(),
                            |s, t| {
                                t.clone_into(&mut s.tag_presentation.sibling_connector);
                                Ok(())
                            },
                        ),
                        check(
                            "Fade the colour of the sibling connector string on Qt6: ",
                            |s| s.sibling_connector_colours.fade,
                            |s, value| s.sibling_connector_colours.fade = value,
                        ),
                        enabled(
                            noneable_text_default(
                                "Namespace for the colour of the sibling connecting string: ",
                                "use ideal tag colour",
                                "system",
                                |s| s.sibling_connector_colours.namespace.clone(),
                                |s, value| s.sibling_connector_colours.namespace = value,
                            ),
                            |s| !s.sibling_connector_colours.fade,
                        ),
                        opt(
                            "Namespace for the OR top row: ",
                            Kind::Text,
                            Rc::new(|s| {
                                Value::PlainNoneableText(s.namespace_colours.or_connector.clone())
                            }),
                            Rc::new(|s, value| match value {
                                Value::PlainNoneableText(text) => {
                                    s.namespace_colours.or_connector.clone_from(text);
                                    Ok(())
                                }
                                _ => Err(wrong("OR row namespace")),
                            }),
                        ),
                        check(
                            "EXPERIMENTAL: Replace all underscores with spaces: ",
                            |s| s.tag_presentation.replace_underscores,
                            |s, v| s.tag_presentation.replace_underscores = v,
                        ),
                        check(
                            "EXPERIMENTAL: Replace all emojis with □: ",
                            |s| s.tag_presentation.replace_emojis,
                            |s, v| s.tag_presentation.replace_emojis = v,
                        ),
                    ],
                ),
                boxed(
                    "namespace colours",
                    vec![opt(
                        "",
                        Kind::NamespaceColours,
                        Rc::new(|settings| {
                            Value::NamespaceColours(settings.namespace_colours.colours.clone())
                        }),
                        Rc::new(|settings, value| match value {
                            Value::NamespaceColours(colours) => {
                                settings.namespace_colours.colours.clone_from(colours);
                                Ok(())
                            }
                            _ => Err(wrong("namespace colours")),
                        }),
                    )],
                ),
                boxed(
                    "default taglist display type (advanced)",
                    vec![
                        boxed(
                            "Do not edit these unless you know exactly what they do!",
                            Vec::new(),
                        ),
                        choice(
                            "Tag display type for new page sidebar taglists: ",
                            &hydrus_core::tag_presentation::TagDisplayType::LABELS,
                            |s| s.tag_presentation.sidebar_display_type.choice(),
                            |s, n| {
                                if let Some(mode) =
                                    hydrus_core::tag_presentation::TagDisplayType::from_choice(n)
                                {
                                    s.tag_presentation.sidebar_display_type = mode;
                                }
                            },
                        ),
                        choice(
                            "Tag display type for new media viewer taglists: ",
                            &hydrus_core::tag_presentation::TagDisplayType::LABELS,
                            |s| s.tag_presentation.viewer_display_type.choice(),
                            |s, n| {
                                if let Some(mode) =
                                    hydrus_core::tag_presentation::TagDisplayType::from_choice(n)
                                {
                                    s.tag_presentation.viewer_display_type = mode;
                                }
                            },
                        ),
                    ],
                ),
            ],
        ),
        page(
            "tag sort",
            vec![boxed(
                "tag sort",
                vec![
                    tag_sort(
                        "Default tag sort in search pages: ",
                        |s| s.tag_presentation.search_page_sort,
                        |s, v| s.tag_presentation.search_page_sort = v,
                    ),
                    tag_sort(
                        "Default tag sort in the media viewer: ",
                        |s| s.tag_presentation.media_viewer_sort,
                        |s, v| s.tag_presentation.media_viewer_sort = v,
                    ),
                ],
            )],
        ),
        page(
            "tag suggestions",
            vec![boxed(
                "suggested tags",
                vec![
                    int(
                        "Width of suggested tags columns: ",
                        (20, 65535),
                        |s| i64::from(s.tag_suggestions.width),
                        |s, v| s.tag_suggestions.width = v as u32,
                    ),
                    choice(
                        "Column layout: ",
                        &["notebook", "side-by-side"],
                        |s| usize::from(s.tag_suggestions.columns),
                        |s, v| s.tag_suggestions.columns = v == 1,
                    ),
                    choice(
                        "Default notebook page: ",
                        &["most used", "related", "file_lookup_scripts", "recent"],
                        |s| {
                            ["favourites", "related", "file_lookup_scripts", "recent"]
                                .iter()
                                .position(|v| *v == s.tag_suggestions.default_page)
                                .unwrap_or(0)
                        },
                        |s, v| {
                            s.tag_suggestions.default_page =
                                ["favourites", "related", "file_lookup_scripts", "recent"][v]
                                    .into();
                        },
                    ),
                    opt(
                        "adjust scores by search tags",
                        Kind::RelatedWeights,
                        Rc::new(|s| Value::RelatedWeights(s.related_tags.weights.clone())),
                        Rc::new(|s, v| {
                            if let Value::RelatedWeights(weights) = v {
                                s.related_tags.weights.clone_from(weights);
                                Ok(())
                            } else {
                                Err(wrong("related tag weights"))
                            }
                        }),
                    ),
                    opt(
                        "Add your most used tags for each particular service here, and then you can just double-click to add, rather than typing every time.",
                        Kind::MostUsedTags,
                        Rc::new(|s| Value::MostUsedTags(s.tag_autocomplete_tabs.most_used.clone())),
                        Rc::new(|s, v| {
                            if let Value::MostUsedTags(tags) = v {
                                s.tag_autocomplete_tabs.most_used.clone_from(tags);
                                Ok(())
                            } else {
                                Err(wrong("most used tags"))
                            }
                        }),
                    ),
                ],
            )],
        ),
        page(
            "thumbnails",
            vec![
                boxed(
                    "appearance",
                    vec![
                        int(
                            "Thumbnail width: ",
                            (20, 2048),
                            |s| i64::from(s.thumbnails.bounding_width),
                            |s, v| s.thumbnails.bounding_width = v as u32,
                        ),
                        int(
                            "Thumbnail height: ",
                            (20, 2048),
                            |s| i64::from(s.thumbnails.bounding_height),
                            |s, v| s.thumbnails.bounding_height = v as u32,
                        ),
                        int(
                            "Thumbnail border: ",
                            (0, 20),
                            |s| i64::from(s.thumbnail_layout.border),
                            |s, v| s.thumbnail_layout.border = v as u32,
                        ),
                        int(
                            "Thumbnail margin: ",
                            (0, 20),
                            |s| i64::from(s.thumbnail_layout.margin),
                            |s, v| s.thumbnail_layout.margin = v as u32,
                        ),
                        choice(
                            "Thumbnail scaling: ",
                            THUMBNAIL_SCALES,
                            |s| {
                                THUMBNAIL_SCALE_ORDER
                                    .iter()
                                    .position(|t| *t == s.thumbnails.scale)
                                    .unwrap_or(0)
                            },
                            |s, i| s.thumbnails.scale = THUMBNAIL_SCALE_ORDER[i],
                        ),
                        int(
                            "Thumbnail UI-scale supersampling %: ",
                            (100, 800),
                            |s| i64::from(s.thumbnails.dpr_percent),
                            |s, v| s.thumbnails.dpr_percent = v as u32,
                        ),
                        int(
                            "Generate video thumbnails this % in: ",
                            (0, 100),
                            |s| i64::from(s.thumbnails.video_percentage_in),
                            |s, v| s.thumbnails.video_percentage_in = v as u32,
                        ),
                    ],
                ),
                boxed(
                    "interaction",
                    vec![
                        check(
                            "When a single thumbnail is selected, show the media viewer's normal top hover file text in the status bar: ",
                            |s| s.info_line.single_file_in_status_bar,
                            |s, v| s.info_line.single_file_in_status_bar = v,
                        ),
                        check(
                            "On ctrl-selection, focus thumbnails in the preview window: ",
                            |s| s.thumbnail_preview_selection.ctrl_focus,
                            |s, v| s.thumbnail_preview_selection.ctrl_focus = v,
                        ),
                        enabled(
                            check(
                                "  Only on files with no duration: ",
                                |s| s.thumbnail_preview_selection.ctrl_only_static,
                                |s, v| s.thumbnail_preview_selection.ctrl_only_static = v,
                            ),
                            |s| s.thumbnail_preview_selection.ctrl_focus,
                        ),
                        check(
                            "On shift-selection, focus thumbnails in the preview window: ",
                            |s| s.thumbnail_preview_selection.shift_focus,
                            |s, v| s.thumbnail_preview_selection.shift_focus = v,
                        ),
                        enabled(
                            check(
                                "  Only on files with no duration: ",
                                |s| s.thumbnail_preview_selection.shift_only_static,
                                |s, v| s.thumbnail_preview_selection.shift_only_static = v,
                            ),
                            |s| s.thumbnail_preview_selection.shift_focus,
                        ),
                        enabled(
                            check(
                                "When shift-selecting, move the \"navigate from here\" position with it: ",
                                |s| s.thumbnail_navigation.shift_moves_origin,
                                |s, v| s.thumbnail_navigation.shift_moves_origin = v,
                            ),
                            |s| {
                                !s.thumbnail_preview_selection.shift_focus
                                    || s.thumbnail_preview_selection.shift_only_static
                            },
                        ),
                        int(
                            "Do not scroll down on key navigation if thumbnail at least this % visible: ",
                            (1, 99),
                            |s| i64::from(s.thumbnail_navigation.visibility_percent),
                            |s, v| s.thumbnail_navigation.visibility_percent = v as u8,
                        ),
                        text(
                            "EXPERIMENTAL: Scroll thumbnails at this rate per scroll tick: ",
                            |s| s.thumbnail_navigation.scroll_rate.clone(),
                            |s, v| {
                                if crate::thumbnail_navigation::parse_rate(v).is_some() {
                                    v.clone_into(&mut s.thumbnail_navigation.scroll_rate);
                                }
                                Ok(())
                            },
                        ),
                    ],
                ),
            ],
        ),
        page(
            "advanced",
            vec![check(
                "Advanced mode: ",
                |s| s.advanced.0,
                |s, v| s.advanced.0 = v,
            )],
        ),
    ];
    // Qt sorts all regular pages, then appends advanced after SortList.
    pages.sort_by_key(|page| (page.name == "advanced", page.name));
    pages
}

/// The options' values as the controls start with them.
pub fn values(pages: &[Page], settings: &Settings) -> Vec<Vec<Value>> {
    pages
        .iter()
        .map(|page| {
            page.options()
                .iter()
                .map(|option| match (&option.kind, (option.get)(settings)) {
                    (Kind::Bytes, Value::Bytes { amount, unit }) => Value::Bytes {
                        amount: amount.clamp(0, 1_048_576),
                        unit: unit.min(4),
                    },
                    (Kind::Int { min, max }, Value::Int(number)) => {
                        Value::Int(number.clamp(*min, *max))
                    }
                    (Kind::Noneable { min, max, .. }, Value::Noneable(number)) => {
                        Value::Noneable(number.map(|number| number.clamp(*min, *max)))
                    }
                    (
                        Kind::NoneableDuration { units, min, .. },
                        Value::NoneableDuration { none, seconds },
                    ) => Value::NoneableDuration {
                        none,
                        seconds: duration_seconds(
                            &noneable_duration_fields(seconds.max(*min), units),
                            units,
                        ),
                    },
                    (_, value) => value,
                })
                .collect()
        })
        .collect()
}

/// `settings` with the values changed from what they were set: each
/// option the user changed is set, the rest left as they are; and why any
/// changed couldn't be (they are left as they were, as the reference
/// leaves slideshow durations it can't read).
pub fn applied(
    pages: &[Page],
    settings: &Settings,
    values: &[Vec<Value>],
) -> (Settings, Vec<String>) {
    let mut out = settings.clone();
    let mut problems = Vec::new();
    for (page, values) in pages.iter().zip(values) {
        for (option, value) in page.options().into_iter().zip(values) {
            if (option.get)(settings) != *value
                && let Err(why) = (option.set)(&mut out, value)
            {
                problems.push(why);
            }
        }
    }
    // The three idle controls expose minute values, so Qt UpdateOptions writes
    // their displayed floor/bounds even when the user did not edit a control.
    // Keep the imported raw seconds until this explicit acceptance boundary.
    for seconds in [
        &mut out.gui_idle.user_seconds,
        &mut out.gui_idle.mouse_seconds,
        &mut out.gui_idle.api_seconds,
    ] {
        *seconds = normalise_idle_timeout(*seconds);
    }
    (out, problems)
}

/// The search box's placeholder, as the reference's.
pub const SEARCH_PLACEHOLDER: &str = "Search options... (Experimental!)";

/// How many suggestions show at once (more scroll), as the reference's.
pub const SEARCH_SHOWN: usize = 10;

/// Something the options search offers, as the reference's does: a box's
/// title or an option's label, then its page ("text (page)"), and the row
/// it is on that page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub text: String,
    pub page: usize,
    pub row: usize,
}

/// What the options search offers, page by page and row by row.
pub fn suggestions(pages: &[Page]) -> Vec<Suggestion> {
    fn walk(items: &[Item], page: usize, name: &str, row: &mut usize, out: &mut Vec<Suggestion>) {
        for item in items {
            // Compound native editors have an internal row label, while Qt's
            // completer sees their actual embedded group-box titles.
            let labels: &[&str] = match item {
                Item::Box(title, _) => std::slice::from_ref(title),
                Item::Opt(option) => match option.kind {
                    Kind::OpenExternally => &["URL calls", "single file calls"],
                    Kind::Shortcuts => &["built-in hydrus shortcut sets", "custom user sets"],
                    _ => std::slice::from_ref(&option.label),
                },
            };
            for text in labels.iter().filter(|text| !text.is_empty()) {
                out.push(Suggestion {
                    text: format!("{text} ({name})"),
                    page,
                    row: *row,
                });
            }
            *row += 1;
            if let Item::Box(_, items) = item {
                walk(items, page, name, row, out);
            }
        }
    }
    let mut out = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        walk(&page.items, i, page.name, &mut 0, &mut out);
    }
    out
}

/// Searchable auxiliary labels and current combo values, captured on opening as
/// the reference captures its widget text when building its completer.
pub fn suggestions_with_values(pages: &[Page], values: &[Vec<Value>]) -> Vec<Suggestion> {
    fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Item>) {
        for item in items {
            out.push(item);
            if let Item::Box(_, children) = item {
                walk(children, out);
            }
        }
    }
    let mut out = suggestions(pages);
    for (page_index, page) in pages.iter().enumerate() {
        let mut option_index = 0;
        let mut rows = Vec::new();
        walk(&page.items, &mut rows);
        for (row, item) in rows.into_iter().enumerate() {
            let Item::Opt(option) = item else { continue };
            let value = &values[page_index][option_index];
            option_index += 1;
            let mut labels = Vec::new();
            match (&option.kind, value) {
                (Kind::Choice(items), Value::Choice(index)) => {
                    if let Some(text) = items.get(*index) {
                        labels.push(*text);
                    }
                }
                (Kind::SavedSession, Value::SavedSession(name)) => {
                    labels.push(name.as_deref().unwrap_or("just a blank page"));
                }
                (
                    Kind::Noneable {
                        none_phrase, unit, ..
                    },
                    _,
                ) => {
                    labels.push(*none_phrase);
                    labels.extend(*unit);
                }
                (Kind::NoneableText { none_phrase }, _) => labels.push(*none_phrase),
                (Kind::CanvasTicks, _) => {
                    labels.extend(["media views", "preview views", "client api views"]);
                }
                (Kind::Duration { units, .. }, _) => {
                    labels.extend(units.iter().map(|unit| unit.label()));
                }
                (
                    Kind::NoneableDuration {
                        units, none_phrase, ..
                    },
                    _,
                ) => {
                    labels.push(*none_phrase);
                    labels.extend(units.iter().map(|unit| unit.label()));
                }
                (Kind::Velocity { per, units, .. }, _) => {
                    labels.push(*per);
                    labels.extend(units.iter().map(|unit| unit.label()));
                }
                _ => {}
            }
            for label in labels.into_iter().filter(|label| !label.is_empty()) {
                out.push(Suggestion {
                    text: format!("{label} ({})", page.name),
                    page: page_index,
                    row,
                });
            }
        }
    }
    out
}

/// A row of the page shown: a box's title, or an option.
#[derive(Debug)]
pub enum Row<'a> {
    Title {
        title: &'static str,
        depth: usize,
    },
    Opt {
        option: &'a Opt,
        depth: usize,
        value: &'a Value,
        /// For a number that may be none, the number it shows.
        number: i64,
        enabled: bool,
    },
}

/// The options window's state: the page shown, and the values as edited.
#[derive(Debug)]
pub struct Editor {
    pages: Vec<Page>,
    before: Settings,
    values: Vec<Vec<Value>>,
    /// Each noneable number's number while it is none (as the reference's
    /// spin box keeps it).
    numbers: Vec<Vec<i64>>,
    page: usize,
    suggestions: Vec<Suggestion>,
    /// The rows gone to from the search (page, row): highlighted while the
    /// window is open, as the reference leaves them.
    found: std::collections::BTreeSet<(usize, usize)>,
}

impl Editor {
    pub fn new(settings: Settings) -> Self {
        let pages = pages(&settings);
        let values = values(&pages, &settings);
        let numbers = pages
            .iter()
            .zip(&values)
            .map(|(page, values)| {
                page.options()
                    .iter()
                    .zip(values)
                    .map(|(option, value)| match (&option.kind, value) {
                        (Kind::Noneable { default, .. }, Value::Noneable(n)) => {
                            n.unwrap_or(*default)
                        }
                        _ => 0,
                    })
                    .collect()
            })
            .collect();
        let suggestions = suggestions_with_values(&pages, &values);
        let default_page = pages.iter().position(|p| p.name == "gui").unwrap_or(0);
        let page = if settings.options_preferences.remember_panel {
            pages
                .iter()
                .position(|p| p.name == settings.options_preferences.last_panel)
                .unwrap_or(default_page)
        } else {
            default_page
        };
        Self {
            pages,
            before: settings,
            values,
            numbers,
            page,
            suggestions,
            found: std::collections::BTreeSet::new(),
        }
    }

    /// Resolve service controls when opening, as `BetterChoice.SetValue` falls
    /// back to its first item if a service has been removed. Capture their
    /// displayed service names for the options search at the same time.
    pub fn resolve_tag_services(&mut self, store: &hydrus_store::Store) {
        for (page_index, page) in self.pages.iter().enumerate() {
            for (option, value) in page.options().iter().zip(&mut self.values[page_index]) {
                let (Kind::TagService { combined }, Value::TagService(key)) = (&option.kind, value)
                else {
                    continue;
                };
                let choices = tag_service_choices(store, *combined);
                let chosen = choices
                    .iter()
                    .find(|(service, _)| service == &*key)
                    .or_else(|| choices.first());
                if let Some((service, name)) = chosen {
                    *key = service.clone();
                    let label = format!("{} ({})", option.label, page.name);
                    if let Some(row) = self
                        .suggestions
                        .iter()
                        .find(|suggestion| suggestion.text == label)
                        .map(|suggestion| suggestion.row)
                    {
                        self.suggestions.push(Suggestion {
                            text: format!("{name} ({})", page.name),
                            page: page_index,
                            row,
                        });
                    }
                }
            }
        }
    }

    /// Suggestions containing the query, ignoring case; none for no query.
    pub fn search(&self, query: &str) -> Vec<&Suggestion> {
        let query = query.to_lowercase();
        if query.is_empty() {
            return Vec::new();
        }
        self.suggestions
            .iter()
            .filter(|s| s.text.to_lowercase().contains(&query))
            .collect()
    }

    /// Show the page of a suggestion chosen, its row highlighted.
    pub fn go_to(&mut self, suggestion: &Suggestion) {
        self.show_page(suggestion.page);
        self.found.insert((suggestion.page, suggestion.row));
    }

    /// Whether the page shown's row was gone to from the search.
    pub fn found(&self, row: usize) -> bool {
        self.found.contains(&(self.page, row))
    }

    /// The page to remember, using the switch captured when this window opened.
    pub fn remembered_panel(&self) -> Option<&'static str> {
        self.before
            .options_preferences
            .remember_panel
            .then_some(self.pages[self.page].name)
    }

    pub fn page_names(&self) -> Vec<&'static str> {
        self.pages.iter().map(|p| p.name).collect()
    }

    pub fn page(&self) -> usize {
        self.page
    }

    pub fn show_page(&mut self, page: usize) {
        if page < self.pages.len() {
            self.page = page;
        }
    }

    /// The page shown's rows.
    pub fn rows(&self) -> Vec<Row<'_>> {
        fn walk<'a>(
            items: &'a [Item],
            depth: usize,
            values: &'a [Value],
            numbers: &[i64],
            settings: &Settings,
            next: &mut usize,
            out: &mut Vec<Row<'a>>,
        ) {
            for item in items {
                match item {
                    Item::Box(title, items) => {
                        out.push(Row::Title { title, depth });
                        walk(items, depth + 1, values, numbers, settings, next, out);
                    }
                    Item::Opt(option) => {
                        out.push(Row::Opt {
                            option,
                            depth,
                            value: &values[*next],
                            number: numbers[*next],
                            enabled: (option.enabled)(settings),
                        });
                        *next += 1;
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(
            &self.pages[self.page].items,
            0,
            &self.values[self.page],
            &self.numbers[self.page],
            &self.applied().0,
            &mut 0,
            &mut out,
        );
        out
    }

    /// Which option the page shown's row `row` is.
    fn option_at(&self, row: usize) -> Option<usize> {
        let rows = self.rows();
        let Row::Opt { .. } = rows.get(row)? else {
            return None;
        };
        Some(
            rows[..row]
                .iter()
                .filter(|r| matches!(r, Row::Opt { .. }))
                .count(),
        )
    }

    fn kind(&self, option: usize) -> &Kind {
        &self.pages[self.page].options()[option].kind
    }

    pub fn check(&mut self, row: usize, checked: bool) {
        if let Some(i) = self.option_at(row) {
            self.values[self.page][i] = Value::Check(checked);
        }
    }

    /// Toggle one reference viewing-canvas tick, retaining its displayed order.
    pub fn canvas(&mut self, row: usize, index: usize, checked: bool) {
        use hydrus_core::CanvasType;
        const CANVASES: [CanvasType; 3] = [
            CanvasType::MediaViewer,
            CanvasType::Preview,
            CanvasType::ClientApi,
        ];
        let Some(i) = self.option_at(row) else { return };
        let Some(canvas) = CANVASES.get(index) else {
            return;
        };
        let Value::Canvases(canvases) = &mut self.values[self.page][i] else {
            return;
        };
        canvases.retain(|c| c != canvas);
        if checked {
            canvases.push(*canvas);
        }
        canvases.sort_by_key(|c| CANVASES.iter().position(|v| v == c));
    }

    pub fn number(&mut self, row: usize, number: i64) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        match self.kind(i) {
            Kind::Bytes => {
                if let Value::Bytes { amount, .. } = &mut self.values[self.page][i] {
                    *amount = number.clamp(0, 1_048_576);
                }
            }
            Kind::Int { min, max } => {
                self.values[self.page][i] = Value::Int(number.clamp(*min, *max));
            }
            Kind::Velocity { .. } => {
                if let Value::Velocity(_, seconds) = self.values[self.page][i] {
                    self.values[self.page][i] = Value::Velocity(number, seconds);
                }
            }
            Kind::Noneable { min, max, .. } => {
                let number = number.clamp(*min, *max);
                self.numbers[self.page][i] = number;
                if let Value::Noneable(Some(_)) = self.values[self.page][i] {
                    self.values[self.page][i] = Value::Noneable(Some(number));
                }
            }
            _ => {}
        }
    }

    pub fn none(&mut self, row: usize, none: bool) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        let value = &mut self.values[self.page][i];
        if let Value::NoneableText { none: was, .. } | Value::NoneableDuration { none: was, .. } =
            value
        {
            *was = none;
        } else {
            let number = self.numbers[self.page][i];
            *value = Value::Noneable((!none).then_some(number));
        }
    }

    pub fn text(&mut self, row: usize, text: &str) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        let value = &mut self.values[self.page][i];
        *value = match (self.pages[self.page].options()[i].kind.clone(), &*value) {
            (Kind::Float { .. }, _) => Value::Float(text.to_owned()),
            (Kind::Text, Value::PlainNoneableText(_)) => {
                Value::PlainNoneableText(Some(text.to_owned()))
            }
            (Kind::NoneableText { .. }, Value::NoneableText { none, .. }) => Value::NoneableText {
                none: *none,
                text: text.to_owned(),
            },
            _ => Value::Text(text.to_owned()),
        };
    }

    /// A time's field (of a time, or a rate's time) set to `n`.
    pub fn field(&mut self, row: usize, field: usize, n: i64) {
        let Some(i) = self.option_at(row) else {
            return;
        };
        let units = match self.kind(i) {
            Kind::Duration { units, .. }
            | Kind::NoneableDuration { units, .. }
            | Kind::Velocity { units, .. } => *units,
            _ => return,
        };
        let set = |seconds: f64| {
            let mut fields = duration_fields(seconds, units);
            if let (Some(f), Some(unit)) = (fields.get_mut(field), units.get(field)) {
                *f = n.clamp(0, unit.max());
            }
            duration_seconds(&fields, units)
        };
        let value = &mut self.values[self.page][i];
        *value = match *value {
            Value::Duration(seconds) => Value::Duration(set(seconds)),
            Value::NoneableDuration { none, seconds } => Value::NoneableDuration {
                none,
                seconds: set(seconds),
            },
            Value::Velocity(number, seconds) => Value::Velocity(number, set(seconds)),
            _ => return,
        };
    }

    pub fn choose(&mut self, row: usize, index: usize) {
        if let Some(i) = self.option_at(row) {
            if let Value::Bytes { unit, .. } = &mut self.values[self.page][i] {
                *unit = index.min(4);
            } else {
                self.values[self.page][i] = Value::Choice(index);
            }
        }
    }

    /// Accept the location selector's draft even if another page is now shown.
    pub fn set_local_location(&mut self, location: hydrus_core::search::context::LocationContext) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::Location(_)) {
                *value = Value::Location(location);
                return;
            }
        }
    }

    pub fn saved_session(&mut self, row: usize, name: Option<String>) {
        if let Some(index) = self.option_at(row)
            && matches!(self.kind(index), Kind::SavedSession)
        {
            self.values[self.page][index] = Value::SavedSession(name);
        }
    }

    /// A service key chosen from the current editable dropdown.
    pub fn tag_service(&mut self, row: usize, service: hydrus_core::ServiceKey) {
        if !matches!(self.rows().get(row), Some(Row::Opt { enabled: true, .. })) {
            return;
        }
        if let Some(index) = self.option_at(row)
            && matches!(self.kind(index), Kind::TagService { .. })
        {
            self.values[self.page][index] = Value::TagService(service);
        }
    }

    /// A sort's type or order chosen.
    pub fn sort(&mut self, row: usize, sort: PageSort) {
        if let Some(i) = self.option_at(row)
            && matches!(self.values[self.page][i], Value::Sort(_))
        {
            self.values[self.page][i] = Value::Sort(sort);
        }
    }

    /// A tag sort's type, order or grouping chosen (`part` 0, 1 or 2; each
    /// by its place among its choices).
    pub fn tag_sort(&mut self, row: usize, part: usize, index: usize) {
        if let Some(i) = self.option_at(row)
            && let Value::TagSort(sort) = &self.values[self.page][i]
        {
            self.values[self.page][i] = Value::TagSort(tag_sort_chosen(sort, part, index));
        }
    }

    /// A collect's choice checked or not, or its unmatched files' choice.
    pub fn collect(&mut self, row: usize, collect: PageCollect) {
        if let Some(i) = self.option_at(row)
            && matches!(self.values[self.page][i], Value::Collect(_))
        {
            self.values[self.page][i] = Value::Collect(collect);
        }
    }

    /// Checker options edited (their editor's "ok").
    pub fn checker(&mut self, row: usize, options: CheckerOptions) {
        if let Some(i) = self.option_at(row)
            && matches!(self.values[self.page][i], Value::Checker(_))
        {
            self.values[self.page][i] = Value::Checker(options);
        }
    }

    /// The staged default downloader pair, independent of the selected page.
    pub fn edited_gallery_source(&self) -> Option<crate::gallery_source::KeyAndName> {
        self.values
            .iter()
            .flatten()
            .find_map(|value| match value {
                Value::GallerySource(current) => Some(current.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.before.gallery.gug.clone())
    }

    pub fn set_gallery_source(&mut self, current: Option<crate::gallery_source::KeyAndName>) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::GallerySource(_)) {
                *value = Value::GallerySource(current);
                return;
            }
        }
    }

    /// The current favourites draft, independent of the selected options page.
    pub fn edited_regex_favourites(&self) -> RegexFavourites {
        self.values
            .iter()
            .flatten()
            .find_map(|value| {
                if let Value::RegexFavourites(favourites) = value {
                    Some(favourites.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.before.regex_favourites.clone())
    }

    /// Accept the child list editor’s draft without writing the parent settings.
    pub fn set_regex_favourites(&mut self, favourites: RegexFavourites) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::RegexFavourites(_)) {
                *value = Value::RegexFavourites(favourites);
                return;
            }
        }
    }

    /// All defaults/profiles and their presentation preference, staged together.
    pub fn edited_import_options(&self) -> crate::import_options_panel::Value {
        self.values
            .iter()
            .flatten()
            .find_map(|value| {
                if let Value::ImportOptions(value) = value {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| crate::import_options_panel::Value {
                manager: self.before.import_options.clone(),
                ui: self.before.import_options_ui.clone(),
            })
    }
    pub fn set_import_options(&mut self, draft: crate::import_options_panel::Value) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::ImportOptions(_)) {
                *value = Value::ImportOptions(draft);
                return;
            }
        }
    }

    pub fn edited_banner(
        &self,
        row: usize,
    ) -> Option<(
        crate::tag_banner::Target,
        hydrus_core::tag_summary::TagSummaryGenerator,
    )> {
        let index = self.option_at(row)?;
        match (self.kind(index), &self.values[self.page][index]) {
            (Kind::TagBanner(target), Value::TagBanner(value)) => Some((*target, value.clone())),
            _ => None,
        }
    }

    /// A child can apply after its parent changes page; identify the original button.
    pub fn set_banner(
        &mut self,
        target: crate::tag_banner::Target,
        draft: hydrus_core::tag_summary::TagSummaryGenerator,
    ) {
        for (page, values) in self.pages.iter().zip(&mut self.values) {
            for (option, value) in page.options().iter().zip(values) {
                if matches!(option.kind, Kind::TagBanner(found) if found == target) {
                    *value = Value::TagBanner(draft);
                    return;
                }
            }
        }
    }

    pub fn edited_namespace_sorts(&self) -> Vec<PageSort> {
        self.values
            .iter()
            .flatten()
            .find_map(|value| match value {
                Value::NamespaceSorts(sorts) => Some(sorts.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.before.sorts.namespace_sorts.clone())
    }
    pub fn set_namespace_sorts(&mut self, sorts: Vec<PageSort>) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::NamespaceSorts(_)) {
                *value = Value::NamespaceSorts(sorts);
                return;
            }
        }
    }

    pub fn edited_shortcuts(&self) -> hydrus_core::shortcuts::Settings {
        self.values
            .iter()
            .flatten()
            .find_map(|value| match value {
                Value::Shortcuts(settings) => Some(settings.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.before.shortcuts.clone())
    }

    pub fn set_shortcuts(&mut self, settings: hydrus_core::shortcuts::Settings) {
        if let Some(value) = self
            .values
            .iter_mut()
            .flatten()
            .find(|v| matches!(v, Value::Shortcuts(_)))
        {
            *value = Value::Shortcuts(settings);
        }
    }

    pub fn edited_frame_locations(
        &self,
    ) -> std::collections::BTreeMap<String, hydrus_core::windows::FrameLocation> {
        self.values
            .iter()
            .flatten()
            .find_map(|v| match v {
                Value::FrameLocations(frames) => Some(frames.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.before.windows.frames())
    }
    pub fn set_frame_locations(
        &mut self,
        frames: std::collections::BTreeMap<String, hydrus_core::windows::FrameLocation>,
    ) {
        if let Some(value) = self
            .values
            .iter_mut()
            .flatten()
            .find(|v| matches!(v, Value::FrameLocations(_)))
        {
            *value = Value::FrameLocations(frames);
        }
    }

    /// Registered calls staged by the external programs table.
    pub fn edited_open_externally(&self) -> hydrus_core::open_externally::Routing {
        self.values
            .iter()
            .flatten()
            .find_map(|value| match value {
                Value::OpenExternally(routing) => Some(routing.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.before.open_externally.clone())
    }
    pub fn set_open_externally(&mut self, routing: hydrus_core::open_externally::Routing) {
        if let Some(value) = self
            .values
            .iter_mut()
            .flatten()
            .find(|value| matches!(value, Value::OpenExternally(_)))
        {
            *value = Value::OpenExternally(routing);
        }
    }
    pub fn edited_external_calls(&self) -> hydrus_core::external_calls::Manager {
        self.values
            .iter()
            .flatten()
            .find_map(|v| {
                if let Value::ExternalCalls(calls) = v {
                    Some(calls.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.before.external_calls.clone())
    }
    /// Replace only the external-call draft, regardless of the visible page.
    pub fn set_external_calls(&mut self, calls: hydrus_core::external_calls::Manager) {
        if let Some(v) = self
            .values
            .iter_mut()
            .flatten()
            .find(|v| matches!(v, Value::ExternalCalls(_)))
        {
            *v = Value::ExternalCalls(calls);
        }
    }

    /// Provider order staged independently of which options page is visible.
    /// Reason queue edits stay in the parent draft until Options applies.
    pub fn edited_deletion_reasons(&self) -> Vec<String> {
        self.values
            .iter()
            .flatten()
            .find_map(|v| {
                if let Value::DeletionReasons(reasons) = v {
                    Some(reasons.clone())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }
    pub fn set_deletion_reasons(&mut self, reasons: Vec<String>) {
        if let Some(value) = self
            .values
            .iter_mut()
            .flatten()
            .find(|v| matches!(v, Value::DeletionReasons(_)))
        {
            *value = Value::DeletionReasons(reasons);
        }
    }
    pub fn edited_provider_order(&self) -> Vec<Provider> {
        self.values
            .iter()
            .flatten()
            .find_map(|value| match value {
                Value::ProviderOrder(order) => Some(order.clone()),
                _ => None,
            })
            .unwrap_or_else(|| self.before.command_palette.provider_order.clone())
    }
    /// Accept queue edits into the parent options draft without writing preferences.
    pub fn set_provider_order(&mut self, order: Vec<Provider>) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::ProviderOrder(_)) {
                *value = Value::ProviderOrder(order);
                return;
            }
        }
    }

    /// The shared favourite tags draft, independent of the selected page.
    pub fn edited_favourite_tags(&self) -> FavouriteTags {
        self.values
            .iter()
            .flatten()
            .find_map(|value| {
                if let Value::FavouriteTags(tags) = value {
                    Some(tags.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.before.favourite_tags.clone())
    }

    /// Detached namespace tables; child acceptance stages, parent acceptance saves.
    pub fn edited_related_weights(&self) -> hydrus_store::related_tags::Weights {
        self.values
            .iter()
            .flatten()
            .find_map(|v| {
                if let Value::RelatedWeights(weights) = v {
                    Some(weights.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.before.related_tags.weights.clone())
    }
    pub fn set_related_weights(&mut self, weights: hydrus_store::related_tags::Weights) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::RelatedWeights(_)) {
                *value = Value::RelatedWeights(weights);
                return;
            }
        }
    }

    /// Per-service most-used draft; accepting a child never writes preferences.
    pub fn edited_most_used_tags(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        self.values
            .iter()
            .flatten()
            .find_map(|v| {
                if let Value::MostUsedTags(tags) = v {
                    Some(tags.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.before.tag_autocomplete_tabs.most_used.clone())
    }
    /// Replace only the staged most-used map; its writer merges changed services.
    pub fn set_most_used_tags(&mut self, tags: std::collections::BTreeMap<String, Vec<String>>) {
        for v in self.values.iter_mut().flatten() {
            if matches!(v, Value::MostUsedTags(_)) {
                *v = Value::MostUsedTags(tags);
                return;
            }
        }
    }

    /// Accept the child draft; only the parent Apply writes these tags.
    pub fn set_favourite_tags(&mut self, tags: &[String]) {
        let mut tags: Vec<String> = tags
            .iter()
            .filter_map(|tag| hydrus_core::Tag::new(tag))
            .map(|tag| tag.as_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        hydrus_core::sort::human_sort(&mut tags);
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::FavouriteTags(_)) {
                *value = Value::FavouriteTags(FavouriteTags(tags));
                return;
            }
        }
    }

    /// Current namespace RGB list staged independently of the OR namespace field.
    pub fn edited_namespace_colours(&self) -> crate::namespace_colours::Colours {
        self.values
            .iter()
            .flatten()
            .find_map(|value| {
                if let Value::NamespaceColours(colours) = value {
                    Some(colours.clone())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.before.namespace_colours.colours.clone())
    }
    /// Accept an owned namespace operation into the Options draft only.
    pub fn set_namespace_colours(&mut self, colours: crate::namespace_colours::Colours) {
        for value in self.values.iter_mut().flatten() {
            if matches!(value, Value::NamespaceColours(_)) {
                *value = Value::NamespaceColours(colours);
                return;
            }
        }
    }

    /// The settings as edited, those they started as, and why any edits
    /// couldn't be made.
    pub fn applied(&self) -> (Settings, &Settings, Vec<String>) {
        let (mut after, problems) = applied(&self.pages, &self.before, &self.values);
        if let Some(name) = self.remembered_panel() {
            name.clone_into(&mut after.options_preferences.last_panel);
        }
        (after, &self.before, problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        let dir = tempfile::tempdir().unwrap();
        let store = hydrus_store::Store::open(dir.path()).unwrap();
        store.read(Settings::load).unwrap()
    }

    #[test]
    fn only_the_options_changed_are_set() {
        let before = settings();
        let pages = pages(&before);
        let mut values = values(&pages, &before);
        let mut displayed = before.clone();
        // Qt's A-intensity spin box displays the minimum one for the saved
        // default zero. Every Options Apply accepts that normalization.
        displayed.duplicate_colours.intensity_a = Some(1);
        assert_eq!(applied(&pages, &before, &values), (displayed, vec![]));
        let trash = pages
            .iter()
            .position(|p| p.name == "files and trash")
            .unwrap();
        let trash_options = pages[trash].options();
        let age = trash_options
            .iter()
            .position(|option| {
                option.label
                    == "Number of hours a file will stay in the trash before being deleted: "
            })
            .unwrap();
        let copy = trash_options.iter().position(|option| option.label == "TEST: Import local files directly from source, do not copy to temp dir beforehand.").unwrap();
        values[trash][age] = Value::Noneable(None);
        values[trash][copy] = Value::Check(true);
        let (after, problems) = applied(&pages, &before, &values);
        assert!(problems.is_empty());
        assert_eq!(after.trash.max_age_hours, None);
        assert!(
            !after.folders.copy_import_files_to_temp_dir,
            "(the option is its opposite)"
        );
        assert_eq!(after.trash.max_size_mb, before.trash.max_size_mb);
    }

    #[test]
    #[allow(clippy::float_cmp)] // (values set, not computed)
    fn values_that_cant_be_had_are_left_saying_why() {
        let before = settings();
        let pages = pages(&before);
        let mut values = values(&pages, &before);
        let viewer = pages.iter().position(|p| p.name == "media viewer").unwrap();
        let option_index = |page: usize, label: &str| {
            pages[page]
                .options()
                .iter()
                .position(|option| option.label == label)
                .unwrap()
        };
        let durations = option_index(viewer, "Slideshow durations:");
        let once = option_index(viewer, "Always play media once through before moving on:");
        // (the rest is set regardless)
        values[viewer][durations] = Value::Text("1.0,soon".into());
        values[viewer][once] = Value::Check(true);
        let (after, problems) = applied(&pages, &before, &values);
        assert_eq!(
            problems,
            ["Could not parse those slideshow durations, so they were not saved!"]
        );
        assert_eq!(after.slideshow.durations, before.slideshow.durations);
        assert!(after.slideshow.once_through);
        // those above zero are kept; with none, they are left
        values[viewer][durations] = Value::Text("2.5, 7, 0".into());
        assert_eq!(
            applied(&pages, &before, &values).0.slideshow.durations,
            [2.5, 7.0]
        );
        values[viewer][durations] = Value::Text("0".into());
        assert_eq!(
            applied(&pages, &before, &values).0.slideshow.durations,
            before.slideshow.durations
        );
        // (media zooms are read as the reference reads them)
        let playback = pages
            .iter()
            .position(|p| p.name == "media playback")
            .unwrap();
        let zooms = option_index(playback, "Media zooms:");
        values[playback][zooms] = Value::Text("0.5,big".into());
        assert_eq!(
            applied(&pages, &before, &values).1,
            ["Could not parse those zooms, so they were not saved!"]
        );
        values[playback][zooms] = Value::Text("0.5, 2".into());
        assert_eq!(
            applied(&pages, &before, &values).0.media_viewer.media_zooms,
            [0.5, 2.0]
        );
        let ratings = pages.iter().position(|p| p.name == "ratings").unwrap();
        let rating_size = option_index(
            ratings,
            "Media viewer like/dislike and numerical rating icon size:",
        );
        for bad in ["big", "300"] {
            values[ratings][rating_size] = Value::Float(bad.into());
            let (after, problems) = applied(&pages, &before, &values);
            assert_eq!(problems.len(), 1, "{bad}");
            assert_eq!(
                after.media_viewer.rating_icon_size,
                before.media_viewer.rating_icon_size
            );
        }
        values[ratings][rating_size] = Value::Float("16.5".into());
        assert_eq!(
            applied(&pages, &before, &values)
                .0
                .media_viewer
                .rating_icon_size,
            16.5
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // (values set, not computed)
    fn thumbnail_rating_sizes_go_up_to_the_thumbnails_width() {
        let mut before = settings();
        before.thumbnails.bounding_width = 200;
        let pages = pages(&before);
        let ratings = pages.iter().position(|p| p.name == "ratings").unwrap();
        let mut values = values(&pages, &before);
        let indices = [
            "Thumbnail like/dislike and numerical rating icon size: ",
            "Thumbnail inc/dec rating height: ",
            "Give thumbnail ratings a flat background: ",
            "Always draw thumbnail numerical ratings collapsed: ",
        ]
        .map(|label| {
            pages[ratings]
                .options()
                .iter()
                .position(|option| option.label == label)
                .unwrap()
        });
        values[ratings][indices[0]] = Value::Float("200".into());
        values[ratings][indices[1]] = Value::Float("201".into());
        values[ratings][indices[2]] = Value::Check(false);
        values[ratings][indices[3]] = Value::Check(true);
        let (after, problems) = applied(&pages, &before, &values);
        assert_eq!(
            problems,
            ["Thumbnail inc/dec rating height: must be from 2 to 200"]
        );
        let ratings_after = after.thumbnail_ratings;
        assert_eq!(ratings_after.icon_size, 200.0);
        assert_eq!(
            ratings_after.incdec_height,
            before.thumbnail_ratings.incdec_height
        );
        assert!(!ratings_after.background);
        assert!(ratings_after.numerical_collapsed);
        values[ratings][indices[1]] = Value::Float("1.5".into());
        assert_eq!(applied(&pages, &before, &values).1.len(), 1, "below 2");
        values[ratings][indices[1]] = Value::Float("2".into());
        let (after, problems) = applied(&pages, &before, &values);
        assert!(problems.is_empty());
        assert_eq!(after.thumbnail_ratings.incdec_height, 2.0);
        // (and they show as they are)
        let shown = super::values(&pages, &after);
        assert_eq!(
            indices.map(|index| shown[ratings][index].clone()),
            [
                Value::Float("200.0".into()),
                Value::Float("2.0".into()),
                Value::Check(false),
                Value::Check(true),
            ]
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // (whole numbers of ms)
    fn times_show_as_the_references_fields() {
        let dhms = [Unit::Days, Unit::Hours, Unit::Minutes, Unit::Seconds];
        assert_eq!(duration_fields(129_600.0, &dhms), [1, 12, 0, 0]);
        assert_eq!(duration_fields(5400.0, &dhms), [0, 1, 30, 0]);
        let sms = [Unit::Seconds, Unit::Milliseconds];
        assert_eq!(duration_fields(0.1, &sms), [0, 100]);
        assert_eq!(duration_fields(30.0, &sms), [30, 0]);
        assert_eq!(duration_fields(2.25, &sms), [2, 250]);
        // (a unit without the larger ones takes what it can hold)
        let hms = [Unit::Hours, Unit::Minutes, Unit::Seconds];
        assert_eq!(duration_fields(600.0, &hms), [0, 10, 0]);
        assert_eq!(duration_fields(100_000.0, &hms)[0], 23);
        assert_eq!(duration_seconds(&[1, 12, 0, 0], &dhms), 129_600.0);
        assert_eq!(duration_seconds(&[2, 250], &sms), 2.25);
    }

    #[test]
    fn times_rates_and_text_that_may_be_none_are_edited() {
        let before = settings();
        let mut editor = Editor::new(before.clone());
        let page = |editor: &mut Editor, name: &str| {
            let at = editor.page_names().iter().position(|n| *n == name).unwrap();
            editor.show_page(at);
        };
        let row = |editor: &Editor, label: &str| {
            editor
                .rows()
                .iter()
                .position(|r| matches!(r, Row::Opt { option, .. } if option.label == label))
                .unwrap()
        };
        page(&mut editor, "downloading");
        // a day more on a wait of 1 hour 30 minutes
        let wait = row(&editor, "Delay time on a gallery/watcher network error:");
        editor.field(wait, 0, 1);
        // less than the least is the least
        let other = row(&editor, "Delay time on a subscription other error:");
        for field in 0..4 {
            editor.field(other, field, 0);
        }
        page(&mut editor, "connection");
        let errors = row(
            &editor,
            "Halt new jobs as long as this many network infrastructure errors on their domain (0 for never wait): ",
        );
        editor.number(errors, 7);
        editor.field(errors, 1, 2);
        let http = row(&editor, "http: ");
        editor.text(http, "http://127.0.0.1:8080");
        editor.none(http, false);
        // the text is kept while none
        let no_proxy = row(&editor, "no_proxy: ");
        editor.none(no_proxy, true);
        let (after, _, problems) = editor.applied();
        assert!(problems.is_empty(), "{problems:?}");
        let n = &after.network;
        assert_eq!(
            n.downloader_network_error_delay,
            before.network.downloader_network_error_delay + 86400
        );
        assert_eq!(n.subscription_other_error_delay, 600);
        assert_eq!(n.domain_error_number, 7);
        assert_eq!(
            n.domain_error_window,
            2 * 60 + before.network.domain_error_window % 60
        );
        assert_eq!(n.http_proxy.as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(n.no_proxy, None);
        editor.none(no_proxy, false);
        assert_eq!(editor.applied().0.network.no_proxy, before.network.no_proxy);
    }

    #[test]
    fn the_search_finds_options_by_their_labels_and_boxes() {
        let mut editor = Editor::new(settings());
        let texts = |editor: &Editor, query| -> Vec<String> {
            editor
                .search(query)
                .iter()
                .map(|s| s.text.clone())
                .collect()
        };
        assert!(texts(&editor, "").is_empty());
        // (any case, anywhere in the text, the page's name too)
        assert_eq!(
            texts(&editor, "PROXY"),
            ["proxy settings (connection)", "no_proxy:  (connection)"]
        );
        assert!(texts(&editor, "(media viewer hovers)").len() > 5);
        // gone to: its page shown, its row highlighted
        let found = editor.search("Maximum size of trash")[0].clone();
        editor.go_to(&found);
        assert_eq!(editor.page_names()[editor.page()], "files and trash");
        let Row::Opt { option, .. } = &editor.rows()[found.row] else {
            panic!("an option's row");
        };
        assert_eq!(option.label, "Maximum size of trash (MB): ");
        assert!(editor.found(found.row) && !editor.found(found.row + 1));
    }

    #[test]
    fn the_editor_edits_the_page_shown() {
        let before = settings();
        let mut editor = Editor::new(before.clone());
        let trash = editor
            .page_names()
            .iter()
            .position(|n| *n == "files and trash")
            .unwrap();
        editor.show_page(trash);
        let rows = editor.rows();
        // Each reference box title is a separate row.
        let titles: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Title { title, depth } => Some((*title, *depth)),
                Row::Opt { .. } => None,
            })
            .collect();
        assert_eq!(
            titles,
            [
                ("delete lock", 0),
                ("advanced file deletion and custom reasons", 0)
            ]
        );
        let find = |label: &str| {
            rows.iter()
                .position(|row| matches!(row, Row::Opt { option, .. } if option.label == label))
                .unwrap()
        };
        let age = find("Number of hours a file will stay in the trash before being deleted: ");
        let archived = find("Do not permit archived files to be deleted from the trash: ");
        let title = rows
            .iter()
            .position(|row| matches!(row, Row::Title { title, .. } if *title == "delete lock"))
            .unwrap();
        assert!(matches!(rows[archived], Row::Opt { depth: 1, .. }));
        // a noneable number keeps its number while none
        editor.none(age, true);
        editor.number(age, 99);
        editor.check(archived, true);
        let (after, _, problems) = editor.applied();
        assert!(problems.is_empty());
        assert_eq!(after.trash.max_age_hours, None);
        assert!(after.delete_lock.archived);
        editor.none(age, false);
        assert_eq!(editor.applied().0.trash.max_age_hours, Some(99));
        // (a title is no option)
        editor.check(title, true);
        assert_eq!(editor.applied().0.file_handling, before.file_handling);
    }
}
