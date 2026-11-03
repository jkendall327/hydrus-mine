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
//! The nested slot types (a dialog's children, and theirs) are walked by the
//! `retire_*` functions below, which take each window out of its slot and
//! hide it, as `add` does for a top-level one. (The database maintenance
//! window is not a slot: its owner, a field of the binding, hides it when
//! the last clone goes.)

use std::cell::RefCell;
use std::rc::Rc;

use slint::ComponentHandle as _;

/// Alive until this thread's thread-locals are being destroyed. It is first
/// touched by [`watch`], after Slint's own thread-locals, so it is destroyed
/// before them (thread-locals go in the reverse order they were made).
struct Alive;
impl Drop for Alive {
    fn drop(&mut self) {}
}
thread_local! {
    static ALIVE: Alive = const { Alive };
}

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
        // A binding dropped while the thread's thread-locals are destroyed
        // (one kept by something a thread-local owns) leaves its windows as
        // they are: Slint's state may be gone, and a panic in a thread-local
        // destructor aborts the process. Nor may a panic here escape.
        if ALIVE.try_with(|_| ()).is_err() {
            return;
        }
        for forget in self.0.get_mut().drain(..) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(forget));
        }
    }
}

/// Take the window out of `slot` and hide it.
fn forget<T: slint::ComponentHandle>(slot: &Rc<RefCell<Option<T>>>) {
    let window = slot.borrow_mut().take();
    if let Some(window) = window {
        let _ = window.hide();
    }
}

fn retire_png(s: &crate::png_export_window::Slots) {
    forget(&s.0);
}

fn retire_exchange(s: &crate::downloader_interchange_window::Slots) {
    forget(&s.0);
    retire_png(&s.1);
}

fn retire_strings(s: &crate::string_processor_window::Slots) {
    forget(&s.processor);
    forget(&s.step);
    forget(&s.tag_filter);
    forget(&s.converter);
    forget(&s.conversion);
    retire_exchange(&s.exchange);
    retire_png(&s.png);
    forget(&s.favourites);
}

fn retire_formula(s: &crate::formula_window::Slots) {
    retire_exchange(&s.exchange);
    forget(&s.formula);
    forget(&s.rule);
    let child = s.child.borrow_mut().take();
    if let Some(child) = child {
        retire_formula(&child);
    }
    retire_strings(&s.strings);
    retire_png(&s.png);
}

fn retire_parsers(s: &crate::parser_editors_window::Slots) {
    retire_exchange(&s.exchange);
    forget(&s.list);
    forget(&s.links);
    forget(&s.picker);
    forget(&s.page);
    forget(&s.content);
    retire_formula(&s.formula);
    let child = s.child.borrow_mut().take();
    if let Some(child) = child {
        retire_parsers(&child);
    }
}

fn retire_sidecars(s: &crate::sidecars_window::Slots) {
    forget(&s.routers);
    retire_exchange(&s.exchange);
    forget(&s.router);
    forget(&s.node);
    forget(&s.object_name);
    retire_strings(&s.strings);
    retire_formula(&s.formula);
}

fn retire_cookies(s: &crate::login_cookies_window::Slots) {
    forget(&s.window);
    retire_strings(&s.strings);
}

fn retire_login_step(s: &crate::login_step_window::Slots) {
    forget(&s.step);
    retire_parsers(&s.parsers);
    retire_exchange(&s.exchange);
    retire_cookies(&s.cookies);
    forget(&s.argument);
}

fn retire_login_domains(s: &crate::login_domains_window::Slots) {
    forget(&s.domains);
    forget(&s.credentials);
    forget(&s.entry.window);
    forget(&s.entry.credentials);
}

fn retire_login(s: &crate::login_workflows_window::Slots) {
    forget(&s.scripts);
    retire_login_domains(&s.domains);
    forget(&s.script);
    retire_login_step(&s.step);
    s.test_control.borrow_mut().take();
    forget(&s.result);
    forget(&s.test_domain);
    retire_cookies(&s.cookies);
    forget(&s.example);
    forget(&s.definition);
    forget(&s.credentials);
    retire_strings(&s.strings);
    retire_exchange(&s.exchange);
}

fn retire_downloader_definitions(s: &crate::downloader_definitions_window::Slots) {
    retire_exchange(&s.class_exchange);
    retire_exchange(&s.gug_exchange);
    forget(&s.classes);
    forget(&s.gugs);
    forget(&s.class_edit);
    forget(&s.gug_edit);
    forget(&s.rule);
    retire_strings(&s.strings);
    forget(&s.domains.entry);
    forget(&s.domains.favourites);
}

fn retire_simple_formulae(s: &crate::simple_formulae_window::Slots) {
    retire_exchange(&s.exchange);
    forget(&s.list);
    retire_formula(&s.formula);
    retire_png(&s.png);
    forget(&s.pngs.0);
}

fn retire_auto_resolution(s: &crate::auto_resolution_rules_window::Slots) {
    forget(&s.list);
    forget(&s.rule);
    let comparators: Vec<_> = s.comparators.borrow_mut().drain(..).collect();
    for (_, window) in comparators {
        let _ = window.hide();
    }
    forget(&s.conditional.predicate_editor);
    s.conditional.search_or.retire();
    forget(&s.merge_options);
    forget(&s.locations);
    forget(&s.preview_filter);
    retire_png(&s.png);
}

fn retire_external_call(s: &crate::external_call_window::Slots) {
    forget(&s.defaults);
    forget(&s.editor);
    forget(&s.command);
    forget(&s.question);
    retire_strings(&s.strings);
    retire_exchange(&s.exchange);
}

/// Registers `bound`'s dialog slots with its teardown.
pub(crate) fn watch(bound: &crate::Bound) {
    ALIVE.with(|_| ());
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
    t.add(&bound.export_files.window);
    // the nested slot types: every descendant of a dialog
    fn nested<S: Clone + 'static>(t: &Teardown, slots: &S, retire: fn(&S)) {
        let slots = slots.clone();
        t.push(move || retire(&slots));
    }
    nested(t, &bound.export_files.sidecars, retire_sidecars);
    nested(t, &bound.filename_tagging_sidecars, retire_sidecars);
    nested(t, &folders.sidecars, retire_sidecars);
    nested(t, &bound.subscription_exchange, retire_exchange);
    nested(t, &bound.parser_editors, retire_parsers);
    nested(t, &bound.login_workflows, retire_login);
    nested(t, &bound.downloader_definitions, retire_downloader_definitions);
    nested(t, &bound.simple_formulae, retire_simple_formulae);
    nested(t, &bound.auto_resolution, retire_auto_resolution);
    nested(t, &bound.options_external_calls, retire_external_call);
    t.add(&bound.options_open_externally.files);
    t.add(&bound.options_open_externally.choice);
    t.add(&bound.options_open_externally.question);
    t.add(&bound.options_suggested_tags_slot.editor);
    t.add(&bound.options_suggested_tags_slot.tags);
    t.add(&bound.options_suggested_tags_slot.weights);
    t.add(&bound.services_editor.manage);
    t.add(&bound.services_editor.edit);
    // (the child editors first: a child left shown outlives its parent)
    let sessions = bound.network_sessions.clone();
    t.push(move || sessions.retire_children());
    t.add(&bound.network_sessions.browser);
    t.add(&bound.network_sessions.cookies);
    t.add(&bound.network_sessions.headers);
    let network = bound.network_data.clone();
    t.push(move || network.close());
    let search_or = bound.search_or.clone();
    t.push(move || search_or.retire());
    let favourite_or = bound.favourites.search_or.clone();
    t.push(move || favourite_or.retire());
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
