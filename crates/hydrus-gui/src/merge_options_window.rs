//! The duplicate metadata merge options editor, bound
//! (`ui/merge_options.slint`): hydrus-gui-model's [`MergeOptionsEditor`]
//! shown and driven, its select dialogs and questions asked in the
//! window's own panel, a tag service's filter edited in the tag filter
//! editor, and "apply" handing the options back.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::ServiceKey;
use hydrus_core::tag_filter::TagFilter;
use hydrus_store::Store;
use hydrus_store::duplicates::PairRelationship;
use hydrus_store::duplicates::merge::{MergeAction, MergeOptions};

use crate::list_selection::ListSelection;
use crate::merge_options_editor::{
    Choices, FILTER_TITLE, MergeOptionsEditor, NOTE_CONFLICT_LABEL, NOTE_CONFLICTS,
    NOTE_EXTEND_LABEL, RATINGS_NOTE, REMOVE_SELECTED, ROW_LABELS, Service, TITLE,
    note_conflict_label,
};
use crate::{MergeOptionsWindow, TableRow};

/// The window while it is open.
pub type Slot = Rc<RefCell<Option<MergeOptionsWindow>>>;

thread_local! {
    /// The editor opened last, for tests to reach.
    static LAST: RefCell<Option<slint::Weak<MergeOptionsWindow>>> = const { RefCell::new(None) };
}

/// The editor opened last, if it is still open.
pub fn last_opened() -> Option<MergeOptionsWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

const TAGS: usize = 0;
const RATINGS: usize = 1;

/// What the window's panel asks.
#[derive(Debug, Clone)]
enum Asking {
    /// A service to add to a list.
    Service(usize, Choices<ServiceKey>),
    /// A service's action: for one being added, or edited.
    Action {
        list: usize,
        service: ServiceKey,
        choices: Choices<MergeAction>,
        adding: bool,
    },
    Remove(usize, Vec<ServiceKey>),
    Warning(&'static str),
}

struct State {
    editor: MergeOptionsEditor,
    selections: [ListSelection<usize>; 2],
    asking: Option<Asking>,
    note_settings: bool,
}

impl State {
    fn keys(&self, list: usize) -> Vec<ServiceKey> {
        if list == TAGS {
            self.editor.tag_rows().into_iter().map(|(_, k)| k).collect()
        } else {
            self.editor
                .rating_rows()
                .into_iter()
                .map(|(_, k)| k)
                .collect()
        }
    }

