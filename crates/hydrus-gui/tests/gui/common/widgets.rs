//! Real clicks and key presses for GUI tests: a widget found by what the
//! user reads on it (its accessible label: a button's or check box's text)
//! and clicked where it is laid out, as a pointer press and release at its
//! centre, so the click goes through the `.slint` file's own touch areas
//! and handlers rather than a callback invoked from the test.
//!
//! Everything here goes through `window.dispatch_event`, as a user's devices
//! do: left, right and middle clicks, double clicks, modifiers held (`Mods`),
//! the wheel, hovering and keys. A test that tags a mouse or keyboard
//! behaviour should use these, not `ui.invoke_*`, which skips the question
//! of whether the click reaches the element at all (a missing `TouchArea`,
//! or one covered by another element, passes every `invoke_*` test).
//!
//! The widget is found by its accessible label: a button's or check box's
//! text, a `Text` element's text, or an explicit `accessible-label`. For
//! one with none (or several alike), give a place, from an exported
//! geometry property: `click_at`, `scroll_at` and the rest take a
//! `LogicalPosition`. Lay the window out first (`lay_out`): a headless
//! window is 0-sized until then. Widgets whose places are reported back on
//! timers (the notebook tabs) also need the window shown and `settle` run a
//! few times before they take a click.

#![allow(dead_code)]

use std::ops::ControlFlow;

use i_slint_core::accessibility::AccessibleStringProperty;
use i_slint_core::item_tree::ItemRc;
use i_slint_core::items::TouchArea;
use i_slint_core::lengths::LogicalPoint;
use i_slint_core::window::{PopupWindowLocation, WindowInner};
use slint::platform::{Key, PointerEventButton, WindowEvent};
use slint::{LogicalPosition, SharedString};

/// Give a window a size if it has none yet (a headless window has none
/// until laid out), so its widgets have places to be clicked at.
pub fn lay_out(window: &slint::Window, width: f32, height: f32) {
    if window.size().width == 0 || window.size().height == 0 {
        window.set_size(slint::LogicalSize::new(width, height));
    }
}

/// The visible items labelled `label`, with where their centres are in the
/// window: those of open popups first (they are on top), then the window's.
fn labelled(window: &slint::Window, label: &str) -> Vec<(ItemRc, LogicalPosition)> {
    let inner = WindowInner::from_pub(window);
    let mut roots: Vec<(ItemRc, (f32, f32))> = inner
        .active_popups()
        .iter()
        .rev()
        .map(|popup| {
            let offset = match &popup.location {
                PopupWindowLocation::ChildWindow(p) => (p.x, p.y),
                PopupWindowLocation::TopLevel(_) => (0.0, 0.0),
            };
            (ItemRc::new_root(popup.component.clone()), offset)
        })
        .collect();
    roots.push((ItemRc::new_root(inner.component()), (0.0, 0.0)));
    let mut found = Vec::new();
    for (root, (dx, dy)) in roots {
        root.visit_descendants(|item| {
            if item
                .accessible_string_property(AccessibleStringProperty::Label)
                .is_some_and(|l| l == label)
                && item.is_visible()
            {
                // (where it takes the pointer: its own touch area, if it
                // has one inside, else itself)
                let mut target = item.clone();
                if item.downcast::<TouchArea>().is_none() {
                    item.visit_descendants(|child| {
                        if child.downcast::<TouchArea>().is_some() {
                            target = child.clone();
                            ControlFlow::Break(())
                        } else {
                            ControlFlow::Continue(())
                        }
                    });
                }
                let size = target.geometry().size;
                let centre =
                    target.map_to_window(LogicalPoint::new(size.width / 2.0, size.height / 2.0));
                found.push((
                    item.clone(),
                    LogicalPosition::new(centre.x + dx, centre.y + dy),
                ));
            }
            ControlFlow::<()>::Continue(())
        });
    }
    found
}

/// Whether a visible widget is labelled `label`.
pub fn shows(window: &slint::Window, label: &str) -> bool {
    !labelled(window, label).is_empty()
}

/// Where the `n`th visible widget labelled `label` is. Matches come in tree
/// order, popups first: for a label that more than one widget has, `n` picks
/// one (the last in tree order is the one drawn on top, as a menu is over
/// the window), and `count` says how many there are.
pub fn position(window: &slint::Window, label: &str, n: usize) -> LogicalPosition {
    settle();
    let found = labelled(window, label);
    found
        .get(n)
        .unwrap_or_else(|| {
            panic!(
                "no visible widget labelled {label:?} (#{n}; {} found)",
                found.len()
            )
        })
        .1
}

