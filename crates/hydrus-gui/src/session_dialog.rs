//! The session saving dialog (pages > sessions > save), driven by
//! [`session_saving::Saving`]: it asks a name or a question as the
//! reference does, step by step, and saves the open pages as the session
//! when it comes to that ([`Pages::save_session`]).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, SharedString};

use crate::session_saving::{NAME_MESSAGE, NAME_TITLE, Saving, Step};
use crate::{Pages, SessionDialog};

/// Save the open pages as the session `name` (asking first whether to
/// overwrite it), or as a new one (`None`, asking its name). The dialog
/// forgets itself from `slot` when done.
pub(crate) fn open(
    pages: &Rc<RefCell<Pages>>,
    name: Option<&str>,
    slot: &Rc<RefCell<Option<SessionDialog>>>,
) -> Result<SessionDialog, String> {
    let existing: Vec<String> = pages
        .borrow()
        .store()
        .read(hydrus_store::sessions::names)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let (saving, first) = match name {
        Some(name) => Saving::over(name),
        None => Saving::new_session(existing),
    };
    let saving = Rc::new(RefCell::new(saving));
    let window = SessionDialog::new().map_err(|e| e.to_string())?;
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    // (each step shown, or done)
    let step: Rc<dyn Fn(Step)> = Rc::new({
        let weak = window.as_weak();
        let pages = pages.clone();
        let close = close.clone();
        move |step| {
            let Some(window) = weak.upgrade() else { return };
            match step {
                Step::AskName { warning } => {
                    window.set_window_title(NAME_TITLE.into());
                    window.set_message(NAME_MESSAGE.into());
                    window.set_warning(warning.unwrap_or_default().into());
                    window.set_text(SharedString::new());
                    window.set_asking_name(true);
                }
                Step::Ask(question) => {
                    window.set_window_title(question.title.into());
                    window.set_message(question.message.into());
                    window.set_yes_label(question.yes.into());
                    window.set_no_label(question.no.into());
                    window.set_asking_name(false);
                }
                Step::Save(name) => {
                    let now = hydrus_core::time::TimestampMs::now().millis();
                    if let Err(e) = pages.borrow_mut().save_session_at_ms(&name, now) {
                        eprintln!("could not save the session: {e}");
                    }
                    close();
                }
                Step::Stop => close(),
            }
        }
    });
    window.on_name_entered({
        let saving = saving.clone();
        let step = step.clone();
        move |text| {
            let next = saving.borrow_mut().named(Some(text.as_str()));
            step(next);
        }
    });
    window.on_cancelled({
        let step = step.clone();
        move || step(Step::Stop)
    });
    window.on_answered({
        let saving = saving.clone();
        let step = step.clone();
        move |yes| {
            let next = saving.borrow_mut().answered(Some(yes));
            step(next);
        }
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    step(first);
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
