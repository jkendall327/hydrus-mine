//! Staged Options namespace rows and explicitly owned questions and warning notices.
use crate::{NamespaceColourRow, OptionsWindow, SessionDialog};
use hydrus_gui_model::{namespace_colours, options::Editor};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Options' owned namespace Enter Text, deletion question or warning notice.
pub type Slot = Rc<RefCell<Option<SessionDialog>>>;
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
// Warnings follow the completed Enter Text question in the same private owner slot.
fn warning_notice(child: &Slot, show: &Rc<dyn Fn()>, message: &str) -> Result<(), String> {
    let notice =
        crate::app_title::new::<crate::SessionDialog>().map_err(|error| error.to_string())?;
    notice.set_window_title("Warning".into());
    notice.set_message(message.into());
    notice.set_notice_only(true);
    notice.set_notice_ok_label("OK".into());
    let alive = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let child = Rc::downgrade(child);
        let weak = notice.as_weak();
        let show = show.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            if let Some(notice) = weak.upgrade() {
                let _ = notice.hide();
            }
            if let Some(child) = child.upgrade() {
                child.borrow_mut().take();
            }
            show();
        }
    });
    notice.on_cancelled({
        let close = close.clone();
        move || close()
    });
    notice.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    *child.borrow_mut() = Some(notice.clone_strong());
    if let Err(error) = notice.show() {
        notice.invoke_cancelled();
        return Err(error.to_string());
    }
    show();
    Ok(())
}

thread_local! {
    // (the namespace colour picker shown, kept until answered)
    static PICKER: RefCell<Option<crate::GuiColourPickerWindow>> = const { RefCell::new(None) };
}

/// The colour picker the edit button last opened, if still shown.
pub fn last_picker() -> Option<crate::GuiColourPickerWindow> {
    PICKER.with(|p| p.borrow().as_ref().map(ComponentHandle::clone_strong))
}

