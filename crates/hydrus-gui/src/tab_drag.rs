//! Persistent notebook-bar pointer routing; all gesture state belongs to its owner.
use crate::{ChangePages, MainWindow, Pages, TabRow};
use hydrus_core::pages::PageKey;
use hydrus_gui_model::tab_drag::{Edge, Pointer};
use hydrus_store::settings::{self, TabDragSettings};
use slint::{ComponentHandle as _, Model as _, ModelRc};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};
#[derive(Clone, Debug)]
struct Rect {
    key: Option<PageKey>,
    parent: Option<PageKey>,
    depth: i32,
    index: i32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}
impl Rect {
    fn live(&self, rows: &ModelRc<TabRow>) -> bool {
        let Some(row) = usize::try_from(self.depth)
            .ok()
            .and_then(|depth| rows.row_data(depth))
        else {
            return false;
        };
        row.parent.as_str() == self.parent.map(|key| key.to_hex()).unwrap_or_default()
            && self.key.is_none_or(|key| {
                usize::try_from(self.index)
                    .ok()
                    .and_then(|index| row.keys.row_data(index))
                    .is_some_and(|text| text.as_str() == key.to_hex())
            })
    }
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}
#[derive(Default)]
struct State {
    tabs: HashMap<PageKey, Rect>,
    spaces: HashMap<i32, Rect>,
    pointer: Pointer,
}
impl State {
    fn hit(&mut self, window: &MainWindow, x: f32, y: f32) -> Option<Rect> {
        let rows = window.get_tab_rows();
        self.tabs.retain(|_, rect| rect.live(&rows));
        self.spaces.retain(|_, rect| rect.live(&rows));
        self.tabs
            .values()
            .chain(self.spaces.values())
            .find(|rect| rect.contains(x, y))
            .cloned()
    }
}
pub(crate) fn bind(window: &MainWindow, pages: &Rc<RefCell<Pages>>, change: ChangePages) {
    let state = Rc::new(RefCell::new(State::default()));
    window.on_tab_geometry({
        let state = state.clone();
        move |key, parent, depth, index, x, y, w, h| {
            let parent = if parent.is_empty() {
                None
            } else {
                let Some(key) = PageKey::from_hex(&parent) else {
                    return;
                };
                Some(key)
            };
            let rect = Rect {
                key: PageKey::from_hex(&key),
                parent,
                depth,
                index,
                x,
                y,
                w,
                h,
            };
            let mut state = state.borrow_mut();
            if let Some(key) = rect.key {
                state.tabs.insert(key, rect);
            } else {
                state.spaces.insert(depth, rect);
            }
        }
    });
    window.on_tab_pointer({
        let weak = window.as_weak();
        let pages = pages.clone();
        move |action, button, x, y, shift| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let prefs: TabDragSettings = pages
                .borrow()
                .store()
                .read(settings::get)
                .unwrap_or_default();
            let hit = state.borrow_mut().hit(&window, x, y);
            window.set_tab_drag_hover_key(
                hit.as_ref()
                    .and_then(|rect| rect.key)
                    .map(|key| key.to_hex())
                    .unwrap_or_default()
                    .into(),
            );
            match action {
                0 => {
                    state.borrow_mut().pointer.cancel();
                    window.set_tab_drag_active(false);
                    if button == 0
                        && let Some(rect) = hit.clone()
                    {
                        if let Some(key) = rect.key {
                            if !prefs.disabled {
                                state
                                    .borrow_mut()
                                    .pointer
                                    .start(key, rect.parent, Instant::now());
                            }
                            window.invoke_tab_chosen(rect.depth, rect.index);
                        } else {
                            window.invoke_tab_space_pressed(rect.depth, false);
                        }
                    } else if button == 1
                        && let Some(rect) = hit.clone()
                        && rect.key.is_none()
                    {
                        window.invoke_tab_menu_requested(rect.depth, rect.index, x, y);
                    } else if button == 2
                        && let Some(rect) = hit
                        && rect.key.is_none()
                    {
                        window.invoke_tab_space_pressed(rect.depth, true);
                    }
                }
                1 => {
                    let grab = state.borrow_mut().pointer.release();
                    window.set_tab_drag_active(false);
                    let valid = grab.is_some_and(|grab| {
                        pages.borrow().tab_parent(grab.key) == Some(grab.parent)
                    });
                    if button == 0
                        && !prefs.disabled
                        && let (Some(grab), Some(rect)) = (grab, hit.clone())
                        && valid
                    {
                        let edge = if rect.key.is_none() {
                            Edge::Body
                        } else if x - rect.x < 15.0 {
                            Edge::Left
                        } else if rect.x + rect.w - x <= 15.0 {
                            Edge::Right
                        } else {
                            Edge::Body
                        };
                        change(&|pages| {
                            pages.drop_tab(
                                grab.key,
                                rect.parent,
                                rect.key,
                                edge,
                                prefs.chase(shift),
                            );
                            Ok(())
                        });
                    } else if button == 1
                        && let Some(rect) = hit
                        && rect.key.is_some()
                    {
                        window.invoke_tab_menu_requested(rect.depth, rect.index, x, y);
                    } else if button == 2
                        && let Some(rect) = hit
                        && rect.key.is_some()
                    {
                        window.invoke_close_tab(rect.depth, rect.index);
                    }
                }
                2 => {
                    let grab = state
                        .borrow_mut()
                        .pointer
                        .moved(Instant::now(), prefs.disabled);
                    window.set_tab_drag_active(grab.is_some());
                    if grab.is_some()
                        && prefs.navigate(shift)
                        && let Some(rect) = hit.clone()
                        && rect.key.is_some()
                    {
                        window.invoke_tab_chosen(rect.depth, rect.index);
                    } else if let Some(rect) = hit
                        && let Some(key) = rect.key
                    {
                        let name = pages
                            .borrow()
                            .session()
                            .all_pages()
                            .into_iter()
                            .find(|page| page.key == key)
                            .map(|page| {
                                page.name
                                    .lines()
                                    .flat_map(str::chars)
                                    .take(256)
                                    .collect::<String>()
                            })
                            .unwrap_or_default();
                        window.invoke_tab_drag_hovered(name.into(), rect.x, rect.y + rect.h, true);
                    } else {
                        window.invoke_tab_drag_hovered("".into(), x, y, false);
                    }
                }
                _ => {
                    state.borrow_mut().pointer.cancel();
                    window.set_tab_drag_active(false);
                }
            }
        }
    });
}
