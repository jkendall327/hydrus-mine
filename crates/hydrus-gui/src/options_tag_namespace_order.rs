//! Staged raw namespace grouping order and its owned Enter Text/questions.
use crate::{OptionsWindow, SessionDialog, TableRow};
use hydrus_gui_model::{
    options::Editor,
    tag_namespace_order::{self, Editor as Queue},
};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Retire = Rc<RefCell<Option<Rc<dyn Fn()>>>>;
type Slot = Rc<RefCell<Option<SessionDialog>>>;
thread_local! { static LAST: RefCell<Option<slint::Weak<SessionDialog>>> = const { RefCell::new(None) }; }
/// The latest still-owned Enter Text or removal question, without retaining it.
pub fn last_opened() -> Option<SessionDialog> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
}
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
pub(crate) fn bind(
    window: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
) -> Binding {
    let queue = Rc::new(RefCell::new(Queue::new(
        &editor.borrow().edited_tag_namespace_order(),
    )));
    let child: Slot = Rc::default();
    let retire: Retire = Rc::default();
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let queue = queue.clone();
        let child = Rc::downgrade(&child);
        move || {
            if let Some(window) = weak.upgrade() {
                let queue = queue.borrow();
                window.set_tag_namespace_rows(ModelRc::new(VecModel::from(
                    queue
                        .0
                        .rows()
                        .iter()
                        .map(|(id, text)| TableRow {
                            cells: ModelRc::new(VecModel::from(vec![Queue::label(text).into()])),
                            selected: queue.0.selection.is_selected(*id),
                        })
                        .collect::<Vec<_>>(),
                )));
                window.set_tag_namespace_selected(!queue.0.selection.is_empty());
                window.set_tag_namespace_child_open(
                    child
                        .upgrade()
                        .is_some_and(|child| child.borrow().is_some()),
                );
            }
        }
    });
    let cancel: Rc<dyn Fn()> = Rc::new({
        let retire = retire.clone();
        move || {
            let close = retire.borrow().clone();
            if let Some(close) = close {
                close();
            }
        }
    });
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        move || {
            active.get()
                && weak
                    .upgrade()
                    .is_some_and(|window| window.window().is_visible())
                && {
                    let editor = editor.borrow();
                    editor
                        .page_names()
                        .get(editor.page())
                        .is_some_and(|name| *name == "tag sort")
                }
        }
    });
    window.on_tag_namespace_clicked({
        let valid = valid.clone();
        let queue = queue.clone();
        let child = child.clone();
        let show = show.clone();
        move |index, ctrl, shift| {
            if valid()
                && child.borrow().is_none()
                && let Ok(index) = usize::try_from(index)
            {
                queue.borrow_mut().0.click(index, ctrl, shift);
                show();
            }
        }
    });
    window.on_tag_namespace_activated({
        let weak = window.as_weak();
        move |index| {
            if let Some(window) = weak.upgrade() {
                window.invoke_tag_namespace_clicked(index, false, false);
                window.invoke_tag_namespace_action("edit".into());
            }
        }
    });
    window.on_tag_namespace_action({
        let valid = valid.clone();
        let queue = queue.clone();
        let editor = editor.clone();
        let child = child.clone();
        let show = show.clone();
        let weak = window.as_weak();
        let retire = retire.clone();
        move |action| {
            if !valid() || child.borrow().is_some() {
                return;
            }
            if action == "up" || action == "down" {
                queue.borrow_mut().0.move_selected(action == "down");
                editor
                    .borrow_mut()
                    .set_tag_namespace_order(queue.borrow().0.values());
                show();
                return;
            }
            let editing = if action == "add" {
                Some((None, "namespace".to_owned()))
            } else if action == "edit" {
                queue
                    .borrow()
                    .0
                    .editing()
                    .map(|(id, text)| (Some(id), text.to_owned()))
            } else {
                None
            };
            let question = (action == "delete")
                .then(|| queue.borrow().0.removal_question())
                .flatten();
            if editing.is_none() && question.is_none() {
                return;
            }
            let dialog = match crate::app_title::new::<crate::SessionDialog>() {
                Ok(dialog) => dialog,
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_tag_namespace_error(error.to_string().into());
                    }
                    return;
                }
            };
            dialog.set_asking_name(editing.is_some());
            dialog.set_name_ok_label("apply".into());
            dialog.set_window_title(
                if editing.is_some() {
                    "Enter Text"
                } else {
                    "Are you sure?"
                }
                .into(),
            );
            dialog.set_message(
                question
                    .as_deref()
                    .unwrap_or(tag_namespace_order::MESSAGE)
                    .into(),
            );
            if let Some((_, text)) = &editing {
                dialog.set_text(text.as_str().into());
            }
            let alive = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let alive = alive.clone();
                let child = Rc::downgrade(&child);
                let weak = dialog.as_weak();
                let show = show.clone();
                let retire = Rc::downgrade(&retire);
                move || {
                    if !alive.replace(false) {
                        return;
                    }
                    if let Some(dialog) = weak.upgrade() {
                        let _ = dialog.hide();
                        if let Some(child) = child.upgrade() {
                            let owns = child.borrow().as_ref().is_some_and(|current| {
                                std::ptr::eq(current.window(), dialog.window())
                            });
                            if owns {
                                child.borrow_mut().take();
                            }
                        }
                    }
                    if let Some(retire) = retire.upgrade() {
                        retire.borrow_mut().take();
                    }
                    show();
                }
            });
            let child_valid: Rc<dyn Fn() -> bool> = Rc::new({
                let valid = valid.clone();
                let alive = alive.clone();
                let child = Rc::downgrade(&child);
                let weak = dialog.as_weak();
                move || {
                    valid()
                        && alive.get()
                        && weak.upgrade().is_some_and(|dialog| {
                            dialog.window().is_visible()
                                && child.upgrade().is_some_and(|child| {
                                    child.borrow().as_ref().is_some_and(|current| {
                                        std::ptr::eq(current.window(), dialog.window())
                                    })
                                })
                        })
                }
            });
            dialog.on_name_entered({
                let valid = child_valid.clone();
                let queue = queue.clone();
                let editor = editor.clone();
                let close = close.clone();
                move |text| {
                    if !valid() {
                        return;
                    }
                    if let Some((key, _)) = &editing {
                        if let Some(key) = key {
                            queue.borrow_mut().0.replace(*key, text.to_string());
                        } else {
                            queue.borrow_mut().0.add(text.to_string());
                        }
                        editor
                            .borrow_mut()
                            .set_tag_namespace_order(queue.borrow().0.values());
                        close();
                    }
                }
            });
            dialog.on_answered({
                let valid = child_valid.clone();
                let queue = queue.clone();
                let editor = editor.clone();
                let close = close.clone();
                move |yes| {
                    if !valid() {
                        return;
                    }
                    if question.is_some() {
                        if yes {
                            queue.borrow_mut().0.remove_selected();
                            editor
                                .borrow_mut()
                                .set_tag_namespace_order(queue.borrow().0.values());
                        }
                        close();
                    }
                }
            });
            dialog.on_cancelled({
                let valid = child_valid.clone();
                let close = close.clone();
                move || {
                    if valid() {
                        close();
                    }
                }
            });
            dialog.window().on_close_requested({
                let close = close.clone();
                move || {
                    if child_valid() {
                        close();
                        slint::CloseRequestResponse::HideWindow
                    } else {
                        slint::CloseRequestResponse::KeepWindowShown
                    }
                }
            });
            *retire.borrow_mut() = Some(close.clone());
            *child.borrow_mut() = Some(dialog.clone_strong());
            LAST.with(|last| *last.borrow_mut() = Some(dialog.as_weak()));
            if let Err(error) = dialog.show() {
                close();
                if let Some(window) = weak.upgrade() {
                    window.set_tag_namespace_error(error.to_string().into());
                }
            }
            show();
        }
    });
    show();
    Binding {
        show,
        cancel,
        has_open: Rc::new({
            let child = Rc::downgrade(&child);
            move || {
                child
                    .upgrade()
                    .is_some_and(|child| child.borrow().is_some())
            }
        }),
    }
}
