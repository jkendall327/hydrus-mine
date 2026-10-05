//! Staged empty-OR and advanced Boolean input children of read autocomplete.
use crate::{PredicateEditorWindow, SearchOrWindow, SearchPage};
use hydrus_gui_model::search_or::advanced;
use hydrus_search::{FileSearchContext, Predicate, TextContext};
use hydrus_store::{Store, settings};
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Inspection belongs to the same explicit owner as the dialog and its children.
#[derive(Clone, Default)]
pub struct Slot {
    window: Rc<RefCell<Option<SearchOrWindow>>>,
    nested: Rc<RefCell<Option<Self>>>,
    pub system: Rc<RefCell<Option<PredicateEditorWindow>>>,
}
impl std::fmt::Debug for Slot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Slot")
            .field("window_open", &self.window.borrow().is_some())
            .field("nested_open", &self.child().is_some())
            .field("system_open", &self.system.borrow().is_some())
            .finish_non_exhaustive()
    }
}
impl std::ops::Deref for Slot {
    type Target = RefCell<Option<SearchOrWindow>>;
    fn deref(&self) -> &Self::Target {
        &self.window
    }
}
impl Slot {
    /// The currently open recursive child of this owner, if any.
    pub fn child(&self) -> Option<Self> {
        self.nested
            .borrow()
            .clone()
            .filter(|child| child.borrow().is_some())
    }
}
pub type Applied = Rc<dyn Fn(Vec<Predicate>)>;
pub type ValidOwner = Rc<dyn Fn() -> bool>;
pub fn cancel(slot: &Slot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_cancel();
    }
}
struct State {
    window: slint::Weak<SearchOrWindow>,
    page: RefCell<SearchPage>,
    text: TextContext,
    active: Rc<Cell<bool>>,
    owner: ValidOwner,
    nested: Slot,
    system: Rc<RefCell<Option<PredicateEditorWindow>>>,
    watch_owner: slint::Timer,
}
impl State {
    fn owned(&self) -> bool {
        self.active.get() && (self.owner)()
    }
    fn valid(&self) -> bool {
        self.owned()
            && self
                .window
                .upgrade()
                .is_some_and(|w| w.window().is_visible())
    }
    fn blocked(&self) -> bool {
        self.nested.borrow().is_some() || self.system.borrow().is_some()
    }
}
fn show(window: &SearchOrWindow, state: &State) {
    window.set_blocked(state.blocked());
    if window.get_advanced() {
        let (preview, valid) = advanced::preview(window.get_input().as_str(), &state.text);
        window.set_preview(preview.into());
        window.set_valid(valid);
    } else {
        let page = state.page.borrow();
        let autocomplete = page.autocomplete();
        let colours: hydrus_core::tag_presentation::NamespaceColours =
            page.store().read(settings::get).unwrap_or_default();
        window.set_input(autocomplete.text().into());
        window.set_predicates(ModelRc::new(VecModel::from(
            page.predicates()
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
        )));
        window.set_suggestions(ModelRc::new(VecModel::from(
            autocomplete
                .suggestions()
                .iter()
                .map(|suggestion| {
                    crate::list_text(
                        &suggestion.label,
                        colours.predicate_text(&suggestion.predicate),
                    )
                })
                .collect::<Vec<_>>(),
        )));
        window.set_highlighted(
            autocomplete
                .highlighted()
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1),
        );
        window.set_selected(ModelRc::new(VecModel::from(autocomplete.selected())));
        window.set_tab_index(i32::try_from(autocomplete.tab().index()).unwrap_or(0));
        window.set_or_active(page.or_terms().is_some());
        window.set_or_rewind_visible(page.or_terms().is_some_and(|terms| terms.len() > 1));
        window.set_error(page.error().unwrap_or_default().into());
    }
}
fn system_editor(window: &SearchOrWindow, state: &Rc<State>) {
    let Some((blank, shift)) = state.page.borrow_mut().take_system_editor_wanted() else {
        return;
    };
    let store = state.page.borrow().store().clone();
    let snapshot = store.snapshot();
    let classes = snapshot
        .url_classes
        .settings()
        .url_classes
        .iter()
        .filter(|class| class.should_be_associated_with_files)
        .map(|class| class.name.clone())
        .collect();
    let context = crate::predicate_editors::Context::new(
        &snapshot.services,
        classes,
        hydrus_search::Clock::system().today(),
    );
    let editor = crate::predicate_editors::Editor::new(blank, &context);
    let applied: Applied = Rc::new({
        let state = Rc::downgrade(state);
        let weak = window.as_weak();
        move |predicates| {
            let Some(state) = state.upgrade() else {
                return;
            };
            if !state.valid() {
                return;
            }
            state
                .page
                .borrow_mut()
                .apply_system_editor(predicates, shift);
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    if let Err(error) = crate::predicate_editor_window::open(
        &state.system,
        &store,
        editor,
        context,
        state.text.clone(),
        applied,
        Some(Rc::new({
            let state = Rc::downgrade(state);
            move || state.upgrade().is_some_and(|state| state.valid())
        })),
    ) {
        window.set_error(error.into());
    } else {
        let child = state
            .system
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(child) = child {
            window.set_blocked(true);
            let weak = window.as_weak();
            let state = Rc::downgrade(state);
            child.on_closed(move || {
                let Some(state) = state.upgrade() else {
                    return;
                };
                if state.active.get()
                    && let Some(window) = weak.upgrade()
                {
                    show(&window, &state);
                }
            });
        }
    }
}
/// Every child validates its live caller again before applying, including nested editors.
pub fn open(
    store: Arc<Store>,
    context: FileSearchContext,
    advanced_input: bool,
    slot: &Slot,
    owner: ValidOwner,
    applied: Applied,
) -> Result<SearchOrWindow, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = SearchOrWindow::new()?;
    window
        .global::<crate::TagTextHistory<'_>>()
        .on_record(crate::write_tag_history::record);
    window
        .global::<crate::TagTextHistory<'_>>()
        .on_undo(crate::write_tag_history::undo);
    window
        .global::<crate::TagTextHistory<'_>>()
        .on_redo(crate::write_tag_history::redo);
    let theme_store = store.clone();
    let snapshot = store.snapshot();
    let viewing = store.read(settings::get).unwrap_or_default();
    let text = TextContext::from_store(&snapshot.services, &viewing);
    let mode: settings::AdvancedMode = store.read(settings::get).unwrap_or_default();
    window.set_advanced(advanced_input);
    window.set_advanced_visible(mode.0);
    window.set_window_title(
        if advanced_input {
            "parse advanced predicate string"
        } else {
            "edit OR predicate"
        }
        .into(),
    );
    let state = Rc::new(State {
        window: window.as_weak(),
        page: RefCell::new(SearchPage::restored(
            store,
            context,
            false,
            None,
            Vec::new(),
        )),
        text,
        active: Rc::new(Cell::new(true)),
        owner,
        nested: Slot::default(),
        system: slot.system.clone(),
        watch_owner: slint::Timer::default(),
    });
    crate::gui_colours::bind(
        window.global::<crate::Theme<'_>>(),
        &theme_store,
        state.active.clone(),
    );
    *slot.nested.borrow_mut() = Some(state.nested.clone());
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let nested = Rc::downgrade(&slot.nested);
        let slot = Rc::downgrade(&slot.window);
        let state = state.clone();
        move || {
            state.watch_owner.stop();
            if !state.active.replace(false) {
                return;
            }
            cancel(&state.nested);
            let system = state.system.borrow_mut().take();
            if let Some(system) = system {
                system.invoke_cancel();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
                window.invoke_closed();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(nested) = nested.upgrade() {
                nested.borrow_mut().take();
            }
        }
    });
    window.on_edited({
        let state = state.clone();
        let weak = window.as_weak();
        move |text| {
            if !state.valid() || state.blocked() {
                return;
            }
            if let Some(window) = weak.upgrade() {
                if window.get_advanced() {
                    window.set_input(text);
                } else {
                    state.page.borrow_mut().type_text(text.as_str());
                }
                show(&window, &state);
            }
        }
    });
    window.on_enter({
        let state = state.clone();
        let weak = window.as_weak();
        move |shift| {
            if !state.valid() || state.blocked() {
                return;
            }
            state.page.borrow_mut().enter_or(shift);
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
                system_editor(&window, &state);
            }
        }
    });
    window.on_chosen({
        let state = state.clone();
        let weak = window.as_weak();
        move |index| {
            if !state.valid() || state.blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                state.page.borrow_mut().choose(index);
            }
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
                system_editor(&window, &state);
            }
        }
    });
    window.on_remove({
        let state = state.clone();
        let weak = window.as_weak();
        move |index| {
            if !state.valid() || state.blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                state.page.borrow_mut().remove_predicate(index);
            }
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    window.on_selection_clicked({
        let state = state.clone();
        let weak = window.as_weak();
        move |index, ctrl, shift| {
            if !state.valid() || state.blocked() {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                state
                    .page
                    .borrow_mut()
                    .select_suggestion(index, ctrl, shift);
            }
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    window.on_deselect({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if !state.valid() || state.blocked() {
                return false;
            }
            let handled = state.page.borrow_mut().deselect_suggestions();
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
            handled
        }
    });
    window.on_select_all({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if !state.valid() || state.blocked() {
                return;
            }
            state.page.borrow_mut().select_all_suggestions();
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    window.on_tab_chosen({
        let state = state.clone();
        let weak = window.as_weak();
        move |index| {
            if !state.valid() || state.blocked() {
                return;
            }
            state.page.borrow_mut().set_autocomplete_tab(
                hydrus_gui_model::write_autocomplete::Tab::from_index(
                    usize::try_from(index).unwrap_or(0),
                ),
            );
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    window.on_move_highlight({
        let state = state.clone();
        let weak = window.as_weak();
        move |by| {
            if !state.valid() || state.blocked() {
                return;
            }
            state.page.borrow_mut().move_highlight(by as isize);
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    window.on_fetch({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if !state.valid() || state.blocked() {
                return;
            }
            state.page.borrow_mut().fetch_autocomplete();
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
        }
    });
    window.on_escape({
        let state = state.clone();
        let weak = window.as_weak();
        move || {
            if !state.valid() || state.blocked() {
                return false;
            }
            let handled = state.page.borrow_mut().escape_or();
            if let Some(window) = weak.upgrade() {
                show(&window, &state);
            }
            handled
        }
    });
    window.on_or_action({
        let state = state.clone();
        let weak = window.as_weak();
        move |action| {
            if !state.valid() || state.blocked() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            match action {
                1 | 2 => {
                    state.page.borrow_mut().change_or_draft(action == 1);
                    show(&window, &state);
                }
                3 | 4 => {
                    let context = FileSearchContext {
                        location: state.page.borrow().location().clone(),
                        tags: state.page.borrow().tag_context().clone(),
                        predicates: Vec::new(),
                    };
                    let owner: ValidOwner = Rc::new({
                        let state = Rc::downgrade(&state);
                        move || state.upgrade().is_some_and(|state| state.valid())
                    });
                    let applied: Applied = Rc::new({
                        let state = Rc::downgrade(&state);
                        let weak = window.as_weak();
                        move |predicates| {
                            let Some(state) = state.upgrade() else {
                                return;
                            };
                            if !state.valid() {
                                return;
                            }
                            state.page.borrow_mut().apply_or_editor(predicates);
                            if let Some(window) = weak.upgrade() {
                                show(&window, &state);
                            }
                        }
                    });
                    match open(
                        state.page.borrow().store().clone(),
                        context,
                        action == 4,
                        &state.nested,
                        owner,
                        applied,
                    ) {
                        Ok(child) => {
                            let weak = window.as_weak();
                            let state = Rc::downgrade(&state);
                            child.on_closed(move || {
                                let Some(state) = state.upgrade() else {
                                    return;
                                };
                                if state.active.get()
                                    && let Some(window) = weak.upgrade()
                                {
                                    window.set_blocked(false);
                                }
                            });
                            window.set_blocked(true);
                        }
                        Err(error) => window.set_error(error.to_string().into()),
                    }
                }
                _ => {}
            }
        }
    });
    window.on_apply({
        let state = state.clone();
        let weak = window.as_weak();
        let close = close.clone();
        move || {
            if !state.owned() {
                close();
                return;
            }
            if !state.valid() || state.blocked() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            if !window.window().is_visible() {
                return;
            }
            let result = if window.get_advanced() {
                advanced::predicates(window.get_input().as_str(), &state.text)
            } else {
                let mut predicates = state
                    .page
                    .borrow()
                    .favourite_to_save()
                    .unwrap()
                    .search
                    .predicates;
                if predicates.len() > 1 {
                    predicates = vec![Predicate::Or(predicates)];
                }
                Ok(predicates)
            };
            match result {
                Ok(predicates) if !window.get_advanced() || !predicates.is_empty() => {
                    close();
                    applied(predicates);
                }
                _ => window.set_error(
                    "Please enter a string that parses into a set of search rules.".into(),
                ),
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    state.watch_owner.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        {
            let state = Rc::downgrade(&state);
            let weak = window.as_weak();
            move || {
                if let Some(state) = state.upgrade()
                    && state.active.get()
                    && !(state.owner)()
                    && let Some(window) = weak.upgrade()
                {
                    window.invoke_cancel();
                }
            }
        },
    );
    show(&window, &state);
    *slot.borrow_mut() = Some(window.clone_strong());
    window.show()?;
    Ok(window)
}
