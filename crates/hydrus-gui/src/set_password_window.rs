//! Database > set a password (hydrus-gui-model's `set_password`): one
//! dialog shown in turn as the reference's text entries, its clear question
//! and its mismatch warning, saving the lock when the flow says so.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_gui_model::set_password::{CLEAR, SetPassword, Step};
use hydrus_store::Store;
use slint::ComponentHandle;

use crate::SessionDialog;

pub type Slot = Rc<RefCell<Option<SessionDialog>>>;

/// Open the flow (unless it is open).
pub fn open(store: &Arc<Store>, slot: &Slot) -> Result<(), String> {
    if slot
        .borrow()
        .as_ref()
        .is_some_and(|w| w.window().is_visible())
    {
        return Ok(());
    }
    let window = SessionDialog::new().map_err(|e| e.to_string())?;
    let flow = Rc::new(RefCell::new(SetPassword::default()));
    let show = Rc::new({
        let weak = window.as_weak();
        let store = store.clone();
        move |step: Step| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            match step {
                Step::Ask { message, .. } => {
                    window.set_window_title("Enter Text".into());
                    window.set_message(message.into());
                    window.set_text("".into());
                    window.set_asking_name(true);
                    window.set_notice_only(false);
                }
                Step::ConfirmClear => {
                    window.set_window_title("Are you sure?".into());
                    window.set_message(CLEAR.into());
                    window.set_asking_name(false);
                    window.set_notice_only(false);
                }
                Step::Problem { title, message } => {
                    window.set_window_title(title.into());
                    window.set_message(message.into());
                    window.set_asking_name(false);
                    window.set_notice_only(true);
                }
                Step::Save(lock) => {
                    if let Err(error) =
                        store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &lock))
                    {
                        eprintln!("could not save the password: {error}");
                    }
                    let _ = window.hide();
                }
                Step::Done => {
                    let _ = window.hide();
                }
            }
        }
    });
    window.on_name_entered({
        let flow = flow.clone();
        let show = show.clone();
        move |text| {
            let step = flow.borrow_mut().entered(&text);
            show(step);
        }
    });
    window.on_answered({
        let flow = flow.clone();
        let show = show.clone();
        move |yes| {
            let step = flow.borrow_mut().answered(yes);
            show(step);
        }
    });
    window.on_cancelled({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
        }
    });
    let first = flow.borrow_mut().start();
    show(first);
    window.show().map_err(|e| e.to_string())?;
    *slot.borrow_mut() = Some(window);
    Ok(())
}
