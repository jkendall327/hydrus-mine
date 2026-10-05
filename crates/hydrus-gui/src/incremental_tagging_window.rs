//! Owned Incremental Tagging child; mappings are accepted by its private parent draft.
use crate::IncrementalTaggingWindow;
use hydrus_core::{HashId, Tag};
use hydrus_gui_model::incremental_tagging::IncrementalTagging;
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type Slot = Rc<RefCell<Option<IncrementalTaggingWindow>>>;
pub type Applied = Rc<dyn Fn(Vec<(HashId, Tag)>) -> Result<(), String>>;
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}
pub fn open(
    model: IncrementalTagging,
    slot: &Slot,
    applied: Applied,
    closed: Rc<dyn Fn()>,
) -> Result<IncrementalTaggingWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = IncrementalTaggingWindow::new()?;
    let model = Rc::new(RefCell::new(model));
    let active = Rc::new(Cell::new(true));
    crate::gui_colours::bind(
        window.global::<crate::Theme<'_>>(),
        model.borrow().store(),
        active.clone(),
    );
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let model = model.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let model = model.borrow();
                window.set_namespace(model.namespace.as_str().into());
                window.set_prefix(model.prefix.as_str().into());
                window.set_suffix(model.suffix.as_str().into());
                window.set_start(model.start);
                window.set_step(model.step);
                window.set_reverse(model.reverse);
                window.set_summary(model.summary().into());
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            closed();
        }
    });
    window.on_text_edited({
        let model = model.clone();
        let active = active.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move |field, text| {
            if !active.get() {
                return;
            }
            let result = model
                .borrow_mut()
                .set_text(usize::try_from(field).unwrap_or(usize::MAX), &text);
            if let Some(window) = weak.upgrade() {
                window.set_error(result.err().unwrap_or_default().into());
            }
            refresh();
        }
    });
    window.on_numbers_edited({
        let model = model.clone();
        let active = active.clone();
        let weak = window.as_weak();
        let refresh = refresh.clone();
        move |start, step, reverse| {
            if !active.get() {
                return;
            }
            let result = model.borrow_mut().set_numbers(start, step, reverse);
            if let Some(window) = weak.upgrade() {
                window.set_error(result.err().unwrap_or_default().into());
            }
            refresh();
        }
    });
    window.on_apply({
        let active = active.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !active.get() {
                return;
            }
            let result = model
                .borrow()
                .cleaned_pairs()
                .and_then(|pairs| applied(pairs));
            match result {
                Ok(()) => close(),
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error.into());
                    }
                }
            }
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
    refresh();
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
