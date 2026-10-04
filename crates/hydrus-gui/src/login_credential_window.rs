//! Credential definition and entry editors, staged under their owning windows.
use crate::{LoginCredentialDefinitionWindow, LoginCredentialRow, LoginCredentialsWindow};
use hydrus_gui_model::login_workflows::CredentialsEditor;
use hydrus_parse::login::{CredentialDefinition, CredentialKind};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};
/// Credential definition draft owned by the login script window.
pub type DefinitionSlot = Rc<RefCell<Option<LoginCredentialDefinitionWindow>>>;
/// Credentials draft owned by a domain or test editor.
pub type CredentialsSlot = Rc<RefCell<Option<LoginCredentialsWindow>>>;
/// Accepted credential definition; failure keeps the child open.
pub type DefinitionApplied = Rc<dyn Fn(CredentialDefinition) -> Result<(), String>>;
/// Accepted credential values; failure keeps the child open.
pub type CredentialsApplied = Rc<dyn Fn(BTreeMap<String, String>) -> Result<(), String>>;

/// Cancel a credential definition, closing its unfinished matcher too.
pub fn cancel_definition(slot: &DefinitionSlot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("cancel".into());
    }
}
/// Cancel a credential entry without accepting or persisting its values.
pub fn cancel_credentials(slot: &CredentialsSlot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("cancel".into());
    }
}
/// Edit a name, hidden/normal presentation and shared permitted-input matcher.
pub fn open_definition(
    store: &Arc<Store>,
    value: &CredentialDefinition,
    slot: &DefinitionSlot,
    strings: &crate::string_processor_window::Slots,
    applied: DefinitionApplied,
) -> Result<LoginCredentialDefinitionWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = LoginCredentialDefinitionWindow::new()?;
    window.set_name(value.name.as_str().into());
    window.set_kind(i32::from(value.kind == CredentialKind::Hidden));
    window.set_matcher(value.string_match.describe(false, false).into());
    let state = Rc::new(RefCell::new(value.clone()));
    let active = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let strings = strings.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            strings.cancel_all();
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
        let state = state.clone();
        let strings = strings.clone();
        let active = active.clone();
        let store = store.clone();
        let close = close.clone();
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
            if strings.has_open() {
                return;
            }
            match action.as_str() {
                "matcher" => {
                    let matcher = state.borrow().string_match.clone();
                    crate::string_processor_window::open_match(
                        &store,
                        &matcher,
                        &strings,
                        Rc::new({
                            let weak = weak.clone();
                            let state = state.clone();
                            let active = active.clone();
                            move |matcher| {
                                if !active.get() {
                                    return;
                                }
                                if let Some(window) = weak.upgrade() {
                                    window.set_matcher(matcher.describe(false, false).into());
                                }
                                state.borrow_mut().string_match = matcher;
                            }
                        }),
                    );
                    if let Some(child) = strings.step.borrow().as_ref() {
                        window.set_child_open(true);
                        let weak = weak.clone();
                        child.on_closed(move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_child_open(false);
                            }
                        });
                    }
                }
                "apply" => {
                    let mut value = state.borrow().clone();
                    value.name = window.get_name().to_string();
                    value.kind = if window.get_kind() == 1 {
                        CredentialKind::Hidden
                    } else {
                        CredentialKind::Normal
                    };
                    match applied(value) {
                        Ok(()) => close(),
                        Err(error) => window.set_error(error.into()),
                    }
                }
                _ => {}
            }
        }
    });
    window.on_closed(|| {});
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
fn show_credentials(window: &LoginCredentialsWindow, editor: &CredentialsEditor) {
    let rows = editor
        .rows()
        .into_iter()
        .map(|row| LoginCredentialRow {
            name: row.definition.name.into(),
            value: row.value.into(),
            hidden: row.definition.kind == CredentialKind::Hidden,
            label: row.label.into(),
            valid: row.valid,
        })
        .collect::<Vec<_>>();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
}
/// Enter credentials with live validation and explicit advisory confirmation.
pub fn open_credentials(
    definitions: &[CredentialDefinition],
    credentials: &BTreeMap<String, String>,
    slot: &CredentialsSlot,
    applied: CredentialsApplied,
) -> Result<LoginCredentialsWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = LoginCredentialsWindow::new()?;
    let editor = Rc::new(RefCell::new(CredentialsEditor::new(
        definitions,
        credentials,
    )));
    show_credentials(&window, &editor.borrow());
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
    window.on_edited({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        move |index, value| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.get_question().is_empty() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                editor.borrow_mut().set(index, value.to_string());
                show_credentials(&window, &editor.borrow());
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let editor = editor.clone();
        let active = active.clone();
        let close = close.clone();
        move |action| {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            match action.as_str() {
                "cancel" => close(),
                "back" => window.set_question("".into()),
                "apply" if window.get_question().is_empty() => {
                    if let Some(question) = editor.borrow().warning() {
                        window.set_question(question.into());
                    } else {
                        match applied(editor.borrow().value()) {
                            Ok(()) => close(),
                            Err(error) => window.set_error(error.into()),
                        }
                    }
                }
                "confirm" if !window.get_question().is_empty() => {
                    match applied(editor.borrow().value()) {
                        Ok(()) => close(),
                        Err(error) => window.set_error(error.into()),
                    }
                }
                _ => {}
            }
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
