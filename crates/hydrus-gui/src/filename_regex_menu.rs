//! Shared RegexButton menus attached to the two filename extraction inputs.
use crate::{
    FilenameTaggingWindow,
    popup_menu::{Chosen, Popup},
};
use hydrus_gui_model::filename_rules::{RegexAction, regex_menu};
use hydrus_store::Store;
use slint::{ComponentHandle as _, Model as _};
use std::{rc::Rc, sync::Arc};

/// Regex favourites and menus owned by the filename-tagging window.
#[derive(Clone)]
pub(crate) struct Controls {
    slot: crate::regex_favourites_window::Slot,
    popup: Rc<Popup<RegexAction>>,
}
impl Controls {
    pub(crate) fn blocking(&self) -> bool {
        crate::regex_favourites_window::has_open(&self.slot) || self.popup.model().row_count() > 0
    }
    pub(crate) fn cancel(&self) {
        self.popup.close();
        crate::regex_favourites_window::cancel(&self.slot);
    }
}

pub(crate) fn bind(
    window: &FilenameTaggingWindow,
    store: &Arc<Store>,
    alive: Rc<dyn Fn() -> bool>,
) -> Controls {
    let controls = Controls {
        slot: Rc::default(),
        popup: Popup::new(),
    };
    window.set_regex_panes(controls.popup.model());
    window.on_regex_menu({
        let controls = controls.clone();
        let store = store.clone();
        let alive = alive.clone();
        move |x, y| {
            if !alive() || crate::regex_favourites_window::has_open(&controls.slot) {
                return;
            }
            if let Ok(value) = store.read(hydrus_store::regex_favourites::load) {
                let (entries, actions) = regex_menu(&value);
                controls.popup.open(entries, actions, x, y);
            }
        }
    });
    window.on_regex_placed({
        let popup = controls.popup.clone();
        move |p, x, y, width| popup.placed(p, x, y, width)
    });
    window.on_regex_hovered({
        let popup = controls.popup.clone();
        move |p, l, r, t, left| popup.hover(p, l, r, t, left)
    });
    window.on_regex_dismissed({
        let popup = controls.popup.clone();
        move || popup.close()
    });
    window.on_regex_clicked({
        let controls = controls.clone();
        let store = store.clone();
        let weak = window.as_weak();
        move |p, l, r, t, left| {
            if !alive() || crate::regex_favourites_window::has_open(&controls.slot) {
                controls.popup.close();
                return;
            }
            match controls.popup.click(p, l, r, t, left) {
                Some(Chosen::Action(RegexAction::Copy(value))) => crate::copy_to_clipboard(&value),
                Some(Chosen::Action(RegexAction::Help(url))) => crate::launch(&url),
                Some(Chosen::Action(RegexAction::Manage)) => {
                    let Ok(value) = store.read(hydrus_store::regex_favourites::load) else {
                        return;
                    };
                    let applied: crate::regex_favourites_window::Applied = Rc::new({
                        let store = store.clone();
                        let alive = alive.clone();
                        move |value| {
                            if !alive() {
                                return Err("The filename tagging input has closed.".into());
                            }
                            store
                                .write_and_refresh(move |ctx| {
                                    hydrus_store::settings::set(ctx.conn(), &value)
                                })
                                .map_err(|error| error.to_string())
                        }
                    });
                    if let Ok(child) =
                        crate::regex_favourites_window::open(&value, &controls.slot, applied)
                        && let Some(window) = weak.upgrade()
                    {
                        window.set_regex_child_open(true);
                        let weak = window.as_weak();
                        child.on_closed(move || {
                            if let Some(window) = weak.upgrade() {
                                window.set_regex_child_open(false);
                            }
                        });
                    }
                }
                _ => (),
            }
        }
    });
    controls
}
