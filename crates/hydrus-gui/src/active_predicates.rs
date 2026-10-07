//! Active-list selection and captured commands, with page-owned edit children.
use crate::{ActivePredicateAction, MainWindow, PredicateEditorWindow, SearchPage};
use hydrus_gui_model::{
    active_predicates::{
        self, Command,
        routes::{self, Route},
    },
    list_selection::ListSelection,
    predicate_editors::{Context, Editor},
};
use hydrus_search::Predicate;
use slint::{ComponentHandle as _, Model as _, ModelRc, VecModel};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(crate) type Launch = Rc<dyn Fn(hydrus_search::LocationContext, Vec<Vec<Predicate>>, bool)>;

#[derive(Clone, Copy)]
enum Action {
    Search(Command),
    Route(Route),
}
impl Action {
    fn id(self) -> i32 {
        match self {
            Self::Search(command) => command.id(),
            Self::Route(route) => route.id(),
        }
    }
    fn group(self) -> i32 {
        match self {
            Self::Search(_) => 2,
            Self::Route(route) => route.group(),
        }
    }
}

#[derive(Default)]
struct State {
    page: Weak<RefCell<SearchPage>>,
    predicates: Vec<Predicate>,
    selection: ListSelection<usize>,
    menu: Vec<(Action, String)>,
    captured: Vec<Predicate>,
}
impl State {
    fn refresh(&mut self, page: &Rc<RefCell<SearchPage>>) {
        if !self
            .page
            .upgrade()
            .is_some_and(|old| Rc::ptr_eq(&old, page))
            || self.predicates != page.borrow().active_predicates()
        {
            self.page = Rc::downgrade(page);
            self.predicates = page.borrow().active_predicates().to_vec();
            self.selection = ListSelection::default();
            self.menu.clear();
            self.captured.clear();
        }
    }
    fn selected(&self) -> Vec<Predicate> {
        self.predicates
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selection.is_selected(*i))
            .map(|(_, p)| p.clone())
            .collect()
    }
    fn show(&self, window: &MainWindow) {
        window.set_active_predicate_selected(ModelRc::new(VecModel::from(
            (0..self.predicates.len())
                .map(|i| self.selection.is_selected(i))
                .collect::<Vec<_>>(),
        )));
        let actions = self
            .menu
            .iter()
            .map(|(command, label)| ActivePredicateAction {
                id: command.id(),
                group: command.group(),
                label: label.as_str().into(),
            })
            .collect::<Vec<_>>();
        let group = |id| {
            ModelRc::new(VecModel::from(
                actions
                    .iter()
                    .filter(|a| a.group == id)
                    .cloned()
                    .collect::<Vec<_>>(),
            ))
        };
        window.set_active_predicate_copy_menu(group(0));
        window.set_active_predicate_open_menu(group(1));
        window.set_active_predicate_search_menu(group(2));
        window.set_active_predicate_menu(ModelRc::new(VecModel::from(actions)));
    }
}
fn context(page: &SearchPage) -> Context {
    let snapshot = page.store().snapshot();
    Context::new(
        &snapshot.services,
        snapshot
            .url_classes
            .settings()
            .url_classes
            .iter()
            .filter(|c| c.should_be_associated_with_files)
            .map(|c| c.name.clone())
            .collect(),
        hydrus_search::Clock::system().today(),
    )
}
fn permits_input(window: &MainWindow) -> bool {
    window.window().is_visible()
        && window.get_question().is_empty()
        && !window.get_search_or_open()
        && window.get_chooser_labels().row_count() == 0
}

