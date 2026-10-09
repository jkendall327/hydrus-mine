//! Undeleting files as the reference's `UndeleteMedia` does: back to the one
//! local file domain they were deleted from (asking first while "Confirm
//! sending files to trash" is on), or to the domain chosen in an "Undelete
//! for?" chooser when they were deleted from several.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::HashId;
use hydrus_gui_model::media_actions;
use hydrus_store::Store;

use crate::ChoiceButtonsWindow;

thread_local! {
    /// The open chooser, kept alive until answered.
    static CHOOSER: RefCell<Option<ChoiceButtonsWindow>> = const { RefCell::new(None) };
}

/// The last "Undelete for?" chooser opened, for tests.
pub fn last_chooser() -> Option<ChoiceButtonsWindow> {
    CHOOSER.with(|c| c.borrow().as_ref().map(slint::ComponentHandle::clone_strong))
}

/// Asks a yes/no question, running the closure on yes.
pub type AskYesNo<'a> = &'a dyn Fn(String, Rc<dyn Fn()>);

/// Undelete `files`; `done` runs once they are restored.
pub fn undelete(store: &Arc<Store>, files: &[HashId], ask: AskYesNo<'_>, done: Rc<dyn Fn()>) {
    let Some(undeletion) = media_actions::undeletion(store, files) else {
        return;
    };
    let restore = {
        let (store, files) = (store.clone(), files.to_vec());
        move |domain| match media_actions::undelete_to(&store, &files, domain) {
            Ok(()) => done(),
            Err(e) => eprintln!("could not undelete the files: {e}"),
        }
    };
    if let Some(domain) = undeletion.only() {
        match undeletion.question(store) {
            Some(question) => ask(question, Rc::new(move || restore(domain))),
            None => restore(domain),
        }
        return;
    }
    let opened = crate::choice_buttons::open(
        &crate::choice_buttons::Ask {
            title: media_actions::UNDELETE_CHOOSER_TITLE,
            message: "",
            choices: undeletion.choices(),
            no_label: "",
        },
        move |choice| {
            if let Some(domain) = choice.and_then(|i| undeletion.chosen(i)) {
                restore(domain);
            }
        },
    );
    match opened {
        Ok(window) => CHOOSER.with(|c| *c.borrow_mut() = window),
        Err(e) => eprintln!("could not ask where to undelete to: {e}"),
    }
}
