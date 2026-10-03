//! The "filename tagging" dialog, bound (`ui/filename_tagging.slint`):
//! hydrus-gui-model's [`filename_tagging`](crate::filename_tagging) for
//! each real tag service, a tab each, with the paths and the tags each
//! gets shown again as the options change. "apply" gives the tags for
//! each path to `done` (which starts the import, as the reference's does).

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle as _, ModelRc, SharedString, VecModel};

use hydrus_store::queues::PathTags;

use crate::filename_tagging::{
    DIRECTORIES, ServiceTagging, parse_quick_namespaces, parse_regexes, quick_namespaces_text, row,
    value,
};
use crate::list_selection::ListSelection;
use crate::{FilenameTaggingWindow, MiscRow, TableRow};

struct State {
    /// (key hex, name, its tab's options)
    services: Vec<(String, String, ServiceTagging)>,
    /// Each tab's "misc" rows as shown: ticked, and the namespace typed
    /// (kept while unticked, as the reference's boxes keep it).
    misc: Vec<[(bool, String); 7]>,
    current: usize,
    paths: Vec<String>,
    selection: ListSelection<usize>,
    /// The selected files' single tags as their box last showed them.
    shown_single: Vec<String>,
    errors: Vec<String>,
}

impl State {
    fn tab(&self) -> &ServiceTagging {
        &self.services[self.current].2
    }

    fn tab_mut(&mut self) -> &mut ServiceTagging {
        &mut self.services[self.current].2
    }

    /// Set the tab's filename and directories from its "misc" rows.
    fn apply_misc(&mut self) {
        let misc = self.misc[self.current].clone();
        let tab = self.tab_mut();
        tab.options.add_filename = misc[0].0.then(|| misc[0].1.clone());
        tab.options.directories = DIRECTORIES
            .iter()
            .zip(&misc[1..])
            .filter(|(_, (on, _))| *on)
            .map(|((_, index), (_, namespace))| (*index, namespace.clone()))
            .collect();
    }

