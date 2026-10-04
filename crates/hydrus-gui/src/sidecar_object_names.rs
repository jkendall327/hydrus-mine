//! The JSON destination's literal object-name queue and owned text/question child.
use super::{Editing, NodeState};
use crate::{SessionDialog, SidecarNodeWindow};
use hydrus_gui_model::sidecar_editors::object_names::PROMPT;
use slint::ComponentHandle;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
type Slot = Rc<RefCell<Option<SessionDialog>>>;

pub(super) fn cancel(slot: &Slot) {
    let child = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    if let Some(child) = child {
        child.invoke_cancelled();
    }
}
pub(super) fn bind(
    window: &SidecarNodeWindow,
    slot: &Slot,
    state: Rc<RefCell<NodeState>>,
    active: Rc<Cell<bool>>,
    refresh: Rc<dyn Fn()>,
) {
    let editable: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let state = state.clone();
        let slot = slot.clone();
        move || {
            active.get()
                && slot.borrow().is_none()
                && state.borrow().asking.is_none()
                && matches!(&state.borrow().editing,Editing::Destination(e) if e.shown().nested)
        }
    });
    window.on_nested_clicked({
        let editable = editable.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        move |index, ctrl, shift| {
            if editable()
                && let Ok(index) = usize::try_from(index)
            {
                state.borrow_mut().names.click(index, ctrl, shift);
                refresh();
            }
        }
    });
    window.on_nested_action({
        let weak = window.as_weak();
        let slot = slot.clone();
        move |action| {
            if !editable() {
                return;
            }
            let editing = match action.as_str() {
                "add" => Some((None, String::new())),
                "edit" => state
                    .borrow()
                    .names
                    .first_selected()
                    .map(|(id, name)| (Some(id), name)),
                "up" | "down" => {
                    state
                        .borrow_mut()
                        .names
                        .move_selected(if action == "up" { -1 } else { 1 });
                    refresh();
                    return;
                }
                "delete" => None,
                _ => return,
            };
            if action == "edit" && editing.is_none() {
                return;
            }
            let deleting = action == "delete";
            let question = if deleting {
                state.borrow().names.delete_question()
            } else {
                None
            };
            if action == "delete" && question.is_none() {
                return;
            }
            let Ok(child) = SessionDialog::new() else {
                return;
            };
            child.set_asking_name(editing.is_some());
            child.set_message(question.as_deref().unwrap_or(PROMPT).into());
            if let Some((_, name)) = &editing {
                child.set_text(name.as_str().into());
            }
            if editing.is_none() {
                child.set_window_title("Are you sure?".into());
            }
            let child_active = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let child_weak = child.as_weak();
                let slot = slot.clone();
                let parent = weak.clone();
                let child_active = child_active.clone();
                let active = active.clone();
                let refresh = refresh.clone();
                move || {
                    if !child_active.replace(false) {
                        return;
                    }
                    if let Some(child) = child_weak.upgrade() {
                        let _ = child.hide();
                    }
                    slot.borrow_mut().take();
                    if let Some(parent) = parent.upgrade() {
                        parent.set_object_child_open(false);
                    }
                    if active.get() {
                        refresh();
                    }
                }
            });
            child.on_name_entered({
                let state = state.clone();
                let close = close.clone();
                let child_active = child_active.clone();
                let active = active.clone();
                let weak = child.as_weak();
                move |text| {
                    if !active.get() || !child_active.get() {
                        return;
                    }
                    let Some((id, _)) = &editing else {
                        return;
                    };
                    let result = state.borrow_mut().names.accept(*id, text.to_string());
                    match result {
                        Ok(()) => {
                            close();
                        }
                        Err(error) => {
                            if let Some(child) = weak.upgrade() {
                                child.set_warning(error.into());
                            }
                        }
                    }
                }
            });
            child.on_answered({
                let state = state.clone();
                let close = close.clone();
                let child_active = child_active.clone();
                let active = active.clone();
                move |yes| {
                    if !deleting || !active.get() || !child_active.get() {
                        return;
                    }
                    if yes {
                        state.borrow_mut().names.delete();
                    }
                    close();
                }
            });
            child.on_cancelled({
                let close = close.clone();
                move || close()
            });
            child.window().on_close_requested({
                let close = close.clone();
                move || {
                    close();
                    slint::CloseRequestResponse::HideWindow
                }
            });
            if let Some(parent) = weak.upgrade() {
                parent.set_object_child_open(true);
            }
            *slot.borrow_mut() = Some(child.clone_strong());
            if child.show().is_err() {
                close();
            }
        }
    });
}
