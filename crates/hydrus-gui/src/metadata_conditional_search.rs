//! The search inside the "test A or B using search terms" comparator's
//! editor ([`hydrus_gui_model::metadata_conditional`]), bound to its
//! `ConditionalSearch` (`ui/metadata_conditional.slint`): typing, keys,
//! choosing results (a system predicate that needs a value opens its editor),
//! removing predicates, paste, empty search and the OR construction, whose
//! "OR" button opens the OR editor with a metadata conditional's results.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use hydrus_gui_model::autocomplete::Navigation;
use hydrus_gui_model::metadata_conditional::{Chosen, ConditionalSearch};
use hydrus_search::{FileSearchContext, Predicate, TextContext};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use crate::ComparatorWindow;

/// The search's child editors while they are open.
#[derive(Clone, Default)]
pub struct Children {
    /// A system predicate's editor.
    pub predicate_editor: Rc<RefCell<Option<crate::PredicateEditorWindow>>>,
    /// The OR editor.
    pub search_or: crate::search_or_window::Slot,
}

impl std::fmt::Debug for Children {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Children")
            .field(
                "predicate_editor",
                &self.predicate_editor.borrow().is_some(),
            )
            .field("search_or", &self.search_or)
            .finish()
    }
}

/// A result as drawn: the OR being built as "OR:" over its terms, indented.
fn drawn(search: &ConditionalSearch, index: usize, label: &str, text: &TextContext) -> String {
    match search.or_terms() {
        Some(terms) if index == 0 && search.suggestions()[0].predicate.is_empty() => {
            std::iter::once("OR:".to_owned())
                .chain(
                    terms
                        .iter()
                        .map(|t| format!("    {}", hydrus_search::predicate_text(t, text))),
                )
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => label.to_owned(),
    }
}

/// Show the search as it is.
pub fn show(window: &ComparatorWindow, search: &ConditionalSearch, text: &TextContext) {
    let strings = |items: Vec<String>| {
        ModelRc::new(VecModel::from(
            items
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        ))
    };
    if window.get_search_typed().as_str() != search.typed() {
        window.set_search_typed(search.typed().into());
    }
    window.set_search_suggestions(strings(
        search
            .suggestions()
            .iter()
            .enumerate()
            .map(|(i, s)| drawn(search, i, &s.label, text))
            .collect(),
    ));
    window.set_search_highlighted(
        search
            .highlighted()
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
    );
    window.set_search_predicates(strings(search.shown()));
    window.set_search_or_active(search.or_terms().is_some());
    window.set_search_or_rewind_visible(search.or_terms().is_some_and(|t| t.len() > 1));
}

/// Bind the window's search to `search`; `changed` runs after each change
/// it makes (to show the comparator's summary).
pub fn bind(
    window: &ComparatorWindow,
    store: &Arc<Store>,
    search: &Rc<RefCell<ConditionalSearch>>,
    text: &TextContext,
    children: &Children,
    changed: Rc<dyn Fn()>,
) {
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let search = search.clone();
        let text = text.clone();
        let children = children.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &search.borrow(), &text);
                window.set_search_blocked(
                    children.predicate_editor.borrow().is_some()
                        || children.search_or.borrow().is_some(),
                );
            }
            changed();
        }
    });
    let blocked = {
        let children = children.clone();
        move || {
            children.predicate_editor.borrow().is_some() || children.search_or.borrow().is_some()
        }
    };
    // (a child applies only while this editor is open)
    let owner: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        move || weak.upgrade().is_some_and(|w| w.window().is_visible())
    });
    window.on_search_edited({
        let weak = window.as_weak();
        let search = search.clone();
        let refresh = refresh.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            search
                .borrow_mut()
                .set_typed(window.get_search_typed().as_str());
            refresh();
        }
    });
    window.on_search_moved({
        let search = search.clone();
        let refresh = refresh.clone();
        move |key| {
            let key = match key {
                -1 => Navigation::Up,
                1 => Navigation::Down,
                2 => Navigation::Home,
                3 => Navigation::End,
                4 => Navigation::PageUp,
                5 => Navigation::PageDown,
                _ => return,
            };
            search.borrow_mut().navigate(key);
            refresh();
        }
    });
    window.on_search_chosen({
        let weak = window.as_weak();
        let store = store.clone();
        let search = search.clone();
        let text = text.clone();
        let children = children.clone();
        let refresh = refresh.clone();
        let owner = owner.clone();
        let blocked = blocked.clone();
        move |index, shift| {
            if blocked() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            let chosen = {
                let mut search = search.borrow_mut();
                search.set_typed(window.get_search_typed().as_str());
                search.choose(usize::try_from(index).ok(), shift)
            };
            if let Chosen::Editor(blank, shift) = chosen {
                open_editor(
                    &store, blank, shift, &search, &text, &children, &owner, &refresh,
                );
            }
            refresh();
        }
    });
    window.on_search_removed({
        let search = search.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move |index| {
            if blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                search.borrow_mut().activate_row(index);
            }
            refresh();
        }
    });
    window.on_search_paste({
        let search = search.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            // (with nothing on the clipboard there is nothing to paste)
            if let Ok(Some(text)) = crate::clipboard_text() {
                search.borrow_mut().paste(&text);
            }
            refresh();
        }
    });
    window.on_search_empty({
        let search = search.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return;
            }
            search.borrow_mut().empty();
            refresh();
        }
    });
    window.on_search_escape({
        let search = search.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        move || {
            if blocked() {
                return false;
            }
            let used = search.borrow_mut().escape();
            refresh();
            used
        }
    });
    window.on_search_or_action({
        let store = store.clone();
        let search = search.clone();
        let children = children.clone();
        let refresh = refresh.clone();
        let owner = owner.clone();
        move |action| {
            if blocked() {
                return;
            }
            match action {
                1 => search.borrow_mut().rewind(),
                2 => search.borrow_mut().cancel_or(),
                3 => {
                    let applied: crate::search_or_window::Applied = Rc::new({
                        let search = search.clone();
                        let refresh = refresh.clone();
                        let owner = owner.clone();
                        move |predicates: Vec<Predicate>| {
                            if owner() {
                                search.borrow_mut().enter_or_editor(predicates);
                                refresh();
                            }
                        }
                    });
                    let opened = crate::search_or_window::open_conditional(
                        store.clone(),
                        FileSearchContext::default(),
                        &children.search_or,
                        owner.clone(),
                        applied,
                    );
                    match opened {
                        Ok(child) => {
                            let refresh = refresh.clone();
                            child.on_closed(move || refresh());
                        }
                        Err(error) => eprintln!("could not open the OR editor: {error}"),
                    }
                }
                _ => {}
            }
            refresh();
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn open_editor(
    store: &Arc<Store>,
    blank: hydrus_gui_model::predicate_editors::Blank,
    shift: bool,
    search: &Rc<RefCell<ConditionalSearch>>,
    text: &TextContext,
    children: &Children,
    owner: &Rc<dyn Fn() -> bool>,
    refresh: &Rc<dyn Fn()>,
) {
    let snapshot = store.snapshot();
    let classes = snapshot
        .url_classes
        .settings()
        .url_classes
        .iter()
        .filter(|c| c.should_be_associated_with_files)
        .map(|c| c.name.clone())
        .collect();
    let context = crate::predicate_editors::Context::new(
        &snapshot.services,
        classes,
        hydrus_search::Clock::system().today(),
    );
    let editor = crate::predicate_editors::Editor::new(blank, &context);
    let chosen: Rc<dyn Fn(Vec<Predicate>)> = Rc::new({
        let search = search.clone();
        let refresh = refresh.clone();
        let owner = owner.clone();
        move |predicates| {
            if owner() {
                search.borrow_mut().apply_editor(predicates, shift);
            }
            refresh();
        }
    });
    if let Err(error) = crate::predicate_editor_window::open(
        &children.predicate_editor,
        store,
        editor,
        context,
        text.clone(),
        chosen,
        Some(owner.clone()),
    ) {
        eprintln!("could not open the predicate editor: {error}");
        return;
    }
    let child = children
        .predicate_editor
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(child) = child {
        // (the editor leaves its slot as it closes)
        let refresh = refresh.clone();
        child.on_closed(move || refresh());
    }
}
