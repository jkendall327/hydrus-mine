//! Owner-scoped advanced local deletion. Cancel and a closed parent discard the
//! draft; Accept applies the frozen action and remembered choices atomically.
use crate::DeleteFilesWindow;
use hydrus_gui_model::delete_files::Draft;
use hydrus_store::Store;
use hydrus_store::settings::DeletionAction;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

/// The advanced deletion dialog owned by a thumbnail panel or viewer.
pub type Slot = Rc<RefCell<Option<DeleteFilesWindow>>>;
/// A parent identity/lifetime check, consulted before accepting a mutation.
pub type Guard = Rc<dyn Fn() -> bool>;
/// Refresh the owner after the accepted mutation.
pub type Applied = Rc<dyn Fn()>;
pub type AppliedChoice = Rc<dyn Fn(&hydrus_gui_model::delete_files::Choice)>;

/// Discard the owned draft, including callbacks retained by a stale handle.
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}

fn show(window: &DeleteFilesWindow, draft: &Draft) {
    window.set_actions(ModelRc::new(VecModel::from(
        draft
            .choices
            .iter()
            .map(|c| c.label.as_str().into())
            .collect::<Vec<_>>(),
    )));
    window.set_reasons(ModelRc::new(VecModel::from(
        draft
            .reasons
            .iter()
            .map(|r| r.label.as_str().into())
            .collect::<Vec<_>>(),
    )));
    window.set_selected_action(i32::try_from(draft.action).unwrap_or(0));
    window.set_selected_reason(i32::try_from(draft.reason).unwrap_or(0));
    window.set_reason_enabled(draft.reason_enabled());
    window.set_custom_enabled(draft.custom_enabled());
}

/// Open on actual local files, with a suggested domain/action. Store writes wait
/// for Accept and a live parent; unavailable/locked actions are never offered.
pub fn open(
    slot: &Slot,
    store: &Arc<Store>,
    files: &[hydrus_core::HashId],
    suggested: Option<&DeletionAction>,
    default_reason: &str,
    guard: Guard,
    applied: Applied,
) -> Result<Option<DeleteFilesWindow>, String> {
    open_with_choice(
        slot,
        store,
        files,
        suggested,
        default_reason,
        guard,
        Rc::new(move |_| applied()),
    )
}
/// Report the captured actionable choice rather than the original mixed selection.
pub fn open_with_choice(
    slot: &Slot,
    store: &Arc<Store>,
    files: &[hydrus_core::HashId],
    suggested: Option<&DeletionAction>,
    default_reason: &str,
    guard: Guard,
    applied: AppliedChoice,
) -> Result<Option<DeleteFilesWindow>, String> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(Some(window.clone_strong()));
    }
    let draft = Draft::load(store, files, suggested, default_reason).map_err(|e| e.to_string())?;
    if draft.choices.is_empty() {
        return Err("No valid delete choices!".into());
    }
    let preferences: hydrus_store::settings::DeletionPreferences = store
        .read(hydrus_store::settings::get)
        .map_err(|e| e.to_string())?;
    if draft.already_resolved(preferences.confirm_trash) {
        if guard() {
            if let Some(choice) = draft.apply_with_choice(store).map_err(|e| e.to_string())? {
                applied(&choice);
            }
        }
        return Ok(None);
    }
    let window = DeleteFilesWindow::new().map_err(|e| e.to_string())?;
    window.set_custom(draft.custom.as_str().into());
    show(&window, &draft);
    let draft = Rc::new(RefCell::new(draft));
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
    window.on_action_selected({
        let active = active.clone();
        let draft = draft.clone();
        let weak = window.as_weak();
        move |index| {
            if !active.get() {
                return;
            }
            if let Ok(index) = usize::try_from(index)
                && index < draft.borrow().choices.len()
            {
                draft.borrow_mut().action = index;
                if let Some(window) = weak.upgrade() {
                    show(&window, &draft.borrow());
                }
            }
        }
    });
    window.on_reason_selected({
        let active = active.clone();
        let draft = draft.clone();
        let weak = window.as_weak();
        move |index| {
            if !active.get() {
                return;
            }
            if let Ok(index) = usize::try_from(index)
                && index < draft.borrow().reasons.len()
                && draft.borrow().reason_enabled()
            {
                draft.borrow_mut().reason = index;
                if let Some(window) = weak.upgrade() {
                    show(&window, &draft.borrow());
                }
            }
        }
    });
    window.on_accept_deletion({
        let active = active.clone();
        let draft = draft.clone();
        let weak = window.as_weak();
        let close = close.clone();
        let store = store.clone();
        move || {
            if !active.get() {
                return;
            }
            if !guard() {
                close();
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            draft.borrow_mut().custom = window.get_custom().to_string();
            match draft.borrow().apply_with_choice(&store) {
                Ok(choice) => {
                    close();
                    if let Some(choice) = choice {
                        applied(&choice);
                    }
                }
                Err(error) => {
                    window.set_error(error.to_string().into());
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
    *slot.borrow_mut() = Some(window.clone_strong());
    window.show().map_err(|e| e.to_string())?;
    Ok(Some(window))
}
