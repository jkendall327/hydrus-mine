//! The tag filter editor, bound (`ui/tag_filter.slint`): hydrus-gui-model's
//! [`TagFilterEditor`] shown and driven, its lists selected as the
//! reference's select, "Remove all selected?" and the help asked in the
//! window's own panel, and "apply" handing the filter back.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_core::tag_filter::TagFilter;
use hydrus_store::Store;

use crate::list_selection::ListSelection;
use crate::tag_filter_editor::{
    BLACKLIST_TEST_NOTE, FAVOURITE_NAME, FavouriteTagFilters, GLOBAL_BOXES, HELP, INSTRUCTIONS,
    NO_FAVOURITES, REMOVE_SELECTED, TagFilterEditor, delete_favourite, export_favourite,
    import_favourite, overwrite_favourite, pretty_slice,
};
use crate::{TableRow, TagFilterWindow, Tick};

/// The window while it is open.
pub type Slot = Rc<RefCell<Option<TagFilterWindow>>>;

thread_local! {
    /// The editor opened last, for tests to reach.
    static LAST: RefCell<Option<slint::Weak<TagFilterWindow>>> = const { RefCell::new(None) };
}

/// The editor opened last, if it is still open.
pub fn last_opened() -> Option<TagFilterWindow> {
    LAST.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
        .filter(|w| w.window().is_visible())
}

/// The lists, as the window numbers them.
const WHITELIST: usize = 0;
const BLACKLIST: usize = 1;
const EXCLUDE: usize = 2;
const EXCEPT: usize = 3;

/// What the window's panel shows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Asking {
    /// "Remove all selected?" of a list.
    Remove(usize),
    Help,
    FavouriteMenu(String, Vec<String>),
    FavouriteName(TagFilter, bool),
    OverwriteFavourite(String, TagFilter, bool),
    DeleteFavourite(String),
    Exchange(bool),
    Error(String),
}

struct State {
    closed: bool,
    editor: TagFilterEditor,
    selections: [ListSelection<usize>; 4],
    asking: Option<Asking>,
    redundant: String,
    /// Clears the redundant-rule message after a moment.
    redundant_timer: slint::Timer,
}

impl State {
    fn list(&self, list: usize) -> Vec<String> {
        let view = self.editor.view();
        match list {
            WHITELIST => view.whitelist.list,
            BLACKLIST => view.blacklist.list,
            EXCLUDE => view.advanced_blacklist,
            _ => view.advanced_whitelist,
        }
    }

    fn selected(&self, list: usize) -> Vec<String> {
        let entries = self.list(list);
        let order: Vec<usize> = (0..entries.len()).collect();
        self.selections[list]
            .in_order(&order)
            .into_iter()
            .filter_map(|i| entries.get(i).cloned())
            .collect()
    }

    /// Take `slices` out of a list (double-clicked, or removed after
    /// asking).
    fn remove(&mut self, list: usize, slices: &[String]) {
        match list {
            WHITELIST => self.editor.remove_simple_whitelist(slices),
            BLACKLIST => self.editor.remove_simple_blacklist(slices),
            EXCLUDE => self.editor.delete_advanced_blacklist(slices),
            _ => self.editor.delete_advanced_whitelist(slices),
        }
        self.changed();
    }

    /// After a change: the lists' selections go, and any message that an
    /// entry was already covered is shown for two seconds.
    fn changed(&mut self) {
        self.selections = Default::default();
        if let Some(said) = self.editor.take_redundant() {
            self.redundant = said;
        }
    }
}

fn strings(items: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    ModelRc::new(VecModel::from(items))
}

fn ticks(labels: impl IntoIterator<Item = String>, on: &[bool]) -> ModelRc<Tick> {
    let ticks: Vec<Tick> = labels
        .into_iter()
        .zip(on)
        .map(|(label, &on)| Tick {
            label: label.into(),
            on,
        })
        .collect();
    ModelRc::new(VecModel::from(ticks))
}

