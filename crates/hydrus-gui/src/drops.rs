//! Files dropped on a window (the reference's `FileDropTarget`). Winit
//! tells of each file dropped in turn, so the files dropped together are
//! gathered for a moment and handed over at once. (A window not shown by
//! winit, as in the tests, has none.)

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::winit::event::WindowEvent;
use slint::winit_030::{EventResult, WinitWindowAccessor as _};

type Handler = Rc<dyn Fn(Vec<String>)>;
type Registry<H> = RefCell<Vec<(usize, H)>>;

thread_local! {
    /// The drop handler of each window that has one, by the window's address,
    /// so a test can hand a window the files winit would (a headless window
    /// is sent no events). Dead handlers are simply not found.
    static HANDLERS: Registry<std::rc::Weak<dyn Fn(Vec<String>)>> = const { RefCell::new(Vec::new()) };
    /// The handlers of windows with no native window, whose filter is dropped
    /// at once (there is nothing to drop files on but the test hook).
    static HELD: Registry<Handler> = const { RefCell::new(Vec::new()) };
}

fn key(window: &slint::Window) -> usize {
    std::ptr::from_ref(window) as usize
}

/// Call `handle` with the paths of the files dropped on `window`, those
/// dropped together at once.
pub(crate) fn on_files_dropped(window: &slint::Window, handle: impl Fn(Vec<String>) + 'static) {
    window.on_winit_window_event(file_handler_for(window, handle));
}

/// [`file_handler`] for a window whose filter the caller composes itself.
pub(crate) fn file_handler_for<T: Fn(Vec<String>) + 'static>(
    window: &slint::Window,
    handle: T,
) -> impl FnMut(&slint::Window, &WindowEvent) -> EventResult + use<T> {
    let handle: Handler = Rc::new(handle);
    HANDLERS.with(|handlers| {
        let mut handlers = handlers.borrow_mut();
        handlers.retain(|(at, weak)| *at != key(window) && weak.strong_count() > 0);
        handlers.push((key(window), Rc::downgrade(&handle)));
    });
    HELD.with(|held| {
        let mut held = held.borrow_mut();
        held.retain(|(at, _)| *at != key(window));
        if !window.has_winit_window() {
            held.push((key(window), handle.clone()));
        }
    });
    gathering(handle)
}

/// Hand `window` the files dropped on it, as winit would after the gather:
/// whether it takes drops.
pub fn deliver_dropped_files(window: &slint::Window, paths: Vec<String>) -> bool {
    let handler = HANDLERS.with(|handlers| {
        handlers
            .borrow()
            .iter()
            .find(|(at, _)| *at == key(window))
            .and_then(|(_, weak)| weak.upgrade())
    });
    handler.map(|handle| handle(paths)).is_some()
}

/// Window-owned drop routing, composable with other observers of the same filter.
pub(crate) fn file_handler(
    handle: impl Fn(Vec<String>) + 'static,
) -> impl FnMut(&slint::Window, &WindowEvent) -> EventResult {
    gathering(Rc::new(handle))
}

fn gathering(handle: Handler) -> impl FnMut(&slint::Window, &WindowEvent) -> EventResult {
    let pending: Rc<RefCell<Vec<String>>> = Rc::default();
    let timer = Rc::new(slint::Timer::default());
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
