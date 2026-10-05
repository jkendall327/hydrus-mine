//! Files-and-Trash's staged ordered reason queue and owned Enter Text child.
use crate::{OptionsWindow, SessionDialog, TableRow};
use hydrus_gui_model::delete_files::ReasonQueue;
use hydrus_gui_model::options::Editor;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Options-owned queue hooks. Parent cancellation invalidates every child callback.
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
/// The custom reason Enter Text/question child explicitly owned by Options.
pub type Slot = Rc<RefCell<Option<SessionDialog>>>;

pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
    child: &Slot,
) -> Binding {
    let queue = Rc::new(RefCell::new(ReasonQueue::new(
        &editor.borrow().edited_deletion_reasons(),
    )));
    let child = child.clone();
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let queue = queue.clone();
        let child = Rc::downgrade(&child);
        move || {
            if let Some(window) = weak.upgrade() {
                let queue = queue.borrow();
                window.set_reason_rows(ModelRc::new(VecModel::from(
                    queue
                        .rows()
                        .iter()
                        .map(|(key, text)| TableRow {
                            cells: ModelRc::new(VecModel::from(vec![text.as_str().into()])),
                            selected: queue.selection.is_selected(*key),
                        })
                        .collect::<Vec<_>>(),
                )));
                window.set_reason_selected(!queue.selection.is_empty());
                window.set_reason_child_open(
                    child
                        .upgrade()
                        .is_some_and(|child| child.borrow().is_some()),
                );
            }
        }
    });
    let cancel: Rc<dyn Fn()> = Rc::new({
        let child = child.clone();
        move || {
            let dialog = child
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(dialog) = dialog {
                dialog.invoke_cancelled();
            }
        }
    });
    window.on_reason_clicked({
        let active = active.clone();
        let queue = queue.clone();
        let editor = editor.clone();
        let child = child.clone();
        let show = show.clone();
        move |index, ctrl, shift| {
            if !active.get()
                || child.borrow().is_some()
                || !editor.borrow().applied().0.deletion.advanced
            {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                queue.borrow_mut().click(index, ctrl, shift);
                show();
            }
        }
    });
    window.on_reason_activated({
        let weak = window.as_weak();
        move |index| {
            if let Some(window) = weak.upgrade() {
                window.invoke_reason_clicked(index, false, false);
                window.invoke_reason_action("edit".into());
            }
        }
    });
    window.on_reason_action({
        let active = active.clone();
        let weak = window.as_weak();
        let queue = queue.clone();
        let editor = editor.clone();
        let child = child.clone();
        let show = show.clone();
        move |action| {
            if !active.get()
                || child.borrow().is_some()
                || !editor.borrow().applied().0.deletion.advanced
            {
                return;
            }
            match action.as_str() {
                "up" | "down" => {
                    queue.borrow_mut().move_selected(action == "down");
                    editor
                        .borrow_mut()
                        .set_deletion_reasons(queue.borrow().values());
                    show();
                    return;
                }
                "add" | "edit" | "delete" => {}
                _ => return,
            }
            let editing = if action == "edit" {
                queue
                    .borrow()
                    .editing()
                    .map(|(key, value)| (Some(key), value.to_owned()))
            } else if action == "add" {
                Some((None, "I do not like the file.".into()))
            } else {
                None
            };
            let question = (action == "delete")
                .then(|| queue.borrow().removal_question())
                .flatten();
            if editing.is_none() && question.is_none() {
                return;
            }
            let Ok(dialog) = SessionDialog::new() else {
                return;
            };
            dialog.set_message(question.as_deref().unwrap_or("Enter the reason").into());
            dialog.set_asking_name(editing.is_some());
            if let Some((_, text)) = &editing {
                dialog.set_text(text.as_str().into());
            }
            let child_active = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let child = Rc::downgrade(&child);
                let dialog = dialog.as_weak();
                let child_active = child_active.clone();
                let show = show.clone();
                move || {
                    if !child_active.replace(false) {
                        return;
                    }
                    if let Some(dialog) = dialog.upgrade() {
                        let _ = dialog.hide();
                    }
                    if let Some(child) = child.upgrade() {
                        child.borrow_mut().take();
                    }
                    show();
                }
            });
            dialog.on_name_entered({
                let queue = queue.clone();
                let editor = editor.clone();
                let active = active.clone();
                let child_active = child_active.clone();
                let close = close.clone();
                let owner = weak.clone();
                move |value| {
                    if !active.get() || !child_active.get() || owner.upgrade().is_none() {
                        return;
                    }
                    if let Some((key, _)) = &editing {
                        if let Some(key) = key {
                            queue.borrow_mut().replace(*key, value.to_string());
                        } else {
                            queue.borrow_mut().add(value.to_string());
                        }
                        editor
                            .borrow_mut()
                            .set_deletion_reasons(queue.borrow().values());
                    }
                    close();
                }
            });
            dialog.on_answered({
                let queue = queue.clone();
                let editor = editor.clone();
                let active = active.clone();
                let child_active = child_active.clone();
                let close = close.clone();
                let owner = weak.clone();
                move |yes| {
                    if !active.get() || !child_active.get() || owner.upgrade().is_none() {
                        return;
                    }
                    if yes && question.is_some() {
                        queue.borrow_mut().remove_selected();
                        editor
                            .borrow_mut()
                            .set_deletion_reasons(queue.borrow().values());
                    }
                    close();
                }
            });
            dialog.on_cancelled({
                let close = close.clone();
                move || close()
            });
            dialog.window().on_close_requested(move || {
                close();
                slint::CloseRequestResponse::HideWindow
            });
            *child.borrow_mut() = Some(dialog.clone_strong());
            let _ = dialog.show();
            show();
        }
    });
    show();
    Binding {
        show,
        cancel,
        has_open: Rc::new(move || child.borrow().is_some()),
    }
}
