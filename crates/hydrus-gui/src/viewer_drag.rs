//! A viewer-owned drag state; native cursor warps use its existing winit window.
use crate::MediaViewerWindow;
use hydrus_gui_model::viewer_drag::Drag;
use slint::{ComponentHandle as _, winit_030::WinitWindowAccessor as _};
use std::{cell::RefCell, rc::Rc};
pub type ViewerSlot = Rc<RefCell<Option<MediaViewerWindow>>>;
pub(crate) fn bind(window: &MediaViewerWindow, slot: &ViewerSlot) {
    let state = Rc::new(RefCell::new(Drag::default()));
    let owner = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        move || {
            let window = weak.upgrade()?;
            let slot = slot.upgrade()?;
            let current = slot
                .borrow()
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
            (current && window.window().is_visible()).then_some(window)
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
    window.on_drag_moved(move |x, y| {
        let Some(window) = owner() else {
            state.borrow_mut().end();
            return;
        };
        let motion = state.borrow_mut().step(
            (x.round() as i32, y.round() as i32),
            window.get_anchor_drag(),
            window.get_touch_drag_unanchor(),
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