/// Recolour the queued entries one after another (`_EditNamespaceColour`);
/// a cancelled picker keeps an entry's colour.
fn edit_next(
    mut queue: Vec<(Option<String>, [u8; 3])>,
    list: Rc<RefCell<namespace_colours::Editor>>,
    editor: Rc<RefCell<Editor>>,
    show: Rc<dyn Fn()>,
    valid: Rc<dyn Fn() -> bool>,
) {
    if queue.is_empty() || !valid() {
        return;
    }
    let (namespace, [red, green, blue]) = queue.remove(0);
    let Ok(picker) = crate::app_title::new::<crate::GuiColourPickerWindow>() else {
        return;
    };
    picker.set_red(i32::from(red));
    picker.set_green(i32::from(green));
    picker.set_blue(i32::from(blue));
    let answered = Rc::new(Cell::new(false));
    let queue = Rc::new(RefCell::new(Some(queue)));
    let finish: Rc<dyn Fn(Option<[u8; 3]>)> = Rc::new({
        let weak = picker.as_weak();
        move |rgb: Option<[u8; 3]>| {
            if answered.replace(true) {
                return;
            }
            if let Some(picker) = weak.upgrade() {
                let _ = picker.hide();
            }
            if let Some(rgb) = rgb
                && valid()
            {
                list.borrow_mut().set_colour(namespace.as_deref(), rgb);
                editor
                    .borrow_mut()
                    .set_namespace_colours(list.borrow().values());
                show();
            }
            if let Some(rest) = queue.borrow_mut().take() {
                edit_next(
                    rest,
                    list.clone(),
                    editor.clone(),
                    show.clone(),
                    valid.clone(),
                );
            }
        }
    });
    picker.on_accepted({
        let finish = finish.clone();
        move |r, g, b| {
            let channel = |v: i32| u8::try_from(v.clamp(0, 255)).unwrap_or(0);
            finish(Some([channel(r), channel(g), channel(b)]));
        }
    });
    picker.on_cancelled({
        let finish = finish.clone();
        move || finish(None)
    });
    picker.window().on_close_requested(move || {
        finish(None);
        slint::CloseRequestResponse::HideWindow
    });
    if picker.show().is_ok() {
        PICKER.with(|p| *p.borrow_mut() = Some(picker));
    }
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
            if action == "edit" {
                let queue = list.borrow().selected();
                edit_next(
                    queue,
                    list.clone(),
                    editor.clone(),
                    show.clone(),
                    valid.clone(),
                );
                return;
            }
            let adding = action == "add";
            let question = (action == "delete")
                .then(|| list.borrow().removal_question())
                .flatten();
            if !adding && question.is_none() {
                return;
            }
            if let Some(window) = weak.upgrade() {
                window.set_namespace_colour_error("".into());
            }
            let dialog = match crate::app_title::new::<crate::SessionDialog>() {
                Ok(dialog) => dialog,
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_namespace_colour_error(error.to_string().into());
                    }
                    return;
                }
            };
            dialog.set_message(question.unwrap_or("Enter the namespace.").into());
            dialog.set_window_title(
                if adding {
                    "Enter Text"
                } else {
                    "Are you sure?"
                }
                .into(),
            );
            dialog.set_asking_name(adding);
            dialog.set_reject_blank_submission(adding);
            let entry_warning: Slot = Rc::new(RefCell::new(None));
            let child_active = Rc::new(Cell::new(true));
            let close: Rc<dyn Fn()> = Rc::new({
                let child = Rc::downgrade(&child);
                let weak = dialog.as_weak();
                let child_active = child_active.clone();
                let show = show.clone();
                let entry_warning = entry_warning.clone();
                move || {
                    if !child_active.replace(false) {
                        return;
                    }
                    let notice = entry_warning.borrow_mut().take();
                    if let Some(notice) = notice {
                        notice.invoke_cancelled();
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
            dialog.on_blank_submitted({
                let entry_warning = entry_warning.clone();
                let child_active = child_active.clone();
                let valid = valid.clone();
                let parent = weak.clone();
                let entry = dialog.as_weak();
                let sync: Rc<dyn Fn()> = Rc::new({
                    let entry = dialog.as_weak();
                    let entry_warning = Rc::downgrade(&entry_warning);
                    let show = show.clone();
                    move || {
                        if let Some(entry) = entry.upgrade() {
                            entry.set_child_open(
                                entry_warning
                                    .upgrade()
                                    .is_some_and(|slot| slot.borrow().is_some()),
                            );
                        }
                        show();
                    }
                });
                move || {
                    if !adding
                        || !valid()
                        || !child_active.get()
                        || entry_warning.borrow().is_some()
                        || entry
                            .upgrade()
                            .is_none_or(|entry| !entry.window().is_visible())
                    {
                        return;
                    }
                    if let Err(error) =
                        warning_notice(&entry_warning, &sync, "Cannot enter blank text here!")
                        && let Some(parent) = parent.upgrade()
                    {
                        parent.set_namespace_colour_error(error.into());
                    }
                }
            });
            dialog.on_name_entered({
                let list = list.clone();
                let editor = editor.clone();
                let valid = valid.clone();
                let child_active = child_active.clone();
                let close = close.clone();
                let weak = weak.clone();
                let child = Rc::downgrade(&child);
                let show = show.clone();
                let entry_warning = entry_warning.clone();
                move |text| {
                    if !adding
                        || !valid()
                        || !child_active.get()
                        || entry_warning.borrow().is_some()
                    {
                        return;
                    }
                    let result = list.borrow_mut().add_random(text.as_str());
                    close();
                    if let Err(warning) = result {
                        if let Some(child) = child.upgrade()
                            && let Err(error) = warning_notice(&child, &show, warning)
                            && let Some(window) = weak.upgrade()
                        {
                            window.set_namespace_colour_error(format!("{warning}\n{error}").into());
                        }
                    } else {
                        editor
                            .borrow_mut()
                            .set_namespace_colours(list.borrow().values());
                    }
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
                    window.set_namespace_colour_error(error.to_string().into());
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
