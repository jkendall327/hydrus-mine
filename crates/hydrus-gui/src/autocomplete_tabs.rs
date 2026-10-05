//! Read-tab result selection and immediate favourite edits, owned by the visible page.
use crate::{MainWindow, SearchPage, write_tag_menu::TagMenu};
use hydrus_gui_model::write_tag_menu::{Action, Entry, favourite_entries};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

type Page = Rc<RefCell<SearchPage>>;

pub(crate) fn bind(
    window: &MainWindow,
    page: impl Fn() -> Page + Clone + 'static,
    shown: impl Fn(bool) + Clone + 'static,
) -> Rc<slint::Timer> {
    let store = page().borrow().store().clone();
    let captured: Rc<RefCell<Option<Page>>> = Rc::default();
    let valid = Rc::new({
        let weak = window.as_weak();
        let page = page.clone();
        let captured = captured.clone();
        move || {
            weak.upgrade().is_some_and(|window| {
                let current = page();
                let editable = {
                    let page = current.borrow();
                    page.lock().is_none() && page.note().is_none()
                };
                editable
                    && window.window().is_visible()
                    && !window.get_search_locked()
                    && !window.get_search_or_open()
                    && window.get_note().is_empty()
                    && captured
                        .borrow()
                        .as_ref()
                        .is_none_or(|owner| Rc::ptr_eq(owner, &current))
            })
        }
    });
    let menu = TagMenu::new(
        store.clone(),
        valid.clone(),
        Rc::new(|_| {}),
        Rc::new({
            let page = page.clone();
            let shown = shown.clone();
            move || {
                page().borrow_mut().refresh_autocomplete_tab();
                shown(false);
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |question| {
                if let Some(window) = weak.upgrade() {
                    window.set_tag_menu_question(question.into());
                }
            }
        }),
        Rc::new({
            let weak = window.as_weak();
            move |error| {
                if let Some(window) = weak.upgrade() {
                    window.set_error(error.into());
                }
            }
        }),
    );
    crate::write_tag_menu::bind!(window, menu);
    window.on_suggestion_selection_clicked({
        let weak = window.as_weak();
        let page = page.clone();
        let shown = shown.clone();
        let menu = menu.clone();
        move |index, ctrl, shift| {
            if menu.busy()
                || !weak.upgrade().is_some_and(|window| {
                    window.window().is_visible() && !window.get_search_or_open()
                })
            {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                page().borrow_mut().select_suggestion(index, ctrl, shift);
                shown(false);
            }
        }
    });
    window.on_suggestions_select_all({
        let weak = window.as_weak();
        let page = page.clone();
        let shown = shown.clone();
        let menu = menu.clone();
        move || {
            if menu.busy()
                || !weak.upgrade().is_some_and(|window| {
                    window.window().is_visible() && !window.get_search_or_open()
                })
            {
                return;
            }
            page().borrow_mut().select_all_suggestions();
            shown(false);
        }
    });
    window.on_suggestions_deselected({
        let page = page.clone();
        let shown = shown.clone();
        let menu = menu.clone();
        let weak = window.as_weak();
        move || {
            if menu.busy()
                || !weak.upgrade().is_some_and(|window| {
                    window.window().is_visible() && !window.get_search_or_open()
                })
            {
                return false;
            }
            let handled = page().borrow_mut().deselect_suggestions();
            shown(false);
            handled
        }
    });
    window.on_suggestions_activated({
        let weak = window.as_weak();
        let menu = menu.clone();
        move |shift| {
            if menu.busy() {
                return;
            }
            if let Some(window) = weak
                .upgrade()
                .filter(|window| window.window().is_visible() && !window.get_search_or_open())
            {
                if shift {
                    window.invoke_search_or_action(0);
                } else {
                    window.invoke_search_accepted();
                }
            }
        }
    });
    let updates = watch(
        &store,
        Rc::new({
            let menu = menu.clone();
            let weak = window.as_weak();
            let page = page.clone();
            move || {
                !menu.busy()
                    && {
                        let current = page();
                        let current = current.borrow();
                        current.lock().is_none() && current.note().is_none()
                    }
                    && weak.upgrade().is_some_and(|window| {
                        window.window().is_visible() && !window.get_search_or_open()
                    })
            }
        }),
        Rc::new({
            let page = page.clone();
            let shown = shown.clone();
            move || {
                page().borrow_mut().refresh_autocomplete_tab();
                shown(false);
            }
        }),
    );
    window.on_suggestion_context_menu(move |index, x, y| {
        if menu.busy() {
            return;
        }
        let current = page();
        *captured.borrow_mut() = Some(current.clone());
        if !valid() || menu.busy() {
            return;
        }
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut current = current.borrow_mut();
        if !current
            .autocomplete()
            .selected()
            .get(index)
            .copied()
            .unwrap_or(false)
        {
            current.select_suggestion(index, false, false);
        }
        let selected = current.autocomplete().selected_suggestions();
        let tags: Vec<_> = selected
            .iter()
            .filter_map(|row| {
                if row.editor.is_none() {
                    hydrus_core::Tag::new(&row.predicate).map(|tag| tag.as_str().to_owned())
                } else {
                    None
                }
            })
            .collect();
        let mut entries = vec![Entry::Item(
            "copy selected tags".into(),
            Action::Copy(tags.join("\n")),
        )];
        if let [tag] = tags.as_slice() {
            entries.push(Entry::Menu(
                "favourites".into(),
                favourite_entries(&store, tag),
            ));
        }
        drop(current);
        shown(false);
        menu.open(&entries, x, y);
    });
    updates
}

/// One owner-held settings subscription, shared by embedded read and detached write panes.
/// Only a changed favourites list or children cap triggers a refresh; queries and drafts remain owned.
pub(crate) fn watch(
    store: &Arc<hydrus_store::Store>,
    valid: Rc<dyn Fn() -> bool>,
    changed: Rc<dyn Fn()>,
) -> Rc<slint::Timer> {
    let timer = Rc::new(slint::Timer::default());
    let store = store.clone();
    let revision = Cell::new(store.snapshot().revision);
    let read = |store: &hydrus_store::Store| {
        store.read(|conn| {
            let favourites: hydrus_store::settings::FavouriteTags =
                hydrus_store::settings::get(conn)?;
            let tabs: hydrus_store::settings::TagAutocompleteTabs =
                hydrus_store::settings::get(conn)?;
            Ok((favourites.0, tabs.children_limit))
        })
    };
    let mut previous = read(&store).ok();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        move || {
            if !valid() {
                return;
            }
            let current = store.snapshot().revision;
            if revision.get() == current {
                return;
            }
            if let Ok(settings) = read(&store) {
                revision.set(current);
                if previous.as_ref() != Some(&settings) {
                    previous = Some(settings);
                    changed();
                }
            }
        },
    );
    timer
}
