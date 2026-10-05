//! Owned tag-hover scrolling: only an unconsumed edge wheel can navigate.
use crate::{MediaViewerWindow, viewer_drag::ViewerSlot};
use hydrus_gui_model::viewer_tag_wheel::WheelGate;
use hydrus_store::settings::TagWheelPropagation;
use slint::ComponentHandle as _;
use std::{cell::RefCell, rc::Rc, time::Instant};
pub(crate) fn bind(window: &MediaViewerWindow, slot: &ViewerSlot) {
    let clock = Instant::now();
    let gate = Rc::new(RefCell::new(WheelGate::default()));
    window.on_tag_media_changed({
        let gate = gate.clone();
        move || {
            gate.borrow_mut()
                .media_changed(clock.elapsed().as_secs_f64());
        }
    });
    window.on_tag_wheel({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        move |delta, control, x, y| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some(slot) = slot.upgrade() else {
                return;
            };
            let current = slot
                .borrow()
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
            if !current
                || !window.window().is_visible()
                || !window.get_tags_showing()
                || delta == 0.0
                || !delta.is_finite()
            {
                return;
            }
            let now = clock.elapsed().as_secs_f64();
            let direction = if delta > 0.0 { 1 } else { -1 };
            let maximum = window.get_tag_scroll_maximum().max(0.0);
            let before = window.get_tag_scroll_offset().clamp(0.0, maximum);
            // Winit normalises a line tick to 60 logical pixels. Qt's default is
            // three rows per tick; the native list's measured row height is used.
            let after =
                (before - delta / 60.0 * 3.0 * window.get_tag_row_height()).clamp(0.0, maximum);
            if (after - before).abs() > f32::EPSILON {
                window.set_tag_scroll_offset(after);
                gate.borrow_mut().list_scrolled(now, direction);
                return;
            }
            let policy = TagWheelPropagation::from_code(window.get_tag_wheel_policy() as u16)
                .unwrap_or_default();
            let allow = gate
                .borrow_mut()
                .propagates(policy, maximum > 0.0, now, direction);
            // Drop the state borrow before navigation synchronously resets media.
            if allow {
                if control {
                    window.invoke_zoom(direction, true, x, y);
                } else if direction > 0 {
                    window.invoke_previous();
                } else {
                    window.invoke_next();
                }
            }
        }
    });
}
