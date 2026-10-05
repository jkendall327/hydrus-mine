//! Options > shortcuts' set lists (`ShortcutsPanel`): the built-in sets
//! and the custom ones, their names and sizes, adding, renaming and
//! deleting custom sets, restoring a built-in set's defaults, and the
//! words each says (`oracle/dump_shortcut_sets.py`).

use std::collections::{BTreeMap, BTreeSet};

use hydrus_core::shortcuts::{Binding, Settings, default_sets};

/// The built-in sets (`SHORTCUTS_RESERVED_NAMES`).
pub const RESERVED: [&str; 11] = [
    "global",
    "archive_delete_filter",
    "duplicate_filter",
    "media",
    "thumbnails",
    "tags_autocomplete",
    "main_gui",
    "media_viewer_browser",
    "media_viewer",
    "media_viewer_media_window",
    "preview_media_window",
];

/// The order the built-in list sorts them in (`shortcut_names_sorted`).
pub const SORTED: [&str; 11] = [
    "global",
    "main_gui",
    "tags_autocomplete",
    "media",
    "thumbnails",
    "media_viewer",
    "media_viewer_browser",
    "archive_delete_filter",
    "duplicate_filter",
    "preview_media_window",
    "media_viewer_media_window",
];

pub const RESERVED_BOX: &str = "built-in hydrus shortcut sets";
pub const CUSTOM_BOX: &str = "custom user sets";
pub const COLUMNS: [&str; 2] = ["name", "number of shortcuts"];
pub const CUSTOM_MESSAGE: &str = "Custom shortcuts are advanced. They apply to the media viewer and must be turned on to take effect.";
pub const DELETE_QUESTION: &str = "Remove all selected?";
pub const NEW_NAME: &str = "new shortcuts";
pub const RESTORE_TITLE: &str = "select which default to restore";
pub const HELP_TOOLTIP: &str = "Show help regarding editing shortcuts.";
pub const EDITOR_NOTE: &str = "Please note the shortcut system does not support multiple commands per shortcut yet. If there are shortcut duplicates in this list, only one command will ever fire.";
pub const HELP: &str = "I am in the process of converting the multiple old messy shortcut systems to this single unified engine. Many actions are not yet available here, and mouse support is very limited. I expect to overwrite the reserved shortcut sets back to (new and expanded) defaults at least once more, so don't remap everything yet unless you are ok with doing it again.\n\n---\n\nIn hydrus, shortcuts are split into different sets that are active in different contexts. Depending on where the program focus is, multiple sets can be active at the same time. On a keyboard or mouse event, the active sets will be consulted one after another (typically from the smallest and most precise focus to the largest and broadest parent) until an action match is found.\n\nThere are two kinds--ones built-in to hydrus, and custom sets that you turn on and off:\n\nThe built-in shortcut sets are always active in their contexts--the 'main_gui' one is always consulted when you hit a key on the main gui window, for instance. They have limited actions to choose from, appropriate to their context. If you would prefer to, say, open the manage tags dialog with Ctrl+F3, edit or add that entry in the 'media' set and that new shortcut will apply anywhere you are focused on some particular media.\n\nCustom shortcuts sets are those you can create and rename at will. They are only ever active in the media viewer window, and only when you set them so from the top hover-window's keyboard icon. They are primarily meant for setting tags and ratings with shortcuts, and are intended to be turned on and off as you perform different 'filtering' jobs--for instance, you might like to set the 1-5 keys to the different values of a five-star rating system, or assign a few simple keystrokes to a number of common tags.\n\nThe built-in 'media' set also supports tag and rating actions, if you would like some of those to always be active.";

pub fn is_reserved(name: &str) -> bool {
    RESERVED.contains(&name)
}

/// A built-in set's name as the lists show it
/// (`shortcut_names_to_pretty_names`); a custom set's own name.
pub fn pretty_name(name: &str) -> &str {
    match name {
        "archive_delete_filter" => "media viewers - archive/delete filter",
        "duplicate_filter" => "media viewers - duplicate filter",
        "global" => "global",
        "main_gui" => "the main window",
        "media" => "media actions, either thumbnails or the viewer",
        "media_viewer" => "media viewers - all",
        "media_viewer_browser" => "media viewers - 'normal' browser",
        "media_viewer_media_window" => "the actual media in a media viewer (mouse only)",
        "preview_media_window" => "the actual media in a preview window (mouse only)",
        "tags_autocomplete" => "tag autocomplete",
        "thumbnails" => "thumbnails",
        other => other,
    }
}

