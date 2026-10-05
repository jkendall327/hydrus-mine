//! Staged Options namespace rows and explicitly owned add/delete questions.
use crate::{NamespaceColourRow, OptionsWindow, SessionDialog};
use hydrus_gui_model::{namespace_colours, options::Editor};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Options' owned namespace Enter Text or deletion question.
pub type Slot = Rc<RefCell<Option<SessionDialog>>>;
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
    let list = Rc::new(RefCell::new(namespace_colours::Editor::new(
        editor.borrow().edited_namespace_colours(),
    )));
    let child = child.clone();
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let list = list.clone();
        let child = Rc::downgrade(&child);
        move || {
            if let Some(window) = weak.upgrade() {
                let list = list.borrow();
                window.set_namespace_colour_rows(ModelRc::new(VecModel::from(
                    list.rows()
                        .into_iter()
                        .map(|row| NamespaceColourRow {
                            label: row.label.into(),
                            colour: slint::Color::from_rgb_u8(row.rgb[0], row.rgb[1], row.rgb[2]),
                            selected: row.selected,
                        })
                        .collect::<Vec<_>>(),
                )));
                window.set_namespace_colour_can_delete(list.removal_question().is_some());
                window.set_namespace_colour_child_open(
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
            let dialog = child.borrow().as_ref().map(ComponentHandle::clone_strong);
            if let Some(dialog) = dialog {
                dialog.invoke_cancelled();
            }
        }
    });
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            active.get()
                && weak
                    .upgrade()
                    .is_some_and(|window| window.window().is_visible())
        }
    });
    window.on_namespace_colour_clicked({
        let list = list.clone();
        let valid = valid.clone();
        let show = show.clone();
        let child = child.clone();
        move |index, ctrl, shift| {
            if valid()
                && child.borrow().is_none()
                && let Ok(index) = usize::try_from(index)
            {
                list.borrow_mut().click(index, ctrl, shift);
                show();
            }
        }
    });
    window.on_namespace_colour_action({
        let weak = window.as_weak();
        let list = list.clone();
        let editor = editor.clone();
        let valid = valid.clone();
        let child = child.clone();
        let show = show.clone();
        move |action| {
            if !valid() || child.borrow().is_some() {
                return;
            }
            let adding = action == "add";
            let question = (action == "delete")
                .then(|| list.borrow().removal_question())
                .flatten();
            if !adding && question.is_none() {
                return;
            }
            let Ok(dialog) = SessionDialog::new() else {
                return;
            };
            dialog.set_message(question.unwrap_or("Enter the namespace.").into());
            dialog.set_asking_name(adding);
            let child_active = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let child = Rc::downgrade(&child);
                let weak = dialog.as_weak();
                let child_active = child_active.clone();
                let show = show.clone();
                move || {
                    if !child_active.replace(false) {
                        return;
                    }
                    if let Some(dialog) = weak.upgrade() {
                        let _ = dialog.hide();
                    }
                    if let Some(child) = child.upgrade() {
                        child.borrow_mut().take();
                    }
                    show();
                }
            });
            dialog.on_name_entered({
                let list = list.clone();
                let editor = editor.clone();
                let valid = valid.clone();
                let child_active = child_active.clone();
                let close = close.clone();
                let weak = weak.clone();
                move |text| {
                    if !adding || !valid() || !child_active.get() {
                        return;
                    }
                    let result = list.borrow_mut().add_random(text.as_str());
                    if let Err(warning) = result {
                        if let Some(window) = weak.upgrade() {
                            window.set_error(warning.into());
                        }
                    } else {
                        editor
                            .borrow_mut()
                            .set_namespace_colours(list.borrow().values());
                        if let Some(window) = weak.upgrade() {
                            window.set_error("".into());
                        }
                    }
                    close();
                }
            });
            dialog.on_answered({
                let list = list.clone();
                let editor = editor.clone();
                let valid = valid.clone();
                let child_active = child_active.clone();
                let close = close.clone();
                move |yes| {
                    if adding || !valid() || !child_active.get() {
                        return;
                    }
                    if yes {
                        list.borrow_mut().remove_selected();
                        editor
                            .borrow_mut()
                            .set_namespace_colours(list.borrow().values());
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
            if let Err(error) = dialog.show() {
                dialog.invoke_cancelled();
                if let Some(window) = weak.upgrade() {
                    window.set_error(error.to_string().into());
                }
            }
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
