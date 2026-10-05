//! Detached shortcut capture, matching the original command editor widgets.
use hydrus_core::shortcuts::Gesture;

/// The original cumulative vertical wheel conversion, strict beyond one tick.
#[derive(Debug, Clone, Default)]
pub struct Wheel {
    remainder: i32,
}
impl Wheel {
    pub fn key(&mut self, mut delta: i32) -> Option<u32> {
        if delta.unsigned_abs() < 120 {
            self.remainder = self.remainder.saturating_add(delta);
            if self.remainder.unsigned_abs() <= 120 {
                return None;
            }
            delta = self.remainder;
            self.remainder = 0;
        }
        match delta.cmp(&0) {
            std::cmp::Ordering::Greater => Some(3),
            std::cmp::Ordering::Less => Some(4),
            std::cmp::Ordering::Equal => None,
        }
    }
}
/// Keyboard and mouse values remain independent when the radio mode changes.
#[derive(Debug, Clone)]
pub struct Capture {
    pub keyboard: Gesture,
    pub mouse: Gesture,
    pub mouse_selected: bool,
    pub release: bool,
    pub merge_numpad: bool,
}
impl Capture {
    pub fn new(value: Gesture, merge_numpad: bool) -> Self {
        let mut out = Self {
            keyboard: Gesture::default(),
            mouse: Gesture::new(1, 0, 0, 0),
            mouse_selected: value.kind == 1,
            release: value.kind == 1 && value.press == 1,
            merge_numpad,
        };
        if value.kind == 1 {
            out.mouse = value;
        } else {
            out.keyboard = value;
        }
        out
    }
    pub fn value(&self) -> &Gesture {
        if self.mouse_selected {
            &self.mouse
        } else {
            &self.keyboard
        }
    }
    pub fn keyboard(&mut self, kind: u8, key: u32, bits: u8) -> bool {
        let Some(value) = Gesture::keyboard(kind, key, bits, self.merge_numpad) else {
            return false;
        };
        self.keyboard = value;
        self.mouse_selected = false;
        true
    }
    pub fn mouse(&mut self, key: u32, press: u8, bits: u8) -> bool {
        if !matches!(key, 0 | 1 | 2 | 7 | 8 | 9)
            || press > 2
            || (press < 2 && (press == 1) != self.release)
        {
            return false;
        }
        self.mouse = Gesture::new(1, key, press, bits & !(1 << 3));
        self.mouse_selected = true;
        true
    }
    pub fn wheel(&mut self, delta: i32, bits: u8, wheel: &mut Wheel) -> bool {
        let Some(key) = wheel.key(delta) else {
            return false;
        };
        self.mouse = Gesture::new(1, key, 0, bits & !(1 << 3));
        self.mouse_selected = true;
        true
    }
    pub fn choose_release(&mut self, release: bool) {
        self.mouse_selected = true;
        self.release = release;
        if self.mouse.appropriate_for_release() {
            self.mouse.press = u8::from(release);
        }
    }
}

/// Only commands with an existing native executor are offered in this slice.
pub fn commands(set: &str) -> &'static [(i32, &'static str)] {
    if set == "main_gui" {
        &[
            (78, "refresh page search/sort"),
            (7, "pages: close current"),
            (56, "pages: new page"),
        ]
    } else {
        &[
            (6, "close media viewer"),
            (99, "media navigation: next"),
            (100, "media navigation: previous"),
            (101, "zoom: in"),
            (102, "zoom: out"),
            (
                92,
                "switch between fullscreen borderless and regular framed window",
            ),
        ]
    }
}
