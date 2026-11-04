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
    open_accepting(
        slot,
        store,
        files,
        suggested,
        default_reason,
        guard,
        Accepting {
            closed: None,
            accept: Rc::new(move |draft: &Draft, store: &Store| {
                let choice = draft.apply_with_choice(store).map_err(|e| e.to_string())?;
                let applied = applied.clone();
                Ok(Box::new(move || {
                    if let Some(choice) = choice {
                        applied(&choice);
                    }
                }) as Box<dyn FnOnce()>)
            }),
        },
    )
}

/// The dialog's choice, not applied: `chosen` gets it to apply later (the
/// reference's `just_get_content_update_packages`, as the duplicate filter
/// asks for it). The choice and reason are remembered on OK, as when
/// applied. `closed` is told when a dialog shown closes, however it does.
pub fn open_deferred(
    slot: &Slot,
    store: &Arc<Store>,
    files: &[hydrus_core::HashId],
    suggested: Option<&DeletionAction>,
    default_reason: &str,
    guard: Guard,
    deferring: Deferring,
) -> Result<Option<DeleteFilesWindow>, String> {
    let Deferring { chosen, closed } = deferring;
    open_accepting(
        slot,
        store,
        files,
        suggested,
        default_reason,
        guard,
        Accepting {
            closed: Some(closed),
            accept: Rc::new(move |draft: &Draft, store: &Store| {
                let deferred = draft.defer(store).map_err(|e| e.to_string())?;
                let chosen = chosen.clone();
                Ok(Box::new(move || {
                    if let Some(deferred) = deferred {
                        chosen(deferred);
                    }
                }) as Box<dyn FnOnce()>)
            }),
        },
    )
}

/// What a deferred dialog tells its owner: the choice, on OK; and that it
/// closed, however it did.
pub struct Deferring {
    pub chosen: Rc<dyn Fn(hydrus_gui_model::delete_files::Deferred)>,
    pub closed: Rc<dyn Fn()>,
}

impl std::fmt::Debug for Deferring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deferring").finish_non_exhaustive()
    }
}

/// What OK does, and who to tell when the dialog closes.
struct Accepting {
    closed: Option<Rc<dyn Fn()>>,
    accept: Accept,
}

/// What OK does with the draft: apply or freeze it, then (after the dialog
/// closes) tell its owner.
type Accept = Rc<dyn Fn(&Draft, &Store) -> Result<Box<dyn FnOnce()>, String>>;

thread_local! {
    static LAST: RefCell<Option<slint::Weak<DeleteFilesWindow>>> = const { RefCell::new(None) };
}

/// The dialog last opened, while it is shown (for tests).
pub fn last_opened() -> Option<DeleteFilesWindow> {
    LAST.with(|l| l.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|window| window.window().is_visible())
}

fn open_accepting(
    slot: &Slot,
    store: &Arc<Store>,
    files: &[hydrus_core::HashId],
    suggested: Option<&DeletionAction>,
    default_reason: &str,
    guard: Guard,
    accepting: Accepting,
) -> Result<Option<DeleteFilesWindow>, String> {
    let Accepting { closed, accept } = accepting;
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(Some(window.clone_strong()));
    }
    let preferences: hydrus_store::settings::DeletionPreferences = store
        .read(hydrus_store::settings::get)
        .map_err(|e| e.to_string())?;
    let draft = Draft::load_mode(
        store,
        files,
        suggested,
        default_reason,
        preferences.advanced,
    )
    .map_err(|e| e.to_string())?;
    if draft.choices.is_empty() {
        return Err("No valid delete choices!".into());
    }
    if draft.already_resolved(preferences.confirm_trash) {
        if guard() {
            accept(&draft, store)?();
        }
        return Ok(None);
    }
    let window = crate::app_title::new::<crate::DeleteFilesWindow>().map_err(|e| e.to_string())?;
    window.set_custom(draft.custom.as_str().into());
    window.set_simple(draft.simple);
    show(&window, &draft);
    // (the reference focuses the action radios, else the reason ones)
    let initial_radio = match (draft.choices.len() <= 1, draft.simple) {
        (false, _) => 0,
        (true, false) => 1,
        (true, true) => -1,
    };
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
            if let Some(closed) = &closed {
                closed();
            }
        }
    });
    window.on_force_radio_ok({
        let active = active.clone();
        let guard = guard.clone();
        let weak = window.as_weak();
        let store = store.clone();
        move || {
            if !active.get() || !guard() || weak.upgrade().is_none_or(|w| !w.window().is_visible())
            {
                return false;
            }
            match store.read(hydrus_store::radio_return::load) {
                Ok(policy) => policy.force_dialog_ok,
                Err(error) => {
                    eprintln!("Could not read the radio Return preference: {error}");
                    false
                }
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
            if !window.window().is_visible() {
                return;
            }
            draft.borrow_mut().custom = window.get_custom().to_string();
            let accepted = accept(&draft.borrow(), &store);
            match accepted {
                Ok(then) => {
                    close();
                    then();
                }
                Err(error) => {
                    window.set_error(error.into());
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
    LAST.with(|l| *l.borrow_mut() = Some(window.as_weak()));
    window.show().map_err(|e| e.to_string())?;
    window.set_radio_focus(initial_radio);
    Ok(Some(window))
}