fn rows(entries: &[String], selection: &ListSelection<usize>) -> ModelRc<TableRow> {
    let rows: Vec<TableRow> = entries
        .iter()
        .enumerate()
        .map(|(i, slice)| TableRow {
            cells: strings([pretty_slice(slice)]),
            selected: selection.is_selected(i),
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

/// Each tag typed's sibling chain across the real tag services, itself
/// included (`tag_siblings_lookup` on all known tags).
fn siblings(store: &Store, tags: &[String]) -> Vec<Vec<String>> {
    let snapshot = store.snapshot();
    let services: Vec<_> = snapshot
        .services
        .all()
        .filter(|s| s.service_type().is_real_tag_service())
        .map(|s| s.id)
        .collect();
    tags.iter()
        .map(|tag| {
            let chain = store.read(|conn| {
                let Some(parsed) = hydrus_core::Tag::new(tag) else {
                    return Ok(Vec::new());
                };
                let Some(id) = hydrus_store::master::tag_id(conn, &parsed)? else {
                    return Ok(Vec::new());
                };
                let mut ids = vec![id];
                for &service in &services {
                    for chained in snapshot.display.get(service).chain(id) {
                        if !ids.contains(&chained) {
                            ids.push(chained);
                        }
                    }
                }
                Ok(hydrus_store::master::tags(conn, &ids)?
                    .into_values()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>())
            });
            match chain {
                Ok(chain) if !chain.is_empty() => chain,
                _ => vec![tag.clone()],
            }
        })
        .collect()
}

fn show(window: &TagFilterWindow, state: &State, store: &Store) {
    let editor = &state.editor;
    let view = editor.view();
    let tabs: Vec<String> = editor.tabs().iter().map(|t| t.label().to_owned()).collect();
    window.set_tabs(strings(tabs));
    let advanced = store
        .read(hydrus_store::settings::get::<hydrus_store::settings::AdvancedMode>)
        .unwrap_or_default()
        .0;
    window.set_show_other_panels(editor.show_other_panels_offered(advanced));
    window.set_show_other_panels_tooltip(
        hydrus_gui_model::tag_filter_editor::SHOW_OTHER_PANELS_TOOLTIP.into(),
    );
    let namespaces: Vec<String> = editor.namespaces().to_vec();
    let global = || GLOBAL_BOXES.iter().map(|&b| b.to_owned());
    window.set_white_enabled(view.whitelist.enabled);
    window.set_white_error(view.whitelist.error.into());
    window.set_white_global(ticks(global(), &view.whitelist.global));
    window.set_white_namespaces(ticks(namespaces.clone(), &view.whitelist.namespaces));
    window.set_white_namespaces_enabled(view.whitelist.namespaces_enabled);
    window.set_white_rows(rows(&view.whitelist.list, &state.selections[WHITELIST]));
    window.set_black_enabled(view.blacklist.enabled);
    window.set_black_error(view.blacklist.error.into());
    window.set_black_global(ticks(global(), &view.blacklist.global));
    window.set_black_namespaces(ticks(namespaces, &view.blacklist.namespaces));
    window.set_black_namespaces_enabled(view.blacklist.namespaces_enabled);
    window.set_black_rows(rows(&view.blacklist.list, &state.selections[BLACKLIST]));
    window.set_exclude_rows(rows(&view.advanced_blacklist, &state.selections[EXCLUDE]));
    window.set_except_rows(rows(&view.advanced_whitelist, &state.selections[EXCEPT]));
    window.set_except_input_enabled(view.except_input_enabled);
    window.set_redundant(state.redundant.as_str().into());
    window.set_current(view.current.into());
    let typed = window.get_test_input();
    let (result, good) = editor.test(&typed, &|tags| siblings(store, tags));
    window.set_test_result(result.into());
    window.set_test_good(match good {
        None => 0,
        Some(true) => 1,
        Some(false) => -1,
    });
    window.set_asking(state.asking.is_some());
    window.set_favourite_naming(matches!(state.asking, Some(Asking::FavouriteName(..))));
    window.set_favourite_exchange(matches!(state.asking, Some(Asking::Exchange(..))));
    let Some(asking) = state.asking.as_ref() else {
        return;
    };
    let (title, message, choices): (&str, String, Vec<String>) = match asking {
        Asking::Remove(_) => (
            "Are you sure?",
            REMOVE_SELECTED.into(),
            vec!["yes".into(), "no".into()],
        ),
        Asking::Help => ("information", HELP.into(), vec!["ok".into()]),
        Asking::FavouriteMenu(action, names) => (
            action,
            String::new(),
            if names.is_empty() {
                vec![NO_FAVOURITES.into()]
            } else {
                names.clone()
            },
        ),
        Asking::FavouriteName(..) => (
            "save favourite",
            FAVOURITE_NAME.into(),
            vec!["cancel".into()],
        ),
        Asking::OverwriteFavourite(name, ..) => (
            "Are you sure?",
            overwrite_favourite(name),
            vec!["yes".into(), "no".into()],
        ),
        Asking::DeleteFavourite(name) => (
            "Are you sure?",
            delete_favourite(name),
            vec!["yes".into(), "no".into()],
        ),
        Asking::Exchange(importing) => (
            if *importing {
                "import favourite"
            } else {
                "export favourite"
            },
            String::new(),
            vec!["cancel".into()],
        ),
        Asking::Error(message) => ("Problem importing!", message.clone(), vec!["ok".into()]),
    };
    window.set_asking_title(title.into());
    window.set_asking_message(message.into());
    window.set_asking_choices(strings(choices));
}

/// Open the editor on `filter`, titled `title` (`message` explaining it,
/// if not empty); "apply" calls `applied` with the filter edited. It
/// forgets itself from `slot` when closed.
pub fn open(
    store: &Arc<Store>,
    filter: &TagFilter,
    blacklist_only: bool,
    title: &str,
    message: &str,
    slot: &Slot,
    applied: Rc<dyn Fn(TagFilter)>,
) -> Result<TagFilterWindow, slint::PlatformError> {
    let window = TagFilterWindow::new()?;
    window.set_window_title(title.into());
    window.set_message(message.into());
    window.set_instructions(INSTRUCTIONS.into());
    let namespaces = store
        .read(hydrus_store::settings::get::<hydrus_parse::Downloaders>)
        .unwrap_or_default()
        .parser_namespaces();
    let editor = TagFilterEditor::new(filter, blacklist_only, &namespaces);
    let start = editor.tabs().iter().position(|&t| t == editor.start_tab());
    window.set_tab(start.and_then(|t| i32::try_from(t).ok()).unwrap_or(0));
    window.set_test_note(if blacklist_only {
        BLACKLIST_TEST_NOTE.into()
    } else {
        SharedString::new()
    });
    let state = Rc::new(RefCell::new(State {
        editor,
        closed: false,
        selections: Default::default(),
        asking: None,
        redundant: String::new(),
        redundant_timer: slint::Timer::default(),
    }));
    let refresh: Rc<dyn Fn()> = {
        let weak = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        Rc::new(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let state_ref = state.borrow();
            show(&window, &state_ref, &store);
            if !state_ref.redundant.is_empty() && !state_ref.redundant_timer.running() {
                let state = state.clone();
                let weak = weak.clone();
                let store = store.clone();
                state_ref.redundant_timer.start(
                    slint::TimerMode::SingleShot,
                    std::time::Duration::from_secs(2),
                    move || {
                        state.borrow_mut().redundant.clear();
                        if let Some(window) = weak.upgrade() {
                            show(&window, &state.borrow(), &store);
                        }
                    },
                );
            }
        })
    };
    refresh();
    let change = {
        let state = state.clone();
        let refresh = refresh.clone();
        move |f: &dyn Fn(&mut State)| {
            {
                let mut state = state.borrow_mut();
                if state.closed || state.asking.is_some() {
                    return;
                }
                f(&mut state);
            }
            refresh();
        }
    };
    window.on_tab_chosen({
        let weak = window.as_weak();
        move |tab| {
            if let Some(window) = weak.upgrade() {
                window.set_tab(tab);
            }
        }
    });
    window.on_global_clicked({
        let change = change.clone();
        move |list, i| {
            let Ok(i) = usize::try_from(i) else { return };
            change(&|state| {
                if list == 0 {
                    state.editor.whitelist_global(i);
                } else {
                    state.editor.blacklist_global(i);
                }
                state.changed();
            });
        }
    });
    window.on_namespace_clicked({
        let change = change.clone();
        move |list, i| {
            let Ok(i) = usize::try_from(i) else { return };
            change(&|state| {
                if list == 0 {
                    state.editor.whitelist_namespace(i);
                } else {
                    state.editor.blacklist_namespace(i);
                }
                state.changed();
            });
        }
    });
    window.on_row_clicked({
        let change = change.clone();
        move |list, row, ctrl, shift| {
            let (Ok(list), Ok(row)) = (usize::try_from(list), usize::try_from(row)) else {
                return;
            };
            change(&|state| {
                if list < 4 {
                    let order: Vec<usize> = (0..state.list(list).len()).collect();
                    state.selections[list].click(&order, row, ctrl, shift);
                }
            });
        }
    });
    // a double-click takes the entries selected out (`_Activate`)
    window.on_row_activated({
        let change = change.clone();
        move |list, row| {
            let (Ok(list), Ok(row)) = (usize::try_from(list), usize::try_from(row)) else {
                return;
            };
            change(&|state| {
                if list >= 4 {
                    return;
                }
                let mut slices = state.selected(list);
                if slices.is_empty() {
                    slices.extend(state.list(list).get(row).cloned());
                }
                if !slices.is_empty() {
                    state.remove(list, &slices);
                }
            });
        }
    });
    window.on_typed({
        let change = change.clone();
        move |list, text| {
            let typed: Vec<String> = if text.contains('\n') {
                text.lines().map(str::to_owned).collect()
            } else {
                vec![text.to_string()]
            };
            change(&|state| {
                match list {
                    0 => state.editor.add_simple_whitelist(&typed),
                    1 => state.editor.add_simple_blacklist(&typed),
                    2 => state.editor.add_advanced_blacklist(&typed),
                    _ => state.editor.add_advanced_whitelist(&typed),
                }
                state.changed();
            });
        }
    });
    window.on_remove({
        let change = change.clone();
        move |list| {
            let Ok(list) = usize::try_from(list) else {
                return;
            };
            change(&|state| {
                if list < 4 && !state.selected(list).is_empty() {
                    state.asking = Some(Asking::Remove(list));
                }
            });
        }
    });
    window.on_show_panels({
        let change = change.clone();
        move || change(&|state| state.editor.show_other_panels())
    });
    window.on_paste({
        let state = state.clone();
        let refresh = refresh.clone();
        move |list| {
            let Ok(list) = usize::try_from(list) else {
                return;
            };
            let mut s = state.borrow_mut();
            if s.closed || s.asking.is_some() || list >= 4 {
                return;
            }
            match crate::from_clipboard() {
                Ok(text) => {
                    s.editor.paste_slices(list, &text);
                    s.changed();
                }
                Err(e) => s.asking = Some(Asking::Error(format!("Problem pasting! {e}"))),
            }
            drop(s);
            refresh();
        }
    });
    window.on_block_everything({
        let change = change.clone();
        move || {
            change(&|state| {
                state.editor.block_everything();
                state.changed();
            });
        }
    });
    window.on_test_edited({
        let refresh = refresh.clone();
        move || refresh()
    });
    window.on_help({
        let change = change.clone();
        move || change(&|state| state.asking = Some(Asking::Help))
    });
    window.on_favourite({
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        move |action| {
            if state.borrow().closed || state.borrow().asking.is_some() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let result = (|| -> Result<Asking, String> {
                match action.as_str() {
                    "save" => {
                        w.set_favourite_name("".into());
                        Ok(Asking::FavouriteName(state.borrow().editor.value(), false))
                    }
                    "import" => {
                        w.set_exchange_importing(true);
                        match crate::from_clipboard() {
                            Ok(text) => {
                                w.set_exchange_text(text.into());
                                w.set_exchange_error("".into());
                            }
                            Err(error) => {
                                w.set_exchange_text("".into());
                                w.set_exchange_error(format!("Problem importing! {error}").into());
                            }
                        }
                        w.set_exchange_path("".into());
                        Ok(Asking::Exchange(true))
                    }
                    action => {
                        let saved = FavouriteTagFilters::load(&store).map_err(|e| e.to_string())?;
                        let mut names: Vec<String> = saved.0.into_iter().map(|(n, _)| n).collect();
                        if action == "export" {
                            names.insert(0, "this tag filter".into());
                        }
                        Ok(Asking::FavouriteMenu(action.into(), names))
                    }
                }
            })();
            state.borrow_mut().asking = Some(result.unwrap_or_else(Asking::Error));
            refresh();
        }
    });
    window.on_favourite_named({
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        move |name| {
            if name.is_empty() {
                return;
            }
            let mut s = state.borrow_mut();
            let Some(Asking::FavouriteName(filter, imported)) = s.asking.clone() else {
                return;
            };
            match FavouriteTagFilters::save(&store, name.to_string(), filter.clone(), false) {
                Ok(false) => {
                    s.asking = Some(Asking::OverwriteFavourite(
                        name.to_string(),
                        filter,
                        imported,
                    ));
                }
                Ok(true) => {
                    if imported {
                        s.editor.set_value(&filter);
                        s.changed();
                    }
                    s.asking = None;
                }
                Err(e) => s.asking = Some(Asking::Error(e.to_string())),
            }
            drop(s);
            refresh();
        }
    });
    window.on_exchange_action({
        let state = state.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        move |action| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let Some(Asking::Exchange(importing)) = state.borrow().asking.clone() else {
                return;
            };
            let result = (|| -> Result<(), String> {
                match action.as_str() {
                    "paste" => w.set_exchange_text(crate::from_clipboard()?.into()),
                    "copy" => crate::copy_to_clipboard(w.get_exchange_text().as_str()),
                    "browse" => {
                        let dialog = rfd::FileDialog::new()
                            .add_filter("Hydrus tag filter JSON", &["json", "txt"]);
                        let path = if importing {
                            dialog.pick_file()
                        } else {
                            dialog.set_file_name("tag_filter.json").save_file()
                        };
                        if let Some(path) = path {
                            w.set_exchange_path(path.to_string_lossy().as_ref().into());
                        }
                    }
                    "save" => {
                        let path = w.get_exchange_path();
                        if path.is_empty() {
                            return Err("Choose an export path first.".into());
                        }
                        let parent = std::path::Path::new(path.as_str())
                            .parent()
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or_else(|| std::path::Path::new("."));
                        let mut file =
                            tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
                        std::io::Write::write_all(&mut file, w.get_exchange_text().as_bytes())
                            .map_err(|e| e.to_string())?;
                        file.persist(path.as_str()).map_err(|e| e.to_string())?;
                    }
                    "open" | "import" => {
                        let text = if action == "open" {
                            use std::io::Read as _;
                            let file = std::fs::File::open(w.get_exchange_path().as_str())
                                .map_err(|e| e.to_string())?;
                            let mut text = String::new();
                            file.take(16 * 1024 * 1024 + 1)
                                .read_to_string(&mut text)
                                .map_err(|e| e.to_string())?;
                            text
                        } else {
                            w.get_exchange_text().to_string()
                        };
                        let filter = import_favourite(&text)?;
                        w.set_favourite_name("".into());
                        state.borrow_mut().asking = Some(Asking::FavouriteName(filter, true));
                    }
                    _ => (),
                }
                Ok(())
            })();
            w.set_exchange_error(result.err().unwrap_or_default().into());
            refresh();
        }
    });
    let answer = {
        let state = state.clone();
        let store = store.clone();
        let refresh = refresh.clone();
        let weak = window.as_weak();
        move |index: Option<usize>| {
            let mut s = state.borrow_mut();
            let Some(asking) = s.asking.take() else {
                return;
            };
            let result = (|| -> Result<(), String> {
                match (asking, index) {
                    (Asking::Remove(list), Some(0)) => {
                        let slices = s.selected(list);
                        s.remove(list, &slices);
                    }
                    (Asking::FavouriteMenu(action, names), Some(i)) => {
                        let Some(name) = names.get(i) else {
                            return Ok(());
                        };
                        let current = action == "export" && i == 0;
                        let filter = if current {
                            Some(s.editor.value())
                        } else {
                            FavouriteTagFilters::load(&store)
                                .map_err(|e| e.to_string())?
                                .get(name)
                        };
                        if let Some(filter) = filter {
                            match action.as_str() {
                                "load" => {
                                    s.editor.set_value(&filter);
                                    s.changed();
                                }
                                "delete" => s.asking = Some(Asking::DeleteFavourite(name.clone())),
                                "export" => {
                                    if let Some(w) = weak.upgrade() {
                                        let text = export_favourite(&filter);
                                        crate::copy_to_clipboard(&text);
                                        w.set_exchange_text(text.into());
                                        w.set_exchange_path("".into());
                                        w.set_exchange_error("".into());
                                        w.set_exchange_importing(false);
                                        s.asking = Some(Asking::Exchange(false));
                                    }
                                }
                                _ => (),
                            }
                        }
                    }
                    (Asking::OverwriteFavourite(name, filter, imported), Some(0)) => {
                        FavouriteTagFilters::save(&store, name, filter.clone(), true)
                            .map_err(|e| e.to_string())?;
                        if imported {
                            s.editor.set_value(&filter);
                            s.changed();
                        }
                    }
                    (Asking::DeleteFavourite(name), Some(0)) => {
                        FavouriteTagFilters::delete(&store, name).map_err(|e| e.to_string())?;
                    }
                    _ => (),
                }
                Ok(())
            })();
            if let Err(e) = result {
                s.asking = Some(Asking::Error(e));
            }
            drop(s);
            refresh();
        }
    };
    window.on_chosen({
        let answer = answer.clone();
        move |i| answer(usize::try_from(i).ok())
    });
    window.on_cancelled(move || answer(None));
    let active = Rc::new(Cell::new(true));
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let active = active.clone();
        let state = state.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            state.borrow_mut().asking = None;
            state.borrow_mut().closed = true;
            let window = weak.upgrade();
            if let Some(window) = &window {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
            if let Some(window) = window {
                window.invoke_closed();
            }
        }
    };
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        let active = active.clone();
        move || {
            if !active.get() {
                return;
            }
            let filter = state.borrow().editor.value();
            close();
            applied(filter);
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
