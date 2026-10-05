//! A viewer-owned drag state; native cursor warps use its existing winit window.
use crate::{MediaViewerWindow, viewing_tracking::CanvasTracker};
use hydrus_gui_model::viewer_drag::Drag;
use hydrus_store::{
    Store,
    settings::{self, ViewerPointerSettings},
};
use slint::{ComponentHandle as _, winit_030::WinitWindowAccessor as _};
use std::{cell::RefCell, rc::Rc, sync::Arc};
pub(crate) fn bind(window: &MediaViewerWindow, lifetime: &CanvasTracker, store: &Arc<Store>) {
    let state = Rc::new(RefCell::new(Drag::default()));
    let owner = Rc::new({
        let weak = window.as_weak();
        let lifetime = lifetime.clone();
        move || {
            let window = weak.upgrade()?;
            (lifetime.active() && window.window().is_visible()).then_some(window)
        }
    });
    window.on_drag_started({
        let state = state.clone();
        let owner = owner.clone();
        move |x, y| {
            if owner().is_some() {
                state
                    .borrow_mut()
                    .begin((x.round() as i32, y.round() as i32));
            }
        }
    });
    window.on_drag_finished({
        let state = state.clone();
        move || state.borrow_mut().end()
    });
    let store = store.clone();
    window.on_drag_moved(move |x, y| {
        let Some(window) = owner() else {
            state.borrow_mut().end();
            return;
        };
        // Qt reads shared saved preferences on every movement, including in
        // still-visible viewers opened before the current main-window slot.
        let preferences = match store.read(settings::get::<ViewerPointerSettings>) {
            Ok(preferences) => preferences,
            Err(error) => {
                eprintln!("could not read viewer drag preferences: {error}");
                return;
            }
        };
        window.set_anchor_drag(preferences.anchor_drag);
        window.set_touch_drag_unanchor(preferences.touch_unanchors);
        let motion = state.borrow_mut().step(
            (x.round() as i32, y.round() as i32),
            preferences.anchor_drag,
            preferences.touch_unanchors,
        );
        if let Some(motion) = motion {
            if let Some((x, y)) = motion.warp {
                let _ = window.window().with_winit_window(|native| {
                    native.set_cursor_position(slint::winit_030::winit::dpi::LogicalPosition::new(
                        f64::from(x),
                        f64::from(y),
                    ))
                });
            }
            window.invoke_drag(motion.delta.0 as f32, motion.delta.1 as f32);
        }
    });
}
