//! Where windows open and how big, as the reference keeps them
//! (`ClientGUITopLevelWindows`): placed from their frame as they open, and
//! the frame saved from them.

use hydrus_core::windows::{FrameLocation, WindowSettings, WindowState};
use hydrus_gui_model::window_rescue::{self, Point, Rect, Screen};
use hydrus_store::{Store, settings::WindowRescueSettings};
use slint::winit_030::{EventResult, WinitWindowAccessor as _, winit::event::WindowEvent};

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
    place_named_geometry(window, store, name);
    watch_named_events(window, store, name, |_, _| EventResult::Propagate);
}

/// Placement for an owner whose combined event filter is already installed.
pub fn place_named_geometry(window: &slint::Window, store: &Store, name: &str) {
    if let Some(frame) = settings(store).frame(name) {
        place(window, frame);
    }
}

/// Install one owned filter. Winit's accessor replaces its previous filter;
/// observers must compose here rather than register a second callback.
pub(crate) fn watch_named_events(
    window: &slint::Window,
    store: &Store,
    name: &str,
    next: impl FnMut(&slint::Window, &WindowEvent) -> EventResult + 'static,
) {
    let opening = settings(store).frame(name).and_then(|frame| {
        frame
            .last_position
            .filter(|_| frame.remember_position)
            .map(|position| {
                OpeningRescue::new(
                    (i64::from(position.0), i64::from(position.1)),
                    name,
                    store.read(hydrus_store::settings::get).unwrap_or_default(),
                )
            })
    });
    let mut events = NamedEvents { opening, next };
    window.on_winit_window_event(move |window, event| {
        if events
            .opening
            .as_ref()
            .is_some_and(|opening| !opening.finished)
            && window.is_visible()
        {
            events.event(
                window,
                event,
                native_screens(window).as_deref().unwrap_or_default(),
                native_size(window).unwrap_or_default(),
            )
        } else {
            (events.next)(window, event)
        }
    });
}

struct NamedEvents<F> {
    opening: Option<OpeningRescue>,
    next: F,
}
impl<F: FnMut(&slint::Window, &WindowEvent) -> EventResult> NamedEvents<F> {
    fn event(
        &mut self,
        window: &slint::Window,
        event: &WindowEvent,
        screens: &[Screen],
        size: (i64, i64),
    ) -> EventResult {
        if let Some(opening) = self.opening.as_mut() {
            opening.observe_size(window, screens, size);
        }
        (self.next)(window, event)
    }
}

/// Opening state is private to this window. The monitor query is delayed until
/// Winit has created its native window; it never borrows another owner's frame.
#[derive(Debug, Clone)]
pub struct OpeningRescue {
    desired: Point,
    name: String,
    settings: WindowRescueSettings,
    finished: bool,
}
impl OpeningRescue {
    pub fn new(desired: Point, name: &str, settings: WindowRescueSettings) -> Self {
        Self {
            desired,
            name: name.into(),
            settings,
            finished: false,
        }
    }

    /// The real opening consumer, also replayable with recorded screen geometry.
    /// A hidden or retired owner does not consume the pending opening decision.
    pub fn observe(&mut self, window: &slint::Window, screens: &[Screen]) -> Option<Point> {
        let size = state(window).size;
        self.observe_size(window, screens, (i64::from(size.0), i64::from(size.1)))
    }

    fn observe_size(
        &mut self,
        window: &slint::Window,
        screens: &[Screen],
        size: (i64, i64),
    ) -> Option<Point> {
        if self.finished || !window.is_visible() || screens.is_empty() || size.0 <= 0 || size.1 <= 0
        {
            return None;
        }
        let result =
            window_rescue::safe_position(self.desired, Some(size), &self.settings, screens)?;
        self.finished = true;
        if result != self.desired {
            let (Ok(x), Ok(y)) = (i32::try_from(result.0), i32::try_from(result.1)) else {
                return None;
            };
            #[allow(clippy::cast_precision_loss)]
            window.set_position(slint::LogicalPosition::new(x as f32, y as f32));
            eprintln!(
                "A window with frame key {:?} that wanted to display at {:?} was rescued from apparent off-screen to the new location at {:?}.",
                self.name, self.desired, result
            );
        }
        Some(result)
    }
}

/// The filter runs before Slint updates its cached size for Resize/Redraw.
/// Read the current native client area, and defer while it is still zero-sized.
#[allow(clippy::cast_possible_truncation)]
fn native_size(window: &slint::Window) -> Option<(i64, i64)> {
    window.with_winit_window(|native| {
        let size = native
            .inner_size()
            .to_logical::<f64>(f64::from(window.scale_factor()));
        (size.width.round() as i64, size.height.round() as i64)
    })
}

