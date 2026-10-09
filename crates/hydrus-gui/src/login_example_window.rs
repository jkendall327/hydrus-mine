//! Example-domain entry preserves the reference's final-description cancellation.
use crate::LoginExampleDomainWindow;
use hydrus_gui_model::login_workflows::ExampleDraft;
use hydrus_parse::login::{Access, ExampleDomain};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
/// Owned example editor, canceled when its script closes.
pub type Slot = Rc<RefCell<Option<LoginExampleDomainWindow>>>;
/// Accept one staged example; persistence remains with the script list.
pub type Applied = Rc<dyn Fn(ExampleDomain) -> Result<(), String>>;
/// Close an unfinished example without accepting its fields.
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("cancel".into());
    }
}
/// Ask for domain, access and description in the same order as the reference.
pub fn open(
    value: Option<&ExampleDomain>,
    rows: &[ExampleDomain],
    index: Option<usize>,
    slot: &Slot,
    applied: Applied,
) -> Result<LoginExampleDomainWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = crate::app_title::new::<crate::LoginExampleDomainWindow>()?;
    let draft = Rc::new(RefCell::new(ExampleDraft::new(value)));
    let rows = rows.to_vec();
    window.set_domain(draft.borrow().domain.as_str().into());
    window.set_access(draft.borrow().access.code() as i32);
    let active = Rc::new(Cell::new(true));
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
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let active = active.clone();
        let close = close.clone();
        let draft = draft.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if action == "cancel" {
                close();
                return;
            }
            window.set_error("".into());
            match (window.get_stage(), action.as_str()) {
                (0, "next") => {
                    draft.borrow_mut().domain = window.get_domain().to_string();
                    match draft.borrow().validate_domain(&rows, index) {
                        Ok(()) => window.set_stage(1),
                        Err(error) => window.set_error(error.into()),
                    }
                }
                (1, "next") => {
                    let Some(access) = Access::from_code(i64::from(window.get_access())) else {
                        return;
                    };
                    draft.borrow_mut().select_access(access);
                    window.set_description(draft.borrow().description.as_str().into());
                    window.set_stage(2);
                }
                (2, "save" | "keep-description") => {
                    let description = window.get_description().to_string();
                    match draft
                        .borrow()
                        .value((action == "save").then_some(description.as_str()))
                        .and_then(|value| applied(value))
                    {
                        Ok(()) => close(),
                        Err(error) => window.set_error(error.into()),
                    }
                }
                _ => {}
            }
        }
    });
    window.window().on_close_requested({
        let weak = window.as_weak();
        move || {
            if let Some(window) = weak.upgrade()
                && window.get_stage() == 2
            {
                window.invoke_action("keep-description".into());
                if active.get() {
                    return slint::CloseRequestResponse::KeepWindowShown;
                }
            } else {
                close();
            }
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
