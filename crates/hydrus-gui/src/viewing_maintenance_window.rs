//! Owned database confirmations; acceptance uses the current saved rules.
use crate::SessionDialog;
use hydrus_gui_model::viewing_maintenance::Operation;
use hydrus_store::Store;
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

pub type Slot = Rc<RefCell<Option<SessionDialog>>>;

pub fn open(
    store: &Arc<Store>,
    slot: &Slot,
    operation: Operation,
    valid: Rc<dyn Fn() -> bool>,
) -> Result<SessionDialog, String> {
    if !valid() {
        return Err("The owning window is closed.".into());
    }
    let predecessor = slot.borrow().as_ref().map(ComponentHandle::clone_strong);
    if let Some(predecessor) = predecessor {
        predecessor.invoke_cancelled();
    }
    let window = SessionDialog::new().map_err(|error| error.to_string())?;
    window.set_window_title("Are you sure?".into());
    window.set_message(operation.question().into());
    window.set_yes_label("do it".into());
    window.set_no_label("forget it".into());
    let active = Rc::new(Cell::new(true));
    let accepted = Rc::new(Cell::new(false));
    let timer = Rc::new(RefCell::new(Some(Timer::default())));
    let close = Rc::new({
        let active = active.clone();
        let slot = slot.clone();
        let timer = timer.clone();
        let weak = window.as_weak();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(timer) = timer.borrow_mut().take() {
                timer.stop();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    });
    window.on_answered({
        let active = active.clone();
        let valid = valid.clone();
        let close = close.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move |yes| {
            if !active.get() {
                return;
            }
            if !valid() {
                close();
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.window().is_visible() || accepted.get() {
                return;
            }
            if !yes {
                close();
                return;
            }
            accepted.set(true);
            match operation.apply(&store) {
                Ok(()) => {
                    window.set_window_title("Information".into());
                    window.set_message(operation.completed().into());
                }
                Err(error) => {
                    window.set_window_title("Warning".into());
                    window.set_message(error.to_string().into());
                }
            }
            window.set_notice_only(true);
            window.set_notice_ok_label("OK".into());
        }
    });
    window.on_cancelled({
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
    if let Some(timer) = timer.borrow().as_ref() {
        timer.start(TimerMode::Repeated, Duration::from_millis(250), {
            let weak = window.as_weak();
            let close = close.clone();
            move || {
                if !valid() || weak.upgrade().is_none_or(|w| !w.window().is_visible()) {
                    close();
                }
            }
        });
    }
    if let Err(error) = window.show() {
        close();
        return Err(error.to_string());
    }
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
