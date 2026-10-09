//! The application display name on every window's title, as Qt does for the
//! reference (`setApplicationDisplayName`): a window titled "edit rules" is
//! shown as "edit rules - hydrus client 688".
//!
//! Each secondary window has a `title-suffix` property its title ends with.
//! [`new`] makes a window with the current suffix and remembers it, so
//! changing the name in Options retitles the windows already open too
//! (`UpdateAppDisplayName`).
use std::cell::RefCell;

use slint::{ComponentHandle, SharedString};

type Retitle = Box<dyn Fn(&SharedString) -> bool>;

thread_local! {
    static SUFFIX: RefCell<SharedString> = RefCell::new(SharedString::new());
    /// Each window made, as something that sets its suffix; false once it is gone.
    static WINDOWS: RefCell<Vec<Retitle>> = const { RefCell::new(Vec::new()) };
}

/// The title suffix for the display name `name` (with the version, as the
/// main window's own title has it).
pub fn suffix_for(name: &str) -> String {
    format!(" - {name} {}", env!("CARGO_PKG_VERSION"))
}

/// Make windows opened from now on end their titles with the display name.
pub fn set_display_name(name: &str) {
    let suffix: SharedString = suffix_for(name).into();
    SUFFIX.with(|s| s.borrow_mut().clone_from(&suffix));
    WINDOWS.with(|windows| windows.borrow_mut().retain(|retitle| retitle(&suffix)));
}

/// The suffix windows made now get.
pub fn current() -> SharedString {
    SUFFIX.with(|s| s.borrow().clone())
}

/// A window with a title suffix.
pub trait Titled: ComponentHandle + Sized + 'static {
    /// Make the window.
    fn create() -> Result<Self, slint::PlatformError>;
    /// Set its title suffix.
    fn set_suffix(&self, suffix: SharedString);
}

/// A new `C`, its title ending with the application display name.
///
/// # Errors
/// If the window cannot be made.
pub fn new<C: Titled>() -> Result<C, slint::PlatformError> {
    let window = C::create()?;
    window.set_suffix(current());
    let weak = window.as_weak();
    WINDOWS.with(|windows| {
        windows.borrow_mut().push(Box::new(move |suffix| {
            weak.upgrade().is_some_and(|window| {
                window.set_suffix(suffix.clone());
                true
            })
        }));
    });
    Ok(window)
}

macro_rules! titled {
    ($($t:ident),* $(,)?) => {
        $(impl Titled for crate::$t {
            fn create() -> Result<Self, slint::PlatformError> {
                crate::$t::new()
            }
            fn set_suffix(&self, suffix: SharedString) {
                self.set_title_suffix(suffix);
            }
        })*
    };
}

titled!(
    AboutWindow,
    ApiRequestWindow,
    ArchiveDeleteWindow,
    ArchiveRepairWindow,
    AutoResolutionReviewWindow,
    AutoResolutionRuleWindow,
    AutoResolutionRulesWindow,
    BandwidthWindow,
    CheckerOptionsWindow,
    ChoiceButtonsWindow,
    ClientApiKeysWindow,
    CommandPaletteWindow,
    ComparatorWindow,
    ConversionWindow,
    CookieImportWindow,
    DatabaseLocationsWindow,
    DateTimeEditorWindow,
    DebugFetchWindow,
    DeleteFilesWindow,
    DownloaderDefinitionEditWindow,
    DownloaderDefinitionsWindow,
    DownloaderDisplayWindow,
    DownloaderExchangeWindow,
    DuplicateFilterWindow,
    EditApiPermissionsWindow,
    EditBandwidthRulesWindow,
    EditNetworkValueWindow,
    EditServiceWindow,
    EditSubscriptionWindow,
    EditValueWindow,
    EmbeddedMetadataWindow,
    ExportFilesWindow,
    ExportFolderWindow,
    ExternalCallWindow,
    ExternalCommandWindow,
    ExternalDefaultsWindow,
    ExternalRoutingChoiceWindow,
    FavouriteEditWindow,
    FavouritesWindow,
    FileHistoryWindow,
    FileLogWindow,
    FileMaintenanceWindow,
    FilenameTaggingWindow,
    FoldersWindow,
    ForceFiletypeWindow,
    FormulaRuleWindow,
    FormulaWindow,
    FrameLocationWindow,
    GallerySourceWindow,
    GranularityWindow,
    GuiColourPickerWindow,
    HeaderApprovalWindow,
    HowBonedWindow,
    ImportFavouritePromptWindow,
    ImportFolderWindow,
    ImportOptionsOverwriteWindow,
    ImportOptionsPanelWindow,
    ImportOptionsWindow,
    IncrementalTaggingWindow,
    LocalTransferWindow,
    LocationMaxSizeWindow,
    LocationsWindow,
    LoginCookiesWindow,
    LoginCredentialDefinitionWindow,
    LoginCredentialsWindow,
    LoginDomainEntryWindow,
    LoginDomainsWindow,
    LoginExampleDomainWindow,
    LoginScriptWindow,
    LoginScriptsWindow,
    LoginStepWindow,
    LoginTestResultWindow,
    ManageNotesWindow,
    ManageRatingsWindow,
    ManageTagsWindow,
    ManageTimesWindow,
    ManageUrlsWindow,
    MediaViewWindow,
    MediaViewerWindow,
    MergeOptionsWindow,
    MostUsedTagsWindow,
    NamespaceSortsWindow,
    NetworkDataWindow,
    NetworkErrorWindow,
    NetworkJobsWindow,
    OpenFileCallsWindow,
    OptionsWindow,
    ParserEditWindow,
    ParserListWindow,
    ParserPickerWindow,
    PngExportWindow,
    PopupModalWindow,
    PredicateEditorWindow,
    RegexFavouritesWindow,
    RelatedWeightsWindow,
    ReviewImportsWindow,
    SearchLogImportWindow,
    SearchOrWindow,
    ServicesEditorWindow,
    ServicesReviewWindow,
    SessionDialog,
    ShortcutCommandWindow,
    ShortcutSetWindow,
    SidecarNodeWindow,
    SidecarRouterWindow,
    SidecarRoutersWindow,
    SimpleFormulaeWindow,
    StringConverterWindow,
    StringProcessorWindow,
    StringStepWindow,
    SubscriptionGalleryWindow,
    SubscriptionsWindow,
    TagBannerWindow,
    TagDisplayWindow,
    TagFilterWindow,
    TagMigrationProgressWindow,
    TagMigrationWindow,
    TagRelationshipsWindow,
    TagSyncReviewWindow,
    UnlockWindow,
    VacuumReviewWindow,
    WriteTagsWindow,
);
