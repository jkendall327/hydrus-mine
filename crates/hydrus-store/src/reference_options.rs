//! Client options hydrus-rs keeps as the reference keeps them (its
//! `new_options` booleans, integers and strings, by name), for Options rows
//! whose subsystem hydrus-rs lacks or does differently (Qt styles, the
//! system tray, the image tile cache) and a few it reads (mpv's audio
//! device and looping). Each has the reference's default
//! (`ClientOptions`); an imported client brings its own values.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The booleans kept, with their defaults.
pub const BOOLEANS: &[(&str, bool)] = &[
    ("always_show_system_tray_icon", false),
    ("close_client_to_system_tray", false),
    ("discord_dnd_fix", false),
    ("do_macos_debug_dialog_menus", false),
    ("do_not_setgeometry_on_an_mpv", false),
    ("draw_top_right_hover_in_preview_window_background", true),
    ("enable_truncated_images_pil", true),
    ("file_system_waits_on_wakeup", false),
    ("force_hide_page_signal_on_new_page", false),
    ("freeze_message_manager_when_mouse_on_other_monitor", false),
    (
        "fuzzy_relocate_on_get_safe_position_test_only_for_self_sizing_media_viewer_canvas",
        false,
    ),
    ("hover_window_duplicates_always_on_top", true),
    ("load_images_with_pil", true),
    ("make_child_frames_qt_tool", true),
    ("minimise_client_to_system_tray", false),
    (
        "minimise_client_to_system_tray_bugfix_deferred_state_set",
        false,
    ),
    (
        "minimise_client_to_system_tray_bugfix_restore_after_show",
        false,
    ),
    ("mpv_destruction_test", false),
    ("mpv_loop_playlist_instead_of_file", false),
    ("mpv_null_audio_on_silent_media", false),
    ("persist_media_window_mpv", false),
    ("persist_media_window_qt_media_player", false),
    ("preview_uses_its_own_audio_volume", true),
    ("preview_window_hover_top_right_shows_popup", true),
    ("qt_media_player_null_audio_on_silent_media", false),
    ("qt_media_player_opengl_test", false),
    ("secret_discord_dnd_fix", false),
    ("set_requests_ca_bundle_env", false),
    ("show_destination_page_when_dnd_url", true),
    ("show_file_lookup_script_tags", false),
    ("start_client_in_system_tray", false),
    ("use_legacy_mpv_mediator", false),
    ("use_native_menubar", false),
    ("use_qt_file_dialogs", false),
    ("use_qt_locale_for_human_int", false),
    ("use_system_ffmpeg", false),
];

/// The integers kept, with their defaults.
pub const INTEGERS: &[(&str, i64)] = &[
    ("ideal_tile_dimension", 768),
    ("image_tile_cache_size", 268_435_456),
    ("image_tile_cache_timeout", 300),
    ("num_recent_petition_reasons", 5),
    ("video_buffer_size", 100_663_296),
];

/// The strings kept (none for unset), with their defaults.
pub const STRINGS: &[(&str, Option<&str>)] = &[
    ("curl_cffi_definition", None),
    ("discord_dnd_filename_pattern", Some("{hash}")),
    ("mpv_preferred_audio_device", None),
    // (`options_ratings_panel_template_service_key`, a key as hex: the
    // reference's rating preview service, "ratings preview object service")
    (
        "options_ratings_panel_template_service_key",
        Some("726174696e67732070726576696577206f626a6563742073657276696365"),
    ),
    ("qt_media_player_preferred_audio_device_id_hex", None),
    ("qt_style_name", None),
    ("qt_stylesheet_name", None),
];

/// The string lists kept (all empty by default).
pub const STRING_LISTS: &[&str] = &["default_media_viewer_custom_shortcuts"];

/// The values set, by name; anything unset reads as its default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceOptions {
    pub booleans: BTreeMap<String, bool>,
    pub integers: BTreeMap<String, i64>,
    pub strings: BTreeMap<String, Option<String>>,
    pub string_lists: BTreeMap<String, Vec<String>>,
}

