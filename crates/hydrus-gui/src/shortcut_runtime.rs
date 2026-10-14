//! Persisted capture identities dispatch into the existing application consumers.
use crate::{MainWindow, MediaViewerWindow};
use hydrus_core::shortcuts::{Gesture, Settings};
use hydrus_gui_model::shortcut_capture::Wheel;
use hydrus_store::Store;
use slint::{
    ComponentHandle as _, Model as _,
    winit_030::{
        EventResult,
        winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Debug, Default)]
struct Native {
    input: crate::shortcut_input::Input,
    cursor: (f64, f64),
    click: Option<(MouseButton, Instant, (f64, f64))>,
    mouse: Option<(Option<u32>, u8)>,
    wheel: Option<f32>,
}
impl Native {
    fn observe(&mut self, event: &WindowEvent) {
        self.input.observe(event);
        match event {
            WindowEvent::CursorMoved { position, .. } => self.cursor = (position.x, position.y),
            WindowEvent::MouseInput { state, button, .. } => {
                let key = match button {
                    MouseButton::Left => Some(0),
                    MouseButton::Right => Some(1),
                    MouseButton::Middle => Some(2),
                    MouseButton::Back => Some(7),
                    MouseButton::Forward => Some(8),
                    MouseButton::Other(5 | 10 | 0x117) => Some(9),
                    MouseButton::Other(_) => None,
                };
                let mut press = u8::from(*state == ElementState::Released);
                if *state == ElementState::Pressed {
                    let now = Instant::now();
                    if self.click.as_ref().is_some_and(|(old, time, point)| {
                        old == button
                            && now.duration_since(*time) <= Duration::from_millis(400)
                            && (point.0 - self.cursor.0).abs() + (point.1 - self.cursor.1).abs()
                                <= 5.0
                    }) {
                        press = 2;
                        self.click = None;
                    } else {
                        self.click = Some((*button, now, self.cursor));
                    }
                }
                self.mouse = Some((key, press));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.wheel = Some(match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * 120.0,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                });
            }
            WindowEvent::Focused(false) => {
                self.click = None;
                self.mouse = None;
                self.wheel = None;
            }
            _ => (),
        }
    }
}
/// What shortcut report mode calls the main window where the reference
/// quotes the Python repr of its frame.
const MAIN_WINDOW: &str = "main window";
/// The media viewer's, the same way.
const MEDIA_VIEWER: &str = "media viewer";
#[derive(Debug, Clone)]
pub(crate) struct Route(Rc<RefCell<Native>>, Rc<Cell<bool>>);
impl Default for Route {
    fn default() -> Self {
        Self(Rc::default(), Rc::new(Cell::new(true)))
    }
}
impl Route {
    pub(crate) fn retire(&self) {
        self.1.set(false);
    }
    pub(crate) fn observer(
        &self,
    ) -> impl FnMut(&slint::Window, &WindowEvent) -> EventResult + 'static {
        let state = self.0.clone();
        move |_, event| {
            state.borrow_mut().observe(event);
            EventResult::Propagate
        }
    }
    fn keyboard(&self, text: &str, bits: u8, merge: bool) -> Option<Gesture> {
        let input = self
            .0
            .borrow_mut()
            .input
            .pending
            .take()
            .or_else(|| crate::shortcut_input::slint_key(text, bits));
        let (kind, key, bits) = input?;
        let gesture = Gesture::keyboard(kind, key, bits, merge)?;
        caught("key", &gesture);
        Some(gesture)
    }
    fn mouse(&self, key: u32, press: u8, bits: u8) -> Option<Gesture> {
        let mut state = self.0.borrow_mut();
        let (key, press) = state.mouse.take().unwrap_or((Some(key), press));
        let gesture = Gesture::new(1, key?, press, bits | (state.input.bits & 16));
        caught("mouse", &gesture);
        Some(gesture)
    }
}
/// GUI report mode (`ClientGUIShortcuts`): a key or mouse event became a shortcut.
fn caught(kind: &str, gesture: &Gesture) {
    hydrus_core::debug_flags::report(hydrus_core::debug_flags::Flag::GuiReport, || {
        format!("{kind} event caught: Shortcut: {}", gesture.text(false))
    });
}
/// Shortcut report mode (`ClientGUIShortcuts`): a key or mouse press went
/// through a window's shortcut handler (`eventFilter`), which says whether it
/// was in a state to catch it. `parent` names the window the reference
/// quotes by its Python repr.
fn passing_through(kind: &str, gesture: &Gesture, parent: &str, in_state: bool) {
    hydrus_core::debug_flags::report(hydrus_core::debug_flags::Flag::ShortcutReport, || {
        format!(
            "{kind} shortcut \"{}\" passing through {parent}. I am {} a state to catch it.",
            gesture.text(false),
            if in_state { "in" } else { "not in" }
        )
    });
}
/// Shortcut report mode: a set had a command for a gesture
/// (`ShortcutsManager.GetCommand`).
fn matched_on(set: &str, gesture: &Gesture, command: i32) {
    hydrus_core::debug_flags::report(hydrus_core::debug_flags::Flag::ShortcutReport, || {
        format!(
            "Shortcut \"{}\" matched on \"{set}\" set to \"{}\" command.",
            gesture.text(false),
            hydrus_gui_model::shortcut_capture::command_name(command)
        )
    });
}
/// Shortcut report mode: the command was run (`_ProcessShortcut`).
fn matched_to(gesture: &Gesture, command: i32, parent: &str, processed: bool) {
    hydrus_core::debug_flags::report(hydrus_core::debug_flags::Flag::ShortcutReport, || {
        format!(
            "Shortcut \"{}\" matched to command \"{}\" on {parent}. It was {}processed.",
            gesture.text(false),
            hydrus_gui_model::shortcut_capture::command_name(command),
            if processed { "" } else { "not " }
        )
    });
}
/// A set's command for a gesture, said under shortcut report mode.
fn command_in(settings: &Settings, set: &str, gesture: &Gesture) -> Option<i32> {
    let command = settings.command(set, gesture)?;
    matched_on(set, gesture, command);
    Some(command)
}
fn settings(store: &Store) -> Settings {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}
/// What applies a tag or rating shortcut to the thumbnails selected, once
/// the main window has it.
pub(crate) type GridContent =
    Rc<RefCell<Option<Rc<dyn Fn(&hydrus_core::shortcuts::ContentCommand) -> bool>>>>;

