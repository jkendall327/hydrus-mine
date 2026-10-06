//! Database > set a password (hydrus-gui-model's `set_password`): one
//! dialog shown in turn as the reference's text entries, its clear question
//! and its mismatch warning, saving the lock when the flow says so.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::set_password::{CLEAR, SetPassword, Step};
use hydrus_store::Store;
use slint::ComponentHandle;

use crate::SessionDialog;

pub type Slot = Rc<RefCell<Option<SessionDialog>>>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Text,
    Clear,
    Notice,
    Done,
}

/// Open the flow (unless it is open).
pub fn open(store: &Arc<Store>, slot: &Slot) -> Result<(), String> {
    if slot
        .borrow()
        .as_ref()
        .is_some_and(|w| w.window().is_visible())
    {
        return Ok(());
    }
    // A retained handle must not revive an earlier hidden dialog's callbacks.
    if let Some(previous) = slot.borrow_mut().take() {
        previous.invoke_force_close();
    }
    let window = SessionDialog::new().map_err(|e| e.to_string())?;
    let active = Rc::new(Cell::new(true));
    let mode = Rc::new(Cell::new(Mode::Text));
    let admitted = Rc::new({
        let active = active.clone();
        let slot = Rc::downgrade(slot);
        let weak = window.as_weak();
        move || {
            let valid = active.get()
                && weak.upgrade().is_some_and(|window| {
                    window.window().is_visible()
                        && slot.upgrade().is_some_and(|slot| {
                            slot.borrow().as_ref().is_some_and(|current| {
                                std::ptr::eq(current.window(), window.window())
                            })
                        })
                });
            if !valid {
                active.set(false);
            }
            valid
        }
    });
    let close = Rc::new({
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            active.set(false);
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        }
    });
    let flow = Rc::new(RefCell::new(SetPassword::default()));
    let show = Rc::new({
        let active = active.clone();
        let mode = mode.clone();
        let close = close.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move |step: Step| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            match step {
                Step::Ask { message, .. } => {
                    mode.set(Mode::Text);
                    window.set_window_title("Enter Text".into());
                    window.set_message(message.into());
                    window.set_text("".into());
                    window.set_asking_name(true);
                    window.set_notice_only(false);
                }
                Step::ConfirmClear => {
                    mode.set(Mode::Clear);
                    window.set_window_title("Are you sure?".into());
                    window.set_message(CLEAR.into());
                    window.set_asking_name(false);
                    window.set_notice_only(false);
                }
                Step::Problem { title, message } => {
                    mode.set(Mode::Notice);
                    window.set_window_title(title.into());
                    window.set_message(message.into());
                    window.set_asking_name(false);
                    window.set_notice_only(true);
                }
                Step::Save(lock) => {
                    mode.set(Mode::Done);
                    active.set(false);
                    if let Err(error) =
                        store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &lock))
                    {
                        eprintln!("could not save the password: {error}");
                    }
                    close();
                }
                Step::Done => {
                    mode.set(Mode::Done);
                    close();
                }
            }
        }
    });
    window.on_name_entered({
        let admitted = admitted.clone();
        let mode = mode.clone();
        let flow = flow.clone();
        let show = show.clone();
        move |text| {
            if !admitted() || mode.get() != Mode::Text {
                return;
            }
            let step = flow.borrow_mut().entered(&text);
            show(step);
        }
    });
    window.on_answered({
        let admitted = admitted.clone();
        let mode = mode.clone();
        let flow = flow.clone();
        let show = show.clone();
        move |yes| {
            if !admitted() || mode.get() != Mode::Clear {
                return;
            }
            let step = flow.borrow_mut().answered(yes);
            show(step);
        }
    });
    window.on_cancelled({
        let close = close.clone();
        move || close()
    });
    window.on_force_close({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    let first = flow.borrow_mut().start();
    show(first);
    window.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some(window);
    Ok(())
}