impl crate::settings::Setting for ReferenceOptions {
    const KEY: &'static str = "reference_options";
}

impl ReferenceOptions {
    /// `name`'s boolean (`GetBoolean`).
    ///
    /// # Panics
    /// If `name` isn't one kept.
    pub fn boolean(&self, name: &str) -> bool {
        self.booleans.get(name).copied().unwrap_or_else(|| {
            BOOLEANS
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("{name} isn't a kept boolean"))
                .1
        })
    }

    pub fn set_boolean(&mut self, name: &str, value: bool) {
        self.booleans.insert(name.to_owned(), value);
    }

    /// `name`'s integer (`GetInteger`).
    ///
    /// # Panics
    /// If `name` isn't one kept.
    pub fn integer(&self, name: &str) -> i64 {
        self.integers.get(name).copied().unwrap_or_else(|| {
            INTEGERS
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("{name} isn't a kept integer"))
                .1
        })
    }

    pub fn set_integer(&mut self, name: &str, value: i64) {
        self.integers.insert(name.to_owned(), value);
    }

    /// `name`'s string, none if unset (`GetNoneableString`, `GetString`).
    ///
    /// # Panics
    /// If `name` isn't one kept.
    pub fn string(&self, name: &str) -> Option<String> {
        self.strings.get(name).cloned().unwrap_or_else(|| {
            STRINGS
                .iter()
                .find(|(n, _)| *n == name)
                .unwrap_or_else(|| panic!("{name} isn't a kept string"))
                .1
                .map(str::to_owned)
        })
    }

    pub fn set_string(&mut self, name: &str, value: Option<String>) {
        self.strings.insert(name.to_owned(), value);
    }

    /// `name`'s string list (`GetStringList`).
    ///
    /// # Panics
    /// If `name` isn't one kept.
    pub fn string_list(&self, name: &str) -> Vec<String> {
        assert!(
            STRING_LISTS.contains(&name),
            "{name} isn't a kept string list"
        );
        self.string_lists.get(name).cloned().unwrap_or_default()
    }

    pub fn set_string_list(&mut self, name: &str, value: Vec<String>) {
        self.string_lists.insert(name.to_owned(), value);
    }

    /// The kept values an imported client's options set.
    pub fn import(
        booleans: &BTreeMap<String, bool>,
        integers: &BTreeMap<String, i64>,
        strings: &BTreeMap<String, String>,
        noneable_strings: &BTreeMap<String, Option<String>>,
        string_lists: &BTreeMap<String, Vec<String>>,
    ) -> Self {
        let mut out = Self::default();
        for name in STRING_LISTS {
            if let Some(value) = string_lists.get(*name) {
                out.set_string_list(name, value.clone());
            }
        }
        for (name, _) in BOOLEANS {
            if let Some(&value) = booleans.get(*name) {
                out.set_boolean(name, value);
            }
        }
        for (name, _) in INTEGERS {
            if let Some(&value) = integers.get(*name) {
                out.set_integer(name, value);
            }
        }
        for (name, _) in STRINGS {
            if let Some(value) = noneable_strings.get(*name) {
                out.set_string(name, value.clone());
            } else if let Some(value) = strings.get(*name) {
                out.set_string(name, Some(value.clone()));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_values_read_as_their_defaults() {
        let mut options = ReferenceOptions::default();
        assert!(options.boolean("load_images_with_pil"));
        assert_eq!(options.integer("ideal_tile_dimension"), 768);
        assert_eq!(
            options.string("discord_dnd_filename_pattern").as_deref(),
            Some("{hash}")
        );
        assert_eq!(options.string("qt_style_name"), None);
        options.set_boolean("load_images_with_pil", false);
        options.set_string("qt_style_name", Some("Fusion".into()));
        assert!(!options.boolean("load_images_with_pil"));
        assert_eq!(options.string("qt_style_name").as_deref(), Some("Fusion"));
    }
}
