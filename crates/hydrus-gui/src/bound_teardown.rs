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
//!
//! Not covered yet: child slots a dialog makes for itself when it opens, which
//! no field of the binding reaches: the options window's and the tag migration
//! window's `location_slot` (and `locations_window`'s close holds its slot
//! strongly, a cycle of its own), and a favourite editor's `child` in
//! `favourites_window`. A child left shown when the binding goes outlives its
//! parent there. The network session browsers' children are covered by
//! registering them with their owner (`SlotsOwner::retire`).

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
            // (and what the closures hold, strong clones of slots whose own
            // drops act on their windows, is leaked rather than dropped)
            std::mem::forget(std::mem::take(self.0.get_mut()));
            return;
        }
        for forget in self.0.get_mut().drain(..) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(forget));
        }
    }
}

/// Take the window out of `slot` and hide it.
pub(crate) fn forget<T: slint::ComponentHandle>(slot: &Rc<RefCell<Option<T>>>) {
    let window = slot.borrow_mut().take();
    if let Some(window) = window {
        let _ = window.hide();
    }
}

// (Each walker destructures its type without `..`, so a field added later
// fails to compile here until it is handled. Types with private fields
// walk themselves, in their own modules, with the helpers below.)

pub(crate) fn retire_png(s: &crate::png_export_window::Slots) {
    let crate::png_export_window::Slots(window) = s;
    forget(window);
}

pub(crate) fn retire_exchange(s: &crate::downloader_interchange_window::Slots) {
    let crate::downloader_interchange_window::Slots(window, png) = s;
    forget(window);
    retire_png(png);
}

fn retire_formula(s: &crate::formula_window::Slots) {
    let crate::formula_window::Slots {
        exchange,
        formula,
        rule,
        child,
        strings,
        png,
    } = s;
    retire_exchange(exchange);
    forget(formula);
    forget(rule);
    let child = child.borrow_mut().take();
    if let Some(child) = child {
        retire_formula(&child);
        drop(child);
    }
    strings.retire();
    retire_png(png);
}

fn retire_parsers(s: &crate::parser_editors_window::Slots) {
    let crate::parser_editors_window::Slots {
        exchange,
        list,
        links,
        picker,
        page,
        content,
        formula,
        child,
    } = s;
    retire_exchange(exchange);
    forget(list);
    forget(links);
    forget(picker);
    forget(page);
    forget(content);
    retire_formula(formula);
    let child = child.borrow_mut().take();
    if let Some(child) = child {
        retire_parsers(&child);
        drop(child);
    }
}

fn retire_sidecars(s: &crate::sidecars_window::Slots) {
    let crate::sidecars_window::Slots {
        routers,
        exchange,
        router,
        node,
        object_name,
        strings,
        formula,
        test_objects: _,
    } = s;
    forget(routers);
    retire_exchange(exchange);
    forget(router);
    forget(node);
    forget(object_name);
    strings.retire();
    retire_formula(formula);
}

fn retire_cookies(s: &crate::login_cookies_window::Slots) {
    let crate::login_cookies_window::Slots { window, strings } = s;
    forget(window);
    strings.retire();
}

fn retire_login_step(s: &crate::login_step_window::Slots) {
    let crate::login_step_window::Slots {
        step,
        parsers,
        exchange,
        cookies,
        argument,
    } = s;
    forget(step);
    retire_parsers(parsers);
    retire_exchange(exchange);
    retire_cookies(cookies);
    forget(argument);
}

fn retire_login_domains(s: &crate::login_domains_window::Slots) {
    let crate::login_domains_window::Slots {
        domains,
        credentials,
        run,
        entry,
        status: _,
    } = s;
    let crate::login_domain_entry::Slots {
        window,
        credentials: entry_credentials,
    } = entry;
    forget(domains);
    forget(credentials);
    run.stop();
    forget(window);
    forget(entry_credentials);
}