/// Let the windows' timers run a few times (layouts that measure
/// themselves, such as the tabs' fitted names, settle on timers).
pub fn settle() {
    for _ in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(2));
        slint::platform::update_timers_and_animations();
    }
}

/// Keyboard modifiers held while an event is sent.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        ctrl: false,
        shift: false,
        alt: false,
    };
    pub const CTRL: Mods = Mods {
        ctrl: true,
        ..Mods::NONE
    };
    pub const SHIFT: Mods = Mods {
        shift: true,
        ..Mods::NONE
    };
    pub const ALT: Mods = Mods {
        alt: true,
        ..Mods::NONE
    };

    fn keys(self) -> Vec<SharedString> {
        let mut keys = Vec::new();
        if self.ctrl {
            keys.push(Key::Control.into());
        }
        if self.shift {
            keys.push(Key::Shift.into());
        }
        if self.alt {
            keys.push(Key::Alt.into());
        }
        keys
    }
}

/// Run `events` with `mods` held down, releasing them after.
fn holding(window: &slint::Window, mods: Mods, events: impl FnOnce()) {
    let keys = mods.keys();
    for text in &keys {
        window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    }
    events();
    for text in keys.into_iter().rev() {
        window.dispatch_event(WindowEvent::KeyReleased { text });
    }
}

/// Press and release a pointer button at a place in the window.
pub fn click_at(window: &slint::Window, position: LogicalPosition, button: PointerEventButton) {
    click_at_with(window, position, button, Mods::NONE);
}

/// [`click_at`], with modifiers held.
pub fn click_at_with(
    window: &slint::Window,
    position: LogicalPosition,
    button: PointerEventButton,
    mods: Mods,
) {
    holding(window, mods, || {
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerPressed { position, button });
        window.dispatch_event(WindowEvent::PointerReleased { position, button });
    });
    window.dispatch_event(WindowEvent::PointerExited);
    settle();
}

/// Two quick clicks of a button at a place: a double click.
pub fn double_click_at(
    window: &slint::Window,
    position: LogicalPosition,
    button: PointerEventButton,
    mods: Mods,
) {
    // (Slint counts clicks within half a second of each other at the same
    // place as one run, and fires double-clicked on the even ones: let any
    // click before this one go by first)
    hydrus_gui::headless::advance_time(std::time::Duration::from_millis(600));
    holding(window, mods, || {
        window.dispatch_event(WindowEvent::PointerMoved { position });
        for _ in 0..2 {
            window.dispatch_event(WindowEvent::PointerPressed { position, button });
            window.dispatch_event(WindowEvent::PointerReleased { position, button });
        }
    });
    window.dispatch_event(WindowEvent::PointerExited);
    settle();
}

/// Move the pointer to a place and leave it there (hover; a menu's
/// submenu opens this way).
pub fn hover_at(window: &slint::Window, position: LogicalPosition) {
    window.dispatch_event(WindowEvent::PointerMoved { position });
    settle();
}

/// Turn the wheel at a place, by `delta_x` and `delta_y` logical pixels
/// (positive y is up, as Slint has it), with modifiers held.
pub fn scroll_at(
    window: &slint::Window,
    position: LogicalPosition,
    delta_x: f32,
    delta_y: f32,
    mods: Mods,
) {
    holding(window, mods, || {
        window.dispatch_event(WindowEvent::PointerMoved { position });
        window.dispatch_event(WindowEvent::PointerScrolled {
            position,
            delta_x,
            delta_y,
        });
    });
    window.dispatch_event(WindowEvent::PointerExited);
    settle();
}

/// Press a pointer button at one place, move to another and release it
/// there: a drag (moved in between too, as a device does).
pub fn drag(
    window: &slint::Window,
    from: LogicalPosition,
    to: LogicalPosition,
    button: PointerEventButton,
) {
    window.dispatch_event(WindowEvent::PointerMoved { position: from });
    window.dispatch_event(WindowEvent::PointerPressed {
        position: from,
        button,
    });
    let middle = LogicalPosition::new(f32::midpoint(from.x, to.x), f32::midpoint(from.y, to.y));
    settle();
    window.dispatch_event(WindowEvent::PointerMoved { position: middle });
    settle();
    window.dispatch_event(WindowEvent::PointerMoved { position: to });
    settle();
    window.dispatch_event(WindowEvent::PointerReleased {
        position: to,
        button,
    });
    window.dispatch_event(WindowEvent::PointerExited);
    settle();
}

