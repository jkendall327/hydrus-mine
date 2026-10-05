//! Historical autosaves alongside the live Client API session synchronization.
use crate::{MainWindow, Pages};
use hydrus_gui_model::session_lifecycle::{Action, Autosave, Idle, SizeWarning};
use hydrus_store::settings::{self, GuiIdleSettings, GuiSessionSettings};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

thread_local! {
    static MONITORS: RefCell<Vec<Weak<Inner>>> = const { RefCell::new(Vec::new()) };
    static POINTER_OBSERVERS: RefCell<Vec<PointerObserver>> = const { RefCell::new(Vec::new()) };
    static FOCUS_OBSERVERS: RefCell<Vec<FocusObserver>> = const { RefCell::new(Vec::new()) };
    static APPLICATION_FOCUS_OBSERVERS: RefCell<Vec<ApplicationFocusObserver>> = const { RefCell::new(Vec::new()) };
    static ACTIVE_NATIVE_WINDOW: std::cell::Cell<Option<slint::winit_030::winit::window::WindowId>> = const { std::cell::Cell::new(None) };
}

/// Weak per-native-window motion observers, shared by the desktop event handler.
pub type PointerCallback = Rc<dyn Fn((f64, f64))>;
struct PointerObserver {
    window_id: slint::winit_030::winit::window::WindowId,
    callback: Weak<dyn Fn((f64, f64))>,
}
pub fn watch_native_pointer_id(
    window_id: slint::winit_030::winit::window::WindowId,
    callback: &PointerCallback,
) {
    POINTER_OBSERVERS.with(|observers| {
        let mut observers = observers.borrow_mut();
        observers.retain(|observer| observer.callback.strong_count() > 0);
        if !observers.iter().any(|observer| {
            observer.window_id == window_id && observer.callback.ptr_eq(&Rc::downgrade(callback))
        }) {
            observers.push(PointerObserver {
                window_id,
                callback: Rc::downgrade(callback),
            });
        }
    });
}
/// The same route is exposed for headless native event replay.
pub fn observe_native_pointer(
    window_id: slint::winit_030::winit::window::WindowId,
    position: (f64, f64),
) {
    let callbacks = POINTER_OBSERVERS.with(|observers| {
        let mut observers = observers.borrow_mut();
        observers.retain(|observer| observer.callback.strong_count() > 0);
        observers
            .iter()
            .filter(|observer| observer.window_id == window_id)
            .filter_map(|observer| observer.callback.upgrade())
            .collect::<Vec<_>>()
    });
    for callback in callbacks {
        callback(position);
    }
}

/// Hold this callback in the viewer's owned state; the registry keeps only Weak.
pub type FocusCallback = Rc<dyn Fn(bool)>;
/// Distinguish another active application window from the transient no-window gap.
pub type ApplicationFocusCallback = Rc<dyn Fn(Option<slint::winit_030::winit::window::WindowId>)>;
struct ApplicationFocusObserver {
    callback: Weak<dyn Fn(Option<slint::winit_030::winit::window::WindowId>)>,
}

/// Weakly subscribe and immediately report the last observed active native window.
pub fn watch_native_application_focus(callback: &ApplicationFocusCallback) {
    APPLICATION_FOCUS_OBSERVERS.with(|observers| {
        let mut observers = observers.borrow_mut();
        observers.retain(|observer| observer.callback.strong_count() > 0);
        if !observers
            .iter()
            .any(|observer| observer.callback.ptr_eq(&Rc::downgrade(callback)))
        {
            observers.push(ApplicationFocusObserver {
                callback: Rc::downgrade(callback),
            });
        }
    });
    callback(ACTIVE_NATIVE_WINDOW.with(std::cell::Cell::get));
}

struct FocusObserver {
    window_id: slint::winit_030::winit::window::WindowId,
    callback: Weak<dyn Fn(bool)>,
}

/// Register a native viewer after showing it. Headless windows return false.
pub fn watch_native_focus(window: &slint::Window, callback: &FocusCallback) -> bool {
    use slint::winit_030::WinitWindowAccessor as _;
    window
        .with_winit_window(|native| watch_native_focus_id(native.id(), callback))
        .is_some()
}

/// Register by native identity, also used to replay focus in headless tests.
pub fn watch_native_focus_id(
    window_id: slint::winit_030::winit::window::WindowId,
    callback: &FocusCallback,
) {
    FOCUS_OBSERVERS.with(|observers| {
        let mut observers = observers.borrow_mut();
        observers.retain(|observer| observer.callback.strong_count() > 0);
        if !observers.iter().any(|observer| {
            observer.window_id == window_id && observer.callback.ptr_eq(&Rc::downgrade(callback))
        }) {
            observers.push(FocusObserver {
                window_id,
                callback: Rc::downgrade(callback),
            });
        }
    });
}