    fn selected(&self, list: usize) -> Vec<ServiceKey> {
        let keys = self.keys(list);
        let order: Vec<usize> = (0..keys.len()).collect();
        self.selections[list]
            .in_order(&order)
            .into_iter()
            .filter_map(|i| keys.get(i).cloned())
            .collect()
    }
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn rows(cells: Vec<Vec<String>>, selection: &ListSelection<usize>) -> ModelRc<TableRow> {
    let rows: Vec<TableRow> = cells
        .into_iter()
        .enumerate()
        .map(|(i, cells)| TableRow {
            cells: strings(cells),
            selected: selection.is_selected(i),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn index(i: usize) -> i32 {
    i32::try_from(i).unwrap_or(0)
}

fn show(window: &MergeOptionsWindow, state: &State) {
    let editor = &state.editor;
    window.set_note(editor.note().into());
    window.set_tag_rows(rows(
        editor
            .tag_rows()
            .into_iter()
            .map(|(r, _)| r.to_vec())
            .collect(),
        &state.selections[TAGS],
    ));
    window.set_rating_rows(rows(
        editor
            .rating_rows()
            .into_iter()
            .map(|(r, _)| r.to_vec())
            .collect(),
        &state.selections[RATINGS],
    ));
    window.set_edit_action_shown(editor.edit_action_shown());
    let selected = |list| state.selected(list).len();
    window.set_tag_one_selected(selected(TAGS) == 1);
    window.set_tag_any_selected(selected(TAGS) > 0);
    window.set_rating_one_selected(selected(RATINGS) == 1);
    window.set_rating_any_selected(selected(RATINGS) > 0);
    let choices = [
        editor.archive(),
        editor.file_modified(),
        editor.urls(),
        editor.notes(),
    ];
    let options = |i: usize| strings(choices[i].options.iter().map(|&o| o.to_owned()));
    window.set_archive_options(options(0));
    window.set_archive_index(index(choices[0].value));
    window.set_modified_options(options(1));
    window.set_modified_index(index(choices[1].value));
    window.set_urls_options(options(2));
    window.set_urls_index(index(choices[2].value));
    window.set_notes_options(options(3));
    window.set_notes_index(index(choices[3].value));
    window.set_syncs_enabled(choices[1].enabled);
    window.set_note_settings_enabled(editor.note_settings_enabled());
    window.set_note_settings_open(state.note_settings);
    window.set_asking(state.asking.is_some());
    let Some(asking) = &state.asking else {
        return;
    };
    let (title, message, choices): (&str, &str, Vec<String>) = match asking {
        Asking::Service(_, choices) => (
            choices.title,
            "",
            choices.choices.iter().map(|c| c.0.clone()).collect(),
        ),
        Asking::Action { choices, .. } => (
            choices.title,
            "",
            choices.choices.iter().map(|c| c.0.clone()).collect(),
        ),
        Asking::Remove(..) => (
            "Are you sure?",
            REMOVE_SELECTED,
            vec!["yes".into(), "no".into()],
        ),
        Asking::Warning(warning) => ("warning", warning, vec!["ok".into()]),
    };
    window.set_asking_title(title.into());
    window.set_asking_message(message.into());
    window.set_asking_choices(strings(choices));
}

/// The client's tag and rating services, in its order.
fn services(store: &Store) -> Vec<Service> {
    store
        .snapshot()
        .services
        .all()
        .filter(|s| s.service_type().is_real_tag_service() || s.service_type().is_rating_service())
        .map(|s| Service {
            key: s.key.clone(),
            name: s.name.clone(),
            kind: s.service_type(),
        })
        .collect()
}

/// Open the editor on `options` for `relationship` (`for_custom_action`:
/// a single decision's, whose syncs are never off); "apply" calls
/// `applied`. It forgets itself from `slot` when closed.
pub fn open(
    store: &Arc<Store>,
    relationship: PairRelationship,
    options: &MergeOptions,
    for_custom_action: bool,
    slot: &Slot,
    applied: Rc<dyn Fn(MergeOptions)>,
) -> Result<MergeOptionsWindow, slint::PlatformError> {
    let window = MergeOptionsWindow::new()?;
    window.set_window_title(TITLE.into());
    window.set_ratings_note(RATINGS_NOTE.into());
    window.set_row_labels(strings(ROW_LABELS.iter().map(|&l| l.to_owned())));
    window.set_note_extend_label(NOTE_EXTEND_LABEL.into());
    window.set_note_conflict_label(NOTE_CONFLICT_LABEL.into());
    window.set_note_conflicts(strings(
        NOTE_CONFLICTS
            .iter()
            .map(|&c| note_conflict_label(c).to_owned()),
    ));
    let editor = MergeOptionsEditor::new(relationship, options, for_custom_action, services(store));
    let state = Rc::new(RefCell::new(State {
        editor,
        selections: Default::default(),
        asking: None,
        note_settings: false,
    }));
    let refresh: Rc<dyn Fn()> = {
        let weak = window.as_weak();
        let state = state.clone();
        Rc::new(move || {
            if let Some(window) = weak.upgrade() {
                show(&window, &state.borrow());
            }
        })
    };
    refresh();
    // a tag service's filter, in the tag filter editor: `then` takes it
    let filter_slot: crate::tag_filter_window::Slot = Rc::default();
    let edit_filter = {
        let store = store.clone();
        let filter_slot = filter_slot.clone();
        Rc::new(move |filter: &TagFilter, then: Rc<dyn Fn(TagFilter)>| {
            match crate::tag_filter_window::open(
                &store,
                filter,
                false,
                FILTER_TITLE,
                "",
                &filter_slot,
                then,
            ) {
                Ok(window) => *filter_slot.borrow_mut() = Some(window),
                Err(e) => eprintln!("could not open the tag filter editor: {e}"),
            }
        })
    };
    // a tag service added with its action: its filter, then the service
    let add_tag = {
        let state = state.clone();
        let refresh = refresh.clone();
        let edit_filter = edit_filter.clone();
        Rc::new(move |service: ServiceKey, action: Option<MergeAction>| {
            let state = state.clone();
            let refresh = refresh.clone();
            edit_filter(
                &TagFilter::new(),
                Rc::new(move |filter| {
                    state
                        .borrow_mut()
                        .editor
                        .add_tag(service.clone(), action, filter);
                    refresh();
                }),
            );
        })
    };
    // a service chosen to add: its action asked, if "this is better"
    let service_chosen = {
        let state = state.clone();
        let add_tag = add_tag.clone();
        move |list: usize, service: ServiceKey| {
            let mut s = state.borrow_mut();
            let actions = if list == TAGS {
                s.editor.tag_actions(&service, None)
            } else {
                s.editor.rating_actions(&service, None)
            };
            match actions {
                Some(choices) => {
                    s.asking = Some(Asking::Action {
                        list,
                        service,
                        choices,
                        adding: true,
                    });
                }
                None if list == TAGS => {
                    drop(s);
                    add_tag(service, None);
                }
                None => s.editor.add_rating(service, None),
            }
        }
    };
    window.on_row_clicked({
        let state = state.clone();
        let refresh = refresh.clone();
        move |list, row, ctrl, shift| {
            let (Ok(list), Ok(row)) = (usize::try_from(list), usize::try_from(row)) else {
                return;
            };
            if list < 2 {
                let mut state = state.borrow_mut();
                let order: Vec<usize> = (0..state.keys(list).len()).collect();
                state.selections[list].click(&order, row, ctrl, shift);
            }
            refresh();
        }
    });
    let list_button = {
        let state = state.clone();
        let refresh = refresh.clone();
        let service_chosen = service_chosen.clone();
        let edit_filter = edit_filter.clone();
        Rc::new(move |list: usize, button: &str| {
            {
                let mut s = state.borrow_mut();
                if s.asking.is_some() || list > 1 {
                    return;
                }
                let selected = s.selected(list);
                match button {
                    "add" => {
                        let services = if list == TAGS {
                            s.editor.add_tag_services()
                        } else {
                            s.editor.add_rating_services()
                        };
                        match services {
                            Err(warning) => s.asking = Some(Asking::Warning(warning)),
                            Ok(choices) => match choices.only() {
                                Some(only) => {
                                    drop(s);
                                    service_chosen(list, only);
                                }
                                None => s.asking = Some(Asking::Service(list, choices)),
                            },
                        }
                    }
                    "edit action" => {
                        if let [service] = selected.as_slice() {
                            let choices = if list == TAGS {
                                let current = s.editor.tag_action(service);
                                s.editor.tag_actions(service, current)
                            } else {
                                let current = s.editor.rating_action(service);
                                s.editor.rating_actions(service, current)
                            };
                            if let Some(choices) = choices {
                                s.asking = Some(Asking::Action {
                                    list,
                                    service: service.clone(),
                                    choices,
                                    adding: false,
                                });
                            }
                        }
                    }
                    "edit filter" => {
                        if let ([service], TAGS) = (selected.as_slice(), list) {
                            let filter = s.editor.tag_filter(service).unwrap_or_default();
                            let service = service.clone();
                            drop(s);
                            let state = state.clone();
                            let refresh = refresh.clone();
                            edit_filter(
                                &filter,
                                Rc::new(move |filter| {
                                    state.borrow_mut().editor.set_tag_filter(&service, filter);
                                    refresh();
                                }),
                            );
                        }
                    }
                    "delete" => {
                        if !selected.is_empty() {
                            s.asking = Some(Asking::Remove(list, selected));
                        }
                    }
                    _ => {}
                }
            }
            refresh();
        })
    };
    window.on_list_button({
        let list_button = list_button.clone();
        move |list, button| {
            if let Ok(list) = usize::try_from(list) {
                list_button(list, &button);
            }
        }
    });
    // a rating double-clicked edits its action, as the reference's list does
    window.on_row_activated({
        let state = state.clone();
        move |list, row| {
            if list != 1 {
                return;
            }
            let Ok(row) = usize::try_from(row) else {
                return;
            };
            {
                let mut s = state.borrow_mut();
                let order: Vec<usize> = (0..s.keys(RATINGS).len()).collect();
                s.selections[RATINGS].click(&order, row, false, false);
            }
            list_button(RATINGS, "edit action");
        }
    });
    let answer = {
        let state = state.clone();
        let refresh = refresh.clone();
        let add_tag = add_tag.clone();
        move |chosen: Option<usize>| {
            let asking = state.borrow_mut().asking.take();
            match (asking, chosen) {
                (Some(Asking::Service(list, choices)), Some(i)) => {
                    if let Some((_, service)) = choices.choices.get(i) {
                        service_chosen(list, service.clone());
                    }
                }
                (
                    Some(Asking::Action {
                        list,
                        service,
                        choices,
                        adding,
                    }),
                    Some(i),
                ) => {
                    if let Some(&(_, action)) = choices.choices.get(i) {
                        match (list, adding) {
                            (TAGS, true) => add_tag(service, Some(action)),
                            (TAGS, false) => {
                                state.borrow_mut().editor.set_tag_action(&service, action);
                            }
                            (_, true) => {
                                state.borrow_mut().editor.add_rating(service, Some(action));
                            }
                            (_, false) => {
                                state
                                    .borrow_mut()
                                    .editor
                                    .set_rating_action(&service, action);
                            }
                        }
                    }
                }
                (Some(Asking::Remove(list, services)), Some(0)) => {
                    let mut s = state.borrow_mut();
                    if list == TAGS {
                        s.editor.delete_tags(&services);
                    } else {
                        s.editor.delete_ratings(&services);
                    }
                    s.selections[list] = ListSelection::default();
                }
                _ => {}
            }
            refresh();
        }
    };
    window.on_chosen({
        let answer = answer.clone();
        move |i| answer(usize::try_from(i).ok())
    });
    window.on_cancelled(move || answer(None));
    window.on_choice_changed({
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            {
                let mut s = state.borrow_mut();
                let get = |i: i32| usize::try_from(i).unwrap_or(0);
                s.editor.set_archive(get(window.get_archive_index()));
                s.editor.set_file_modified(get(window.get_modified_index()));
                s.editor.set_urls(get(window.get_urls_index()));
                s.editor.set_notes(get(window.get_notes_index()));
            }
            refresh();
        }
    });
    window.on_note_settings({
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            {
                let mut s = state.borrow_mut();
                let merge = s.editor.note_merge();
                window.set_note_extend(merge.extend_existing);
                window.set_note_conflict(index(
                    NOTE_CONFLICTS
                        .iter()
                        .position(|&c| c == merge.conflict)
                        .unwrap_or(0),
                ));
                s.note_settings = true;
            }
            refresh();
        }
    });
    window.on_note_settings_done({
        let weak = window.as_weak();
        let state = state.clone();
        let refresh = refresh.clone();
        move |apply| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            {
                let mut s = state.borrow_mut();
                if apply {
                    let conflict = usize::try_from(window.get_note_conflict())
                        .ok()
                        .and_then(|i| NOTE_CONFLICTS.get(i).copied());
                    if let Some(conflict) = conflict {
                        s.editor.set_note_merge(hydrus_core::notes::NoteMerge {
                            extend_existing: window.get_note_extend(),
                            conflict,
                        });
                    }
                }
                s.note_settings = false;
            }
            refresh();
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let filter_slot = filter_slot.clone();
        move || {
            if let Some(filter) = filter_slot.borrow_mut().take() {
                let _ = filter.hide();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        move || {
            let options = state.borrow().editor.value();
            close();
            applied(options);
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
    window.show()?;
    LAST.with(|last| *last.borrow_mut() = Some(window.as_weak()));
    Ok(window)
}
