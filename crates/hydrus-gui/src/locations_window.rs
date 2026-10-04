//! The "multiple/deleted locations" list, bound to its tick boxes
//! ([`domains::multiple_ticks`]): ticked as the page searches now, each
//! tick unticking the domains another ticked one covers, as the
//! reference's `_ClearSurplusServices` does, and "apply" searching what is
//! ticked.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, VecModel};

use hydrus_search::LocationContext;
use hydrus_store::Store;

use crate::domains::{self, Tick};
use crate::{LocationTick, LocationsWindow};

thread_local! {
    static LAST: RefCell<Option<slint::Weak<LocationsWindow>>> = const { RefCell::new(None) };
}
/// Most recently opened visible location selector, for widget regression tests.
pub fn last_opened() -> Option<LocationsWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

/// Whether each tick box is ticked, for `location`.
fn ticked_for(ticks: &[Tick], location: &LocationContext) -> Vec<bool> {
    ticks
        .iter()
        .map(|t| {
            if t.deleted {
                location.deleted().contains(&t.service)
            } else {
                location.current().contains(&t.service)
            }
        })
        .collect()
}

/// Open the list on `current`; "apply" hands what is ticked to `chosen`.
/// It forgets itself from `slot` when closed; one already open is shown.
pub(crate) fn open(
    slot: &Rc<RefCell<Option<LocationsWindow>>>,
    store: Arc<Store>,
    current: &LocationContext,
    chosen: Rc<dyn Fn(LocationContext)>,
) -> Result<(), String> {
    if let Some(window) = slot.borrow().as_ref() {
        return window.show().map_err(|e| e.to_string());
    }
    let window = LocationsWindow::new().map_err(|e| e.to_string())?;
    let hydrus_store::settings::AdvancedMode(advanced) =
        store.read(hydrus_store::settings::get).unwrap_or_default();
    let ticks = domains::multiple_ticks(&store.snapshot().services, advanced);
    let ticked = Rc::new(RefCell::new(ticked_for(&ticks, current)));
    let ticks = Rc::new(ticks);
    let show = {
        let weak = window.as_weak();
        let ticks = ticks.clone();
        let ticked = ticked.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let rows: Vec<LocationTick> = ticks
                .iter()
                .zip(ticked.borrow().iter())
                .map(|(t, &checked)| LocationTick {
                    label: t.label.as_str().into(),
                    checked,
                })
                .collect();
            window.set_ticks(ModelRc::new(VecModel::from(rows)));
        }
    };
    show();
    // what is ticked now, without what another ticked domain covers
    let location = {
        let ticks = ticks.clone();
        let ticked = ticked.clone();
        move || {
            let chosen: Vec<(bool, hydrus_core::ServiceKey)> = ticks
                .iter()
                .zip(ticked.borrow().iter())
                .filter(|(_, on)| **on)
                .map(|(t, _)| (t.deleted, t.service.clone()))
                .collect();
            domains::ticked_location(&store.snapshot().services, &chosen)
        }
    };
    window.on_toggled({
        let ticks = ticks.clone();
        let ticked = ticked.clone();
        let location = location.clone();
        let show = show.clone();
        move |i, on| {
            if let Ok(i) = usize::try_from(i)
                && let Some(tick) = ticked.borrow_mut().get_mut(i)
            {
                *tick = on;
            }
            let kept = location();
            *ticked.borrow_mut() = ticked_for(&ticks, &kept);
            show();
        }
    });
    let active = Rc::new(Cell::new(true));
    let close = {
        let active = active.clone();
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            let window = weak.upgrade();
            if let Some(window) = &window {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            if let Some(window) = window {
                window.invoke_closed();
            }
        }
    };
    window.on_apply({
        let active = active.clone();
        let close = close.clone();
        move || {
            if !active.get() {
                return;
            }
            let chosen_location = location();
            close();
            chosen(chosen_location);
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    *slot.borrow_mut() = Some(window);
    Ok(())
}
