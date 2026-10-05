//! Main-title refresh belongs to the current binding; activation is a native request.
use crate::MainWindow;
use hydrus_store::{
    Store,
    settings::{self, GuiSettings},
};
use slint::ComponentHandle as _;
use std::{cell::Cell, rc::Rc, sync::Arc};

pub(crate) fn bind(window: &MainWindow, store: Arc<Store>, active: Rc<Cell<bool>>) {
    let weak = window.as_weak();
    window.on_refresh_application_title(move || {
        if !active.get() {
            return;
        }
        let Some(window) = weak.upgrade() else {
            return;
        };
        let settings: GuiSettings = store.read(settings::get).unwrap_or_default();
        window.set_window_title(
            format!(
                "{} {}",
                settings.application_display_name,
                env!("CARGO_PKG_VERSION")
            )
            .into(),
        );
    });
}

/// Native Windows/macOS/X11 activation. Winit does not support Wayland focus.
pub(crate) fn activate_if_inactive(window: &MainWindow) {
    use slint::winit_030::WinitWindowAccessor as _;
    let focused = window
        .window()
        .with_winit_window(slint::winit_030::winit::window::Window::has_focus)
        .unwrap_or(false);
    if focused {
        return;
    }
    window.set_tag_search_activation_requests(
        window.get_tag_search_activation_requests().wrapping_add(1),
    );
    let _ = window
        .window()
        .with_winit_window(slint::winit_030::winit::window::Window::focus_window);
}
