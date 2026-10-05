//! Translate actual backend key identity before Slint discards keypad location.
use slint::winit_030::winit::{
    event::{ElementState, WindowEvent},
    keyboard::{Key, KeyLocation, ModifiersState, NamedKey},
};

/// Cached raw metadata is window-owned and consumed by the next key callback.
#[derive(Debug, Clone, Default)]
pub struct Input {
    pub bits: u8,
    pub pending: Option<(u8, u32, u8)>,
}
impl Input {
    pub fn observe(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::ModifiersChanged(modifiers) => {
                self.bits = modifier_bits(modifiers.state()) | (self.bits & 16);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.logical_key == Key::Named(NamedKey::AltGraph) {
                    if event.state == ElementState::Pressed {
                        self.bits |= 16;
                    } else {
                        self.bits &= !16;
                    }
                }
                if event.state == ElementState::Pressed {
                    self.pending = key(&event.logical_key, event.location, self.bits);
                }
            }
            WindowEvent::Focused(false) => {
                self.bits = 0;
                self.pending = None;
            }
            _ => (),
        }
    }
}
pub fn modifier_bits(modifiers: ModifiersState) -> u8 {
    let (control, meta) = if cfg!(target_os = "macos") {
        (modifiers.super_key(), modifiers.control_key())
    } else {
        (modifiers.control_key(), modifiers.super_key())
    };
    u8::from(control)
        + 2 * u8::from(modifiers.alt_key())
        + 4 * u8::from(modifiers.shift_key())
        + 32 * u8::from(meta)
}
pub fn key(key: &Key, location: KeyLocation, mut bits: u8) -> Option<(u8, u32, u8)> {
    if location == KeyLocation::Numpad {
        bits |= 8;
    }
    let special = match key {
        Key::Character(text) => return slint_key(text, bits),
        Key::Named(name) => match name {
            NamedKey::Space => 0,
            NamedKey::Backspace => 1,
            NamedKey::Tab => {
                if bits & 4 != 0 {
                    36
                } else {
                    2
                }
            }
            NamedKey::Enter => {
                if location == KeyLocation::Numpad {
                    4
                } else {
                    3
                }
            }
            NamedKey::Pause => 5,
            NamedKey::Escape => 6,
            NamedKey::Insert => 7,
            NamedKey::Delete => 8,
            NamedKey::ArrowUp => 9,
            NamedKey::ArrowDown => 10,
            NamedKey::ArrowLeft => 11,
            NamedKey::ArrowRight => 12,
            NamedKey::Home => 13,
            NamedKey::End => 14,
            NamedKey::PageUp => 15,
            NamedKey::PageDown => 16,
            NamedKey::F1 => 17,
            NamedKey::F2 => 18,
            NamedKey::F3 => 19,
            NamedKey::F4 => 20,
            NamedKey::F5 => 21,
            NamedKey::F6 => 22,
            NamedKey::F7 => 23,
            NamedKey::F8 => 24,
            NamedKey::F9 => 25,
            NamedKey::F10 => 26,
            NamedKey::F11 => 27,
            NamedKey::F12 => 28,
            NamedKey::MediaPlayPause | NamedKey::MediaPlay | NamedKey::MediaPause => 29,
            NamedKey::MediaTrackPrevious => 30,
            NamedKey::MediaTrackNext => 31,
            NamedKey::AudioVolumeDown => 32,
            NamedKey::AudioVolumeUp => 33,
            NamedKey::AudioVolumeMute => 34,
            NamedKey::Control => {
                bits &= if cfg!(target_os = "macos") { !32 } else { !1 };
                35
            }
            NamedKey::Alt => {
                bits &= !2;
                35
            }
            NamedKey::Shift => {
                bits &= !4;
                35
            }
            NamedKey::Super | NamedKey::Meta => {
                bits &= if cfg!(target_os = "macos") { !1 } else { !32 };
                35
            }
            NamedKey::F13 => 37,
            NamedKey::F14 => 38,
            NamedKey::F15 => 39,
            NamedKey::F16 => 40,
            NamedKey::F17 => 41,
            NamedKey::F18 => 42,
            NamedKey::F19 => 43,
            NamedKey::F20 => 44,
            NamedKey::F21 => 45,
            NamedKey::F22 => 46,
            NamedKey::F23 => 47,
            NamedKey::F24 => 48,
            _ => return None,
        },
        _ => return None,
    };
    Some((2, special, bits))
}
pub fn slint_key(text: &str, mut bits: u8) -> Option<(u8, u32, u8)> {
    let c = text.chars().next()?;
    let special = match c {
        ' ' => 0,
        '\u{0008}' => 1,
        '\t' => {
            if bits & 4 != 0 {
                36
            } else {
                2
            }
        }
        '\n' => 3,
        '\u{001b}' => 6,
        '\u{0019}' => 36,
        '\u{007f}' => 8,
        '\u{0010}' | '\u{0015}' => {
            bits &= !4;
            35
        }
        '\u{0011}' | '\u{0016}' => {
            bits &= !1;
            35
        }
        '\u{0012}' => {
            bits &= !2;
            35
        }
        '\u{0017}' | '\u{0018}' => {
            bits &= !32;
            35
        }
        '\u{F700}' => 9,
        '\u{F701}' => 10,
        '\u{F702}' => 11,
        '\u{F703}' => 12,
        '\u{F704}'..='\u{F70F}' => 17 + (u32::from(c) - 0xF704),
        '\u{F710}'..='\u{F71B}' => 37 + (u32::from(c) - 0xF710),
        '\u{F727}' => 7,
        '\u{F729}' => 13,
        '\u{F72B}' => 14,
        '\u{F72C}' => 15,
        '\u{F72D}' => 16,
        _ => return (c != '\0').then_some((0, u32::from(c), bits)),
    };
    Some((2, special, bits))
}
