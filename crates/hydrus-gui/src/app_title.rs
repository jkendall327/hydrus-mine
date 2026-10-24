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
/// A window made by [`new`], as something that lends its `slint::Window`
/// (false once it is gone).
type Lend = std::rc::Rc<dyn Fn(&mut dyn FnMut(&slint::Window)) -> bool>;

thread_local! {
    static SUFFIX: RefCell<SharedString> = RefCell::new(SharedString::new());
    /// Each window made, as something that sets its suffix; false once it is gone.
    static WINDOWS: RefCell<Vec<Retitle>> = const { RefCell::new(Vec::new()) };
    /// The same windows, for hiding them.
    static LENT: RefCell<Vec<Lend>> = const { RefCell::new(Vec::new()) };
}

/// Windows hidden by [`hide_others`].
#[derive(Default)]
pub struct Hidden(Vec<Lend>);

impl std::fmt::Debug for Hidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hidden")
            .field("windows", &self.0.len())
            .finish()
    }
}

impl Hidden {
    /// Show them again (`_RestoreOtherWindows`).
    pub fn restore(self) {
        for lend in self.0 {
            lend(&mut |window| {
                let _ = window.show();
            });
        }
    }
}

/// Hide every window made by [`new`] that is shown and not minimised, except
/// `keep` (`PopupMessageDialogPanel._HideOtherWindows`; the main window is
/// not one of them, and is not hidden).
pub fn hide_others(keep: &slint::Window) -> Hidden {
    let mut hidden = Vec::new();
    LENT.with(|lent| {
        lent.borrow_mut().retain(|lend| {
            let mut hide = false;
            let alive = lend(&mut |window| {
                if std::ptr::eq(window, keep)
                    || !window.is_visible()
                    || crate::popup_freeze::minimized(window) == Some(true)
                {
                    return;
                }
                hide = window.hide().is_ok();
            });
            if hide {
                hidden.push(lend.clone());
            }
            alive
        });
    });
    Hidden(hidden)
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
    // (where it opens: as the reference's ordinary dialogs, unless it has a
    // frame of its own)
    crate::windows::place_regular(&window);
    let weak = window.as_weak();
    LENT.with(|lent| {
        let weak = weak.clone();
        lent.borrow_mut().push(std::rc::Rc::new(move |with| {
            weak.upgrade().is_some_and(|window| {
                with(window.window());
                true
            })
        }));
    });
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
    QuickTagEntryWindow,
    ClientApiKeysWindow,
    CommandPaletteWindow,
    ComparatorWindow,
    ConversionWindow,
    CookieImportWindow,
    DatabaseLocationsWindow,
    DateTimeEditorWindow,
    DebugFetchWindow,
    WhatIsThisWindow,
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
    ExternalProgramEntryWindow,
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
    PngsExportWindow,
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