/// What a built-in set is for, shown in its editor
/// (`shortcut_names_to_descriptions`).
pub fn description(name: &str) -> Option<&'static str> {
    Some(match name {
        "archive_delete_filter" => {
            "Navigation actions for the media viewer during an archive/delete filter. Mouse shortcuts should work."
        }
        "duplicate_filter" => {
            "Navigation actions for the media viewer during a duplicate filter. Mouse shortcuts should work."
        }
        "global" => "Actions for the whole program. Should work in the main gui or a media viewer.",
        "main_gui" => "Actions to control pages in the main window of the program.",
        "media" => "Actions to alter metadata for media in the media viewer or the thumbnail grid.",
        "media_viewer" => "Zoom and pan and player actions for any media viewer.",
        "media_viewer_browser" => "Navigation actions for the regular browsable media viewer.",
        "media_viewer_media_window" => {
            "Actions for any video or audio player in a media viewer window. Mouse only!"
        }
        "preview_media_window" => {
            "Actions for any video or audio player in a preview window. Mouse only!"
        }
        "tags_autocomplete" => {
            "Actions to control tag autocomplete when its input text box is focused."
        }
        "thumbnails" => "Actions that interact with the grid of thumbnails in a normal file page.",
        _ => return None,
    })
}

fn simple_names() -> &'static BTreeMap<String, String> {
    static NAMES: std::sync::OnceLock<BTreeMap<String, String>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("simple_command_names.json"))
            .expect("valid simple command names")
    })
}

/// A binding's command as the set editor lists it (`ToString`).
pub fn command_text(binding: &Binding) -> String {
    binding.text.clone().unwrap_or_else(|| {
        simple_names()
            .get(&binding.action.to_string())
            .cloned()
            .unwrap_or_else(|| format!("command {}", binding.action))
    })
}

/// A row of either list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetRow {
    pub name: String,
    pub pretty: String,
    pub count: usize,
}

impl SetRow {
    pub fn cells(&self) -> [String; 2] {
        [
            self.pretty.clone(),
            hydrus_core::numbers::human_int(self.count as u64),
        ]
    }
}

fn row(name: &str, bindings: &[Binding]) -> SetRow {
    SetRow {
        name: name.to_owned(),
        pretty: pretty_name(name).to_owned(),
        count: bindings.len(),
    }
}

/// The built-in sets the settings hold, in the reference's order.
pub fn reserved_rows(settings: &Settings) -> Vec<SetRow> {
    SORTED
        .iter()
        .filter_map(|name| settings.sets.get(*name).map(|b| row(name, b)))
        .collect()
}

/// The custom sets, by name.
pub fn custom_rows(settings: &Settings) -> Vec<SetRow> {
    settings
        .sets
        .iter()
        .filter(|(name, _)| !is_reserved(name))
        .map(|(name, b)| row(name, b))
        .collect()
}

/// `name`, or `name (1)`, `name (2)`... until it is not in `existing`
/// (`GetNonDupeName`).
pub fn non_dupe_name(name: &str, existing: &BTreeSet<String>) -> String {
    let mut candidate = name.to_owned();
    let mut i = 1;
    while existing.contains(&candidate) {
        candidate = format!("{name} ({i})");
        i += 1;
    }
    candidate
}

/// Store an edited custom set under its chosen name, made unique among the
/// other custom sets (and the built-in names), replacing `old` if it was
/// already there. Returns the name used.
pub fn save_custom(
    settings: &mut Settings,
    old: Option<&str>,
    name: &str,
    bindings: Vec<Binding>,
) -> String {
    if let Some(old) = old {
        settings.sets.remove(old);
    }
    let existing: BTreeSet<String> = settings.sets.keys().cloned().collect();
    let name = non_dupe_name(name, &existing);
    settings.sets.insert(name.clone(), bindings);
    name
}

/// Remove the custom sets named.
pub fn delete_custom(settings: &mut Settings, names: &[String]) {
    for name in names {
        if !is_reserved(name) {
            settings.sets.remove(name);
        }
    }
}

/// The defaults "restore defaults" offers, by name.
pub fn default_names() -> Vec<String> {
    default_sets().into_keys().collect()
}

/// What restoring `name`'s defaults says first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Restore {
    /// The set was missing: this information, then it is restored.
    Missing(String),
    /// The set is there: this question, restored on yes.
    Replace(String),
}

pub fn restore_question(settings: &Settings, name: &str) -> Restore {
    if settings.sets.contains_key(name) {
        Restore::Replace(format!(
            "Are you certain you want to restore the defaults for \"{name}\"? Any custom shortcuts you have set will be wiped."
        ))
    } else {
        Restore::Missing(format!(
            "It looks like your client was missing the \"{name}\" shortcut set! It will now be restored."
        ))
    }
}

/// Put `name`'s defaults back.
pub fn restore(settings: &mut Settings, name: &str) {
    if let Some(bindings) = default_sets().remove(name) {
        settings.sets.insert(name.to_owned(), bindings);
    }
}
