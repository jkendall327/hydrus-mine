//! Qt's independent Ctrl/Shift preview-focus policies. Disabled child values persist.
use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::{Result, settings};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub ctrl_focus: bool,
    pub ctrl_only_static: bool,
    pub shift_focus: bool,
    pub shift_only_static: bool,
}
impl settings::Setting for Preferences {
    const KEY: &'static str = "thumbnail_preview_selection";
}
impl Preferences {
    pub fn apply_legacy(&mut self, booleans: &BTreeMap<String, bool>) {
        for (key, destination) in [
            ("focus_preview_on_ctrl_click", &mut self.ctrl_focus),
            (
                "focus_preview_on_ctrl_click_only_static",
                &mut self.ctrl_only_static,
            ),
            ("focus_preview_on_shift_click", &mut self.shift_focus),
            (
                "focus_preview_on_shift_click_only_static",
                &mut self.shift_only_static,
            ),
        ] {
            if let Some(&value) = booleans.get(key) {
                *destination = value;
            }
        }
    }

    /// Only modifier additions/ranges use this gate; plain/fallback hits focus normally.
    pub fn focus_target(&self, ctrl: bool, shift: bool, has_duration: bool) -> bool {
        let (enabled, only_static) = if ctrl && !shift {
            (self.ctrl_focus, self.ctrl_only_static)
        } else if shift {
            (self.shift_focus, self.shift_only_static)
        } else {
            return false;
        };
        enabled && (!only_static || !has_duration)
    }

    /// Merge edited fields into current settings; Cancel never calls this.
    pub fn save_changed(&self, conn: &Connection, before: &Self) -> Result<()> {
        let mut current: Self = settings::get(conn)?;
        if self.ctrl_focus != before.ctrl_focus {
            current.ctrl_focus = self.ctrl_focus;
        }
        if self.ctrl_only_static != before.ctrl_only_static {
            current.ctrl_only_static = self.ctrl_only_static;
        }
        if self.shift_focus != before.shift_focus {
            current.shift_focus = self.shift_focus;
        }
        if self.shift_only_static != before.shift_only_static {
            current.shift_only_static = self.shift_only_static;
        }
        settings::set(conn, &current)
    }
}
