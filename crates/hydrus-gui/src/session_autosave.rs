//! Historical autosaves alongside the live Client API session synchronization.
use crate::{MainWindow, Pages};
use hydrus_gui_model::session_lifecycle::{Action, Autosave, Idle};
use hydrus_store::settings::{self, GuiIdleSettings, GuiSessionSettings};
use std::{cell::RefCell, rc::Rc};

struct Inner {
    pages: Rc<RefCell<Pages>>,
    window: slint::Weak<MainWindow>,
    schedule: RefCell<Autosave>,
    idle: RefCell<Idle>,
    previous: RefCell<Option<String>>,
    timer: slint::Timer,
}

#[derive(Clone)]
pub struct Monitor(Rc<Inner>);

pub(crate) fn bind(window: &MainWindow, pages: &Rc<RefCell<Pages>>) -> Monitor {
    use slint::ComponentHandle as _;
    let now = hydrus_core::TimestampMs::now().0;
    let config: GuiSessionSettings = pages
        .borrow()
        .store()
        .read(settings::get)
        .unwrap_or_default();
    let monitor = Monitor(Rc::new(Inner {
        pages: pages.clone(),
        window: window.as_weak(),
        schedule: RefCell::new(Autosave::new(now, &config)),
        idle: RefCell::new(Idle::new(now)),
        previous: RefCell::new(None),
        timer: slint::Timer::default(),
    }));
    window.on_session_user_activity({
        let monitor = monitor.clone();
        move || monitor.user_at(hydrus_core::TimestampMs::now().0)
    });
    window.on_session_mouse_activity({
        let monitor = monitor.clone();
        move || monitor.mouse_at(hydrus_core::TimestampMs::now().0)
    });
    let weak = Rc::downgrade(&monitor.0);
    monitor.0.timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            if let Some(inner) = weak.upgrade()
                && let Err(error) = Monitor(inner).poll_at(hydrus_core::TimestampMs::now().0)
            {
                eprintln!("could not autosave the GUI session: {error}");
            }
        },
    );
    monitor
}

impl Monitor {
    pub fn next(&self) -> Option<i64> {
        self.0.schedule.borrow().next()
    }
    pub fn user_at(&self, now_ms: i64) {
        self.0.idle.borrow_mut().user(now_ms);
    }
    pub fn mouse_at(&self, now_ms: i64) {
        self.0.idle.borrow_mut().mouse(now_ms);
    }
    pub fn api_at(&self, now_ms: i64) {
        self.0.idle.borrow_mut().api(now_ms);
    }
    pub fn idle_at(&self, now_ms: i64) -> bool {
        let config: GuiIdleSettings = self
            .0
            .pages
            .borrow()
            .store()
            .read(settings::get)
            .unwrap_or_default();
        self.0.idle.borrow().eligible(now_ms, &config)
    }

    /// One real timer tick, also available for deterministic desktop replay.
    /// Return whether a changed snapshot was saved.
    pub fn poll_at(&self, now_ms: i64) -> hydrus_store::Result<bool> {
        if self.0.window.upgrade().is_none() {
            return Ok(false);
        }
        let store = self.0.pages.borrow().store().clone();
        let config: GuiSessionSettings = store.read(settings::get)?;
        if self
            .0
            .schedule
            .borrow_mut()
            .poll(now_ms, self.idle_at(now_ms), &config)
            != Action::Save
        {
            return Ok(false);
        }
        self.0.pages.borrow_mut().sync(now_ms / 1000)?;
        let session = self.0.pages.borrow().session().clone();
        let previous = self.0.previous.borrow().clone();
        let saved = store.write(move |ctx| {
            hydrus_store::session_backups::save_automatic(
                ctx.conn(),
                &session,
                now_ms,
                previous.as_deref(),
            )
        })?;
        let changed = saved.is_some();
        if let Some(data) = saved {
            *self.0.previous.borrow_mut() = Some(data);
        }
        Ok(changed)
    }
}
