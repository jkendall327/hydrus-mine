//! The main window's "selection tags" box (the reference's
//! `ListBoxTagsMediaSidebar`): its presses, double-clicks and keys, its
//! middle-click searches and its right-click menu, opened in the window's
//! shared tag menu. The selection is the current page's
//! ([`SearchPage::tag_selection`]); the list's own menu entries come back as
//! [`Command`]s, done here on that page.
use crate::{MainWindow, SearchPage, SelectionTagsBox};
use hydrus_core::search::predicate::Predicate;
use hydrus_gui_model::selection_tags::{self, Command, Entered, Place};
use hydrus_gui_model::tag_list_box::{Key, Outcome};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{cell::RefCell, rc::Rc};

/// Show the current page's selection in the box.
pub(crate) fn show(window: &MainWindow, page: &SearchPage) {
    window
        .global::<SelectionTagsBox<'_>>()
        .set_selected(ModelRc::new(VecModel::from(page.tag_selection().mask())));
}

fn tag_predicates(tags: &[String]) -> Vec<Predicate> {
    tags.iter()
        .filter_map(|t| hydrus_core::Tag::new(t))
        .map(|tag| Predicate::Tag {
            tag,
            inclusive: true,
        })
        .collect()
}

/// Wire the box. `launch` opens search pages (as middle-clicks do);
/// `menu` is the window's shared tag menu once made.
#[allow(clippy::too_many_lines)] // (one list's callbacks)
pub(crate) fn bind(
    window: &MainWindow,
    page: impl Fn() -> Rc<RefCell<SearchPage>> + Clone + 'static,
    shown: impl Fn(bool) + Clone + 'static,
    menu: crate::active_predicates::MenuSlot,
    launch: crate::active_predicates::Launch,
    or_slot: &crate::search_or_window::Slot,
    active: &Rc<dyn Fn() -> bool>,
) {
    let tags = window.global::<SelectionTagsBox<'_>>();
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let active = active.clone();
        move || {
            active()
                && weak
                    .upgrade()
                    .is_some_and(|w| crate::active_predicates::permits_input(&w))
        }
    });
    // what the list's own menu entries do, for the tags selected as it opened
    let command: Rc<dyn Fn(Command, Vec<String>)> = Rc::new({
        let page = page.clone();
        let shown = shown.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        let or_slot = or_slot.clone();
        let active = active.clone();
        move |command, tags| {
            if !valid() {
                return;
            }
            let current = page();
            match command {
                Command::SelectAll | Command::SelectAny => {
                    let files = current
                        .borrow()
                        .files_with_tags(&tags, command == Command::SelectAll);
                    current.borrow_mut().select_files(&files);
                    shown(true);
                }
                Command::HideTags | Command::HideNamespaces => {
                    let (_, slices) =
                        selection_tags::hide_question(&tags, command == Command::HideNamespaces);
                    let view = match current.borrow().tag_display_type() {
                        hydrus_core::tag_presentation::TagDisplayType::SingleMedia => {
                            hydrus_store::tag_display::TagView::SingleMedia
                        }
                        _ => hydrus_store::tag_display::TagView::SelectionList,
                    };
                    let store = current.borrow().store().clone();
                    if let Err(error) = selection_tags::hide(&store, view, &slices) {
                        eprintln!("could not hide tags: {error}");
                    }
                    current.borrow_mut().recount_tags();
                    shown(false);
                }
                Command::Display(display) => {
                    current.borrow_mut().set_tag_display_type(display);
                    shown(false);
                }
                Command::StartOr => {
                    let selected = tag_predicates(&tags);
                    let original = Rc::downgrade(&current);
                    let owner: crate::search_or_window::ValidOwner = Rc::new({
                        let page = page.clone();
                        let active = active.clone();
                        let weak = weak.clone();
                        move || {
                            active()
                                && original.upgrade().is_some_and(|original| {
                                    Rc::ptr_eq(&original, &page())
                                        && original.borrow().lock().is_none()
                                })
                                && weak.upgrade().is_some_and(|w| {
                                    w.window().is_visible() && w.get_question().is_empty()
                                })
                        }
                    });
                    let applied: crate::search_or_window::Applied = Rc::new({
                        let owner = owner.clone();
                        let current = Rc::downgrade(&current);
                        let selected = selected.clone();
                        let shown = shown.clone();
                        move |edited| {
                            if owner()
                                && let Some(current) = current.upgrade()
                            {
                                current.borrow_mut().enter_from_tag_list(&[
                                    Entered {
                                        predicates: selected.clone(),
                                        add: false,
                                        remove: true,
                                    },
                                    Entered {
                                        predicates: edited,
                                        add: true,
                                        remove: false,
                                    },
                                ]);
                                shown(true);
                            }
                        }
                    });
                    let context = hydrus_search::FileSearchContext {
                        location: current.borrow().location().clone(),
                        tags: current.borrow().tag_context().clone(),
                        predicates: selected,
                    };
                    let store = current.borrow().store().clone();
                    match crate::search_or_window::open(
                        store, context, false, &or_slot, owner, applied,
                    ) {
                        Ok(child) => {
                            if let Some(window) = weak.upgrade() {
                                window.set_search_or_open(true);
                            }
                            let weak = weak.clone();
                            let active = active.clone();
                            child.on_closed(move || {
                                if active()
                                    && let Some(window) = weak.upgrade()
                                {
                                    window.set_search_or_open(false);
                                }
                            });
                        }
                        Err(error) => eprintln!("could not start an OR predicate: {error}"),
                    }
                }
                search => {
                    let entered = selection_tags::search_command(search, &tag_predicates(&tags));
                    current.borrow_mut().enter_from_tag_list(&entered);
                    shown(true);
                }
            }
        }
    });
    tags.on_pressed({
        let page = page.clone();
        let shown = shown.clone();
        let valid = valid.clone();
        let weak = window.as_weak();
        move |row, button, ctrl, shift, x, y| {
            if !valid() {
                return;
            }
            let current = page();
            current
                .borrow_mut()
                .tag_selection_mut()
                .press(usize::try_from(row).ok(), ctrl, shift);
            shown(false);
            match button {
                // middle: a new search page for the selection (`eventFilter`)
                2 => {
                    let predicates = current.borrow().tag_selection().middle_click(shift);
                    if !predicates.is_empty() {
                        let location = current.borrow().location().clone();
                        launch(location, vec![predicates], false);
                    }
                }
                // right: the menu (`ShowMenuFromSignal`)
                1 => {
                    let Some(tag_menu) = menu.borrow().clone() else {
                        return;
                    };
                    let entries = {
                        let page = current.borrow();
                        let store = page.store().clone();
                        let advanced = store
                            .read(
                                hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>,
                            )
                            .unwrap_or_default()
                            .0;
                        let defaults: hydrus_store::settings::SearchDefaults =
                            store.read(hydrus_store::settings::get).unwrap_or_default();
                        let place = Place {
                            counts: true,
                            search: page.has_search().then(|| page.active_predicates()),
                            select: true,
                            display: page.tag_display_type(),
                            advanced,
                            location: page.location().clone(),
                            tags: hydrus_search::TagContext::new(defaults.tag_service, true, true),
                        };
                        page.tag_selection()
                            .menu(&store, &place, &page.text_context())
                    };
                    let captured = current.borrow().tag_selection().selected_tags();
                    let command = command.clone();
                    let weak = weak.clone();
                    tag_menu.open_owned(
                        &entries,
                        x,
                        y,
                        Rc::new(move |id| {
                            if weak.upgrade().is_some()
                                && let Some(chosen) = Command::from_id(id)
                            {
                                command(chosen, captured.clone());
                            }
                        }),
                    );
                }
                _ => {}
            }
        }
    });
    tags.on_double_clicked({
        let page = page.clone();
        let shown = shown.clone();
        let valid = valid.clone();
        move |row, ctrl, shift| {
            if !valid() {
                return;
            }
            let current = page();
            let predicates = {
                let mut page = current.borrow_mut();
                if ctrl && let Ok(row) = usize::try_from(row) {
                    page.tag_selection_mut().reselect_for_double_click(row);
                }
                page.tag_selection().activation(ctrl, shift)
            };
            activate(&current, predicates);
            shown(true);
        }
    });
    tags.on_key({
        let page = page.clone();
        let shown = shown.clone();
        let valid = valid.clone();
        move |text, ctrl, shift, rows| {
            if !valid() {
                return false;
            }
            let Some(key) = Key::from_text(&text) else {
                return false;
            };
            let current = page();
            let outcome = current.borrow_mut().tag_selection_mut().key(
                key,
                ctrl,
                shift,
                usize::try_from(rows).unwrap_or(1),
            );
            match outcome {
                Outcome::Ignored => return false,
                Outcome::Taken => shown(false),
                Outcome::Copy(text) => {
                    if !text.is_empty() {
                        crate::copy_to_clipboard(&text);
                    }
                }
                Outcome::Activate => {
                    let predicates = current.borrow().tag_selection().activation(ctrl, shift);
                    activate(&current, predicates);
                    shown(true);
                }
            }
            true
        }
    });
}

/// Put the activated predicates in the page's search, as the list's
/// `_Activate` emits them (each one there taken out, each one not put in).
fn activate(page: &Rc<RefCell<SearchPage>>, predicates: Vec<Predicate>) {
    if predicates.is_empty() {
        return;
    }
    page.borrow_mut().enter_from_tag_list(&[Entered {
        predicates,
        add: true,
        remove: true,
    }]);
}
