//! Tab naming uses the existing text-entry window, with a frozen page key so
//! switching notebooks while it is open cannot rename an unrelated page.
use crate::{ChangePages, MainWindow, Pages, SessionDialog};
use slint::ComponentHandle as _;
use std::cell::RefCell;
use std::rc::Rc;

type Slot = Rc<RefCell<Option<SessionDialog>>>;

/// Bind page naming and retain the child window until acceptance/cancellation.
pub(crate) fn bind(window: &MainWindow, pages: &Rc<RefCell<Pages>>, change: ChangePages) -> Slot {
    let slot: Slot = Rc::default();
    window.on_tab_rename_requested({
        let pages = Rc::clone(pages);
        let change = change.clone();
        let slot = slot.clone();
        move |depth, index| {
            let (Ok(depth), Ok(index)) = (usize::try_from(depth), usize::try_from(index)) else {
                return;
            };
            let Some((key, name)) = pages.borrow().tab_identity(depth, index) else {
                return;
            };
            open(&slot, "Enter the new name.", &name, key, change.clone());
        }
    });
    window.on_notebook_rename_requested({
        let slot = slot.clone();
        move |key| {
            let Some(key) = hydrus_core::pages::PageKey::from_hex(&key) else {
                return;
            };
            open(
                &slot,
                "Enter the name for the new page of pages.",
                "pages",
                key,
                change.clone(),
            );
        }
    });
    slot
}

fn open(
    slot: &Slot,
    message: &str,
    name: &str,
    key: hydrus_core::pages::PageKey,
    change: ChangePages,
) {
    if let Some(previous) = slot.borrow_mut().take() {
        let _ = previous.hide();
    }
    let Ok(window) = crate::app_title::new::<crate::SessionDialog>() else {
        return;
    };
    window.set_asking_name(true);
    window.set_message(message.into());
    window.set_text(name.into());
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
    window.on_name_entered({
        let close = close.clone();
        move |name| {
            if name.is_empty() {
                return;
            }
            change(&|pages| {
                pages.rename_key(&key, &name);
                Ok(())
            });
            close();
        }
    });
    window.on_cancelled(close.clone());
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    if window.show().is_ok() {
        *slot.borrow_mut() = Some(window);
    }
}