/// Install the active list without owning the MainWindow or a page back-edge.
pub(crate) fn bind(
    window: &MainWindow,
    slot: &Rc<RefCell<Option<PredicateEditorWindow>>>,
    or_slot: &crate::search_or_window::Slot,
    page: impl Fn() -> Rc<RefCell<SearchPage>> + Clone + 'static,
    shown: impl Fn(bool) + Clone + 'static,
    active: Rc<dyn Fn() -> bool>,
    launch: Launch,
) {
    let state = Rc::new(RefCell::new(State::default()));
    let binding_active = active.clone();
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let page = page.clone();
        let slot = Rc::downgrade(slot);
        move || {
            active()
                && weak.upgrade().is_some_and(|w| permits_input(&w))
                && slot.upgrade().is_some_and(|slot| slot.borrow().is_none())
                && page().borrow().lock().is_none()
                && page().borrow().note().is_none()
        }
    });
    window.on_active_predicates_refreshed({
        let weak = window.as_weak();
        let state = state.clone();
        let page = page.clone();
        let slot = Rc::downgrade(slot);
        let active = binding_active.clone();
        let or_slot = or_slot.clone();
        move || {
            if !active() {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let current = page();
                let changed = {
                    let state = state.borrow();
                    !state
                        .page
                        .upgrade()
                        .is_some_and(|old| Rc::ptr_eq(&old, &current))
                        || state.predicates != current.borrow().active_predicates()
                };
                if changed {
                    crate::search_or_window::cancel(&or_slot);
                    let child = slot.upgrade().and_then(|slot| {
                        slot.borrow()
                            .as_ref()
                            .map(slint::ComponentHandle::clone_strong)
                    });
                    if let Some(child) = child {
                        child.invoke_cancel();
                    }
                }
                let mut state = state.borrow_mut();
                state.refresh(&current);
                state.show(&window);
            }
        }
    });
    window.on_active_predicate_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        let valid = valid.clone();
        let page = page.clone();
        move |i, ctrl, shift| {
            if !valid() {
                return;
            }
            let Ok(i) = usize::try_from(i) else {
                return;
            };
            let mut state = state.borrow_mut();
            state.refresh(&page());
            let order = (0..state.predicates.len()).collect::<Vec<_>>();
            state.selection.click(&order, i, ctrl, shift);
            if let Some(window) = weak.upgrade() {
                state.show(&window);
            }
        }
    });
    let execute: Rc<dyn Fn(Command, Vec<Predicate>)> = Rc::new({
        let weak = window.as_weak();
        let page = page.clone();
        let shown = shown.clone();
        let valid = valid.clone();
        let slot = slot.clone();
        let or_slot = or_slot.clone();
        move |command, selected| {
            if !valid() || selected.is_empty() {
                return;
            }
            let current = page();
            if !selected
                .iter()
                .all(|p| current.borrow().active_predicates().contains(p))
            {
                return;
            }
            if command == Command::StartOr
                || (command == Command::Edit
                    && selected.len() == 1
                    && matches!(selected[0], Predicate::Or(_)))
            {
                let original = Rc::downgrade(&current);
                let owner: crate::search_or_window::ValidOwner = Rc::new({
                    let page = page.clone();
                    let weak = weak.clone();
                    let selected = selected.clone();
                    let active = binding_active.clone();
                    move || {
                        active()
                            && original.upgrade().is_some_and(|original| {
                                Rc::ptr_eq(&original, &page())
                                    && original.borrow().lock().is_none()
                                    && selected
                                        .iter()
                                        .all(|p| original.borrow().active_predicates().contains(p))
                            })
                            && weak.upgrade().is_some_and(|w| {
                                w.window().is_visible()
                                    && w.get_question().is_empty()
                                    && w.get_chooser_labels().row_count() == 0
                            })
                    }
                });
                let applied: crate::search_or_window::Applied = Rc::new({
                    let owner = owner.clone();
                    let store = current.borrow().store().clone();
                    let current = Rc::downgrade(&current);
                    let selected = selected.clone();
                    let shown = shown.clone();
                    move |edited| {
                        if owner()
                            && let Some(current) = current.upgrade()
                        {
                            let recent_values = edited.clone();
                            if let Err(error) = store.write(move |tx| {
                                let mut recent: hydrus_core::search::recent::RecentPredicates =
                                    hydrus_store::settings::get(tx.conn())?;
                                recent.push_all(&recent_values);
                                hydrus_store::settings::set(tx.conn(), &recent)
                            }) {
                                eprintln!("could not keep edited OR predicates: {error}");
                            }
                            current
                                .borrow_mut()
                                .edit_active_predicates(&selected, &edited);
                            shown(true);
                        }
                    }
                });
                let context = hydrus_search::FileSearchContext {
                    location: current.borrow().location().clone(),
                    tags: current.borrow().tag_context().clone(),
                    predicates: if command == Command::Edit {
                        if let Predicate::Or(terms) = &selected[0] {
                            terms.clone()
                        } else {
                            Vec::new()
                        }
                    } else {
                        selected.clone()
                    },
                };
                let store = current.borrow().store().clone();
                match crate::search_or_window::open(store, context, false, &or_slot, owner, applied)
                {
                    Ok(child) => {
                        if let Some(window) = weak.upgrade() {
                            window.set_search_or_open(true);
                        }
                        let weak = weak.clone();
                        let active = binding_active.clone();
                        child.on_closed(move || {
                            if active()
                                && let Some(window) = weak.upgrade()
                            {
                                window.set_search_or_open(false);
                            }
                        });
                    }
                    Err(error) => eprintln!("could not edit the active OR predicate: {error}"),
                }
                return;
            }
            if command != Command::Edit {
                current
                    .borrow_mut()
                    .active_predicate_command(&selected, command);
                shown(true);
                return;
            }
            let context = context(&current.borrow());
            let editor = (selected.len() == 1)
                .then(|| Editor::existing(&selected[0], &context))
                .flatten();
            let editable = selected.iter().any(|p| {
                Editor::existing(p, &context).is_some()
                    || matches!(
                        p,
                        Predicate::Tag { .. }
                            | Predicate::Namespace { .. }
                            | Predicate::Wildcard { .. }
                            | Predicate::Or(_)
                    )
            });
            let inverses = active_predicates::inverses(&selected, &current.borrow().text_context());
            if !editable && inverses.len() == 1 {
                let edited = selected
                    .iter()
                    .map(|p| {
                        p.inverse(&|s| {
                            hydrus_search::entry::is_incdec(s, &current.borrow().text_context())
                        })
                        .unwrap_or_else(|| p.clone())
                    })
                    .collect::<Vec<_>>();
                current
                    .borrow_mut()
                    .edit_active_predicates(&selected, &edited);
                shown(true);
                return;
            }
            let Some(editor) = editor
                .or_else(|| Editor::mixed(&selected, &context, &current.borrow().text_context()))
            else {
                return;
            };
            let store = current.borrow().store().clone();
            let text = current.borrow().text_context();
            let owner: Rc<dyn Fn() -> bool> = Rc::new({
                let original = Rc::downgrade(&current);
                let page = page.clone();
                let weak = weak.clone();
                let selected = selected.clone();
                let active = binding_active.clone();
                move || {
                    active()
                        && original.upgrade().is_some_and(|original| {
                            Rc::ptr_eq(&original, &page())
                                && original.borrow().lock().is_none()
                                && selected
                                    .iter()
                                    .all(|p| original.borrow().active_predicates().contains(p))
                        })
                        && weak.upgrade().is_some_and(|w| permits_input(&w))
                }
            });
            let chosen: Rc<dyn Fn(Vec<Predicate>)> = Rc::new({
                let current = Rc::downgrade(&current);
                let owner = owner.clone();
                let shown = shown.clone();
                move |edited| {
                    if owner()
                        && let Some(current) = current.upgrade()
                    {
                        current
                            .borrow_mut()
                            .edit_active_predicates(&selected, &edited);
                        shown(true);
                    }
                }
            });
            if let Err(error) = crate::predicate_editor_window::open(
                &slot,
                &store,
                editor,
                context,
                text,
                chosen,
                Some(owner),
            ) {
                eprintln!("could not edit the active predicate: {error}");
            }
        }
    });
    window.on_active_predicate_activated({
        let state = state.clone();
        let page = page.clone();
        let execute = execute.clone();
        let valid = valid.clone();
        move |ctrl, shift| {
            if !valid() {
                return;
            }
            let selected = {
                let mut state = state.borrow_mut();
                state.refresh(&page());
                state.selected()
            };
            execute(
                if shift {
                    Command::Edit
                } else if ctrl {
                    Command::InvertToggle
                } else {
                    Command::Remove
                },
                selected,
            );
        }
    });
    window.on_active_predicate_menu_opened({
        let weak = window.as_weak();
        let state = state.clone();
        let page = page.clone();
        let valid = valid.clone();
        move |i| {
            if !valid() {
                return;
            }
            let Ok(i) = usize::try_from(i) else {
                return;
            };
            let current = page();
            let mut state = state.borrow_mut();
            state.refresh(&current);
            if i >= state.predicates.len() {
                return;
            }
            if !state.selection.is_selected(i) {
                state.selection.select_only(Some(i));
            }
            state.captured = state.selected();
            let page = current.borrow();
            let context = context(&page);
            let single_or =
                state.captured.len() == 1 && matches!(state.captured[0], Predicate::Or(_));
            let editable = single_or
                || state.captured.iter().any(|p| {
                    Editor::existing(p, &context).is_some()
                        || matches!(
                            p,
                            Predicate::Tag { .. }
                                | Predicate::Namespace { .. }
                                | Predicate::Wildcard { .. }
                        )
                });
            let editable_terms = state
                .captured
                .iter()
                .filter(|p| {
                    single_or
                        || Editor::existing(p, &context).is_some()
                        || matches!(
                            p,
                            Predicate::Tag { .. }
                                | Predicate::Namespace { .. }
                                | Predicate::Wildcard { .. }
                        )
                        || p.inverse(&|s| hydrus_search::entry::is_incdec(s, &page.text_context()))
                            .is_some()
                })
                .cloned()
                .collect::<Vec<_>>();
            state.menu = routes::menu(
                &state.captured,
                page.active_predicates(),
                &page.text_context(),
            )
            .into_iter()
            .map(|(route, label)| (Action::Route(route), label))
            .collect();
            let search_menu = active_predicates::menu(
                &state.captured,
                page.active_predicates(),
                &page.text_context(),
                editable.then_some(editable_terms.as_slice()),
            );
            state.menu.extend(
                search_menu
                    .into_iter()
                    .map(|(command, label)| (Action::Search(command), label)),
            );
            if let Some(window) = weak.upgrade() {
                state.show(&window);
            }
        }
    });
    window.on_active_predicate_menu_chosen({
        let state = state.clone();
        let page = page.clone();
        let valid = valid.clone();
        move |id| {
            if !valid() {
                return;
            }
            let current = page();
            let chosen = {
                let state = state.borrow();
                if !state
                    .page
                    .upgrade()
                    .is_some_and(|old| Rc::ptr_eq(&old, &current))
                    || state.predicates != current.borrow().active_predicates()
                {
                    return;
                }
                state
                    .menu
                    .iter()
                    .find(|(command, _)| command.id() == id)
                    .map(|(command, _)| (*command, state.captured.clone()))
            };
            if let Some((action, selected)) = chosen {
                match action {
                    Action::Search(command) => execute(command, selected),
                    Action::Route(Route::Copy(copy)) => {
                        let text = {
                            let current = current.borrow();
                            routes::copy_text(
                                &selected,
                                current.active_predicates(),
                                copy,
                                &current.text_context(),
                            )
                        };
                        if !text.is_empty() {
                            crate::copy_to_clipboard(&text);
                        }
                    }
                    Action::Route(Route::Open(open)) => {
                        let location = current.borrow().location().clone();
                        launch(
                            location,
                            routes::searches(&selected, open),
                            open == routes::Open::Duplicates,
                        );
                    }
                }
            }
        }
    });
    window.invoke_active_predicates_refreshed();
}
