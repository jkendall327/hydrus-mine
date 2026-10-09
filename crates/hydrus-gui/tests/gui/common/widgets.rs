//! Real clicks and key presses for GUI tests: a widget found by what the
//! user reads on it (its accessible label: a button's or check box's text)
//! and clicked where it is laid out, as a pointer press and release at its
//! centre, so the click goes through the `.slint` file's own touch areas
//! and handlers rather than a callback invoked from the test.

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

/// Where the `n`th visible widget labelled `label` is.
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

/// Press and release a pointer button at a place in the window.
pub fn click_at(window: &slint::Window, position: LogicalPosition, button: PointerEventButton) {
    window.dispatch_event(WindowEvent::PointerMoved { position });
    window.dispatch_event(WindowEvent::PointerPressed { position, button });
    window.dispatch_event(WindowEvent::PointerReleased { position, button });
    window.dispatch_event(WindowEvent::PointerExited);
    settle();
}

/// Click (left) the visible widget labelled `label`; there must be one.
pub fn click(window: &slint::Window, label: &str) {
    click_nth(window, label, 0);
}

/// Click (left) the `n`th visible widget labelled `label`.
pub fn click_nth(window: &slint::Window, label: &str, n: usize) {
    let at = position(window, label, n);
    click_at(window, at, PointerEventButton::Left);
}

/// Press and release a key (text, or a `slint::platform::Key`).
pub fn key(window: &slint::Window, text: impl Into<SharedString>) {
    let text = text.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window.dispatch_event(WindowEvent::KeyReleased { text });
}

/// Press a key with Ctrl held.
pub fn ctrl_key(window: &slint::Window, text: impl Into<SharedString>) {
    let ctrl: SharedString = Key::Control.into();
    window.dispatch_event(WindowEvent::KeyPressed { text: ctrl.clone() });
    key(window, text);
    window.dispatch_event(WindowEvent::KeyReleased { text: ctrl });
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
