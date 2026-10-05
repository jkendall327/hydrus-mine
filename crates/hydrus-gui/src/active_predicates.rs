//! Active-list selection and captured commands, with page-owned edit children.
use crate::{ActivePredicateAction, MainWindow, PredicateEditorWindow, SearchPage};
use hydrus_gui_model::{
    active_predicates::{self, Command},
    list_selection::ListSelection,
    predicate_editors::{Context, Editor},
};
use hydrus_search::Predicate;
use slint::{ComponentHandle as _, Model as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

#[derive(Default)]
struct State {
    page: Weak<RefCell<SearchPage>>,
    predicates: Vec<Predicate>,
    selection: ListSelection<usize>,
    menu: Vec<(Command, String)>,
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
        window.set_active_predicate_menu(ModelRc::new(VecModel::from(
            self.menu
                .iter()
                .map(|(command, label)| ActivePredicateAction {
                    id: command.id(),
                    label: label.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )));
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
    page: impl Fn() -> Rc<RefCell<SearchPage>> + Clone + 'static,
    shown: impl Fn(bool) + Clone + 'static,
    active: Rc<Cell<bool>>,
) {
    let state = Rc::new(RefCell::new(State::default()));
    let binding_active = active.clone();
    let valid: Rc<dyn Fn() -> bool> = Rc::new({
        let weak = window.as_weak();
        let page = page.clone();
        let slot = Rc::downgrade(slot);
        move || {
            active.get()
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
        move || {
            if !active.get() {
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
                    active.get()
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
            let editable = state.captured.iter().any(|p| {
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
                    Editor::existing(p, &context).is_some()
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
            state.menu = active_predicates::menu(
                &state.captured,
                page.active_predicates(),
                &page.text_context(),
                editable.then_some(editable_terms.as_slice()),
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
            if let Some((command, selected)) = chosen {
                execute(command, selected);
            }
        }
    });
    window.invoke_active_predicates_refreshed();
}
