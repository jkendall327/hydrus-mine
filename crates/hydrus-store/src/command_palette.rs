//! Command palette preferences shared by the options editor and live providers.

use crate::settings::Setting;

/// A provider's stable identity, independent of the user's display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Provider {
    Calculator,
    MainMenu,
    MediaMenu,
    Pages,
    History,
    Favourites,
}

impl Provider {
    /// Reference registration order, also the order of missing providers in the add dialog.
    pub const ALL: [Self; 6] = [
        Self::Calculator,
        Self::MainMenu,
        Self::MediaMenu,
        Self::Pages,
        Self::History,
        Self::Favourites,
    ];

    /// Convert a preserved reference provider code.
    pub fn from_code(code: usize) -> Option<Self> {
        Self::ALL.get(code).copied()
    }

    /// The label in the provider order editor.
    pub fn name(self) -> &'static str {
        match self {
            Self::Calculator => "calculator",
            Self::MainMenu => "main menu",
            Self::MediaMenu => "media menu",
            Self::Pages => "open pages",
            Self::History => "page history",
            Self::Favourites => "favourite searches",
        }
    }

    /// Provider heading in the live palette; the calculator has no heading.
    pub fn title(self) -> &'static str {
        match self {
            Self::Calculator => "",
            Self::MainMenu => "Main Menu",
            Self::MediaMenu => "Media",
            Self::Pages => "Pages",
            Self::History => "Recent Tab History",
            Self::Favourites => "Favourite Searches",
        }
    }
}

/// Persisted preferences. Empty queries and typed-query thresholds are separate policies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CommandPaletteSettings {
    pub show_notebooks: bool,
    pub initially_show_pages: bool,
    pub initially_show_history: bool,
    pub initially_show_favourites: bool,
    pub favourites_new_page: bool,
    pub show_main_menu: bool,
    pub show_media_menu: bool,
    pub threshold: usize,
    pub page_limit: Option<usize>,
    pub history_limit: Option<usize>,
    pub favourite_limit: Option<usize>,
    pub provider_order: Vec<Provider>,
}

impl Default for CommandPaletteSettings {
    fn default() -> Self {
        Self {
            show_notebooks: false,
            initially_show_pages: true,
            initially_show_history: true,
            initially_show_favourites: false,
            favourites_new_page: true,
            show_main_menu: false,
            show_media_menu: false,
            threshold: 1,
            page_limit: None,
            history_limit: Some(10),
            favourite_limit: None,
            provider_order: vec![
                Provider::Calculator,
                Provider::MainMenu,
                Provider::MediaMenu,
                Provider::History,
                Provider::Pages,
                Provider::Favourites,
            ],
        }
    }
}

impl Setting for CommandPaletteSettings {
    const KEY: &'static str = "command_palette";
}
