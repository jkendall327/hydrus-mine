//! A parent-owned detached tag-summary editor; every close invalidates callbacks.
use crate::{TableRow, TagBannerWindow};
use hydrus_core::{
    tag_presentation::TagPresentation,
    tag_summary::{NamespaceInfo, TagSummaryGenerator},
};
use hydrus_gui_model::tag_banner::Editor;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Slot = Rc<RefCell<Option<TagBannerWindow>>>;
pub type Applied = Rc<dyn Fn(TagSummaryGenerator)>;
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("cancel".into());
    }
}
fn show(window: &TagBannerWindow, editor: &Editor) {
    let rows = editor.rows();
    window.set_selected(rows.iter().any(|row| row.selected));
    window.set_rows(ModelRc::new(VecModel::from(
        rows.into_iter()
            .map(|row| TableRow {
                cells: ModelRc::new(VecModel::from(vec![row.label().into()])),
                selected: row.selected,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_preview(editor.preview().into());
    window.set_background_channels(ModelRc::new(VecModel::from(
        editor
            .background
            .into_iter()
            .map(i32::from)
            .collect::<Vec<_>>(),
    )));
    window.set_text_channels(ModelRc::new(VecModel::from(
        editor.text.into_iter().map(i32::from).collect::<Vec<_>>(),
    )));
    let colour = |[r, g, b, a]: [u8; 4]| slint::Color::from_argb_u8(a, r, g, b);
    window.set_background_colour(colour(editor.background));
    window.set_text_colour(colour(editor.text));
}
#[derive(Debug)]
struct Prompt {
    target: Option<u64>,
    info: NamespaceInfo,
    stage: usize,
}
fn prompt(window: &TagBannerWindow, pending: &Prompt) {
    let (message, text) = match pending.stage {
        0 => ("Edit namespace.", &pending.info.namespace),
        1 => ("Edit prefix.", &pending.info.prefix),
        _ => ("Edit separator.", &pending.info.separator),
    };
    window.set_message(message.into());
    window.set_prompt_text(text.as_str().into());
    window.set_asking(true);
}
pub fn open(
    value: &TagSummaryGenerator,
    presentation: TagPresentation,
    slot: &Slot,
    applied: Applied,
) -> Result<TagBannerWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = TagBannerWindow::new()?;
    let editor = Rc::new(RefCell::new(Editor::new(value, presentation)));
    let active = Rc::new(Cell::new(true));
    let pending: Rc<RefCell<Option<Prompt>>> = Rc::default();
    let removal: Rc<RefCell<Vec<u64>>> = Rc::default();
    window.set_showing(value.show);
    window.set_separator(value.separator.clone().into());
    window.set_examples(value.example_tags.join("\n").into());
    show(&window, &editor.borrow());
    let close: Rc<dyn Fn()> = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
                if let Some(slot) = slot.upgrade() {
                    let owns_slot = slot
                        .borrow()
                        .as_ref()
                        .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
                    if owns_slot {
                        slot.borrow_mut().take();
                    }
                }
            }
        }
    });
    window.on_changed({
        let active = active.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        move || {
            if !active.get() {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let mut editor = editor.borrow_mut();
                editor.show = window.get_showing();
                editor.separator = window.get_separator().to_string();
                editor.examples = window.get_examples().to_string();
                show(&window, &editor);
            }
        }
    });
    window.on_colour_edited({
        let active = active.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        move |text, index, value| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let mut editor = editor.borrow_mut();
            let channels = if text {
                &mut editor.text
            } else {
                &mut editor.background
            };
            if let Some(channel) = channels.get_mut(index) {
                *channel = value.clamp(0, 255) as u8;
            }
            show(&window, &editor);
        }
    });
    window.on_row_clicked({
        let active = active.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        move |index, ctrl, shift| {
            if !active.get() {
                return;
            }
            if let (Some(window), Ok(index)) = (weak.upgrade(), usize::try_from(index)) {
                editor.borrow_mut().click(index, ctrl, shift);
                show(&window, &editor.borrow());
            }
        }
    });
    window.on_action({
        let active = active.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        let close = close.clone();
        let pending = pending.clone();
        let removal = removal.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            match action.as_str() {
                "cancel" => close(),
                "apply" if !window.get_asking() && !window.get_confirming() => {
                    let value = editor.borrow().value();
                    applied(value);
                    close();
                }
                "add" | "edit" if !window.get_asking() && !window.get_confirming() => {
                    let selected = if action == "edit" {
                        editor.borrow().first_selected()
                    } else {
                        None
                    };
                    if action == "edit" && selected.is_none() {
                        return;
                    }
                    let (target, info) = selected.map_or(
                        (
                            None,
                            NamespaceInfo {
                                namespace: String::new(),
                                prefix: String::new(),
                                separator: ", ".into(),
                            },
                        ),
                        |(id, info)| (Some(id), info),
                    );
                    let next = Prompt {
                        target,
                        info,
                        stage: 0,
                    };
                    prompt(&window, &next);
                    *pending.borrow_mut() = Some(next);
                }
                "up" | "down" if !window.get_asking() && !window.get_confirming() => {
                    editor.borrow_mut().move_selected(action == "down");
                }
                "delete" if !window.get_asking() && !window.get_confirming() => {
                    let selected = editor.borrow().selected();
                    if !selected.is_empty() {
                        window.set_message(format!("Remove {} selected?", selected.len()).into());
                        *removal.borrow_mut() = selected;
                        window.set_confirming(true);
                    }
                }
                _ => {}
            }
            show(&window, &editor.borrow());
        }
    });
    window.on_prompt_accepted({
        let active = active.clone();
        let weak = window.as_weak();
        let pending = pending.clone();
        let editor = editor.clone();
        move || {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let Some(mut next) = pending.borrow_mut().take() else {
                return;
            };
            let text = window.get_prompt_text().to_string();
            match next.stage {
                0 => next.info.namespace = text,
                1 => next.info.prefix = text,
                _ => next.info.separator = text,
            }
            next.stage += 1;
            if next.stage == 3 {
                editor.borrow_mut().put(next.target, next.info);
                window.set_asking(false);
                show(&window, &editor.borrow());
            } else {
                prompt(&window, &next);
                *pending.borrow_mut() = Some(next);
            }
        }
    });
    window.on_prompt_cancelled({
        let active = active.clone();
        let weak = window.as_weak();
        let pending = pending.clone();
        move || {
            if !active.get() {
                return;
            }
            pending.borrow_mut().take();
            if let Some(window) = weak.upgrade() {
                window.set_asking(false);
            }
        }
    });
    window.on_answer({
        let active = active.clone();
        let weak = window.as_weak();
        let editor = editor.clone();
        move |yes| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let ids = std::mem::take(&mut *removal.borrow_mut());
            if yes {
                editor.borrow_mut().delete(&ids);
            }
            window.set_confirming(false);
            show(&window, &editor.borrow());
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
