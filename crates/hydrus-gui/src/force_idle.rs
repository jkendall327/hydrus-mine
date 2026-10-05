//! Binding-owned debug override consumed by the real live idle monitor.
use crate::{MainWindow, session_autosave::Monitor};
use slint::ComponentHandle as _;
use std::{cell::Cell, rc::Rc};

pub(crate) struct State {
    window: slint::Weak<MainWindow>,
    active: Rc<Cell<bool>>,
    live: Cell<bool>,
    forced: Cell<bool>,
}
impl State {
    /// Retirement/shutdown wins over forced idle; ordinary hiding preserves it.
    pub(crate) fn idle_override(&self) -> Option<bool> {
        if !self.live.get() || !self.active.get() || self.window.upgrade().is_none() {
            Some(false)
        } else if self.forced.get() {
            Some(true)
        } else {
            None
        }
    }
}
/// Runtime debug state, never persisted and never shared with a successor binding.
#[derive(Clone)]
pub struct Control(Rc<State>);
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForceIdle")
            .field("enabled", &self.enabled())
            .finish_non_exhaustive()
    }
}
/// Binding clones share one final-drop retirement lease.
#[derive(Debug)]
pub(crate) struct Owner(Control);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
impl Control {
    pub(crate) fn new(window: &MainWindow, monitor: &Monitor, active: Rc<Cell<bool>>) -> Self {
        let state = Rc::new(State {
            window: window.as_weak(),
            active,
            live: Cell::new(true),
            forced: Cell::new(false),
        });
        monitor.attach_force_idle(Rc::downgrade(&state));
        Self(state)
    }
    pub(crate) fn owner(&self) -> Owner {
        Owner(self.clone())
    }
    /// Current checked state, including permanent binding retirement.
    pub fn enabled(&self) -> bool {
        self.0.idle_override() == Some(true)
    }
    /// Toggle only from this visible, current main outside its pending prompts.
    /// Actual activity timestamps and maintenance/autosave deadlines are untouched.
    pub fn toggle(&self) -> bool {
        let Some(window) = self.0.window.upgrade() else {
            self.retire();
            return false;
        };
        if !self.0.live.get()
            || !self.0.active.get()
            || !window.window().is_visible()
            || window.get_question_visible()
            || window.get_warning_visible()
        {
            return false;
        }
        self.0.forced.set(!self.0.forced.get());
        true
    }
    /// Accepted exit, replacement binding and final binding-clone drop are terminal.
    pub fn retire(&self) {
        self.0.live.set(false);
        self.0.forced.set(false);
    }
}