/// Toggle the check box labelled `label` the way assistive technology
/// does (its default action); the pointer path is `click`.
pub fn toggle(window: &slint::Window, label: &str) {
    settle();
    let found = labelled(window, label);
    let (item, _) = found
        .first()
        .unwrap_or_else(|| panic!("no visible widget labelled {label:?}"));
    item.accessible_action(&i_slint_core::accessibility::AccessibilityAction::Default);
    settle();
}

/// Click (left) the visible widget labelled `label`; there must be one.
pub fn click(window: &slint::Window, label: &str) {
    click_nth(window, label, 0);
}

/// Click (left) the `n`th visible widget labelled `label`.
pub fn click_nth(window: &slint::Window, label: &str, n: usize) {
    click_nth_with(window, label, n, PointerEventButton::Left, Mods::NONE);
}

/// Click the `n`th visible widget labelled `label` with `button`, holding
/// `mods`.
pub fn click_nth_with(
    window: &slint::Window,
    label: &str,
    n: usize,
    button: PointerEventButton,
    mods: Mods,
) {
    let at = position(window, label, n);
    click_at_with(window, at, button, mods);
}

/// Click (left) the first visible widget labelled `label`, holding `mods`.
pub fn click_with(window: &slint::Window, label: &str, mods: Mods) {
    click_nth_with(window, label, 0, PointerEventButton::Left, mods);
}

/// Right-click the first visible widget labelled `label`.
pub fn right_click(window: &slint::Window, label: &str) {
    click_nth_with(window, label, 0, PointerEventButton::Right, Mods::NONE);
}

/// Middle-click the first visible widget labelled `label`.
pub fn middle_click(window: &slint::Window, label: &str) {
    click_nth_with(window, label, 0, PointerEventButton::Middle, Mods::NONE);
}

/// Double-click (left) the first visible widget labelled `label`.
pub fn double_click(window: &slint::Window, label: &str) {
    let at = position(window, label, 0);
    double_click_at(window, at, PointerEventButton::Left, Mods::NONE);
}

/// Point at (hover) the first visible widget labelled `label`. (Unproven:
/// no test calls it yet; `hover_at` is used by the menu helpers.)
pub fn hover(window: &slint::Window, label: &str) {
    let at = position(window, label, 0);
    hover_at(window, at);
}

/// Turn the wheel over the first visible widget labelled `label`. (Unproven:
/// `scroll_at` is used by a test, this label-finding form is not.)
pub fn scroll(window: &slint::Window, label: &str, delta_x: f32, delta_y: f32) {
    let at = position(window, label, 0);
    scroll_at(window, at, delta_x, delta_y, Mods::NONE);
}

/// Press and release a key (text, or a `slint::platform::Key`).
pub fn key(window: &slint::Window, text: impl Into<SharedString>) {
    let text = text.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

/// Press a key with Ctrl held.
pub fn ctrl_key(window: &slint::Window, text: impl Into<SharedString>) {
    key_with(window, text, Mods::CTRL);
}

/// Press a key with `mods` held. (Ctrl is used by tests; Shift and Alt are
/// unproven.)
pub fn key_with(window: &slint::Window, text: impl Into<SharedString>, mods: Mods) {
    let text = text.into();
    holding(window, mods, || key(window, text));
}

/// An accessible property of the first visible widget labelled `label`.
fn property(window: &slint::Window, label: &str, what: AccessibleStringProperty) -> Option<String> {
    let found = labelled(window, label);
    let (item, _) = found
        .first()
        .unwrap_or_else(|| panic!("no visible widget labelled {label:?}"));
    item.accessible_string_property(what).map(|s| s.to_string())
}

/// Whether the widget labelled `label` is enabled (as it tells assistive
/// technology; a widget that says nothing is enabled).
pub fn enabled(window: &slint::Window, label: &str) -> bool {
    property(window, label, AccessibleStringProperty::Enabled).is_none_or(|e| e == "true")
}

/// Whether the check box labelled `label` is ticked.
pub fn checked(window: &slint::Window, label: &str) -> bool {
    property(window, label, AccessibleStringProperty::Checked).is_some_and(|c| c == "true")
}

/// How many visible widgets are labelled `label`.
pub fn count(window: &slint::Window, label: &str) -> usize {
    labelled(window, label).len()
}
