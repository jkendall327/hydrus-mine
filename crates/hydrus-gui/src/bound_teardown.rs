//! Tearing down a binding: the dialogs still open when the last clone of a
//! [`Bound`](crate::Bound) goes are hidden and forgotten.
//!
//! A dialog's callbacks hold the slot that holds the dialog, so a dialog left
//! open is never freed by dropping the binding: the window, its callbacks and
//! what they capture (a store, workers) stay alive in that cycle. Closing a
//! dialog already empties its slot, so this matters only for the dialogs open
//! when the binding goes: at a test's end, or at the client's exit, after the
//! event loop has returned.
//!
//! Not covered yet (nested slot types, each with its own children): the
//! formula, png export, interchange and child-page slots inside the parser
//! editors; the formula, png export and interchange slots inside the simple
//! formulae editor; the comparator, conditional-search, merge options and png
//! slots inside the auto-resolution editor; the top-level subscription
//! interchange; the sidecar, export files, external call, open externally,
//! suggested tags, network data, services, downloader definitions, login,
//! network sessions and search OR slots. Several of those (the interchange,
//! png export, network data and network sessions slots) already close their
//! windows on a `Drop` guarded by `Rc::strong_count(..) == 1`, which a slot
//! clone held by the window's own callbacks defeats. (The database
//! maintenance window is not missing: its owner, a field of the binding,
//! hides it when the last clone goes.)

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

/// What the last clone of a binding hides and forgets, as its own field of
/// [`Bound`](crate::Bound) so that it drops only with the last clone.
#[derive(Default)]
pub(crate) struct Teardown(RefCell<Vec<Box<dyn Fn()>>>);

impl std::fmt::Debug for Teardown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Teardown({} slots)", self.0.borrow().len())
    }
}

impl Teardown {
    fn push(&self, forget: impl Fn() + 'static) {
        self.0.borrow_mut().push(Box::new(forget));
    }

    fn add<T: slint::ComponentHandle + 'static>(&self, slot: &Rc<RefCell<Option<T>>>) {
        let slot = slot.clone();
        self.push(move || {
            let window = slot.borrow_mut().take();
            if let Some(window) = window {
                let _ = window.hide();
            }
        });
    }
}

impl Drop for Teardown {
    fn drop(&mut self) {
        for forget in self.0.get_mut().drain(..) {
            forget();
        }
    }
}

/// Registers `bound`'s dialog slots with its teardown.
pub(crate) fn watch(bound: &crate::Bound) {
    let t = &bound.teardown;
    // (the predicate editors are not here: their owners retire them, with
    // their cleanup notices, when the last clone goes)
    t.add(&bound.viewer);
    t.add(&bound.manage_tags);
    t.add(&bound.manage_tags_relationships);
    t.add(&bound.tag_relationships);
    t.add(&bound.tag_display);
    t.add(&bound.manage_notes);
    t.add(&bound.manage_ratings);
    t.add(&bound.rating_count_editor);
    t.add(&bound.manage_times);
    t.add(&bound.datetime_editor);
    t.add(&bound.force_filetype);
    t.add(&bound.embedded_metadata);
    t.add(&bound.manage_urls);
    t.add(&bound.options);
    t.add(&bound.about);
    t.add(&bound.services_review);
    t.add(&bound.checker_options);
    t.add(&bound.session_dialog);
    t.add(&bound.tab_name_dialog);
    t.add(&bound.subscriptions);
    t.add(&bound.subscription_gallery);
    t.add(&bound.edit_subscription);
    t.add(&bound.auto_resolution_review_filter);
    t.add(&bound.file_log);
    t.add(&bound.archive_delete);
    t.add(&bound.filter);
    t.add(&bound.filename_tagging);
    t.add(&bound.locations);
    t.add(&bound.tag_menu_relationships);
    t.add(&bound.viewer_tag_relationships);
    t.add(&bound.viewer_deletion);
    t.add(&bound.incremental_tags);
    t.add(&bound.tag_migration);
    t.add(&bound.options_reason_child);
    t.add(&bound.options_colour_child);
    t.add(&bound.options_frame_child);
    t.add(&bound.options_banner_child);
    t.add(&bound.command_palette);
    t.add(&bound.archive_repair);
    t.add(&bound.viewing_maintenance);
    t.add(&bound.file_history);
    t.add(&bound.delete_files);
    t.add(&bound.local_transfer);
    let folders = &bound.folders;
    t.add(&folders.import_list);
    t.add(&folders.import_edit);
    t.add(&folders.export_list);
    t.add(&folders.export_edit);
    t.add(&folders.filename_tagging);
    t.add(&folders.import_options);
    t.add(&folders.log);
    t.add(&bound.favourites.list);
    t.add(&bound.favourites.edit);
    let parsers = &bound.parser_editors;
    t.add(&parsers.list);
    t.add(&parsers.links);
    t.add(&parsers.picker);
    t.add(&parsers.page);
    t.add(&parsers.content);
    let rules = &bound.auto_resolution;
    t.add(&rules.list);
    t.add(&rules.rule);
    t.add(&rules.locations);
    t.add(&rules.preview_filter);
    t.add(&bound.simple_formulae.list);
    let reviews = bound.auto_resolution_reviews.clone();
    t.push(move || {
        let windows: Vec<_> = reviews.borrow_mut().drain(..).collect();
        for (_, window) in windows {
            let _ = window.hide();
        }
    });
    let imports = bound.review_imports.clone();
    t.push(move || {
        let review = imports.borrow_mut().take();
        if let Some((window, _)) = review {
            let _ = window.hide();
        }
    });
}
