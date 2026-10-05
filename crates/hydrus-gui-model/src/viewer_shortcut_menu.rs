//! The media viewer top hover's keyboard button (`_ShowShortcutMenu`):
//! "edit shortcuts", then, when there are custom sets, the sets turned on
//! for this viewer and those turned on for every new one.

use hydrus_core::shortcuts::Settings;

pub const EDIT: &str = "edit shortcuts";
pub const CURRENT: &str = "set current shortcuts";
pub const DEFAULTS: &str = "set default shortcuts";
pub const TOOLTIP: &str = "shortcuts";
/// Where the defaults are kept (`default_media_viewer_custom_shortcuts`).
pub const DEFAULTS_OPTION: &str = "default_media_viewer_custom_shortcuts";

/// The custom sets, by name.
pub fn custom_names(settings: &Settings) -> Vec<String> {
    settings
        .sets
        .keys()
        .filter(|name| !crate::shortcut_sets::is_reserved(name))
        .cloned()
        .collect()
}

/// Turn `name` on or off in `active`, keeping it sorted
/// (`FlipActiveCustomShortcutName`, `_FlipActiveDefaultCustomShortcut`).
pub fn flip(active: &mut Vec<String>, name: &str) {
    if let Some(i) = active.iter().position(|n| n == name) {
        active.remove(i);
    } else {
        active.push(name.to_owned());
        active.sort();
    }
}

/// The checks the two submenus show, in [`custom_names`]' order.
pub fn checks(names: &[String], active: &[String]) -> Vec<bool> {
    names.iter().map(|n| active.contains(n)).collect()
}
