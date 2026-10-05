//! Files dropped on a window (the reference's `FileDropTarget`). Winit
//! tells of each file dropped in turn, so the files dropped together are
//! gathered for a moment and handed over at once. (A window not shown by
//! winit, as in the tests, has none.)

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::winit::event::WindowEvent;
use slint::winit_030::{EventResult, WinitWindowAccessor as _};

/// Call `handle` with the paths of the files dropped on `window`, those
/// dropped together at once.
pub(crate) fn on_files_dropped(window: &slint::Window, handle: impl Fn(Vec<String>) + 'static) {
    window.on_winit_window_event(file_handler(handle));
}

/// Window-owned drop routing, composable with other observers of the same filter.
pub(crate) fn file_handler(
    handle: impl Fn(Vec<String>) + 'static,
) -> impl FnMut(&slint::Window, &WindowEvent) -> EventResult {
    let pending: Rc<RefCell<Vec<String>>> = Rc::default();
    let timer = Rc::new(slint::Timer::default());
    let handle = Rc::new(handle);
    move |_, event| {
        let WindowEvent::DroppedFile(path) = event else {
            return EventResult::Propagate;
        };
        if let Some(path) = path.to_str() {
            pending.borrow_mut().push(path.to_owned());
        }
        let (pending, handle) = (pending.clone(), handle.clone());
        timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(100),
            move || {
                let paths = std::mem::take(&mut *pending.borrow_mut());
                if !paths.is_empty() {
                    handle(paths);
                }
            },
        );
        EventResult::PreventDefault
    }
}
