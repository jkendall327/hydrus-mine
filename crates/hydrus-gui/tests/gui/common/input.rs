//! Real pointer presses and key presses with Ctrl and Shift held, for GUI
//! tests that must prove the element under the pointer takes the press
//! (a callback invoked from the test skips Slint's hit-testing, so a list
//! row that never receives a click still passes).
//!
//! The press goes to the window at a position taken from its laid-out
//! items (`widgets::position` finds one by the text a user reads there), so
//! Slint picks the receiver as it would for a user. Modifiers are held as a
//! user holds them: their keys go down before the press and up after it.
//! Two presses of one button within 10 pixels and the platform's click
//! interval make a double-click, so `press` and `double_click` first wait
//! the interval out when the last press was of the same button there.

#![allow(dead_code)]

use slint::LogicalPosition;
use slint::platform::{Key, PointerEventButton, WindowEvent};

/// The modifier keys held for a press.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

pub const NONE: Mods = Mods {
    ctrl: false,
    shift: false,
};
pub const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
};
pub const SHIFT: Mods = Mods {
    ctrl: false,
    shift: true,
};
pub const CTRL_SHIFT: Mods = Mods {
    ctrl: true,
    shift: true,
};

fn hold(window: &slint::Window, mods: Mods, down: bool) {
    for (held, key) in [(mods.ctrl, Key::Control), (mods.shift, Key::Shift)] {
        if held {
            let text: slint::SharedString = key.into();
            window.dispatch_event(if down {
                WindowEvent::KeyPressed { text }
            } else {
                WindowEvent::KeyReleased { text }
            });
        }
    }
}

/// The platform's click interval (Slint's default).
const CLICK_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

thread_local! {
    static LAST: std::cell::Cell<Option<(LogicalPosition, PointerEventButton, std::time::Instant)>> =
        const { std::cell::Cell::new(None) };
}

/// Wait until a press of `button` at `at` cannot make a double-click with
/// the last press.
fn apart(at: LogicalPosition, button: PointerEventButton) {
    if let Some((last, last_button, when)) = LAST.get() {
        let near = (last.x - at.x).powi(2) + (last.y - at.y).powi(2) < 100.0;
        let since = when.elapsed();
        if near && last_button == button && since < CLICK_INTERVAL {
            std::thread::sleep(
                CLICK_INTERVAL.saturating_sub(since) + std::time::Duration::from_millis(20),
            );
        }
    }
}

fn press_release(window: &slint::Window, at: LogicalPosition, button: PointerEventButton) {
    LAST.set(Some((at, button, std::time::Instant::now())));
    window.dispatch_event(WindowEvent::PointerPressed {
        position: at,
        button,
    });
    window.dispatch_event(WindowEvent::PointerReleased {
        position: at,
        button,
    });
}

/// Press and release `button` at `at` with `mods` held.
pub fn press(window: &slint::Window, at: LogicalPosition, button: PointerEventButton, mods: Mods) {
    apart(at, button);
    hold(window, mods, true);
    window.dispatch_event(WindowEvent::PointerMoved { position: at });
    press_release(window, at, button);
    hold(window, mods, false);
    super::widgets::settle();
}

/// Double-click (left) at `at` with `mods` held.
pub fn double_click(window: &slint::Window, at: LogicalPosition, mods: Mods) {
    apart(at, PointerEventButton::Left);
    hold(window, mods, true);
    window.dispatch_event(WindowEvent::PointerMoved { position: at });
    press_release(window, at, PointerEventButton::Left);
    press_release(window, at, PointerEventButton::Left);
    hold(window, mods, false);
    super::widgets::settle();
}

/// Press and release a key (its text, or a `Key`'s) with `mods` held.
pub fn key(window: &slint::Window, text: impl Into<slint::SharedString>, mods: Mods) {
    let text = text.into();
    hold(window, mods, true);
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
    hold(window, mods, false);
    super::widgets::settle();
}
