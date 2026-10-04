//! Historical autosaves alongside the live Client API session synchronization.
use crate::{MainWindow, Pages};
use hydrus_gui_model::session_lifecycle::{Action, Autosave, Idle};
use hydrus_store::settings::{self, GuiIdleSettings, GuiSessionSettings};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

thread_local! {
    static MONITORS: RefCell<Vec<Weak<Inner>>> = const { RefCell::new(Vec::new()) };
}

/// Install the native backend before creating the first desktop window. Its
/// handler observes every auxiliary window as well as the main window.
pub fn install_activity_backend() -> Result<(), slint::PlatformError> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .with_winit_custom_application_handler(ActivityHandler)
        .select()
}

struct ActivityHandler;
impl slint::winit_030::CustomApplicationHandler for ActivityHandler {
    fn window_event(
        &mut self,
        _event_loop: &slint::winit_030::winit::event_loop::ActiveEventLoop,
        _window_id: slint::winit_030::winit::window::WindowId,
        _winit_window: Option<&slint::winit_030::winit::window::Window>,
        _slint_window: Option<&slint::Window>,
        event: &slint::winit_030::winit::event::WindowEvent,
    ) -> slint::winit_030::EventResult {
        observe_window_event(event);
        slint::winit_030::EventResult::Propagate
    }
}

/// The native handler forwards real input without consuming it. Exposed so
/// desktop replay can drive the same callback without opening an OS event loop.
pub fn observe_window_event(event: &slint::winit_030::winit::event::WindowEvent) {
    use slint::winit_030::winit::event::WindowEvent;
    let user = matches!(
        event,
        WindowEvent::KeyboardInput { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Touch(_)
            | WindowEvent::Focused(true)
    );
    let mouse = matches!(
        event,
        WindowEvent::CursorMoved { .. } | WindowEvent::Touch(_)
    );
    if !user && !mouse {
        return;
    }
    let now = hydrus_core::TimestampMs::now().0;
    MONITORS.with(|monitors| {
        monitors.borrow_mut().retain(|weak| {
            let Some(inner) = weak.upgrade() else {
                return false;
            };
            let mut idle = inner.idle.borrow_mut();
            if user {
                idle.user(now);
            }
            if mouse {
                idle.mouse(now);
            }
            true
        });
    });
}

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

impl std::fmt::Debug for Monitor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionAutosave")
            .field("schedule", &self.0.schedule.borrow())
            .field("idle", &self.0.idle.borrow())
            .finish_non_exhaustive()
    }
}

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
    MONITORS.with(|monitors| monitors.borrow_mut().push(Rc::downgrade(&monitor.0)));
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
