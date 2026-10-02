//! The favourite searches' dialogs, bound: "edit favourite searches", the
//! list ([`favourites::Manager`]), sorted on a column as its header is
//! clicked, with "add", "edit" (one selected, or a double-click) and
//! "delete" (asking first); and "edit favourite search" for one
//! ([`favourites::Edit`]), asking before it overwrites another. "apply" on
//! the list keeps its searches in the store, where the star button's menu
//! reads them.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::pages::{FavouriteSearch, SortSettings};
use hydrus_search::{TextContext, parse_api_search, predicate_text};
use hydrus_store::Store;
use hydrus_store::settings::FavouriteSearches;

use crate::favourites::{self, Describe, Edit, Manager};
use crate::{FavouriteEditWindow, FavouriteListRow, FavouritesWindow};

/// The dialogs while they are open.
#[derive(Default, Clone)]
pub struct Slots {
    pub list: Rc<RefCell<Option<FavouritesWindow>>>,
    pub edit: Rc<RefCell<Option<FavouriteEditWindow>>>,
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
    /// The searches selected, and the one a shift-click selects from.
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
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
    let window = FavouritesWindow::new().map_err(|e| e.to_string())?;
    let rows = store
        .read(hydrus_store::settings::get::<FavouriteSearches>)
        .map(|f| f.0)
        .unwrap_or_default();
    let words = Rc::new(Words::new(store.clone()));
    let list = Rc::new(RefCell::new(List {
        manager: Manager::new(rows),
        sort: (0, true),
        order: Vec::new(),
        selected: BTreeSet::new(),
        anchor: None,
        deleting: false,
    }));
    *slots.open.borrow_mut() = Some((list.clone(), words.clone()));
    window.set_columns(ModelRc::new(VecModel::from(
        favourites::COLUMNS
            .iter()
            .map(|c| SharedString::from(*c))
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
                let Some(&index) = list.order.get(shown) else {
                    return;
                };
                match (ctrl, shift, list.anchor) {
                    (_, true, Some(anchor)) => {
                        let from = list
                            .order
                            .iter()
                            .position(|&i| i == anchor)
                            .unwrap_or(shown);
                        let (a, b) = (from.min(shown), from.max(shown));
                        let range: Vec<usize> = list.order[a..=b].to_vec();
                        if !ctrl {
                            list.selected.clear();
                        }
                        list.selected.extend(range);
                    }
                    (true, _, _) => {
                        if !list.selected.remove(&index) {
                            list.selected.insert(index);
                        }
                        list.anchor = Some(index);
                    }
                    _ => {
                        list.selected = BTreeSet::from([index]);
                        list.anchor = Some(index);
                    }
                }
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
                match list.selected.iter().collect::<Vec<_>>()[..] {
                    [&index] => list
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
                {
                    let mut list = list.borrow_mut();
                    list.selected = BTreeSet::from([index]);
                    list.anchor = Some(index);
                }
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
            if list.borrow().selected.is_empty() {
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
                    let selected: Vec<usize> = list.selected.iter().copied().collect();
                    list.manager.delete(&selected);
                    list.selected.clear();
                    list.anchor = None;
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
            if let Some(edit) = slots.edit.borrow_mut().take() {
                let _ = edit.hide();
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
        let shown: Vec<FavouriteListRow> = list
            .order
            .iter()
            .map(|&i| FavouriteListRow {
                cells: ModelRc::new(VecModel::from(
                    rows[i]
                        .iter()
                        .map(|c| SharedString::from(c.as_str()))
                        .collect::<Vec<_>>(),
                )),
                selected: list.selected.contains(&i),
            })
            .collect();
        window.set_rows(ModelRc::new(VecModel::from(shown)));
        window.set_sort_column(i32::try_from(column).unwrap_or(0));
        window.set_ascending(ascending);
        window.set_can_edit(list.selected.len() == 1);
        window.set_can_delete(!list.selected.is_empty());
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
    if let Some(old) = slots.edit.borrow_mut().take() {
        let _ = old.hide();
    }
    let window = FavouriteEditWindow::new().map_err(|e| e.to_string())?;
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
        move || {
            let Some(window) = weak.upgrade() else { return };
            let edit = edit.borrow();
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
    window.on_typed_accepted({
        let weak = window.as_weak();
        let edit = edit.clone();
        let show = show.clone();
        move || {
            let Some(window) = weak.upgrade() else { return };
            let typed = window.get_typed().trim().to_owned();
            if typed.is_empty() {
                return;
            }
            match parse_api_search(&serde_json::json!([typed])) {
                Ok(parsed) => {
                    let mut edit = edit.borrow_mut();
                    for predicate in parsed {
                        if !edit.search.predicates.contains(&predicate) {
                            edit.search.predicates.push(predicate);
                        }
                    }
                    window.set_typed(SharedString::new());
                    window.set_error(SharedString::new());
                }
                Err(e) => window.set_error(e.to_string().into()),
            }
            show();
        }
    });
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
        move || {
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
        move || {
            let value = edit.borrow().value();
            {
                let mut list = list.borrow_mut();
                match target {
                    Target::New => list.manager.added(value.clone()),
                    Target::Existing(index) => list.manager.edited(index, value.clone()),
                }
                let index = list.manager.rows.len() - 1;
                list.selected = BTreeSet::from([index]);
                list.anchor = Some(index);
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
        move || {
            let Some(window) = weak.upgrade() else { return };
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