/// Dispatch one OS window focus notification without consuming the native event.
pub fn observe_native_focus(window_id: slint::winit_030::winit::window::WindowId, focused: bool) {
    let active = ACTIVE_NATIVE_WINDOW.with(|active| {
        if focused {
            active.set(Some(window_id));
        } else if active.get() == Some(window_id) {
            active.set(None);
        }
        active.get()
    });
    let application_callbacks = APPLICATION_FOCUS_OBSERVERS.with(|observers| {
        let mut observers = observers.borrow_mut();
        observers.retain(|observer| observer.callback.strong_count() > 0);
        observers
            .iter()
            .filter_map(|observer| observer.callback.upgrade())
            .collect::<Vec<_>>()
    });
    for callback in application_callbacks {
        callback(active);
    }
    let callbacks = FOCUS_OBSERVERS.with(|observers| {
        let mut observers = observers.borrow_mut();
        observers.retain(|observer| observer.callback.strong_count() > 0);
        observers
            .iter()
            .filter(|observer| observer.window_id == window_id)
            .filter_map(|observer| observer.callback.upgrade())
            .collect::<Vec<_>>()
    });
    for callback in callbacks {
        callback(focused);
    }
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
        window_id: slint::winit_030::winit::window::WindowId,
        _winit_window: Option<&slint::winit_030::winit::window::Window>,
        _slint_window: Option<&slint::Window>,
        event: &slint::winit_030::winit::event::WindowEvent,
    ) -> slint::winit_030::EventResult {
        match event {
            slint::winit_030::winit::event::WindowEvent::Focused(focused) => {
                observe_native_focus(window_id, *focused);
            }
            slint::winit_030::winit::event::WindowEvent::Destroyed => {
                observe_native_focus(window_id, false);
            }
            slint::winit_030::winit::event::WindowEvent::CursorMoved { position, .. } => {
                observe_native_pointer(window_id, (position.x, position.y));
            }
            _ => {}
        }
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
    force_idle: RefCell<Option<Weak<crate::force_idle::State>>>,
    previous: RefCell<Option<String>>,
    api_seen: std::cell::Cell<i64>,
    size_warning: RefCell<SizeWarning>,
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
        force_idle: RefCell::new(None),
        previous: RefCell::new(None),
        api_seen: std::cell::Cell::new(0),
        size_warning: RefCell::new(SizeWarning::default()),
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
    pub(crate) fn attach_force_idle(&self, state: Weak<crate::force_idle::State>) {
        *self.0.force_idle.borrow_mut() = Some(state);
    }
    pub fn idle_at(&self, now_ms: i64) -> bool {
        if let Some(source) = self.0.force_idle.borrow().as_ref() {
            let Some(state) = source.upgrade() else {
                return false;
            };
            if let Some(idle) = state.idle_override() {
                return idle;
            }
        }
        let config: GuiIdleSettings = self
            .0
            .pages
            .borrow()
            .store()
            .read(settings::get)
            .unwrap_or_default();
        if let Ok(Some(at)) =
            hydrus_store::api_activity::latest(self.0.pages.borrow().store().dir())
            && at > self.0.api_seen.get()
        {
            self.0.api_seen.set(at);
            self.api_at(at);
        }
        self.0.idle.borrow().eligible(now_ms, &config)
    }

    /// Check the live session's computed weight and publish its one-boot warning.
    /// The regular monitor tick supplies Pages::session_weight; desktop replay
    /// supplies reference counts without constructing millions of fake seeds.
    pub fn check_size(&self, weight: u64, now_ms: i64) -> hydrus_store::Result<bool> {
        let store = self.0.pages.borrow().store().clone();
        let config: GuiSessionSettings = store.read(settings::get)?;
        let Some(text) = self
            .0
            .size_warning
            .borrow_mut()
            .message(weight, config.warn_large_session)
        else {
            return Ok(false);
        };
        let job = hydrus_store::popups::Job::text(text, now_ms as f64 / 1_000.0);
        store.write(move |ctx| hydrus_store::popups::add(ctx.conn(), &job, now_ms / 1_000))?;
        Ok(true)
    }

    /// One real timer tick, also available for deterministic desktop replay.
    /// Return whether a changed snapshot was saved.
    pub fn poll_at(&self, now_ms: i64) -> hydrus_store::Result<bool> {
        if self.0.window.upgrade().is_none() {
            return Ok(false);
        }
        let store = self.0.pages.borrow().store().clone();
        let config: GuiSessionSettings = store.read(settings::get)?;
        if config.warn_large_session && !self.0.size_warning.borrow().shown() {
            let weight = self.0.pages.borrow().session_weight();
            self.check_size(weight, now_ms)?;
        }
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

#[cfg(test)]
mod pointer_tests {
    use super::*;
    #[test]
    fn native_motion_targets_one_window_and_releases_dead_viewers() {
        let id = slint::winit_030::winit::window::WindowId::from(80001);
        let other = slint::winit_030::winit::window::WindowId::from(80002);
        let calls = Rc::new(RefCell::new(Vec::new()));
        let callback: PointerCallback = Rc::new({
            let calls = calls.clone();
            move |position| calls.borrow_mut().push(position)
        });
        watch_native_pointer_id(id, &callback);
        watch_native_pointer_id(id, &callback);
        observe_native_pointer(other, (5.0, 7.0));
        assert!(calls.borrow().is_empty());
        observe_native_pointer(id, (5.0, 7.0));
        assert_eq!(calls.borrow().len(), 1);
        let weak = Rc::downgrade(&callback);
        drop(callback);
        assert!(weak.upgrade().is_none());
        observe_native_pointer(id, (9.0, 11.0));
        assert_eq!(calls.borrow().len(), 1);
    }
}
