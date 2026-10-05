//! PopupMessage construction-time width preferences.

use std::collections::BTreeMap;

/// Raw persisted integers are retained until Options normalises its displayed spin.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PopupWidth {
    pub characters: i64,
    pub fixed: bool,
}

impl Default for PopupWidth {
    fn default() -> Self {
        Self {
            characters: 56,
            fixed: false,
        }
    }
}

impl crate::settings::Setting for PopupWidth {
    const KEY: &'static str = "popup_width";
}

impl PopupWidth {
    pub fn effective_characters(&self) -> i32 {
        self.characters.clamp(16, 256) as i32
    }

    pub fn apply_legacy(
        &mut self,
        integers: &BTreeMap<String, i64>,
        booleans: &BTreeMap<String, bool>,
    ) {
        if let Some(&value) = integers.get("popup_message_character_width") {
            self.characters = value;
        }
        if let Some(&value) = booleans.get("popup_message_force_min_width") {
            self.fixed = value;
        }
    }
}

/// Options writes only fields edited by this owner, preserving other writers.
pub fn save_changed(
    conn: &rusqlite::Connection,
    after: &PopupWidth,
    before: &PopupWidth,
) -> crate::Result<()> {
    if after == before {
        return Ok(());
    }
    let mut current: PopupWidth = crate::settings::get(conn)?;
    if after.characters != before.characters {
        current.characters = after.characters;
    }
    if after.fixed != before.fixed {
        current.fixed = after.fixed;
    }
    crate::settings::set(conn, &current)
}
