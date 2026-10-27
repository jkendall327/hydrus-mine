//! The favourite searches' dialogs, bound: "edit favourite searches", the
//! list ([`favourites::Manager`]), sorted on a column as its header is
//! clicked, with "add", "edit" (one selected, or a double-click) and
//! "delete" (asking first); and "edit favourite search" for one
//! ([`favourites::Edit`]), asking before it overwrites another. "apply" on
//! the list keeps its searches in the store, where the star button's menu
//! reads them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::pages::{FavouriteSearch, SortSettings};
use hydrus_search::{TextContext, parse_api_search, predicate_text};
use hydrus_store::Store;
use hydrus_store::settings::FavouriteSearches;

use crate::favourites::{self, Describe, Edit, Manager};
use crate::list_selection::ListSelection;
use crate::{FavouriteEditWindow, FavouritesWindow, TableColumn, TableRow};

/// The dialogs while they are open.
#[derive(Default, Clone)]
pub struct Slots {
    pub list: Rc<RefCell<Option<FavouritesWindow>>>,
    pub edit: Rc<RefCell<Option<FavouriteEditWindow>>>,
    /// The system-predicate child, cancelled with its favourite owner.
    pub predicate_editor: Rc<RefCell<Option<crate::PredicateEditorWindow>>>,
    /// The OR editor the favourite editor's buttons open.
    pub search_or: crate::search_or_window::Slot,
    /// The open list's searches, for "save this search" to add to.
    open: Rc<RefCell<Option<Opened>>>,
}

/// The open list's state, and how it writes searches.
type Opened = (Rc<RefCell<List>>, Rc<Words>);

impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slots")
            .field("list", &self.list.borrow().is_some())
            .field("edit", &self.edit.borrow().is_some())
            .finish_non_exhaustive()
    }
}

/// What the list holds and shows.
struct List {
    manager: Manager,
    /// The column sorted on, and whether ascending (the reference's
    /// default: the folder, ascending).
    sort: (usize, bool),
    /// The searches in the order shown.
    order: Vec<usize>,
    /// The searches selected (by their place in the manager's rows), as
    /// the reference's list selects.
    selection: ListSelection<usize>,
    /// Deleting the searches selected, once asked.
    deleting: bool,
}

/// The search being edited: a new one, or one of the list's.
#[derive(Clone, Copy)]
enum Target {
    New,
    Existing(usize),
}

/// How the dialogs write searches, sorts and collects, from the store.
struct Words {
    store: Arc<Store>,
    text: TextContext,
}

impl Words {
    fn new(store: Arc<Store>) -> Self {
        let snapshot = store.snapshot();
        let viewing = store.read(hydrus_store::settings::get).unwrap_or_default();
        let text = TextContext::from_store(&snapshot.services, &viewing);
        Self { store, text }
    }

    fn predicates(&self, search: &hydrus_core::search::context::FileSearchContext) -> Vec<String> {
        search
            .predicates
            .iter()
            .map(|p| predicate_text(p, &self.text))
            .collect()
    }

    fn collect(&self, collect: &hydrus_core::pages::PageCollect) -> String {
        let snapshot = self.store.snapshot();
        favourites::collect_text(collect, &|key| {
            snapshot.services.by_key(key).ok().map(|s| s.name.clone())
        })
    }

    fn rows(&self, manager: &Manager) -> Vec<[String; 5]> {
        let describe = Describe {
            predicates: &|f: &FavouriteSearch| self.predicates(&f.search),
            sort: &|s| crate::sort::sort_text(&self.store, s),
            collect: &|c| self.collect(c),
        };
        manager
            .rows
            .iter()
            .map(|f| favourites::display_row(f, &describe))
            .collect()
    }
}

/// Open the list (or show it, if it is open); with `to_save`, the page's
/// search, open the edit dialog on it too, named so no other search has
/// its name, as "save this search" does.
pub(crate) fn open(
    slots: &Slots,
    store: &Arc<Store>,
    to_save: Option<FavouriteSearch>,
) -> Result<(), String> {
    if let Some(window) = slots.list.borrow().as_ref() {
        window.show().map_err(|e| e.to_string())?;
    } else {
        open_list(slots, store.clone())?;
    }
    if let Some(row) = to_save {
        let opened = slots.open.borrow().clone();
        if let Some((list, words)) = opened {
            let row = list.borrow().manager.to_add(row);
            open_edit(slots, &list, &words, Target::New, &row)?;
        }
    }
    Ok(())
}