/// What runs a simple command (by its number) on the thumbnails selected; whether it did.
pub(crate) type GridSimple = Rc<RefCell<Option<Rc<dyn Fn(i32) -> bool>>>>;

pub(crate) fn main(
    window: &MainWindow,
    store: Arc<Store>,
    grid: GridContent,
    grid_simple: GridSimple,
) -> Route {
    let route = Route::default();
    let active = route.1.clone();
    window.on_shortcut_key({
        let route = route.clone();
        let weak = window.as_weak();
        move |text, bits| {
            if !active.get() {
                return false;
            }
            let Some(window) = weak.upgrade() else {
                return false;
            };
            let in_state = window.window().is_visible()
                && window.get_question().is_empty()
                && window.get_menu_panes().row_count() == 0
                && window.get_chooser_labels().row_count() == 0;
            let settings = settings(&store);
            let Some(gesture) = route.keyboard(&text, bits as u8, settings.merge_numpad) else {
                return false;
            };
            passing_through("Key", &gesture, MAIN_WINDOW, in_state);
            if !in_state {
                return false;
            }
            let command = command_in(&settings, "main_gui", &gesture);
            let processed = match command {
                Some(78) => {
                    window.invoke_refresh_page();
                    true
                }
                Some(7) => {
                    window.invoke_close_page();
                    true
                }
                Some(56) => {
                    window.invoke_new_page();
                    true
                }
                Some(_) => false,
                // the thumbnails take the "media" and "thumbnails" sets' simple commands
                // and the "media" set's tag and rating commands
                None => {
                    if window.get_grid_focused() {
                        let simple = grid_simple.borrow().clone();
                        if let Some(simple) = simple {
                            for set in ["media", "thumbnails"] {
                                if let Some(action) = command_in(&settings, set, &gesture) {
                                    let done = simple(action);
                                    matched_to(&gesture, action, MAIN_WINDOW, done);
                                    if done {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                    let content = grid.borrow().clone();
                    return match (window.get_grid_focused(), content) {
                        (true, Some(content)) => settings
                            .content_command("media", &gesture)
                            .is_some_and(|command| content(command)),
                        _ => false,
                    };
                }
            };
            if let Some(command) = command {
                matched_to(&gesture, command, MAIN_WINDOW, processed);
            }
            processed
        }
    });
    route
}
/// The sets whose tag and rating shortcuts the viewer runs.
const CONTENT_SETS: [&str; 2] = ["media", "media_viewer"];

fn content_command(
    settings: &Settings,
    gesture: &Gesture,
    custom: &[String],
) -> Option<hydrus_core::shortcuts::ContentCommand> {
    CONTENT_SETS
        .iter()
        .copied()
        .chain(custom.iter().map(String::as_str))
        .find_map(|set| settings.content_command(set, gesture).cloned())
}

/// The viewer's simple command for `gesture`: its own set's, else a custom
/// set's that is turned on.
fn viewer_command(settings: &Settings, gesture: &Gesture, custom: &[String]) -> Option<i32> {
    command_in(settings, "media_viewer", gesture)
        .or_else(|| custom.iter().find_map(|set| command_in(settings, set, gesture)))
}

pub(crate) fn viewer(
    window: &MediaViewerWindow,
    store: Arc<Store>,
    canvas: crate::viewing_tracking::CanvasTracker,
    content: &Rc<dyn Fn(&hydrus_core::shortcuts::ContentCommand) -> bool>,
    custom: &Rc<RefCell<Vec<String>>>,
    simple: &Rc<dyn Fn(i32) -> bool>,
) -> Route {
    let route = Route::default();
    let execute: Rc<dyn Fn(i32) -> bool> = Rc::new({
        let weak = window.as_weak();
        let canvas = canvas.clone();
        let simple = simple.clone();
        move |action| {
            if !canvas.active() {
                return false;
            }
            let Some(window) = weak.upgrade().filter(|w| {
                w.window().is_visible() && w.get_question().is_empty() && w.get_warning().is_empty()
            }) else {
                return false;
            };
            match action {
                6 => window.invoke_close_requested(),
                99 => window.invoke_next(),
                100 => window.invoke_previous(),
                101 => window.invoke_zoom(1, false, 0.0, 0.0),
                102 => window.invoke_zoom(-1, false, 0.0, 0.0),
                92 => window.invoke_toggle_fullscreen(),
                // the commands the application handles (open in a new duplicates page)
                _ => return simple(action),
            }
            true
        }
    });
    window.on_shortcut_key({
        let route = route.clone();
        let store = store.clone();
        let execute = execute.clone();
        let canvas = canvas.clone();
        let content = content.clone();
        let custom = custom.clone();
        move |text, bits| {
            if !canvas.active() {
                return false;
            }
            let settings = settings(&store);
            let Some(gesture) = route.keyboard(&text, bits as u8, settings.merge_numpad) else {
                return false;
            };
            passing_through("Key", &gesture, MEDIA_VIEWER, true);
            let custom = custom.borrow().clone();
            if let Some(command) = content_command(&settings, &gesture, &custom) {
                return content(&command);
            }
            let command = viewer_command(&settings, &gesture, &custom);
            let done = command.is_some_and(|action| execute(action));
            if let Some(command) = command {
                matched_to(&gesture, command, MEDIA_VIEWER, done);
            }
            done
        }
    });
    window.on_shortcut_mouse({
        let route = route.clone();
        let store = store.clone();
        let execute = execute.clone();
        let weak = window.as_weak();
        let canvas = canvas.clone();
        let content = content.clone();
        let custom = custom.clone();
        move |key, press, bits| {
            if !canvas.active() {
                return false;
            }
            let settings = settings(&store);
            let Some(gesture) = route.mouse(key as u32, press as u8, bits as u8) else {
                return false;
            };
            let custom = custom.borrow().clone();
            let done = if let Some(command) = content_command(&settings, &gesture, &custom) {
                content(&command)
            } else {
                viewer_command(&settings, &gesture, &custom).is_some_and(|action| execute(action))
            };
            if done
                && gesture.press == 2
                && let Some(window) = weak.upgrade()
            {
                window.set_shortcut_double_consumed(true);
            }
            done
        }
    });
    let wheel = Rc::new(RefCell::new(Wheel::default()));
    window.on_shortcut_wheel({
        let route = route.clone();
        let execute = execute.clone();
        move |delta, bits| {
            if !canvas.active() {
                return false;
            }
            let settings = settings(&store);
            let delta = route.0.borrow_mut().wheel.take().unwrap_or(delta);
            let bits = bits as u8 | (route.0.borrow().input.bits & 16);
            let Some(key) = wheel.borrow_mut().key(delta as i32) else {
                return settings.sets.get("media_viewer").is_some_and(|bindings| {
                    bindings.iter().any(|b| {
                        b.gesture.kind == 1
                            && matches!(b.gesture.key, 3 | 4)
                            && b.gesture.bits() == bits
                    })
                });
            };
            settings
                .command("media_viewer", &Gesture::new(1, key, 0, bits))
                .is_some_and(|action| execute(action))
        }
    });
    route
}
