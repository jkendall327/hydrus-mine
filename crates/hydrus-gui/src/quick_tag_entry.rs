//! The quick tag entry dialog an interactive tag shortcut opens
//! (`GetContentApplicationCommandFromInteractiveContentCommand` with
//! `EnterText`): the recent tags as buttons, the newest in the line
//! (selected), and the tag chosen or typed set on the files the shortcut was
//! used on, and made the newest recent tag. Blank or whitespace text is
//! refused with the reference's words; cancelling does nothing.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::{HashId, ServiceKey};
use hydrus_gui_model::shortcut_content as content;
use hydrus_store::Store;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::QuickTagEntryWindow;

thread_local! {
    static LAST: RefCell<Option<QuickTagEntryWindow>> = const { RefCell::new(None) };
}

/// The dialog last opened on this thread (while it is open), for tests.
pub fn last_window() -> Option<QuickTagEntryWindow> {
    LAST.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|w| w.window().is_visible())
            .map(ComponentHandle::clone_strong)
    })
}

/// Ask for a tag on `service` for `files`; `changed` runs when the files'
/// tags changed.
pub(crate) fn ask(
    store: Arc<Store>,
    service: ServiceKey,
    files: Vec<HashId>,
    changed: Rc<dyn Fn()>,
) {
    // one at a time, as the reference's modal dialog allows: another press
    // while one asks brings that one forward
    if let Some(open) = last_window() {
        let _ = open.show();
        return;
    }
    let entry = match content::quick_entry(&store, &service) {
        Ok(entry) => entry,
        Err(e) => {
            eprintln!("could not ask for a tag: {e}");
            return;
        }
    };
    let window = match crate::app_title::new::<QuickTagEntryWindow>() {
        Ok(window) => window,
        Err(e) => {
            eprintln!("could not open the quick tag dialog: {e}");
            return;
        }
    };
    window.set_message(entry.message.clone().into());
    window.set_suggestions(ModelRc::new(VecModel::from(
        entry
            .suggestions
            .iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    )));
    window.set_text(entry.default.clone().into());
    let suggestions = Rc::new(entry.suggestions);
    let finish = Rc::new({
        let weak = window.as_weak();
        move |text: Option<String>| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.window().is_visible() {
                return;
            }
            if let Some(text) = text {
                match content::enter(&store, &files, &service, &text) {
                    Ok(true) => changed(),
                    Ok(false) => {}
                    Err(e) => {
                        window.set_refusal(e.to_string().into());
                        return;
                    }
                }
            }
            let _ = window.hide();
        }
    });
    window.on_chosen({
        let finish = finish.clone();
        let suggestions = suggestions.clone();
        move |i| {
            if let Some(tag) = usize::try_from(i).ok().and_then(|i| suggestions.get(i)) {
                finish(Some(tag.clone()));
            }
        }
    });
    window.on_applied({
        let finish = finish.clone();
        let weak = window.as_weak();
        move |text| match content::refusal(&text) {
            Some(refusal) => {
                if let Some(window) = weak.upgrade() {
                    window.set_refusal(refusal.into());
                }
            }
            None => finish(Some(text.to_string())),
        }
    });
    window.on_cancelled({
        let finish = finish.clone();
        move || finish(None)
    });
    window.window().on_close_requested(move || {
        finish(None);
        slint::CloseRequestResponse::HideWindow
    });
    if let Err(e) = window.show() {
        eprintln!("could not show the quick tag dialog: {e}");
        return;
    }
    // the starting tag is selected, to be typed over
    window.invoke_select_entry();
    LAST.with(|last| *last.borrow_mut() = Some(window));
}