fn retire_login(s: &crate::login_workflows_window::Slots) {
    let crate::login_workflows_window::Slots {
        scripts,
        domains,
        script,
        step,
        run,
        test_control,
        result,
        test_domain,
        cookies,
        example,
        definition,
        credentials,
        strings,
        exchange,
    } = s;
    forget(scripts);
    retire_login_domains(domains);
    forget(script);
    retire_login_step(step);
    run.stop();
    let control = test_control.borrow_mut().take();
    drop(control);
    forget(result);
    forget(test_domain);
    retire_cookies(cookies);
    forget(example);
    forget(definition);
    forget(credentials);
    strings.retire();
    retire_exchange(exchange);
}

fn retire_downloader_definitions(s: &crate::downloader_definitions_window::Slots) {
    let crate::downloader_definitions_window::Slots {
        class_exchange,
        gug_exchange,
        classes,
        gugs,
        class_edit,
        gug_edit,
        rule,
        strings,
        domains,
    } = s;
    let crate::domain_mask_entry::Slots { entry, favourites } = domains;
    retire_exchange(class_exchange);
    retire_exchange(gug_exchange);
    forget(classes);
    forget(gugs);
    forget(class_edit);
    forget(gug_edit);
    forget(rule);
    strings.retire();
    forget(entry);
    forget(favourites);
}

fn retire_simple_formulae(s: &crate::simple_formulae_window::Slots) {
    let crate::simple_formulae_window::Slots {
        exchange,
        list,
        formula,
        png,
        pngs,
    } = s;
    let crate::png_export_window::PngsSlots(pngs) = pngs;
    retire_exchange(exchange);
    forget(list);
    retire_formula(formula);
    retire_png(png);
    forget(pngs);
}

fn retire_auto_resolution(s: &crate::auto_resolution_rules_window::Slots) {
    let crate::auto_resolution_rules_window::Slots {
        list,
        rule,
        comparators,
        conditional,
        merge_options,
        locations,
        preview_filter,
        open_files,
        png,
    } = s;
    let crate::metadata_conditional_search::Children {
        predicate_editor,
        search_or,
    } = conditional;
    forget(list);
    forget(rule);
    let windows: Vec<_> = comparators.borrow_mut().drain(..).collect();
    for (_, window) in windows {
        let _ = window.hide();
    }
    forget(predicate_editor);
    search_or.retire();
    forget(merge_options);
    forget(locations);
    forget(preview_filter);
    let opener = open_files.borrow_mut().take();
    drop(opener);
    retire_png(png);
}

fn retire_open_externally(s: &crate::options_open_externally::Slots) {
    let crate::options_open_externally::Slots {
        files,
        choice,
        question,
    } = s;
    forget(files);
    forget(choice);
    forget(question);
}

fn retire_suggested_tags(s: &crate::tag_suggestions_window::Slots) {
    let crate::tag_suggestions_window::Slots {
        editor,
        tags,
        weights,
    } = s;
    forget(editor);
    forget(tags);
    forget(weights);
}

fn retire_services_editor(s: &crate::services_editor_window::Slots) {
    let crate::services_editor_window::Slots { manage, edit } = s;
    forget(manage);
    forget(edit);
}

/// Retire a nested slot type's windows with `retire` when the last clone goes.
fn nested<S: Clone + 'static>(t: &Teardown, slots: &S, retire: fn(&S)) {
    let slots = slots.clone();
    t.push(move || retire(&slots));
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
    // the nested slot types reachable from a field of the binding
    nested(t, &bound.export_files.sidecars, retire_sidecars);
    nested(t, &bound.filename_tagging_sidecars, retire_sidecars);
    nested(t, &folders.sidecars, retire_sidecars);
    nested(t, &bound.subscription_exchange, retire_exchange);
    nested(t, &bound.parser_editors, retire_parsers);
    nested(t, &bound.login_workflows, retire_login);
    nested(
        t,
        &bound.downloader_definitions,
        retire_downloader_definitions,
    );
    nested(t, &bound.simple_formulae, retire_simple_formulae);
    nested(t, &bound.auto_resolution, retire_auto_resolution);
    nested(
        t,
        &bound.options_external_calls,
        crate::external_call_window::Slots::retire,
    );
    nested(t, &bound.options_open_externally, retire_open_externally);
    nested(t, &bound.options_suggested_tags_slot, retire_suggested_tags);
    nested(t, &bound.services_editor, retire_services_editor);
    let sessions = bound.network_sessions.clone();
    t.push(move || sessions.retire());
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
