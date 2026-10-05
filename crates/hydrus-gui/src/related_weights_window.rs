//! Options-owned namespace questions and weight drafts, without a global registry.
use crate::RelatedWeightsWindow;
use hydrus_gui_model::related_weights::Editor;
use hydrus_store::related_tags::Weights;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Slot = Rc<RefCell<Option<RelatedWeightsWindow>>>;
pub fn cancel(slot: &Slot) {
    let window = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("cancel".into());
    }
}
#[derive(Debug)]
enum Question {
    Namespace,
    Add(String),
    Edit(usize),
}
pub fn open(
    slot: &Slot,
    weights: &Weights,
    parent: Rc<Cell<bool>>,
    accepted: Rc<dyn Fn(Weights)>,
) -> Result<RelatedWeightsWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        window.show()?;
        return Ok(window.clone_strong());
    }
    let window = RelatedWeightsWindow::new()?;
    let active = Rc::new(Cell::new(true));
    let editor = Rc::new(RefCell::new(Editor::new(weights.clone())));
    let question: Rc<RefCell<Option<Question>>> = Rc::default();
    let refresh = Rc::new({
        let weak = window.as_weak();
        let editor = editor.clone();
        let question = question.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let editor = editor.borrow();
            window.set_rows(ModelRc::new(VecModel::from(
                editor
                    .rows()
                    .iter()
                    .enumerate()
                    .map(|(i, (slice, weight))| crate::TableRow {
                        cells: ModelRc::new(VecModel::from(vec![
                            SharedString::from(Editor::pretty(slice)),
                            SharedString::from(format!(
                                "{}%",
                                hydrus_core::numbers::human_int(u64::from(*weight))
                            )),
                        ])),
                        selected: editor.selected.is_selected(i),
                    })
                    .collect::<Vec<_>>(),
            )));
            window.set_result(editor.result);
            window.set_sort_column(i32::try_from(editor.sort_column()).unwrap_or_default());
            window.set_ascending(editor.ascending());
            window.set_can_delete(editor.can_delete());
            window.set_can_edit(editor.selection().len() == 1);
            match question.borrow().as_ref() {
                None => {
                    window.set_phase(0);
                }
                Some(Question::Namespace) => {
                    window.set_phase(1);
                    window.set_question("enter namespace".into());
                }
                Some(Question::Add(_)) => {
                    window.set_phase(2);
                    window.set_question("set weight".into());
                }
                Some(Question::Edit(_)) => {
                    window.set_phase(2);
                    window.set_question("edit weight".into());
                }
            }
        }
    });
    window.on_choose({
        let active = active.clone();
        let parent = parent.clone();
        let editor = editor.clone();
        let question = question.clone();
        let refresh = refresh.clone();
        move |result| {
            if active.get() && parent.get() && question.borrow().is_none() {
                editor.borrow_mut().choose(result);
                refresh();
            }
        }
    });
    window.on_sort({
        let active = active.clone();
        let parent = parent.clone();
        let editor = editor.clone();
        let question = question.clone();
        let refresh = refresh.clone();
        move |column, ascending| {
            if active.get()
                && parent.get()
                && question.borrow().is_none()
                && let Ok(column) = usize::try_from(column)
            {
                editor.borrow_mut().sort_by(column, ascending);
                refresh();
            }
        }
    });
    window.on_clicked({
        let active = active.clone();
        let parent = parent.clone();
        let editor = editor.clone();
        let question = question.clone();
        let refresh = refresh.clone();
        move |i, c, s| {
            if active.get()
                && parent.get()
                && question.borrow().is_none()
                && let Ok(i) = usize::try_from(i)
            {
                editor.borrow_mut().click(i, c, s);
                refresh();
            }
        }
    });
    let close = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        let question = question.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            question.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_action({
        let active = active.clone();
        let parent = parent.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        let question = question.clone();
        let refresh = refresh.clone();
        let close = close.clone();
        move |action| {
            if action == "cancel" {
                close();
                return;
            }
            if !active.get() || !parent.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.set_error("".into());
            if question.borrow().is_some() {
                match action.as_str() {
                    "cancel-question" => {
                        question.borrow_mut().take();
                    }
                    "accept-question" => {
                        let q = question.borrow_mut().take();
                        match q {
                            Some(Question::Namespace) => {
                                match editor.borrow().namespace(window.get_namespace().as_str()) {
                                    Ok(slice) => {
                                        window.set_weight(100);
                                        *question.borrow_mut() = Some(Question::Add(slice));
                                    }
                                    Err(error) => {
                                        window.set_error(error.into());
                                    }
                                }
                            }
                            Some(Question::Add(slice)) => {
                                if let Ok(weight) = u16::try_from(window.get_weight())
                                    && weight <= 10_000
                                {
                                    editor.borrow_mut().add(slice, weight);
                                }
                            }
                            Some(Question::Edit(index)) => {
                                if let Ok(weight) = u16::try_from(window.get_weight())
                                    && weight <= 10_000
                                {
                                    editor.borrow_mut().edit(index, weight);
                                }
                            }
                            None => {}
                        }
                    }
                    _ => return,
                }
            } else {
                match action.as_str() {
                    "add" => {
                        window.set_namespace("".into());
                        *question.borrow_mut() = Some(Question::Namespace);
                    }
                    "edit" => {
                        if let Some(index) = editor.borrow().selection().first().copied() {
                            window.set_weight(i32::from(editor.borrow().rows()[index].1));
                            *question.borrow_mut() = Some(Question::Edit(index));
                        }
                    }
                    "delete" => {
                        editor.borrow_mut().delete();
                    }
                    "apply" => {
                        accepted(editor.borrow().weights.clone());
                        close();
                        return;
                    }
                    _ => return,
                }
            }
            refresh();
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    refresh();
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
