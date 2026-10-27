//! Tearing down a binding: the dialogs still open lose their slots, which
//! their own callbacks hold, so the window, its callbacks and what they
//! capture (the store among them) are freed with the binding instead of
//! kept by that cycle. Closing a dialog already empties its slot; this is
//! for the ones open when the binding goes (a test's end, the app's exit).

use slint::ComponentHandle as _;

/// Hides and forgets the window in `slot`, if any.
fn forget<T: slint::ComponentHandle>(slot: &std::cell::RefCell<Option<T>>) {
    let window = slot.borrow_mut().take();
    if let Some(window) = window {
        let _ = window.hide();
    }
}

impl Drop for crate::Bound {
    fn drop(&mut self) {
        // (the predicate editor is not here: its owner retires it, with its
        // cleanup notice, when the final clone goes)
        // (Bound is cloned; only the final clone's drop tears down, as the
        // predicate editor's owner, held by Bound clones alone, shows)
        if std::rc::Rc::strong_count(&self._predicate_editor_owner) > 1 {
            return;
        }
        forget(&self.viewer);
        forget(&self.manage_tags);
        forget(&self.tag_relationships);
        forget(&self.tag_display);
        forget(&self.manage_notes);
        forget(&self.manage_ratings);
        forget(&self.rating_count_editor);
        forget(&self.manage_times);
        forget(&self.datetime_editor);
        forget(&self.force_filetype);
        forget(&self.embedded_metadata);
        forget(&self.manage_urls);
        forget(&self.options);
        forget(&self.about);
        forget(&self.services_review);
        forget(&self.checker_options);
        forget(&self.session_dialog);
        forget(&self.tab_name_dialog);
        forget(&self.subscriptions);
        forget(&self.subscription_gallery);
        forget(&self.edit_subscription);
        forget(&self.auto_resolution_review_filter);
        forget(&self.file_log);
        forget(&self.archive_delete);
        forget(&self.filter);
        forget(&self.filename_tagging);
        forget(&self.locations);
        forget(&self.tag_menu_relationships);
    }
}
