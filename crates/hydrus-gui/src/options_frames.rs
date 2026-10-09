//! Parent-owned frame table and detached geometry child. Child Apply only
//! replaces the Options draft; parent Apply alone persists it.
use crate::{FrameLocationWindow, OptionsWindow, TableRow};
use hydrus_core::windows::FrameLocation;
use hydrus_gui_model::frame_locations::{Action, Table, normalised};
use hydrus_gui_model::options::Editor;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The geometry child explicitly retained by its Options owner.
pub type Slot = Rc<RefCell<Option<FrameLocationWindow>>>;
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}

pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
    child: &Slot,
) -> Binding {
    let table = Rc::new(RefCell::new(Table::new(
        editor.borrow().edited_frame_locations(),
    )));
    let show: Rc<dyn Fn()> = Rc::new({
        let table = table.clone();
        let weak = window.as_weak();
        let child = Rc::downgrade(child);
        move || {
            if let Some(window) = weak.upgrade() {
                let table = table.borrow();
                window.set_frame_rows(ModelRc::new(VecModel::from(
                    table
                        .rows
                        .iter()
                        .map(|row| TableRow {
                            cells: ModelRc::new(VecModel::from(
                                row.cells().into_iter().map(Into::into).collect::<Vec<_>>(),
                            )),
                            selected: table.selection.is_selected(row.id),
                        })
                        .collect::<Vec<_>>(),
                )));
                window.set_frame_selected(!table.selection.is_empty());
                window.set_frame_single(table.selected().is_some());
                window.set_frame_sort_column(i32::try_from(table.sort_column).unwrap_or(0));
                window.set_frame_ascending(table.ascending);
                window.set_frame_child_open(child.upgrade().is_some_and(|s| s.borrow().is_some()));
            }
        }
    });
    let cancel: Rc<dyn Fn()> = Rc::new({
        let child = Rc::downgrade(child);
        move || {
            let window = child.upgrade().and_then(|s| {
                s.borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            });
            if let Some(window) = window {
                window.invoke_cancel();
            }
        }
    });
    window.on_frame_clicked({
        let table = table.clone();
        let show = show.clone();
        let active = active.clone();
        let child = child.clone();
        move |i, c, s| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                table.borrow_mut().click(i, c, s);
                show();
            }
        }
    });
    window.on_frame_sort({
        let table = table.clone();
        let show = show.clone();
        let active = active.clone();
        let child = child.clone();
        move |i, a| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                table.borrow_mut().sort(i, a);
                show();
            }
        }
    });
    window.on_frame_activated({
        let weak = window.as_weak();
        move |i| {
            if let Some(window) = weak.upgrade() {
                window.invoke_frame_clicked(i, false, false);
                window.invoke_frame_action("edit".into());
            }
        }
    });
    window.on_frame_action({
        let table = table.clone();
        let show = show.clone();
        let editor = editor.clone();
        let active = active.clone();
        let child = child.clone();
        move |action| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            let action = match action.as_str() {
                "flip-size" => Some(Action::FlipSize),
                "flip-position" => Some(Action::FlipPosition),
                "reset-size" => Some(Action::ResetSize),
                "reset-position" => Some(Action::ResetPosition),
                "edit" => None,
                _ => return,
            };
            if let Some(action) = action {
                table.borrow_mut().action(action);
                editor
                    .borrow_mut()
                    .set_frame_locations(table.borrow().values());
                show();
                return;
            }
            let Some(row) = table.borrow().selected().cloned() else {
                return;
            };
            let Ok(window) = crate::app_title::new::<crate::FrameLocationWindow>() else {
                return;
            };
            let frame = normalised(row.frame);
            let extra = match row.name.as_str() {
                "manage_tags_dialog" => {
                    "\n\nThis is the manage tags dialog launched off the thumbnail grid."
                }
                "manage_tags_frame" => {
                    "\n\nThis is the manage tags dialog launched off the media viewer."
                }
                _ => "",
            };
            window.set_message(
                format!("Setting frame location info for {}.{extra}", row.name).into(),
            );
            window.set_remember_size(frame.remember_size);
            window.set_remember_position(frame.remember_position);
            window.set_size_none(frame.last_size.is_none());
            window.set_position_none(frame.last_position.is_none());
            let (w, h) = frame.last_size.unwrap_or((640, 480));
            let (x, y) = frame.last_position.unwrap_or((20, 20));
            window.set_last_width(w);
            window.set_last_height(h);
            window.set_last_x(x);
            window.set_last_y(y);
            window.set_gravity_x(i32::from(frame.default_gravity.0 < 0));
            window.set_gravity_y(i32::from(frame.default_gravity.1 < 0));
            window.set_default_position(match frame.default_position.as_str() {
                "center" => 1,
                "mouse" => 2,
                _ => 0,
            });
            window.set_maximised(frame.maximised);
            window.set_fullscreen(frame.fullscreen);
            let child_active = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let child = Rc::downgrade(&child);
                let weak = window.as_weak();
                let child_active = child_active.clone();
                let show = show.clone();
                move || {
                    if !child_active.replace(false) {
                        return;
                    }
                    if let Some(window) = weak.upgrade() {
                        let _ = window.hide();
                    }
                    if let Some(child) = child.upgrade() {
                        child.borrow_mut().take();
                    }
                    show();
                }
            });
            window.on_apply({
                let weak = window.as_weak();
                let table = table.clone();
                let editor = editor.clone();
                let active = active.clone();
                let child_active = child_active.clone();
                let close = close.clone();
                move || {
                    if !active.get() || !child_active.get() {
                        return;
                    }
                    let Some(window) = weak.upgrade() else {
                        return;
                    };
                    let frame = FrameLocation {
                        remember_size: window.get_remember_size(),
                        remember_position: window.get_remember_position(),
                        last_size: (!window.get_size_none())
                            .then(|| (window.get_last_width(), window.get_last_height())),
                        last_position: (!window.get_position_none())
                            .then(|| (window.get_last_x(), window.get_last_y())),
                        default_gravity: (
                            if window.get_gravity_x() == 0 { 1 } else { -1 },
                            if window.get_gravity_y() == 0 { 1 } else { -1 },
                        ),
                        default_position: match window.get_default_position() {
                            1 => "center",
                            2 => "mouse",
                            _ => "topleft",
                        }
                        .into(),
                        maximised: window.get_maximised(),
                        fullscreen: window.get_fullscreen(),
                    };
                    table.borrow_mut().replace(row.id, normalised(frame));
                    editor
                        .borrow_mut()
                        .set_frame_locations(table.borrow().values());
                    close();
                }
            });
            window.on_cancel({
                let close = close.clone();
                move || close()
            });
            window.window().on_close_requested(move || {
                close();
                slint::CloseRequestResponse::HideWindow
            });
            *child.borrow_mut() = Some(window.clone_strong());
            let _ = window.show();
            show();
        }
    });
    Binding {
        show,
        cancel,
        has_open: Rc::new({
            let child = Rc::downgrade(child);
            move || child.upgrade().is_some_and(|c| c.borrow().is_some())
        }),
    }
}
