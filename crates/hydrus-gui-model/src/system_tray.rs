//! When the system tray icon exists and what each way of leaving the window
//! does, as `ClientGUI` and `ClientSystemTrayIcon` decide them
//! (`oracle/fixtures/system_tray.json`).

/// The three switches of Options > system tray that choose a behaviour, and
/// "always show".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Options {
    pub always_show: bool,
    pub minimise: bool,
    pub close: bool,
    pub start: bool,
}

pub const ALWAYS_SHOW: &str = "always_show_system_tray_icon";
pub const MINIMISE: &str = "minimise_client_to_system_tray";
pub const CLOSE: &str = "close_client_to_system_tray";
pub const START: &str = "start_client_in_system_tray";

impl Options {
    /// The options as the store keeps them.
    pub fn read(options: &hydrus_store::reference_options::ReferenceOptions) -> Self {
        Self {
            always_show: options.boolean(ALWAYS_SHOW),
            minimise: options.boolean(MINIMISE),
            close: options.boolean(CLOSE),
            start: options.boolean(START),
        }
    }
}

/// `_UpdateSystemTrayIcon`: the icon exists while it is always shown or the
/// client is hidden to it, and only where there is a tray.
pub fn needs_icon(available: bool, always_show: bool, hidden: bool) -> bool {
    available && (always_show || hidden)
}

/// What the main window's close request does (`closeEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    /// Hide to the tray, keeping the client running.
    HideToTray,
    /// Ask to exit, as ever.
    Exit,
}

pub fn close(available: bool, close_to_tray: bool) -> Close {
    if available && close_to_tray {
        Close::HideToTray
    } else {
        Close::Exit
    }
}

/// Whether the client boots hidden to the tray.
pub fn starts_hidden(available: bool, start_in_tray: bool) -> bool {
    available && start_in_tray
}

/// Whether minimising the main window hides it to the tray (`changeEvent`).
pub fn minimise_hides(available: bool, minimise_to_tray: bool) -> bool {
    available && minimise_to_tray
}

/// What a click on the icon does (`_SystemTrayActivation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Hidden: show the window and bring it forward.
    ShowAndRaise,
    /// Active, and minimising goes to the tray: hide to it.
    HideToTray,
    /// Active otherwise: minimise to the taskbar.
    Minimise,
    /// Shown but not in front: bring it forward.
    Raise,
}

pub fn click(hidden: bool, active: bool, minimise_to_tray: bool) -> Click {
    if hidden {
        Click::ShowAndRaise
    } else if active {
        if minimise_to_tray {
            Click::HideToTray
        } else {
            Click::Minimise
        }
    } else {
        Click::Raise
    }
}

/// File > minimise to system tray (`_menubar_file_minimise_to_system_tray`):
/// shown with a tray, on Windows or in advanced mode.
pub fn file_entry_visible(available: bool, windows: bool, advanced: bool) -> bool {
    available && (windows || advanced)
}

pub const FILE_ENTRY: &str = "minimise to system tray";
pub const FILE_ENTRY_TIP: &str = "Hide the client to an icon on your system tray.";

/// The icon's first menu entry, by whether the window is shown.
pub fn show_hide_label(ui_shown: bool) -> &'static str {
    if ui_shown {
        "hide to system tray"
    } else {
        "show"
    }
}

/// The icon's tooltip (`_UpdateTooltip`).
pub fn tooltip(app_display_name: &str, network_paused: bool, subscriptions_paused: bool) -> String {
    let mut tooltip = app_display_name.to_owned();
    if network_paused {
        tooltip.push_str(" - network traffic paused");
    }
    if subscriptions_paused {
        tooltip.push_str(" - subscriptions paused");
    }
    tooltip
}