fn open_list(slots: &Slots, store: Arc<Store>) -> Result<(), String> {
    let window = crate::app_title::new::<crate::FavouritesWindow>().map_err(|e| e.to_string())?;
    let rows = store
        .read(hydrus_store::settings::get::<FavouriteSearches>)
        .map(|f| f.0)
        .unwrap_or_default();
    let words = Rc::new(Words::new(store.clone()));
    let list = Rc::new(RefCell::new(List {
        manager: Manager::new(rows),
        sort: (0, true),
        order: Vec::new(),
        selection: ListSelection::default(),
        deleting: false,
    }));
    *slots.open.borrow_mut() = Some((list.clone(), words.clone()));
    window.set_columns(ModelRc::new(VecModel::from(
        favourites::COLUMNS
            .iter()
            .zip([140.0, 140.0, 360.0, 190.0, 120.0])
            .enumerate()
            .map(|(i, (c, width))| TableColumn {
                title: SharedString::from(*c),
                width,
                stretch: i + 1 == favourites::COLUMNS.len(),
            })
            .collect::<Vec<_>>(),
    )));
    let show = shower(&window, &list, &words);
    show();
    window.on_header_clicked({
        let list = list.clone();
        let show = show.clone();
        move |column| {
            let Ok(column) = usize::try_from(column) else {
                return;
            };
            {
                let mut list = list.borrow_mut();
                list.sort = if list.sort.0 == column {
                    (column, !list.sort.1)
                } else {
                    (column, true)
                };
            }
            show();
        }
    });
    window.on_row_clicked({
        let list = list.clone();
        let show = show.clone();
        move |shown, ctrl, shift| {
            let Ok(shown) = usize::try_from(shown) else {
                return;
            };
            {
                let mut list = list.borrow_mut();
                let order = list.order.clone();
                list.selection.click(&order, shown, ctrl, shift);
            }
            show();
        }
    });
    let edit_selected = {
        let slots = slots.clone();
        let list = list.clone();
        let words = words.clone();
        move || {
            let chosen = {
                let list = list.borrow();
                match list.selection.one() {
                    Some(index) => list
                        .manager
                        .rows
                        .get(index)
                        .cloned()
                        .map(|row| (index, row)),
                    _ => None,
                }
            };
            if let Some((index, row)) = chosen
                && let Err(e) = open_edit(&slots, &list, &words, Target::Existing(index), &row)
            {
                eprintln!("could not open the favourite search: {e}");
            }
        }
    };
    window.on_row_activated({
        let list = list.clone();
        let show = show.clone();
        let edit_selected = edit_selected.clone();
        move |shown| {
            let index = usize::try_from(shown)
                .ok()
                .and_then(|s| list.borrow().order.get(s).copied());
            if let Some(index) = index {
                list.borrow_mut().selection.select_only(Some(index));
                show();
                edit_selected();
            }
        }
    });
    window.on_edit(edit_selected);
    window.on_add({
        let slots = slots.clone();
        let list = list.clone();
        let words = words.clone();
        move || {
            let defaults: hydrus_store::settings::SearchDefaults = words
                .store
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            let blank = favourites::new_search(defaults.local_location);
            let row = list.borrow().manager.to_add(blank);
            if let Err(e) = open_edit(&slots, &list, &words, Target::New, &row) {
                eprintln!("could not open the favourite search: {e}");
            }
        }
    });
    window.on_delete({
        let list = list.clone();
        let weak = window.as_weak();
        move || {
            if list.borrow().selection.is_empty() {
                return;
            }
            list.borrow_mut().deleting = true;
            if let Some(window) = weak.upgrade() {
                window.set_question("Remove all selected?".into());
            }
        }
    });
    window.on_answered({
        let list = list.clone();
        let show = show.clone();
        let weak = window.as_weak();
        move |yes| {
            {
                let mut list = list.borrow_mut();
                if std::mem::take(&mut list.deleting) && yes {
                    let all: Vec<usize> = (0..list.manager.rows.len()).collect();
                    let selected = list.selection.in_order(&all);
                    list.manager.delete(&selected);
                    list.selection = ListSelection::default();
                }
            }
            if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
            show();
        }
    });
    let close = {
        let weak = window.as_weak();
        let slots = slots.clone();
        move || {
            let editing = slots
                .edit
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(edit) = editing {
                edit.invoke_cancel();
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slots.list.borrow_mut().take();
            slots.open.borrow_mut().take();
        }
    };
    window.on_apply({
        let close = close.clone();
        let list = list.clone();
        move || {
            let rows = list.borrow().manager.rows.clone();
            if let Err(e) = store
                .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &FavouriteSearches(rows)))
            {
                eprintln!("could not keep the favourite searches: {e}");
                return;
            }
            close();
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    *slots.list.borrow_mut() = Some(window);
    Ok(())
}

/// What shows the list as it now is.
fn shower(window: &FavouritesWindow, list: &Rc<RefCell<List>>, words: &Rc<Words>) -> Rc<dyn Fn()> {
    let weak = window.as_weak();
    let list = list.clone();
    let words = words.clone();
    Rc::new(move || {
        let Some(window) = weak.upgrade() else { return };
        let mut list = list.borrow_mut();
        let rows = words.rows(&list.manager);
        let (column, ascending) = list.sort;
        list.order = favourites::list_order(&rows, column, ascending);
        let shown: Vec<TableRow> = list
            .order
            .iter()
            .map(|&i| TableRow {
                cells: ModelRc::new(VecModel::from(
                    rows[i]
                        .iter()
                        .map(|c| SharedString::from(c.as_str()))
                        .collect::<Vec<_>>(),
                )),
                selected: list.selection.is_selected(i),
            })
            .collect();
        window.set_rows(ModelRc::new(VecModel::from(shown)));
        window.set_sort_column(i32::try_from(column).unwrap_or(0));
        window.set_ascending(ascending);
        window.set_can_edit(list.selection.one().is_some());
        window.set_can_delete(!list.selection.is_empty());
    })
}

/// Open the edit dialog on `row` (one already open is replaced); "apply"
/// puts what it makes in the list.
fn open_edit(
    slots: &Slots,
    list: &Rc<RefCell<List>>,
    words: &Rc<Words>,
    target: Target,
    row: &FavouriteSearch,
) -> Result<(), String> {
    let old = slots
        .edit
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(old) = old {
        old.invoke_cancel();
    }
    let window =
        crate::app_title::new::<crate::FavouriteEditWindow>().map_err(|e| e.to_string())?;
    let sorts: SortSettings = words
        .store
        .read(hydrus_store::settings::get)
        .unwrap_or_default();
    let edit = Rc::new(RefCell::new(Edit::new(
        row,
        &sorts.default_sort,
        &sorts.default_collect,
    )));
    let existing = list.borrow().manager.existing();
    let active = Rc::new(Cell::new(true));
    // Each favourite owns a distinct child slot: stale child callbacks cannot
    // clear a newly opened favourite's predicate editor.
    let child = Rc::new(RefCell::new(None::<crate::PredicateEditorWindow>));
    let visible_child = slots.predicate_editor.clone();
    let autocomplete = Rc::new(RefCell::new(crate::autocomplete::Autocomplete::new(
        words.store.clone(),
    )));
    // the OR being built (shift+enter), kept apart from the search
    let or_draft = Rc::new(RefCell::new(
        hydrus_gui_model::search_or::Construction::default(),
    ));
    let sort_choices = Rc::new(crate::sort::page_choices(
        &words.store,
        &edit.borrow().sort.by,
    ));
    let collect_choices = Rc::new(crate::collect::choices(&words.store));
    let registry = words.store.snapshot().services.clone();
    let advanced = words
        .store
        .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
        .unwrap_or_default()
        .0;
    let location_choices: Rc<Vec<crate::domains::Row>> = Rc::new(
        crate::domains::location_menu(&registry, advanced, &edit.borrow().search.location)
            .into_iter()
            .flatten()
            .collect(),
    );
    let tag_choices: Rc<Vec<crate::domains::Row>> = Rc::new(
        crate::domains::tag_menu(&registry, &edit.borrow().search.tags)
            .into_iter()
            .flatten()
            .collect(),
    );
    let location_ticks = Rc::new(crate::domains::multiple_ticks(&registry, advanced));
    let pending_locations = Rc::new(RefCell::new(Vec::<(bool, hydrus_core::ServiceKey)>::new()));
    let text_model = |items: Vec<String>| {
        ModelRc::new(VecModel::from(
            items
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        ))
    };
    window.set_location_choices(text_model(
        location_choices.iter().map(|r| r.label.clone()).collect(),
    ));
    window.set_tag_choices(text_model(
        tag_choices.iter().map(|r| r.label.clone()).collect(),
    ));
    window.set_sort_choices(text_model(
        sort_choices.iter().map(|r| r.name.clone()).collect(),
    ));
    window.set_location_ticks(ModelRc::new(VecModel::from(
        location_ticks
            .iter()
            .map(|t| crate::Tick {
                label: t.label.as_str().into(),
                on: false,
            })
            .collect::<Vec<_>>(),
    )));
    {
        let edit = edit.borrow();
        window.set_folder(edit.folder.as_str().into());
        window.set_name(edit.name.as_str().into());
        let snapshot = words.store.snapshot();
        window.set_location_label(
            crate::domains::location_label(&snapshot.services, &edit.search.location).into(),
        );
        window.set_tags_label(
            crate::domains::tag_label(&snapshot.services, &edit.search.tags).into(),
        );
    }
    let show = {
        let weak = window.as_weak();
        let edit = edit.clone();
        let words = words.clone();
        let autocomplete = autocomplete.clone();
        let or_draft = or_draft.clone();
        let location_choices = location_choices.clone();
        let tag_choices = tag_choices.clone();
        let sort_choices = sort_choices.clone();
        let collect_choices = collect_choices.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let edit = edit.borrow();
            let snapshot = words.store.snapshot();
            window.set_location_label(
                crate::domains::location_label(&snapshot.services, &edit.search.location).into(),
            );
            window.set_tags_label(
                crate::domains::tag_label(&snapshot.services, &edit.search.tags).into(),
            );
            window.set_location_index(location_choices.iter().position(|r| matches!(&r.choice, crate::domains::Choice::Location(l) if *l == edit.search.location)).or_else(|| location_choices.iter().position(|r| matches!(r.choice, crate::domains::Choice::Multiple))).and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
            window.set_tag_index(tag_choices.iter().position(|r| matches!(&r.choice, crate::domains::Choice::Tags(t) if *t == edit.search.tags.service)).and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
            window.set_include_current(edit.search.tags.include_current);
            window.set_include_pending(edit.search.tags.include_pending);
            let si = sort_choices
                .iter()
                .position(|c| c.by == edit.sort.by)
                .unwrap_or(0);
            window.set_sort_index(i32::try_from(si).unwrap_or(0));
            window.set_sort_orders(ModelRc::new(VecModel::from(
                sort_choices[si]
                    .orders
                    .iter()
                    .map(|o| SharedString::from(*o))
                    .collect::<Vec<_>>(),
            )));
            window.set_sort_order(i32::from(!edit.sort.ascending));
            window.set_collect_choices(ModelRc::new(VecModel::from(
                collect_choices
                    .iter()
                    .map(|c| crate::Tick {
                        label: c.name.as_str().into(),
                        on: c.checked(&edit.collect),
                    })
                    .collect::<Vec<_>>(),
            )));
            window.set_collect_unmatched(edit.collect.collect_unmatched);
            window.set_collect_tag_index(tag_choices.iter().position(|r| matches!(&r.choice, crate::domains::Choice::Tags(t) if *t == edit.collect.tag_context.service)).and_then(|i| i32::try_from(i).ok()).unwrap_or(-1));
            let mut ac = autocomplete.borrow_mut();
            ac.set_context(&edit.search.location, &edit.search.tags);
            // (the tabs are kept while the text is as it was: typing returns
            // to the results)
            if ac.text() != window.get_typed().as_str() {
                ac.set_text(window.get_typed().as_str());
            }
            ac.set_context_tags(edit.search.predicates.iter().filter_map(|p| {
                if let hydrus_search::Predicate::Tag { tag, .. } = p {
                    Some(tag.as_str().to_owned())
                } else {
                    None
                }
            }));
            window.set_tab_index(match ac.tab() {
                hydrus_gui_model::write_autocomplete::Tab::Tags => 0,
                hydrus_gui_model::write_autocomplete::Tab::Favourites => 1,
                hydrus_gui_model::write_autocomplete::Tab::Children => 2,
            });
            let draft = or_draft.borrow();
            window.set_or_active(draft.terms().is_some());
            window.set_or_rewind_visible(draft.terms().is_some_and(|t| t.len() > 1));
            window.set_advanced_visible(advanced);
            window.set_suggestion_highlight(
                ac.highlighted()
                    .and_then(|i| i32::try_from(i).ok())
                    .unwrap_or(-1),
            );
            window.set_suggestions(ModelRc::new(VecModel::from(
                ac.suggestions()
                    .iter()
                    .map(|s| SharedString::from(s.label.as_str()))
                    .collect::<Vec<_>>(),
            )));
            window.set_predicates(ModelRc::new(VecModel::from(
                words
                    .predicates(&edit.search)
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            )));
            window.set_synchronised(edit.synchronised);
            window.set_sort_label(crate::sort::sort_text(&words.store, &edit.sort).into());
            window.set_save_sort(edit.save_sort);
            window.set_collect_label(words.collect(&edit.collect).into());
            window.set_save_collect(edit.save_collect);
        }
    };
    show();
    window.on_location_chosen({
        let edit = edit.clone();
        let show = show.clone();
        let choices = location_choices.clone();
        let registry = registry.clone();
        let ticks = location_ticks.clone();
        let pending = pending_locations.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move |i| {
            if !active.get() {
                return;
            }
            let Some(row) = usize::try_from(i).ok().and_then(|i| choices.get(i)) else {
                return;
            };
            match &row.choice {
                crate::domains::Choice::Location(location) => edit
                    .borrow_mut()
                    .choose_location(&registry, location.clone()),
                crate::domains::Choice::Multiple => {
                    let location = edit.borrow().search.location.clone();
                    *pending.borrow_mut() = ticks
                        .iter()
                        .filter(|t| {
                            if t.deleted {
                                location.deleted()
                            } else {
                                location.current()
                            }
                            .contains(&t.service)
                        })
                        .map(|t| (t.deleted, t.service.clone()))
                        .collect();
                    if let Some(w) = weak.upgrade() {
                        w.set_selecting_locations(true);
                        w.set_location_ticks(ModelRc::new(VecModel::from(
                            ticks
                                .iter()
                                .map(|t| crate::Tick {
                                    label: t.label.as_str().into(),
                                    on: pending.borrow().contains(&(t.deleted, t.service.clone())),
                                })
                                .collect::<Vec<_>>(),
                        )));
                    }
                }
                crate::domains::Choice::Tags(_) => (),
            }
            show();
        }
    });
    window.on_location_ticked({
        let pending = pending_locations.clone();
        let ticks = location_ticks.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move |i, on| {
            if !active.get() {
                return;
            }
            let Some(tick) = usize::try_from(i).ok().and_then(|i| ticks.get(i)) else {
                return;
            };
            let key = (tick.deleted, tick.service.clone());
            pending.borrow_mut().retain(|k| *k != key);
            if on {
                pending.borrow_mut().push(key);
            }
            if let Some(w) = weak.upgrade() {
                w.set_location_ticks(ModelRc::new(VecModel::from(
                    ticks
                        .iter()
                        .map(|t| crate::Tick {
                            label: t.label.as_str().into(),
                            on: pending.borrow().contains(&(t.deleted, t.service.clone())),
                        })
                        .collect::<Vec<_>>(),
                )));
            }
        }
    });
    window.on_locations_answered({
        let pending = pending_locations.clone();
        let registry = registry.clone();
        let edit = edit.clone();
        let weak = window.as_weak();
        let show = show.clone();
        let active = active.clone();
        move |yes| {
            if !active.get() {
                return;
            }
            if yes {
                edit.borrow_mut().choose_location(
                    &registry,
                    crate::domains::ticked_location(&registry, &pending.borrow()),
                );
            }
            if let Some(w) = weak.upgrade() {
                w.set_selecting_locations(false);
            }
            show();
        }
    });
    window.on_tag_chosen({
        let choices = tag_choices.clone();
        let edit = edit.clone();
        let show = show.clone();
        let store = words.store.clone();
        let active = active.clone();
        move |i| {
            if !active.get() {
                return;
            }
            let Some(row) = usize::try_from(i).ok().and_then(|i| choices.get(i)) else {
                return;
            };
            if let crate::domains::Choice::Tags(key) = &row.choice {
                let defaults = store
                    .read(hydrus_store::settings::get::<hydrus_store::settings::SearchDefaults>)
                    .unwrap_or_default();
                edit.borrow_mut()
                    .choose_tags(key.clone(), &defaults.local_location);
            }
            show();
        }
    });
    window.on_tag_status_ticked({
        let edit = edit.clone();
        let show = show.clone();
        let active = active.clone();
        move |pending, on| {
            if !active.get() {
                return;
            }
            if pending {
                edit.borrow_mut().search.tags.include_pending = on;
            } else {
                edit.borrow_mut().search.tags.include_current = on;
            }
            show();
        }
    });
    window.on_sort_chosen({
        let choices = sort_choices.clone();
        let edit = edit.clone();
        let show = show.clone();
        let active = active.clone();
        move |i| {
            if !active.get() {
                return;
            }
            if let Some(choice) = usize::try_from(i).ok().and_then(|i| choices.get(i)) {
                edit.borrow_mut().choose_sort(choice);
            }
            show();
        }
    });
    window.on_sort_order_chosen({
        let edit = edit.clone();
        let show = show.clone();
        let active = active.clone();
        move |i| {
            if !active.get() {
                return;
            }
            edit.borrow_mut().sort.ascending = i == 0;
            show();
        }
    });
    window.on_collect_ticked({
        let choices = collect_choices.clone();
        let edit = edit.clone();
        let show = show.clone();
        let active = active.clone();
        move |i, on| {
            if !active.get() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut e = edit.borrow_mut();
                e.collect = crate::collect::toggled(&choices, &e.collect, i, on);
            }
            show();
        }
    });
    window.on_collect_tag_chosen({
        let choices = tag_choices.clone();
        let edit = edit.clone();
        let show = show.clone();
        let active = active.clone();
        move |i| {
            if !active.get() {
                return;
            }
            if let Some(row) = usize::try_from(i).ok().and_then(|i| choices.get(i))
                && let crate::domains::Choice::Tags(key) = &row.choice
            {
                let mut e = edit.borrow_mut();
                e.collect.tag_context.service = key.clone();
                e.collect.tag_context.display_service = key.clone();
            }
            show();
        }
    });
    window.on_collect_unmatched_ticked({
        let choices = collect_choices.clone();
        let edit = edit.clone();
        let show = show.clone();
        let active = active.clone();
        move |on| {
            if !active.get() {
                return;
            }
            let mut e = edit.borrow_mut();
            e.collect = crate::collect::with_unmatched(&choices, &e.collect, on);
            drop(e);
            show();
        }
    });
    window.on_typed_edited({
        let show = show.clone();
        move || show()
    });
    // entering predicates goes through the OR being built, as the
    // reference's `_BroadcastChoices` does: shift adds to it, a plain
    // entry commits it with them
    let enter: Rc<dyn Fn(Vec<hydrus_search::Predicate>, bool)> = Rc::new({
        let edit = edit.clone();
        let words = words.clone();
        let or_draft = or_draft.clone();
        let autocomplete = autocomplete.clone();
        move |predicates, shift| {
            let plain = hydrus_search::TextContext {
                presentation: None,
                ..words.text.clone()
            };
            let committed = or_draft.borrow_mut().broadcast(predicates, shift, &plain);
            let label = or_draft
                .borrow()
                .predicate()
                .map(|p| hydrus_search::predicate_text(&p, &words.text));
            autocomplete.borrow_mut().set_or_draft(label);
            if !committed.is_empty() {
                hydrus_search::enter_predicates(
                    &mut edit.borrow_mut().search.predicates,
                    &committed,
                    &words.text,
                );
            }
        }
    });
    window.on_suggestion_moved({
        let ac = autocomplete.clone();
        let weak = window.as_weak();
        let active = active.clone();
        move |by| {
            if !active.get() {
                return;
            }
            let key = match by {
                -1 => crate::autocomplete::Navigation::Up,
                1 => crate::autocomplete::Navigation::Down,
                2 => crate::autocomplete::Navigation::Home,
                3 => crate::autocomplete::Navigation::End,
                4 => crate::autocomplete::Navigation::PageUp,
                5 => crate::autocomplete::Navigation::PageDown,
                _ => return,
            };
            let mut ac = ac.borrow_mut();
            ac.navigate(key);
            if let Some(w) = weak.upgrade() {
                w.set_suggestion_highlight(
                    ac.highlighted()
                        .and_then(|i| i32::try_from(i).ok())
                        .unwrap_or(-1),
                );
            }
        }
    });
    let choose_suggestion: Rc<dyn Fn(Option<usize>, bool)> = Rc::new({
        let or_draft = or_draft.clone();
        let enter = enter.clone();
        let weak = window.as_weak();
        let words = words.clone();
        let autocomplete = autocomplete.clone();
        let child = child.clone();
        let visible_child = visible_child.clone();
        let active = active.clone();
        let show = show.clone();
        move |index, shift| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut ac = autocomplete.borrow_mut();
            if ac.text() != window.get_typed().as_str() {
                ac.set_text(window.get_typed().as_str());
            }
            let resolved = index.or_else(|| ac.highlighted());
            let own_draft =
                resolved == Some(0) && ac.tab() == hydrus_gui_model::write_autocomplete::Tab::Tags;
            let suggestion = resolved.and_then(|i| ac.suggestions().get(i)).cloned();
            drop(ac);
            // (the first result is the OR being built, which enters itself)
            if own_draft && let Some(draft) = or_draft.borrow().predicate() {
                enter(vec![draft], shift);
                window.set_typed("".into());
                window.set_error("".into());
                show();
                return;
            }
            if let Some(blank) = suggestion.as_ref().and_then(|s| s.editor) {
                let store = words.store.clone();
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
                let chosen: Rc<dyn Fn(Vec<hydrus_search::Predicate>)> = Rc::new({
                    let active = active.clone();
                    let enter = enter.clone();
                    let show = show.clone();
                    let weak = weak.clone();
                    let visible_child = visible_child.clone();
                    move |predicates| {
                        if !active.get() {
                            return;
                        }
                        visible_child.borrow_mut().take();
                        enter(predicates, false);
                        if let Some(w) = weak.upgrade() {
                            w.set_child_open(false);
                            w.set_typed("".into());
                            w.set_error("".into());
                        }
                        show();
                    }
                });
                match crate::predicate_editor_window::open(
                    &child,
                    &store,
                    editor,
                    context,
                    words.text.clone(),
                    chosen,
                    Some(Rc::new({
                        let active = active.clone();
                        let weak = weak.clone();
                        move || {
                            active.get()
                                && weak
                                    .upgrade()
                                    .is_some_and(|window| window.window().is_visible())
                        }
                    })),
                ) {
                    Ok(()) => {
                        window.set_child_open(true);
                        *visible_child.borrow_mut() = child
                            .borrow()
                            .as_ref()
                            .map(slint::ComponentHandle::clone_strong);
                    }
                    Err(error) => window.set_error(error.into()),
                }
                return;
            }
            // (with nothing listed there is nothing to enter, as the
            // reference's list has none)
            let Some(typed) = suggestion.map(|s| s.predicate) else {
                return;
            };
            if typed.is_empty() {
                return;
            }
            match parse_api_search(&serde_json::json!([typed])) {
                Ok(parsed) => {
                    enter(parsed, shift);
                    window.set_typed("".into());
                    window.set_error("".into());
                }
                // (text beginning "system:" that is no system predicate is
                // left in the box without a message, as the reference does)
                Err(_) if typed.starts_with("system:") => {}
                Err(error) => window.set_error(error.to_string().into()),
            }
            show();
        }
    });
    window.on_typed_accepted({
        let choose = choose_suggestion.clone();
        move || choose(None, false)
    });
    window.on_suggestion_chosen({
        let choose = choose_suggestion.clone();
        move |i| choose(usize::try_from(i).ok(), false)
    });
    window.on_tab_chosen({
        let autocomplete = autocomplete.clone();
        let show = show.clone();
        let active = active.clone();
        let child = child.clone();
        move |tab| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            autocomplete.borrow_mut().set_tab(
                hydrus_gui_model::write_autocomplete::Tab::from_index(
                    usize::try_from(tab).unwrap_or(0),
                ),
            );
            show();
        }
    });
    // the OR editor the "OR" and "advanced" buttons open
    let or_slot = slots.search_or.clone();
    window.on_or_action({
        let or_slot = or_slot.clone();
        let edit = edit.clone();
        let enter = enter.clone();
        let choose = choose_suggestion;
        let or_draft = or_draft.clone();
        let autocomplete = autocomplete.clone();
        let words = words.clone();
        let show = show.clone();
        let active = active.clone();
        let child = child.clone();
        let weak = window.as_weak();
        move |action| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            match action {
                0 => choose(None, true),
                3 | 4 => {
                    if action == 4 && !advanced {
                        return;
                    }
                    let Some(w) = weak.upgrade() else { return };
                    let context = hydrus_search::FileSearchContext {
                        location: edit.borrow().search.location.clone(),
                        tags: edit.borrow().search.tags.clone(),
                        predicates: Vec::new(),
                    };
                    let owner: crate::search_or_window::ValidOwner = Rc::new({
                        let active = active.clone();
                        let weak = weak.clone();
                        move || {
                            active.get()
                                && weak
                                    .upgrade()
                                    .is_some_and(|window| window.window().is_visible())
                        }
                    });
                    let applied: crate::search_or_window::Applied = Rc::new({
                        let enter = enter.clone();
                        let owner = owner.clone();
                        let show = show.clone();
                        let weak = weak.clone();
                        move |predicates| {
                            if owner() {
                                enter(predicates, false);
                                if let Some(w) = weak.upgrade() {
                                    w.set_typed("".into());
                                }
                                show();
                            }
                        }
                    });
                    match crate::search_or_window::open(
                        words.store.clone(),
                        context,
                        action == 4,
                        &or_slot,
                        owner,
                        applied,
                    ) {
                        Ok(_) => w.set_child_open(true),
                        Err(error) => {
                            w.set_error(format!("could not open the OR editor: {error}").into());
                        }
                    }
                }
                1 | 2 => {
                    if action == 1 {
                        or_draft.borrow_mut().rewind();
                    } else {
                        or_draft.borrow_mut().cancel();
                    }
                    let label = or_draft
                        .borrow()
                        .predicate()
                        .map(|p| hydrus_search::predicate_text(&p, &words.text));
                    let mut ac = autocomplete.borrow_mut();
                    ac.set_or_draft(label);
                    ac.clear();
                    drop(ac);
                    if let Some(w) = weak.upgrade() {
                        w.set_typed("".into());
                    }
                    show();
                }
                _ => {}
            }
        }
    });
    window.on_search_escape({
        let or_draft = or_draft.clone();
        let autocomplete = autocomplete.clone();
        let words = words.clone();
        let show = show.clone();
        let active = active.clone();
        let weak = window.as_weak();
        move || {
            // escape in an empty box rewinds the OR being built
            let Some(w) = weak.upgrade() else {
                return false;
            };
            if !active.get() || or_draft.borrow().terms().is_none() || !w.get_typed().is_empty() {
                return false;
            }
            or_draft.borrow_mut().rewind();
            let label = or_draft
                .borrow()
                .predicate()
                .map(|p| hydrus_search::predicate_text(&p, &words.text));
            autocomplete.borrow_mut().set_or_draft(label);
            show();
            true
        }
    });
    let child_timer = Rc::new(slint::Timer::default());
    child_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(100),
        {
            let weak = window.as_weak();
            let child = child.clone();
            let or_slot = or_slot.clone();
            let visible_child = visible_child.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_child_open(child.borrow().is_some() || or_slot.borrow().is_some());
                    *visible_child.borrow_mut() = child
                        .borrow()
                        .as_ref()
                        .map(slint::ComponentHandle::clone_strong);
                }
            }
        },
    );
    window.on_predicate_removed({
        let edit = edit.clone();
        let show = show.clone();
        move |i| {
            if let Ok(i) = usize::try_from(i) {
                let mut edit = edit.borrow_mut();
                if i < edit.search.predicates.len() {
                    edit.search.predicates.remove(i);
                }
            }
            show();
        }
    });
    // the autocomplete's icon buttons: paste, the star menu, empty search
    window.on_paste_search({
        let edit = edit.clone();
        let words = words.clone();
        let show = show.clone();
        let active = active.clone();
        let child = child.clone();
        let weak = window.as_weak();
        move || {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            let text = match crate::clipboard_text() {
                Ok(Some(text)) => text,
                Ok(None) => {
                    window.set_error("Problem pasting! No text on the clipboard!".into());
                    return;
                }
                Err(error) => {
                    window.set_error(format!("Problem pasting! {error}").into());
                    return;
                }
            };
            // (lines that are no predicate are passed over)
            let predicates: Vec<hydrus_search::Predicate> = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .filter_map(|line| parse_api_search(&serde_json::json!([line])).ok())
                .flatten()
                .collect();
            if !predicates.is_empty() {
                hydrus_search::enter_predicates(
                    &mut edit.borrow_mut().search.predicates,
                    &predicates,
                    &words.text,
                );
                window.set_error("".into());
            }
            show();
        }
    });
    window.on_clear_search({
        let edit = edit.clone();
        let words = words.clone();
        let show = show.clone();
        let active = active.clone();
        let child = child.clone();
        move || {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            let defaults: hydrus_store::settings::SearchDefaults = words
                .store
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            edit.borrow_mut().empty_search(defaults.local_location);
            show();
        }
    });
    let star_menu = crate::popup_menu::Popup::<usize>::new();
    let star_favourites = Rc::new(RefCell::new(
        Vec::<hydrus_core::pages::FavouriteSearch>::new(),
    ));
    window.set_favourite_panes(star_menu.model());
    window.on_favourites_menu({
        let star_menu = star_menu.clone();
        let star_favourites = star_favourites.clone();
        let words = words.clone();
        let active = active.clone();
        let child = child.clone();
        move |x, y| {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            // (read as the menu opens, as the reference's manager has them)
            let read: Vec<hydrus_core::pages::FavouriteSearch> = words
                .store
                .read(hydrus_store::settings::get::<hydrus_store::settings::FavouriteSearches>)
                .map(|f| f.0)
                .unwrap_or_default();
            let (entries, loads) = favourites::editor_menu(&read);
            *star_favourites.borrow_mut() = read;
            if !entries.is_empty() {
                star_menu.open(entries, loads, x, y);
            }
        }
    });
    window.on_favourite_line_hovered({
        let star_menu = star_menu.clone();
        move |p, l, r, t, left| star_menu.hover(p, l, r, t, left)
    });
    window.on_favourite_placed({
        let star_menu = star_menu.clone();
        move |p, x, y, w| star_menu.placed(p, x, y, w)
    });
    window.on_favourite_dismissed({
        let star_menu = star_menu.clone();
        move || star_menu.close()
    });
    window.on_favourite_line_clicked({
        let star_menu = star_menu.clone();
        let star_favourites = star_favourites.clone();
        let edit = edit.clone();
        let words = words.clone();
        let show = show.clone();
        let active = active.clone();
        move |p, l, r, t, left| {
            let chosen = star_menu.click(p, l, r, t, left);
            if !active.get() {
                return;
            }
            if let Some(crate::popup_menu::Chosen::Action(index)) = chosen
                && let Some(row) = star_favourites.borrow().get(index)
            {
                let mut edit = edit.borrow_mut();
                edit.load(row);
                // (the list sorts what it is given)
                hydrus_search::enter_predicates(&mut edit.search.predicates, &[], &words.text);
                drop(edit);
                show();
            }
        }
    });
    window.on_flip_synchronised({
        let edit = edit.clone();
        let show = show.clone();
        move || {
            {
                let mut edit = edit.borrow_mut();
                edit.synchronised = !edit.synchronised;
            }
            show();
        }
    });
    window.on_save_sort_ticked({
        let edit = edit.clone();
        let show = show.clone();
        move |on| {
            edit.borrow_mut().save_sort = on;
            show();
        }
    });
    window.on_save_collect_ticked({
        let edit = edit.clone();
        let show = show.clone();
        move |on| {
            edit.borrow_mut().save_collect = on;
            show();
        }
    });
    let close = {
        let weak = window.as_weak();
        let slot = slots.edit.clone();
        let child = child.clone();
        let active = active.clone();
        let child_timer = child_timer.clone();
        let or_slot = or_slot.clone();
        let visible_child = visible_child.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            child_timer.stop();
            visible_child.borrow_mut().take();
            let pred = child
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(pred) = pred {
                pred.invoke_cancel();
            }
            crate::search_or_window::cancel(&or_slot);
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    // what "apply" does once nothing is in the way: the list takes it, it
    // selected
    let finish = {
        let edit = edit.clone();
        let list = list.clone();
        let list_window = slots.list.clone();
        let words = words.clone();
        let close = close.clone();
        let active = active.clone();
        let child = child.clone();
        move || {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            let value = edit.borrow().value();
            {
                let mut list = list.borrow_mut();
                match target {
                    Target::New => list.manager.added(value.clone()),
                    Target::Existing(index) => list.manager.edited(index, value.clone()),
                }
                let index = list.manager.rows.len() - 1;
                list.selection.select_only(Some(index));
            }
            close();
            if let Some(window) = list_window.borrow().as_ref() {
                shower(window, &list, &words)();
            }
        }
    };
    window.on_apply({
        let weak = window.as_weak();
        let edit = edit.clone();
        let finish = finish.clone();
        let active = active.clone();
        let child = child.clone();
        move || {
            if !active.get() || child.borrow().is_some() {
                return;
            }
            let Some(window) = weak.upgrade() else { return };
            if window.get_selecting_locations() {
                return;
            }
            let question = {
                let mut edit = edit.borrow_mut();
                edit.folder = window.get_folder().to_string();
                edit.name = window.get_name().to_string();
                edit.overwrite_question(&existing)
            };
            match question {
                Some(question) => window.set_question(question.into()),
                None => finish(),
            }
        }
    });
    window.on_answered({
        let weak = window.as_weak();
        let finish = finish.clone();
        move |yes| {
            if let Some(window) = weak.upgrade() {
                window.set_question(SharedString::new());
            }
            if yes {
                finish();
            }
        }
    });
    window.on_cancel({
        let close = close.clone();
        move || close()
    });
    window.window().on_close_requested({
        let close = close.clone();
        move || {
            close();
            slint::CloseRequestResponse::HideWindow
        }
    });
    window.show().map_err(|e| e.to_string())?;
    *slots.edit.borrow_mut() = Some(window);
    Ok(())
}
