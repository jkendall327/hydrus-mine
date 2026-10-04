//! Owner-scoped shared import-option overwrite chooser.
use crate::{ImportOptionsOverwriteWindow, ImportOverwriteRow};
use hydrus_core::import_options::{CallerType, ImportOptionsSlice};
use hydrus_gui_model::import_options_overwrite::{Overwrite, Preset};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

pub type Slot = Rc<RefCell<Option<ImportOptionsOverwriteWindow>>>;

thread_local! {
    static LAST: RefCell<Option<slint::Weak<ImportOptionsOverwriteWindow>>> = const { RefCell::new(None) };
}

/// The latest visible chooser, without retaining its owner or window.
pub fn last_opened() -> Option<ImportOptionsOverwriteWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|window| window.window().is_visible())
}

fn show(
    window: &ImportOptionsOverwriteWindow,
    draft: &Overwrite,
    snapshot: &hydrus_store::store::Snapshot,
) {
    let name = |key: &str| {
        hex::decode(key)
            .ok()
            .and_then(|key| {
                snapshot
                    .services
                    .by_key(&hydrus_core::ServiceKey::new(key))
                    .ok()
            })
            .map_or_else(|| "unknown service".into(), |service| service.name.clone())
    };
    let result = draft.value();
    let rows = draft
        .kinds
        .iter()
        .enumerate()
        .map(|(i, &kind)| ImportOverwriteRow {
            current_label: draft.label(&draft.current, kind, false, &name).into(),
            pasted_label: draft.label(&draft.pasted, kind, true, &name).into(),
            result_label: draft.label(&result, kind, false, &name).into(),
            current_checked: draft.keep_current[i],
            pasted_checked: draft.take_pasted[i],
            pasted_enabled: draft.caller != CallerType::Global || kind.is_set(&draft.pasted),
        })
        .collect::<Vec<_>>();
    window.set_global_options(draft.caller == CallerType::Global);
    window.set_notice(draft.notice().into());
    window.set_rows(ModelRc::new(VecModel::from(rows)));
}

pub fn open(
    store: &Arc<Store>,
    draft: Overwrite,
    slot: &Slot,
    applied: Rc<dyn Fn(ImportOptionsSlice)>,
    closed: Rc<dyn Fn()>,
) -> Result<ImportOptionsOverwriteWindow, String> {
    let window = ImportOptionsOverwriteWindow::new().map_err(|e| e.to_string())?;
    let state = Rc::new(RefCell::new(draft));
    let active = Rc::new(Cell::new(true));
    let snapshot = store.snapshot();
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            closed();
        }
    };
    let change = {
        let weak = window.as_weak();
        let state = state.clone();
        let active = active.clone();
        let snapshot = snapshot.clone();
        Rc::new(move |f: &dyn Fn(&mut Overwrite)| {
            if !active.get() {
                return;
            }
            f(&mut state.borrow_mut());
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow(), &snapshot);
            }
        })
    };
    window.on_preset({
        let change = change.clone();
        move |index| {
            let preset = match index {
                0 => Preset::Merge,
                1 => Preset::FillIn,
                2 => Preset::Replace,
                _ => return,
            };
            change(&|draft| draft.preset(preset));
        }
    });
    window.on_ticked({
        let change = change.clone();
        move |row, pasted, checked| {
            if let Ok(row) = usize::try_from(row) {
                change(&|draft| draft.tick(pasted, row, checked));
            }
        }
    });
    window.on_apply({
        let state = state.clone();
        let active = active.clone();
        let close = close.clone();
        move || {
            if !active.get() {
                return;
            }
            let value = state.borrow().value();
            close();
            applied(value);
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
    show(&window, &state.borrow(), &snapshot);
    window.show().map_err(|e| e.to_string())?;
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    Ok(window)
}
