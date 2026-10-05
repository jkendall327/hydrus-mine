//! A transfer question belongs to its captured thumbnail owner, not the current selection.
use crate::LocalTransferWindow;
use hydrus_gui_model::local_transfer::Transfer;
use hydrus_store::Store;
use slint::ComponentHandle as _;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

pub type Slot = Rc<RefCell<Option<LocalTransferWindow>>>;
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}
/// Open a frozen question, or execute directly when its saved gate is disabled.
/// A successor invalidates all callbacks retained from its retired predecessor.
pub fn open(
    slot: &Slot,
    store: &Arc<Store>,
    transfer: Transfer,
    guard: Rc<dyn Fn() -> bool>,
    applied: Rc<dyn Fn()>,
) -> Result<Option<LocalTransferWindow>, String> {
    open_with_result(slot, store, transfer, guard, Rc::new(move |_| applied()))
}
/// Report only successfully migrated identities to the captured source page.
pub fn open_with_result(
    slot: &Slot,
    store: &Arc<Store>,
    transfer: Transfer,
    guard: Rc<dyn Fn() -> bool>,
    applied: Rc<dyn Fn(&[hydrus_core::HashId])>,
) -> Result<Option<LocalTransferWindow>, String> {
    cancel(slot);
    if !guard() {
        return Ok(None);
    }
    if !transfer.confirm {
        let files = transfer.apply(store).map_err(|e| e.to_string())?;
        applied(&files);
        return Ok(None);
    }
    let window = LocalTransferWindow::new().map_err(|e| e.to_string())?;
    window.set_question(transfer.question.as_str().into());
    let active = Rc::new(Cell::new(true));
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
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    window.on_answer({
        let close = close.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |yes| {
            if !active.get() {
                return;
            }
            if !yes
                || !guard()
                || weak
                    .upgrade()
                    .is_none_or(|window| !window.window().is_visible())
            {
                close();
                return;
            }
            match transfer.apply(&store) {
                Ok(files) => {
                    close();
                    applied(&files);
                }
                Err(error) => {
                    if let Some(window) = weak.upgrade() {
                        window.set_error(error.to_string().into());
                    }
                }
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    *slot.borrow_mut() = Some(window.clone_strong());
    if let Err(error) = window.show() {
        close();
        return Err(error.to_string());
    }
    Ok(Some(window))
}