    fn selected(&self) -> Vec<usize> {
        let order: Vec<usize> = (0..self.paths.len()).collect();
        self.selection.in_order(&order)
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Show the paths' rows and what is wrong.
fn show_rows(window: &FilenameTaggingWindow, state: &State) {
    let tab = state.tab();
    let rows: Vec<TableRow> = state
        .paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let cells: Vec<SharedString> = row(i, path, &tab.tags(i, path))
                .into_iter()
                .map(Into::into)
                .collect();
            TableRow {
                cells: ModelRc::new(VecModel::from(cells)),
                selected: state.selection.is_selected(i),
            }
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_has_selection(!state.selection.is_empty());
    window.set_errors(state.errors.join("\n").into());
}

/// Show the tab whole: its fields too (only as a tab or the selection
/// changes, so a field typed in is left be).
fn show(window: &FilenameTaggingWindow, state: &mut State) {
    let services: Vec<SharedString> = state.services.iter().map(|s| s.1.as_str().into()).collect();
    window.set_services(ModelRc::new(VecModel::from(services)));
    window.set_service_index(i32::try_from(state.current).unwrap_or(0));
    let selected = state.selected();
    state.shown_single = state.tab().selected_single(&selected);
    let tab = state.tab();
    let mut all: Vec<&String> = tab.options.tags_for_all.iter().collect();
    all.sort();
    window.set_tags_all(
        all.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .into(),
    );
    window.set_tags_selected(state.shown_single.join("\n").into());
    let labels = std::iter::once("add filename?").chain(DIRECTORIES.iter().map(|d| d.0));
    let misc: Vec<MiscRow> = labels
        .zip(&state.misc[state.current])
        .map(|(label, (on, namespace))| MiscRow {
            label: label.into(),
            on: *on,
            namespace: namespace.as_str().into(),
        })
        .collect();
    window.set_misc(ModelRc::new(VecModel::from(misc)));
    window.set_quick(quick_namespaces_text(&tab.options.quick_namespaces).into());
    window.set_regexes(tab.options.regexes.join("\n").into());
    window.set_number_base(i32::try_from(tab.number_base).unwrap_or(1));
    window.set_number_step(i32::try_from(tab.number_step).unwrap_or(1));
    window.set_number_namespace(tab.number_namespace.as_str().into());
    show_rows(window, state);
}

/// Read the tab's two-way fields.
fn read(window: &FilenameTaggingWindow, state: &mut State) {
    let selected = state.selected();
    let typed_single = lines(&window.get_tags_selected());
    let added: Vec<String> = typed_single
        .iter()
        .filter(|t| !state.shown_single.contains(t))
        .cloned()
        .collect();
    let removed: Vec<String> = state
        .shown_single
        .iter()
        .filter(|t| !typed_single.contains(t))
        .cloned()
        .collect();
    state.shown_single = typed_single;
    let (quick, mut errors) = parse_quick_namespaces(&window.get_quick());
    let (regexes, regex_errors) = parse_regexes(&window.get_regexes());
    errors.extend(regex_errors);
    state.errors = errors;
    let tab = state.tab_mut();
    tab.options.tags_for_all = lines(&window.get_tags_all()).into_iter().collect();
    if !selected.is_empty() {
        tab.add_single(&selected, &added);
        tab.remove_single(&selected, &removed);
    }
    tab.options.quick_namespaces = quick;
    tab.options.regexes = regexes;
    tab.number_base = i64::from(window.get_number_base());
    tab.number_step = i64::from(window.get_number_step());
    window
        .get_number_namespace()
        .trim()
        .clone_into(&mut tab.number_namespace);
}

/// Open the dialog on `paths` for the real tag services (`(key hex,
/// name)`); "apply" gives the tags for each path to `done`. It forgets
/// itself from `slot` when closed.
pub(crate) fn open(
    services: Vec<(String, String)>,
    paths: Vec<String>,
    slot: &Rc<RefCell<Option<FilenameTaggingWindow>>>,
    done: Rc<dyn Fn(PathTags)>,
) -> Result<FilenameTaggingWindow, String> {
    let window = FilenameTaggingWindow::new().map_err(|e| e.to_string())?;
    // (the reference's defaults: the filename's namespace "filename")
    let misc: [(bool, String); 7] = std::array::from_fn(|i| {
        (
            false,
            if i == 0 {
                "filename".to_owned()
            } else {
                String::new()
            },
        )
    });
    let count = services.len();
    let state = Rc::new(RefCell::new(State {
        misc: vec![misc; count],
        services: services
            .into_iter()
            .map(|(key, name)| (key, name, ServiceTagging::default()))
            .collect(),
        current: 0,
        paths,
        selection: ListSelection::default(),
        shown_single: Vec::new(),
        errors: Vec::new(),
    }));
    if state.borrow().services.is_empty() {
        return Err("there are no tag services".into());
    }
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        move || {
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            slot.borrow_mut().take();
        }
    };
    window.on_service_chosen({
        let weak = window.as_weak();
        let state = state.clone();
        move |i| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if let Some(i) = usize::try_from(i)
                .ok()
                .filter(|i| *i < state.services.len())
            {
                state.current = i;
            }
            show(&window, &mut state);
        }
    });
    window.on_row_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |r, ctrl, shift| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if let Ok(r) = usize::try_from(r) {
                let order: Vec<usize> = (0..state.paths.len()).collect();
                state.selection.click(&order, r, ctrl, shift);
            }
            show(&window, &mut state);
        }
    });
    window.on_changed({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            read(&window, &mut state);
            show_rows(&window, &state);
        }
    });
    // (a "misc" row changed: the rows alone shown again, so the box typed in
    // is left be)
    window.on_misc_toggled({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, on| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let current = state.current;
            if let Some(row) = usize::try_from(i)
                .ok()
                .and_then(|i| state.misc[current].get_mut(i))
            {
                row.0 = on;
            }
            state.apply_misc();
            show_rows(&window, &state);
        }
    });
    window.on_misc_namespace({
        let weak = window.as_weak();
        let state = state.clone();
        move |i, namespace| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let current = state.current;
            let mut tick = false;
            if let Some(row) = usize::try_from(i)
                .ok()
                .and_then(|i| state.misc[current].get_mut(i))
            {
                namespace.trim().clone_into(&mut row.1);
                // (typing a namespace ticks its box, as the reference's
                // does)
                if !row.0 && !row.1.is_empty() {
                    row.0 = true;
                    tick = true;
                }
            }
            state.apply_misc();
            if tick {
                show(&window, &mut state);
            } else {
                show_rows(&window, &state);
            }
        }
    });
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        move || {
            let tags: PathTags = {
                let state = state.borrow();
                let services: Vec<(String, ServiceTagging)> = state
                    .services
                    .iter()
                    .map(|(key, _, tagging)| (key.clone(), tagging.clone()))
                    .collect();
                value(&services, &state.paths)
                    .into_iter()
                    .map(|(path, by_service)| {
                        (
                            path.to_owned(),
                            by_service
                                .into_iter()
                                .map(|(key, tags)| (key.to_owned(), tags.into_iter().collect()))
                                .collect(),
                        )
                    })
                    .collect()
            };
            close();
            done(tags);
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
    show(&window, &mut state.borrow_mut());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
