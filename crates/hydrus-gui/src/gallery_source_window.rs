//! Owned two-stage GUG picker, shared by staged gallery-source controls.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::url::{GugOptions, UrlClassSettings};
use hydrus_gui_model::gallery_source::{KeyAndName, Selector};
use hydrus_parse::Downloaders;
use hydrus_store::{Store, network::NetworkSettings};
use slint::{ComponentHandle as _, ModelRc, VecModel};

use crate::GallerySourceWindow;

pub type Slot = Rc<RefCell<Option<GallerySourceWindow>>>;
pub type Applied = Rc<dyn Fn(KeyAndName) -> Result<(), String>>;

thread_local! {
    static LAST: RefCell<Option<slint::Weak<GallerySourceWindow>>> = const { RefCell::new(None) };
}

pub fn last_opened() -> Option<GallerySourceWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|window| window.window().is_visible())
}

pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(GallerySourceWindow::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}

fn show(window: &GallerySourceWindow, selector: &Selector) {
    window.set_galleries(ModelRc::new(VecModel::from(
        selector
            .entries()
            .iter()
            .map(|entry| entry.label.as_str().into())
            .collect::<Vec<_>>(),
    )));
    window.set_selected(
        selector
            .selected
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
    );
    window.set_message(selector.warning.unwrap_or_default().into());
    window.set_window_title(
        if selector.warning.is_some() {
            "Warning"
        } else {
            "select gallery"
        }
        .into(),
    );
}

/// No settings are written here: the caller owns accepting the selected pair.
/// Cancel at either stage keeps its previous draft and invalidates old callbacks.
pub fn open(
    store: &Arc<Store>,
    current: Option<KeyAndName>,
    for_subscription: bool,
    slot: &Slot,
    applied: Applied,
) -> Result<GallerySourceWindow, String> {
    if let Some(window) = slot.borrow().as_ref() {
        window.show().map_err(|e| e.to_string())?;
        return Ok(window.clone_strong());
    }
    let downloaders: Downloaders = store
        .read(hydrus_store::settings::get)
        .map_err(|e| e.to_string())?;
    let classes: UrlClassSettings = store
        .read(hydrus_store::settings::get)
        .map_err(|e| e.to_string())?;
    let network: NetworkSettings = store
        .read(hydrus_store::settings::get)
        .map_err(|e| e.to_string())?;
    let options = GugOptions {
        percent_twenty_is_space: network.gug_percent_twenty_is_space,
        collapse_leading_slashes: classes.collapse_leading_slashes,
    };
    let selector = Rc::new(RefCell::new(Selector::new(
        &downloaders,
        &classes,
        options,
        current,
        for_subscription,
    )));
    let window = crate::app_title::new::<crate::GallerySourceWindow>().map_err(|e| e.to_string())?;
    show(&window, &selector.borrow());
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
        }
    });
    window.on_accept_clicked({
        let weak = window.as_weak();
        let close = close.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if selector.borrow().warning.is_some() {
                close();
                return;
            }
            selector.borrow_mut().selected = usize::try_from(window.get_selected()).ok();
            let value = selector.borrow_mut().accept();
            if let Some(value) = value {
                match applied(value) {
                    Ok(()) => close(),
                    Err(error) => window.set_error(error.into()),
                }
            } else {
                show(&window, &selector.borrow());
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
    window.show().map_err(|e| e.to_string())?;
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
