//! Where windows open and how big, as the reference keeps them
//! (`ClientGUITopLevelWindows`): placed from their frame as they open, and
//! the frame saved from them.

use hydrus_core::windows::{FrameLocation, WindowSettings, WindowState};
use hydrus_store::Store;

/// The frames now.
pub fn settings(store: &Store) -> WindowSettings {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}

/// Keep the frames.
pub fn keep(store: &Store, settings: WindowSettings) {
    if let Err(e) = store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings)) {
        eprintln!("could not keep the window's size and place: {e}");
    }
}

/// Size and place a window about to open as its frame says
/// (`SetInitialTLWSizeAndPosition`, less its fitting to the screen): its
/// last size and place where it remembers them, then maximised, then
/// fullscreen (never on macOS, as the reference).
pub fn place(window: &slint::Window, frame: &FrameLocation) {
    if frame.remember_size
        && let Some((width, height)) = frame.last_size
    {
        #[allow(clippy::cast_precision_loss)]
        window.set_size(slint::LogicalSize::new(width as f32, height as f32));
    }
    if frame.remember_position
        && let Some((x, y)) = frame.last_position
    {
        #[allow(clippy::cast_precision_loss)]
        window.set_position(slint::LogicalPosition::new(x as f32, y as f32));
    }
    if frame.maximised {
        window.set_maximized(true);
    }
    if frame.fullscreen && !cfg!(target_os = "macos") {
        window.set_fullscreen(true);
    }
}

/// Place an implemented owner from its editable reference frame key.
pub fn place_named(window: &slint::Window, store: &Store, name: &str) {
    if let Some(frame) = settings(store).frame(name) {
        place(window, frame);
    }
}

/// Save only the closing owner's current geometry, preserving other frames.
pub fn save_named(window: &slint::Window, store: &Store, name: &str) {
    let mut frames = settings(store);
    if let Some(frame) = frames.frame(name) {
        let saved = frame.saved(state(window));
        if saved != *frame {
            frames.set_frame(name, saved);
            keep(store, frames);
        }
    }
}

/// A window's size and place now, and whether it is maximised or
/// fullscreen.
pub fn state(window: &slint::Window) -> WindowState {
    let scale = window.scale_factor();
    let size = window.size().to_logical(scale);
    let position = window.position().to_logical(scale);
    #[allow(clippy::cast_possible_truncation)]
    WindowState {
        size: (size.width.round() as i32, size.height.round() as i32),
        position: (position.x.round() as i32, position.y.round() as i32),
        maximised: window.is_maximized(),
        fullscreen: window.is_fullscreen(),
    }
}

/// Between fullscreen and the window it was (the media viewer's
/// `FullscreenSwitch`): out of fullscreen, back to maximised if it was;
/// into it, remembering whether it was maximised. Not on macOS, as the
/// reference.
pub fn switch_fullscreen(window: &slint::Window, maximised_before: &std::cell::Cell<bool>) {
    if window.is_fullscreen() {
        window.set_fullscreen(false);
        window.set_maximized(maximised_before.get());
    } else if !cfg!(target_os = "macos") {
        maximised_before.set(window.is_maximized());
        window.set_fullscreen(true);
    }
}
