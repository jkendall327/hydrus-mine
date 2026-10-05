//! The visible main binding's current-page status scheduler; no background worker.
use crate::{MainWindow, SearchPage};
use slint::{ComponentHandle as _, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub type Clock = Rc<dyn Fn() -> f64>;
pub(crate) fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}
struct State {
    window: slint::Weak<MainWindow>,
    current: Rc<RefCell<Rc<RefCell<SearchPage>>>>,
    active: Rc<Cell<bool>>,
    clock: RefCell<Clock>,
    timer: Timer,
}
impl State {
    fn refresh(&self, force: bool) {
        if !self.active.get() {
            self.timer.stop();
            return;
        }
        let Some(window) = self.window.upgrade() else {
            self.timer.stop();
            return;
        };
        if !window.window().is_visible() {
            return;
        }
        let page = self.current.borrow().clone();
        let clock = self.clock.borrow().clone();
        let now = clock();
        page.borrow_mut().set_import_status_clock(clock);
        let refreshed = page
            .borrow_mut()
            .refresh_import_status_at(now, force)
            .is_some();
        if refreshed {
            let page = page.borrow();
            crate::show_gallery(&window, &page);
            crate::show_watchers(&window, &page);
        }
    }
}
#[derive(Clone)]
pub struct Binding(Rc<State>);
impl std::fmt::Debug for Binding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloaderUpdates")
            .field("active", &self.0.active.get())
            .finish_non_exhaustive()
    }
}
impl Binding {
    pub(crate) fn bind(
        window: &MainWindow,
        current: Rc<RefCell<Rc<RefCell<SearchPage>>>>,
        active: Rc<Cell<bool>>,
    ) -> Self {
        let state = Rc::new(State {
            window: window.as_weak(),
            current,
            active,
            clock: RefCell::new(Rc::new(now)),
            timer: Timer::default(),
        });
        state
            .timer
            .start(TimerMode::Repeated, Duration::from_millis(250), {
                let weak = Rc::downgrade(&state);
                move || {
                    if let Some(state) = weak.upgrade() {
                        state.refresh(false);
                    }
                }
            });
        Self(state)
    }
    /// Supply a binding-owned clock; the existing pending deadline remains intact.
    pub fn set_clock(&self, clock: Clock) {
        if self.0.active.get() {
            self.0
                .current
                .borrow()
                .borrow_mut()
                .set_import_status_clock(clock.clone());
            *self.0.clock.borrow_mut() = clock;
        }
    }
    pub fn refresh(&self) {
        self.0.refresh(false);
    }
    /// Explicit immediate refresh has the reference's reset-to-zero semantics.
    pub fn force(&self) {
        self.0.refresh(true);
    }
}
