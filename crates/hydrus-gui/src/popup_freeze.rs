//! Genuine window state gates toaster UI only; background producers continue.
use slint::winit_030::WinitWindowAccessor as _;

/// Winit's Wayland backend reports None; never substitute focus/occlusion.
/// Software-backed windows have their real Slint minimized property instead.
pub(crate) fn minimized(window: &slint::Window) -> Option<bool> {
    match window.with_winit_window(slint::winit_030::winit::window::Window::is_minimized) {
        Some(value) => value,
        None => Some(window.is_minimized()),
    }
}
pub(crate) fn can_alter(window: &slint::Window, store: &hydrus_store::Store) -> bool {
    if !window.is_visible() {
        return false;
    }
    let preferences = store
        .read(hydrus_store::popup_freeze::load)
        .unwrap_or_default();
    hydrus_gui_model::popup_freeze::can_alter(false, minimized(window), preferences.minimized)
}
/// Retained input cannot act through hidden, minimized or permanently retired owners.
pub(crate) fn accepts_input(window: &slint::Window) -> bool {
    window.is_visible() && minimized(window) != Some(true)
}