/// Winit has monitor bounds, but no portable work-area origin. Keep that native
/// fallback difference explicit rather than inventing taskbar/dock geometry.
#[allow(clippy::cast_possible_truncation)]
fn native_screens(window: &slint::Window) -> Option<Vec<Screen>> {
    window.with_winit_window(|native| {
        let scale = f64::from(window.scale_factor());
        let primary = native.primary_monitor();
        let mut monitors: Vec<_> = native.available_monitors().collect();
        if let Some(primary) = primary {
            monitors.sort_by_key(|monitor| *monitor != primary);
        }
        monitors
            .into_iter()
            .map(|monitor| {
                let position = monitor.position();
                let size = monitor.size();
                let geometry = Rect {
                    x: (f64::from(position.x) / scale).round() as i64,
                    y: (f64::from(position.y) / scale).round() as i64,
                    width: (f64::from(size.width) / scale).round() as i64,
                    height: (f64::from(size.height) / scale).round() as i64,
                };
                Screen {
                    geometry,
                    available_top_left: (geometry.x, geometry.y),
                }
            })
            .collect()
    })
}

/// Save only the closing owner's current geometry, preserving other frames.
pub fn save_named(window: &slint::Window, store: &Store, name: &str) {
    let state = state(window);
    let name = name.to_owned();
    if let Err(error) = store.write(move |ctx| {
        hydrus_gui_model::frame_locations::save_window_state(ctx.conn(), &name, state)
    }) {
        eprintln!("could not keep the window's size and place: {error}");
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

/// Logical size of the owner's current native monitor for cache estimates.
pub(crate) fn display_size(window: &slint::Window) -> Option<(u64, u64)> {
    window
        .with_winit_window(|native| {
            native
                .current_monitor()
                .or_else(|| native.primary_monitor())
                .map(|monitor| {
                    let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
                    (size.width as u64, size.height as u64)
                })
        })
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::ComponentHandle as _;
    use std::{cell::RefCell, rc::Rc, time::Duration};

    #[test]
    fn opening_observer_and_batched_files_share_the_same_owned_filter() {
        let _windows = crate::headless::init();
        let owner = crate::OptionsWindow::new().unwrap();
        owner
            .window()
            .set_size(slint::LogicalSize::new(100.0, 100.0));
        owner.show().unwrap();
        let screen = Screen {
            geometry: Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            available_top_left: (0, 0),
        };
        // MinimalSoftwareWindow has no OS position setter. Assert the exact
        // opening decision, rather than pretending its position can be painted.
        let mut decision =
            OpeningRescue::new((2000, 2000), "main_gui", WindowRescueSettings::default());
        assert_eq!(
            decision.observe_size(owner.window(), &[screen], (0, 0)),
            None
        );
        assert_eq!(
            decision.observe_size(owner.window(), &[screen], (100, 100)),
            Some((40, 40))
        );
        assert_eq!(
            decision.observe_size(owner.window(), &[screen], (200, 200)),
            None
        );
        let received = Rc::new(RefCell::new(Vec::new()));
        let mut events = NamedEvents {
            opening: Some(OpeningRescue::new(
                (2000, 2000),
                "main_gui",
                WindowRescueSettings::default(),
            )),
            next: crate::drops::file_handler({
                let received = received.clone();
                move |paths| received.borrow_mut().push(paths)
            }),
        };
        assert!(matches!(
            events.event(
                owner.window(),
                &WindowEvent::Focused(true),
                &[screen],
                (0, 0)
            ),
            EventResult::Propagate
        ));
        assert!(!events.opening.as_ref().unwrap().finished);
        assert!(matches!(
            events.event(
                owner.window(),
                &WindowEvent::Focused(true),
                &[screen],
                (100, 100)
            ),
            EventResult::Propagate
        ));
        assert!(events.opening.as_ref().unwrap().finished);
        for path in ["/synthetic/first.png", "/synthetic/second.png"] {
            assert!(matches!(
                events.event(
                    owner.window(),
                    &WindowEvent::DroppedFile(path.into()),
                    &[screen],
                    (100, 100)
                ),
                EventResult::PreventDefault
            ));
        }
        assert!(received.borrow().is_empty());
        std::thread::sleep(Duration::from_millis(110));
        slint::platform::update_timers_and_animations();
        assert_eq!(
            *received.borrow(),
            vec![vec![
                "/synthetic/first.png".to_string(),
                "/synthetic/second.png".to_string()
            ]]
        );
        assert!(matches!(
            events.event(
                owner.window(),
                &WindowEvent::Focused(false),
                &[screen],
                (100, 100)
            ),
            EventResult::Propagate
        ));
        assert_eq!(
            events
                .opening
                .as_mut()
                .unwrap()
                .observe_size(owner.window(), &[screen], (200, 200)),
            None
        );
        owner.hide().unwrap();
    }
}
