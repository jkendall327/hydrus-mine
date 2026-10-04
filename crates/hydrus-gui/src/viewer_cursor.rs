//! Real native cursor visibility and inactivity timer owned by each viewer.
use crate::{
    MediaViewerWindow,
    session_autosave::{self, PointerCallback},
};
use hydrus_gui_model::viewer_cursor::CursorWait;
use hydrus_store::{
    Store,
    settings::{self, ViewerCursorSettings},
};
use slint::{
    ComponentHandle as _,
    winit_030::{WinitWindowAccessor as _, winit::window::WindowId},
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

thread_local! {
    static LAST_OPENED: RefCell<Weak<NativeCursor>> = const { RefCell::new(Weak::new()) };
}
/// Read-only owner handle for native/headless event regression.
pub fn last_opened() -> Option<Rc<NativeCursor>> {
    LAST_OPENED.with(|slot| slot.borrow().upgrade())
}

pub struct NativeCursor {
    window: slint::Weak<MediaViewerWindow>,
    store: Arc<Store>,
    origin: Instant,
    wait: RefCell<CursorWait>,
    position: Cell<Option<(f64, f64)>>,
    callback: PointerCallback,
    identity: Cell<Option<WindowId>>,
    timer: slint::Timer,
    closed: Cell<bool>,
}
impl std::fmt::Debug for NativeCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeCursor")
            .field("wait", &self.wait.borrow())
            .field("closed", &self.closed.get())
            .finish_non_exhaustive()
    }
}
impl NativeCursor {
    pub(crate) fn new(window: &MediaViewerWindow, store: Arc<Store>) -> Rc<Self> {
        let result = Rc::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            let callback: PointerCallback = Rc::new(move |position| {
                if let Some(owner) = weak.upgrade() {
                    owner.motion(position);
                }
            });
            Self {
                window: window.as_weak(),
                store,
                origin: Instant::now(),
                wait: RefCell::new(CursorWait::new(0)),
                position: Cell::new(None),
                callback,
                identity: Cell::new(None),
                timer: slint::Timer::default(),
                closed: Cell::new(false),
            }
        });
        LAST_OPENED.with(|slot| *slot.borrow_mut() = Rc::downgrade(&result));
        result.schedule();
        result
    }
    fn now(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    pub fn state(&self) -> CursorWait {
        self.wait.borrow().clone()
    }
    pub fn timer_running(&self) -> bool {
        self.timer.running()
    }
    /// Native registration, also used by the identical headless observer route.
    pub fn watch_id(&self, id: WindowId) {
        session_autosave::watch_native_pointer_id(id, &self.callback);
        self.identity.set(Some(id));
    }
    pub(crate) fn watch_native(&self) {
        if self.identity.get().is_none()
            && let Some(window) = self.window.upgrade()
        {
            let _ = window
                .window()
                .with_winit_window(|native| self.watch_id(native.id()));
        }
    }
    fn apply(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let blank = self.wait.borrow().blank;
        window.set_cursor_idle_hidden(blank);
        // This changes the actual OS cursor. The Slint property also retains
        // the same cursor when the canvas updates its MouseCursor binding.
        let _ = window
            .window()
            .with_winit_window(|native| native.set_cursor_visible(!blank));
    }
    fn schedule(self: &Rc<Self>) {
        self.timer.stop();
        if self.closed.get() {
            return;
        }
        let Some(delay) = self.wait.borrow().next_check_ms else {
            return;
        };
        let weak = Rc::downgrade(self);
        self.timer.start(
            slint::TimerMode::SingleShot,
            Duration::from_millis(u64::from(delay)),
            move || {
                if let Some(owner) = weak.upgrade() {
                    owner.check_at(owner.now());
                }
            },
        );
    }
    #[allow(clippy::float_cmp)] // duplicate native physical positions are exact input equality
    fn motion(self: &Rc<Self>, position: (f64, f64)) {
        if self.closed.get() || self.position.replace(Some(position)) == Some(position) {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let hidden_drag = window.get_drag_accepted() && window.get_hide_during_drag();
        self.wait.borrow_mut().motion(self.now(), hidden_drag);
        self.apply();
        self.schedule();
    }
    /// Same timer consumer, with an explicit monotonic instant for oracle replay.
    pub fn check_at(self: &Rc<Self>, now_ms: u64) {
        if self.closed.get() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            self.timer.stop();
            return;
        };
        let options: ViewerCursorSettings = self.store.read(settings::get).unwrap_or_default();
        let delay = options.autohide_ms.map(|delay| delay.clamp(100, 100000));
        // Slint 1.18's popup stack is the actual menu lifecycle, including
        // cancellation paths that do not invoke a MenuItem callback.
        let menu_open =
            !slint::private_unstable_api::re_exports::WindowInner::from_pub(window.window())
                .active_popups()
                .is_empty();
        self.wait.borrow_mut().check(
            now_ms,
            delay,
            window.get_window_active(),
            window.get_cursor_hide_eligible(),
            menu_open,
        );
        self.apply();
        self.schedule();
    }
    pub(crate) fn menu_starting(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        self.wait.borrow_mut().motion(self.now(), false);
        self.apply();
        self.schedule();
    }
    pub(crate) fn menu_returned(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if slint::private_unstable_api::re_exports::WindowInner::from_pub(window.window())
            .active_popups()
            .is_empty()
        {
            // Native menu execution can block Slint's timer loop. Its return
            // must leave a fresh wait, rather than hiding immediately because
            // the elapsed menu duration exceeded the timeout. Async Slint
            // menus retain their actual stack and tick the normal check.
            self.menu_starting();
        }
    }
    pub(crate) fn refresh(self: &Rc<Self>) {
        self.watch_native();
        self.check_at(self.now());
    }
    pub(crate) fn close(&self) {
        self.closed.set(true);
        self.timer.stop();
    }
}
