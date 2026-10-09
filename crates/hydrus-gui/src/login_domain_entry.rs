//! Reference Add/change-script prompt chain, owned by the domain-manager draft.
use crate::{LoginDomainEntryWindow, TableRow};
use hydrus_gui_model::login_workflows::{DomainEntry, DomainEntryStage as Stage};
use hydrus_parse::login::{DomainLogin, LoginManager};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
/// Domain prompt and its independently owned credential child.
#[derive(Clone, Default)]
pub struct Slots {
    pub window: Rc<RefCell<Option<LoginDomainEntryWindow>>>,
    pub credentials: crate::login_credential_window::CredentialsSlot,
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginDomainEntrySlots")
            .field("open", &self.window.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl Slots {
    /// Parent cancellation discards even the final description prompt.
    pub fn cancel(&self) {
        let window = self
            .window
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = window {
            window.invoke_action("force-close".into());
        }
    }
}
/// Complete one staged domain; persistence remains with the manager's Apply.
pub type Applied = Rc<dyn Fn(String, DomainLogin) -> Result<(), String>>;
/// Reference duplicate/no-scripts warning remains visible on the owning manager.
pub type Report = Rc<dyn Fn(String)>;
struct Context {
    weak: slint::Weak<LoginDomainEntryWindow>,
    active: Rc<Cell<bool>>,
    draft: Rc<RefCell<DomainEntry>>,
    credentials: crate::login_credential_window::CredentialsSlot,
    close: Rc<dyn Fn()>,
    applied: Applied,
    report: Report,
}
fn rows(window: &LoginDomainEntryWindow, draft: &DomainEntry, selected: i32) {
    window.set_rows(ModelRc::new(VecModel::from(
        draft
            .choices()
            .into_iter()
            .enumerate()
            .map(|(i, row)| TableRow {
                cells: ModelRc::new(VecModel::from(vec![row.label.into()])),
                selected: i32::try_from(i).ok() == Some(selected),
            })
            .collect::<Vec<_>>(),
    )));
}
fn present(context: &Rc<Context>) {
    if !context.active.get() {
        return;
    }
    let Some(window) = context.weak.upgrade() else {
        return;
    };
    let stage = context.draft.borrow().stage;
    window.set_choosing(matches!(
        stage,
        Stage::Script | Stage::Example | Stage::Access
    ));
    window.set_activating(stage == Stage::Activate);
    window.set_message("".into());
    window.set_placeholder("".into());
    window.set_text("".into());
    match stage {
        Stage::Script | Stage::Example | Stage::Access => {
            window.set_window_title(
                match stage {
                    Stage::Script => "select the login script to use",
                    Stage::Example => "select the domain to use",
                    _ => "select what type of access the login gives to this domain",
                }
                .into(),
            );
            let draft = context.draft.borrow();
            let selected = draft
                .selected_choice()
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1);
            window.set_selected_index(selected);
            rows(&window, &draft, selected);
        }
        Stage::Domain | Stage::Description => {
            window.set_window_title("Enter Text".into());
            window.set_message(
                if stage == Stage::Domain {
                    "Enter the domain."
                } else {
                    "Edit the access description, if needed."
                }
                .into(),
            );
            if stage == Stage::Domain {
                window.set_placeholder("example.com".into());
            } else {
                window.set_text(context.draft.borrow().description.as_str().into());
            }
        }
        Stage::Credentials => {
            window.set_child_open(true);
            let (definitions, values) = {
                let draft = context.draft.borrow();
                (
                    draft.script().expect("chosen script").credentials.clone(),
                    draft.credentials.clone(),
                )
            };
            let applied: crate::login_credential_window::CredentialsApplied = Rc::new({
                let context = context.clone();
                move |values| {
                    if !context.active.get() {
                        return Err("The domain login editor has closed.".into());
                    }
                    context.draft.borrow_mut().set_credentials(values);
                    if let Some(window) = context.weak.upgrade() {
                        window.set_child_open(false);
                    }
                    present(&context);
                    Ok(())
                }
            });
            match crate::login_credential_window::open_credentials(
                &definitions,
                &values,
                &context.credentials,
                applied,
            ) {
                Ok(child) => {
                    let context = context.clone();
                    child.on_closed(move || {
                        if context.active.get()
                            && context.draft.borrow().stage == Stage::Credentials
                        {
                            context.draft.borrow_mut().cancel();
                            present(&context);
                        }
                    });
                }
                Err(error) => {
                    (context.report)(error.to_string());
                    (context.close)();
                }
            }
        }
        Stage::Activate => {
            window.set_window_title("Question".into());
            window.set_message("Activate this login script for this domain?".into());
        }
        Stage::Done => {
            let value = context.draft.borrow().value();
            if let Some((domain, login)) = value {
                match (context.applied)(domain, login) {
                    Ok(()) => (context.close)(),
                    Err(error) => window.set_error(error.into()),
                }
            }
        }
        Stage::Canceled => (context.close)(),
    }
}
/// Add a domain or change only the top selected domain's script, exactly as Qt.
pub fn open(
    manager: &LoginManager,
    editing: Option<&str>,
    slots: &Slots,
    applied: Applied,
    report: Report,
) -> Result<LoginDomainEntryWindow, String> {
    if let Some(window) = slots.window.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let draft = Rc::new(RefCell::new(DomainEntry::new(manager, editing)?));
    let window = crate::app_title::new::<crate::LoginDomainEntryWindow>()
        .map_err(|error| error.to_string())?;
    let active = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(&slots.window);
        let credentials = slots.credentials.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            crate::login_credential_window::cancel_credentials(&credentials);
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
    let context = Rc::new(Context {
        weak: window.as_weak(),
        active,
        draft,
        credentials: slots.credentials.clone(),
        close,
        applied,
        report,
    });
    window.on_selected({
        let context = context.clone();
        move |index| {
            if !context.active.get() {
                return;
            }
            let Some(window) = context.weak.upgrade() else {
                return;
            };
            if window.get_child_open() {
                return;
            }
            if usize::try_from(index)
                .ok()
                .is_some_and(|i| i < context.draft.borrow().choices().len())
            {
                window.set_selected_index(index);
                rows(&window, &context.draft.borrow(), index);
            }
        }
    });
    window.on_action({
        let context = context.clone();
        move |action| {
            if !context.active.get() {
                return;
            }
            let Some(window) = context.weak.upgrade() else {
                return;
            };
            if action == "force-close" {
                (context.close)();
                return;
            }
            if window.get_child_open() {
                return;
            }
            let stage = context.draft.borrow().stage;
            if action == "cancel" {
                if stage == Stage::Activate {
                    context.draft.borrow_mut().activate(false);
                } else {
                    context.draft.borrow_mut().cancel();
                }
            } else if action == "accept" {
                let result = match stage {
                    Stage::Script | Stage::Example | Stage::Access => {
                        if let Ok(index) = usize::try_from(window.get_selected_index()) {
                            context.draft.borrow_mut().choose(index);
                        }
                        Ok(())
                    }
                    Stage::Domain | Stage::Description => context
                        .draft
                        .borrow_mut()
                        .enter_text(window.get_text().as_str()),
                    Stage::Activate => {
                        context.draft.borrow_mut().activate(true);
                        Ok(())
                    }
                    _ => return,
                };
                if let Err(error) = result {
                    (context.report)(error);
                    (context.close)();
                    return;
                }
            } else {
                return;
            }
            present(&context);
        }
    });
    window.window().on_close_requested({
        let weak = window.as_weak();
        let active = context.active.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                window.invoke_action("cancel".into());
            }
            if active.get() {
                slint::CloseRequestResponse::KeepWindowShown
            } else {
                slint::CloseRequestResponse::HideWindow
            }
        }
    });
    window.show().map_err(|error| error.to_string())?;
    *slots.window.borrow_mut() = Some(window.clone_strong());
    present(&context);
    Ok(window)
}
