//! Owned tag-hover scrolling: only an unconsumed edge wheel can navigate.
use crate::{MediaViewerWindow, viewing_tracking::CanvasTracker};
use hydrus_gui_model::viewer_tag_wheel::WheelGate;
use hydrus_store::{
    Store,
    settings::{self, ViewerTagScrollSettings},
};
use slint::ComponentHandle as _;
use std::{cell::RefCell, rc::Rc, sync::Arc, time::Instant};
pub(crate) fn bind(window: &MediaViewerWindow, lifetime: &CanvasTracker, store: &Arc<Store>) {
    let clock = Instant::now();
    let gate = Rc::new(RefCell::new(WheelGate::default()));
    window.on_tag_media_changed({
        let gate = gate.clone();
        let lifetime = lifetime.clone();
        move || {
            if !lifetime.active() {
                return;
            }
            gate.borrow_mut()
                .media_changed(clock.elapsed().as_secs_f64());
        }
    });
    window.on_tag_wheel({
        let weak = window.as_weak();
        let lifetime = lifetime.clone();
        let store = store.clone();
        move |delta, control, x, y| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !lifetime.active()
                || !window.window().is_visible()
                || !window.get_tags_showing()
                || delta == 0.0
                || !delta.is_finite()
            {
                return;
            }
            let preferences = match store.read(settings::get::<ViewerTagScrollSettings>) {
                Ok(preferences) => preferences,
                Err(error) => {
                    eprintln!("could not read viewer tag-wheel preferences: {error}");
                    return;
                }
            };
            window.set_tag_wheel_policy(i32::from(preferences.0.code()));
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
            let policy = preferences.0;
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
