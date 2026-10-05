//! Reference gesture identities for editable keyboard and mouse shortcuts.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Reference type/key/press/modifier identity (Shortcut version 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gesture {
    pub kind: u8,
    pub key: u32,
    pub press: u8,
    pub modifiers: Vec<u8>,
}
impl Default for Gesture {
    fn default() -> Self {
        Self {
            kind: 2,
            key: 23,
            press: 0,
            modifiers: Vec::new(),
        }
    }
}
impl Gesture {
    /// Modifier bits use the reference's sorted modifier indices.
    pub fn new(kind: u8, key: u32, press: u8, bits: u8) -> Self {
        Self {
            kind,
            key,
            press,
            modifiers: (0..6).filter(|i| bits & (1 << i) != 0).collect(),
        }
    }
    pub fn bits(&self) -> u8 {
        self.modifiers
            .iter()
            .filter(|m| **m < 6)
            .fold(0, |bits, m| bits | (1 << m))
    }
    /// Non-number keypad keys may be folded into their normal-key identity.
    pub fn keyboard(kind: u8, key: u32, bits: u8, merge_numpad: bool) -> Option<Self> {
        let key = if kind == 0 {
            let character = char::from_u32(key).filter(|c| *c != '\0')?;
            hydrus_casefold(character) as u32
        } else if kind == 2 && key <= 48 {
            key
        } else {
            return None;
        };
        let bits = if merge_numpad && !(kind == 0 && (48..=57).contains(&key)) {
            bits & !(1 << 3)
        } else {
            bits
        };
        Some(Self::new(kind, key, 0, bits))
    }
    pub fn appropriate_for_release(&self) -> bool {
        matches!(self.key, 0 | 1 | 2 | 7 | 8 | 9) && self.press != 2
    }
    pub fn text(&self, primary: bool) -> String {
        let names: BTreeMap<String, String> = serde_json::from_str(if self.kind == 1 {
            include_str!("shortcut_mouse_names.json")
        } else {
            include_str!("shortcut_special_names.json")
        })
        .expect("recorded key names");
        let mut parts = Vec::new();
        for (modifier, name) in [
            (5, "control"),
            (
                0,
                if cfg!(target_os = "macos") {
                    "command"
                } else {
                    "ctrl"
                },
            ),
            (
                1,
                if cfg!(target_os = "macos") {
                    "option"
                } else {
                    "alt"
                },
            ),
            (2, "shift"),
            (4, "Mode_switch"),
        ] {
            if self.modifiers.contains(&modifier) {
                parts.push(name.to_owned());
            }
        }
        let key = if self.kind == 0 {
            char::from_u32(self.key)
                .map_or_else(|| format!("unknown key: {}", self.key), |c| c.to_string())
        } else if self.kind == 1 && primary && self.key == 0 {
            "primary-click".into()
        } else if self.kind == 1 && primary && self.key == 1 {
            "secondary-click".into()
        } else {
            names
                .get(&self.key.to_string())
                .cloned()
                .unwrap_or_else(|| format!("unknown key: {}", self.key))
        };
        let prefix = match self.press {
            1 => "release ",
            2 => "double ",
            3 => "drag ",
            _ => "",
        };
        parts.push(format!("{prefix}{key}"));
        let mut text = parts.join("+");
        if self.modifiers.contains(&3) {
            text.push_str(" (on numpad)");
        }
        text
    }
}
fn hydrus_casefold(character: char) -> char {
    crate::casefold::casefold(&character.to_string())
        .chars()
        .next()
        .expect("nonempty casefold")
}

/// One supported simple application command bound to a gesture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub gesture: Gesture,
    pub action: i32,
}
/// Staged named sets and the original capture/display policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub merge_numpad: bool,
    pub primary_labels: bool,
    pub sets: BTreeMap<String, Vec<Binding>>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            merge_numpad: true,
            primary_labels: false,
            sets: BTreeMap::from([
                ("main_gui".into(), Vec::new()),
                ("media_viewer".into(), Vec::new()),
            ]),
        }
    }
}
impl Settings {
    pub fn command(&self, name: &str, gesture: &Gesture) -> Option<i32> {
        let bindings = self.sets.get(name)?;
        bindings
            .iter()
            .find(|binding| {
                binding.gesture == *gesture
                    || (self.merge_numpad
                        && gesture.kind != 1
                        && !(gesture.kind == 0 && (48..=57).contains(&gesture.key))
                        && {
                            let mut alternative = gesture.clone();
                            let bits = alternative.bits() ^ (1 << 3);
                            alternative = Gesture::new(
                                alternative.kind,
                                alternative.key,
                                alternative.press,
                                bits,
                            );
                            binding.gesture == alternative
                        })
            })
            .map(|b| b.action)
    }
}
