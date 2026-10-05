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
                    _ => None,
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
                })
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
        Gesture::keyboard(kind, key, bits, merge)
    }
    fn mouse(&self, key: u32, press: u8, bits: u8) -> Option<Gesture> {
        let mut state = self.0.borrow_mut();
        let (key, press) = state.mouse.take().unwrap_or((Some(key), press));
        Some(Gesture::new(1, key?, press, bits | (state.input.bits & 16)))
    }
}
fn settings(store: &Store) -> Settings {
    store.read(hydrus_store::settings::get).unwrap_or_default()
}
pub(crate) fn main(window: &MainWindow, store: Arc<Store>) -> Route {
    let route = Route::default();
    let active = route.1.clone();
    window.on_shortcut_key({
        let route = route.clone();
        let weak = window.as_weak();
        move |text, bits| {
            if !active.get() {
                return false;
            }
            let Some(window) = weak.upgrade().filter(|w| {
                w.window().is_visible()
                    && w.get_question().is_empty()
                    && w.get_menu_panes().row_count() == 0
                    && w.get_chooser_labels().row_count() == 0
            }) else {
                return false;
            };
            let settings = settings(&store);
            let Some(gesture) = route.keyboard(&text, bits as u8, settings.merge_numpad) else {
                return false;
            };
            match settings.command("main_gui", &gesture) {
                Some(78) => window.invoke_refresh_page(),
                Some(7) => window.invoke_close_page(),
                Some(56) => window.invoke_new_page(),
                _ => return false,
            }
            true
        }
    });
    route
}
pub(crate) fn viewer(
    window: &MediaViewerWindow,
    store: Arc<Store>,
    canvas: crate::viewing_tracking::CanvasTracker,
) -> Route {
    let route = Route::default();
    let execute: Rc<dyn Fn(i32) -> bool> = Rc::new({
        let weak = window.as_weak();
        let canvas = canvas.clone();
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
                _ => return false,
            }
            true
        }
    });
    window.on_shortcut_key({
        let route = route.clone();
        let store = store.clone();
        let execute = execute.clone();
        let canvas = canvas.clone();
        move |text, bits| {
            if !canvas.active() {
                return false;
            }
            let settings = settings(&store);
            let Some(gesture) = route.keyboard(&text, bits as u8, settings.merge_numpad) else {
                return false;
            };
            settings
                .command("media_viewer", &gesture)
                .is_some_and(|action| execute(action))
        }
    });
    window.on_shortcut_mouse({
        let route = route.clone();
        let store = store.clone();
        let execute = execute.clone();
        let weak = window.as_weak();
        let canvas = canvas.clone();
        move |key, press, bits| {
            if !canvas.active() {
                return false;
            }
            let settings = settings(&store);
            let Some(gesture) = route.mouse(key as u32, press as u8, bits as u8) else {
                return false;
            };
            let done = settings
                .command("media_viewer", &gesture)
                .is_some_and(|action| execute(action));
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
