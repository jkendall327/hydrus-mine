//! Native activity for viewer mouseover policies, sharing the desktop observer.
use std::{cell::Cell, rc::Rc};

use slint::ComponentHandle as _;

use crate::{
    MediaViewerWindow,
    session_autosave::{self, ApplicationFocusCallback},
};
use slint::winit_030::winit::window::WindowId;

/// Owned by the viewer's presentation callback; the backend keeps only Weak.
pub struct NativeFocus {
    window: slint::Weak<MediaViewerWindow>,
    identity: Rc<Cell<Option<WindowId>>>,
    callback: ApplicationFocusCallback,
    registered: Cell<bool>,
}

impl std::fmt::Debug for NativeFocus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeFocus")
            .field("registered", &self.registered.get())
            .finish_non_exhaustive()
    }
}

impl NativeFocus {
    pub fn new(window: &MediaViewerWindow) -> Rc<Self> {
        let weak = window.as_weak();
        let identity = Rc::new(Cell::new(None));
        let callback: ApplicationFocusCallback = Rc::new({
            let weak = weak.clone();
            let identity = identity.clone();
            move |active| {
                let Some(own) = identity.get() else { return };
                let Some(window) = weak.upgrade() else { return };
                if active == Some(own) {
                    window.set_another_window_active(false);
                    window.set_window_active(true);
                } else if active.is_some() {
                    window.set_another_window_active(true);
                    window.set_window_active(false);
                } else {
                    // No own-app window: suppress new raises but preserve
                    // an already-raised hover while the pointer remains there.
                    window.set_window_active(false);
                    window.set_another_window_active(false);
                }
            }
        });
        Rc::new(Self {
            window: weak,
            identity,
            callback,
            registered: Cell::new(false),
        })
    }

    /// Register after showing, or retry after Apply if no native window existed.
    pub fn watch_native(&self) {
        use slint::winit_030::WinitWindowAccessor as _;
        if !self.registered.get()
            && let Some(window) = self.window.upgrade()
            && let Some(id) = window
                .window()
                .with_winit_window(slint::winit_030::winit::window::Window::id)
        {
            self.watch_id(id);
            self.registered.set(true);
        }
    }

    /// Use the identical callback/registry route for a headless native identity.
    pub fn watch_id(&self, id: WindowId) {
        self.identity.set(Some(id));
        session_autosave::watch_native_application_focus(&self.callback);
    }
}
