//! The "filename tagging" dialog, bound (`ui/filename_tagging.slint`): a
//! "sidecars" tab, the paths with what each one's sidecars give
//! ([`file_preview`](crate::sidecars::file_preview)) and the routers that
//! read them (edited in the sidecar editors); then hydrus-gui-model's
//! [`filename_tagging`](crate::filename_tagging) for each real tag
//! service, a tab each, with the paths and the tags each gets shown again
//! as the options change. "apply" gives the tags for each path and the
//! routers to `done` (which starts the import, as the reference's does).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};

use hydrus_gui_model::filename_rules::{DELETE_QUESTION, Editor as RuleEditor, INTRO};
use hydrus_parse::sidecar::Router;
use hydrus_store::Store;
use hydrus_store::queues::PathTags;

use crate::filename_tagging::{
    DIRECTORIES, ServiceTagging, parse_quick_namespaces, parse_regexes, quick_namespaces_text, row,
    value,
};
use crate::list_selection::ListSelection;
use crate::{FilenameTaggingWindow, MiscRow, TableRow};

struct State {
    rules: Vec<RuleEditor>,
    regex: Option<crate::filename_regex_menu::Controls>,
    closed: bool,
    tags: crate::write_tag_window::Slot,
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
    /// One service's options alone: the example path whose tags are shown.
    example: Option<String>,
    /// The "sidecars" tab is the one shown.
    on_sidecars: bool,
    routers: Vec<Router>,
    /// The store (for service names) and the sidecar editors, with the
    /// "sidecars" tab.
    sidecars: Option<Sidecars>,
}

/// What the "sidecars" tab needs: the store, for its services, and the
/// sidecar editors' windows.
#[derive(Clone)]
pub(crate) struct Sidecars {
    pub(crate) store: Arc<Store>,
    pub(crate) slots: crate::sidecars_window::Slots,
}

/// What "apply" gives: the tags for each path, and the sidecar routers.
pub(crate) type Done = Rc<dyn Fn(PathTags, Vec<Router>)>;

impl State {
    fn can_edit(&self) -> bool {
        !self.closed
            && self.tags.borrow().is_none()
            && !self
                .regex
                .as_ref()
                .is_some_and(crate::filename_regex_menu::Controls::blocking)
            && !self.rules[self.current].asking()
            && !self
                .sidecars
                .as_ref()
                .is_some_and(|sidecars| sidecars.slots.routers.borrow().is_some())
    }
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

/// Show the "sidecars" tab's rows and its routers button.
fn show_sidecars(window: &FilenameTaggingWindow, state: &State) {
    window.set_on_sidecars(state.on_sidecars);
    let Some(sidecars) = &state.sidecars else {
        return;
    };
    let namer = crate::sidecars_window::namer(&sidecars.store);
    let rows: Vec<TableRow> = state
        .paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let strings = crate::sidecars::file_preview(&state.routers, path, &namer);
            let cells: Vec<SharedString> = vec![
                hydrus_core::numbers::human_int(i as u64 + 1).into(),
                path.as_str().into(),
                strings.join(" | ").into(),
            ];
            TableRow {
                cells: ModelRc::new(VecModel::from(cells)),
                selected: false,
            }
        })
        .collect();
    window.set_sidecar_rows(ModelRc::new(VecModel::from(rows)));
    window.set_sidecars(
        crate::folders_window::sidecars_label(&sidecars.store, &state.routers).into(),
    );
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
    if let Some(example) = &state.example {
        let tags: Vec<String> = tab.options.tags(example).into_iter().collect();
        window.set_example_tags(tags.join(", ").into());
    }
    window.set_has_selection(!state.selection.is_empty());
    window.set_errors(state.errors.join("\n").into());
    show_rules(window, state);
}

fn show_rules(window: &FilenameTaggingWindow, state: &State) {
    let editor = &state.rules[state.current];
    let (column, ascending) = editor.sorting();
    window.set_quick_sort_column(i32::try_from(column).unwrap_or(0));
    window.set_quick_ascending(ascending);
    let quick = editor.quick_rows();
    let selected = quick.iter().filter(|row| row.selected).count();
    window.set_quick_one_selected(selected == 1);
    window.set_quick_any_selected(selected > 0);
    let quick_rows = quick
        .into_iter()
        .map(|row| TableRow {
            cells: ModelRc::new(VecModel::from(vec![row.value.0.into(), row.value.1.into()])),
            selected: row.selected,
        })
        .collect::<Vec<_>>();
    window.set_quick_rows(ModelRc::new(VecModel::from(quick_rows)));
    let regex_rows = editor
        .regex_rows()
        .into_iter()
        .map(|row| TableRow {
            cells: ModelRc::new(VecModel::from(vec![row.value.into()])),
            selected: row.selected,
        })
        .collect::<Vec<_>>();
    window.set_regex_rows(ModelRc::new(VecModel::from(regex_rows)));
    window.set_regex_input(editor.input.as_str().into());
}

fn commit_rules(window: &FilenameTaggingWindow, state: &mut State) {
    let current = state.current;
    state.rules[current].update(&mut state.services[current].2.options);
    let options = &state.services[current].2.options;
    window.set_quick(quick_namespaces_text(&options.quick_namespaces).into());
    window.set_regexes(options.regexes.join("\n").into());
    show_rows(window, state);
}

fn bind_rules(window: &FilenameTaggingWindow, state: &Rc<RefCell<State>>) {
    window.on_rule_sort({
        let weak = window.as_weak();
        let state = state.clone();
        move |column, ascending| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if !state.can_edit() || state.on_sidecars {
                return;
            }
            if let Ok(column) = usize::try_from(column) {
                let current = state.current;
                state.rules[current].input = window.get_regex_input().into();
                state.rules[current].sort(column, ascending);
            }
            commit_rules(&window, &mut state);
        }
    });
    window.on_rule_clicked({
        let weak = window.as_weak();
        let state = state.clone();
        move |quick, index, ctrl, shift| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if !state.can_edit() || state.on_sidecars {
                return;
            }
            if let Ok(index) = usize::try_from(index) {
                let current = state.current;
                state.rules[current].input = window.get_regex_input().into();
                state.rules[current].click(quick, index, ctrl, shift);
            }
            show_rules(&window, &state);
        }
    });
    window.on_rule_action({
        let weak = window.as_weak();
        let state = state.clone();
        move |action| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if !state.can_edit() || state.on_sidecars {
                return;
            }
            state.errors.clear();
            let current = state.current;
            let editor = &mut state.rules[current];
            editor.input = window.get_regex_input().into();
            match action.as_str() {
                "quick_add" | "quick_edit" => {
                    if let Some((namespace, regex)) = editor.begin(action == "quick_edit") {
                        window.set_rule_namespace(namespace.into());
                        window.set_rule_regex(regex.into());
                        window.set_rule_message(INTRO.into());
                        window.set_rule_child(true);
                    }
                }
                "quick_delete" => {
                    if editor.request_delete() {
                        window.set_rule_message(DELETE_QUESTION.into());
                        window.set_rule_delete(true);
                    }
                }
                "regex_add" => {
                    editor.input = window.get_regex_input().into();
                    if let Err(error) = editor.add_regex() {
                        state.errors.push(error);
                    }
                }
                "regex_remove" => editor.remove_regexes(),
                _ => return,
            }
            commit_rules(&window, &mut state);
        }
    });
    window.on_rule_entered({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.closed
                || !window.get_rule_child()
                || window.get_regex_child_open()
                || window.get_regex_panes().row_count() > 0
            {
                return;
            }
            let current = state.current;
            match state.rules[current]
                .accept(&window.get_rule_namespace(), &window.get_rule_regex())
            {
                Ok(()) => {
                    state.errors.clear();
                    window.set_rule_child(false);
                }
                Err(error) => {
                    state.errors = vec![error];
                }
            }
            commit_rules(&window, &mut state);
        }
    });
    window.on_rule_cancel({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.closed || window.get_regex_child_open() {
                return;
            }
            let current = state.current;
            if window.get_regex_panes().row_count() > 0 {
                window.invoke_regex_dismissed();
                return;
            }
            state.rules[current].cancel();
            state.errors.clear();
            window.set_rule_child(false);
            window.set_rule_delete(false);
            show_rows(&window, &state);
        }
    });
    window.on_rule_answer({
        let weak = window.as_weak();
        let state = state.clone();
        move |yes| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if state.closed || !window.get_rule_delete() {
                return;
            }
            let current = state.current;
            state.rules[current].answer(yes);
            window.set_rule_delete(false);
            commit_rules(&window, &mut state);
        }
    });
}

/// Additive autocomplete children and direct paste keep the owner's service
/// and file selection frozen; only accepted tag deltas reach its draft.
fn bind_tags(window: &FilenameTaggingWindow, state: &Rc<RefCell<State>>, store: &Arc<Store>) {
    window.on_edit_tags({
        let owner = window.as_weak();
        let state = state.clone();
        let store = store.clone();
        move |single| {
            let (current, selected, before, service, slot) = {
                let state = state.borrow();
                if !state.can_edit() || state.on_sidecars || (single && state.selection.is_empty())
                {
                    return;
                }
                let selected = state.selected();
                let before = if single {
                    state.tab().selected_single(&selected)
                } else {
                    state.tab().options.tags_for_all.iter().cloned().collect()
                };
                let Ok(service) =
                    hydrus_core::ServiceKey::from_hex(&state.services[state.current].0)
                else {
                    return;
                };
                (state.current, selected, before, service, state.tags.clone())
            };
            let applied = Rc::new({
                let owner = owner.clone();
                let state = Rc::downgrade(&state);
                let before = before.clone();
                move |tags: Vec<String>, additions: Vec<String>| {
                    let (Some(owner), Some(state)) = (owner.upgrade(), state.upgrade()) else {
                        return;
                    };
                    let mut state = state.borrow_mut();
                    if state.closed || state.current != current {
                        return;
                    }
                    let tab = &mut state.services[current].2;
                    if single {
                        tab.apply_selected(&selected, &before, &tags, &additions);
                    } else {
                        tab.options.tags_for_all = tags.into_iter().collect();
                    }
                    state.errors.clear();
                    show(&owner, &mut state);
                }
            });
            let closed = Rc::new({
                let owner = owner.clone();
                move || {
                    if let Some(owner) = owner.upgrade() {
                        owner.set_tags_child_open(false);
                    }
                }
            });
            match crate::write_tag_window::open_additions(
                &store,
                service,
                &before,
                if single {
                    "tags just for selected files"
                } else {
                    "tags for all"
                },
                &slot,
                applied,
                closed,
            ) {
                Ok(_) => {
                    if let Some(owner) = owner.upgrade() {
                        owner.set_tags_child_open(true);
                    }
                }
                Err(error) => {
                    if let Some(owner) = owner.upgrade() {
                        owner.set_errors(error.to_string().into());
                    }
                }
            }
        }
    });
    window.on_paste_tags({
        let owner = window.as_weak();
        let state = state.clone();
        move |single| {
            let Some(owner) = owner.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if !state.can_edit() || state.on_sidecars || (single && state.selection.is_empty()) {
                return;
            }
            let raw = match crate::from_clipboard() {
                Ok(text) => text,
                Err(error) => {
                    state.errors = vec![format!("Problem pasting!\n{error}")];
                    show_rows(&owner, &state);
                    return;
                }
            };
            let tags = hydrus_gui_model::write_autocomplete::pasted_tags(&raw);
            if single {
                let selected = state.selected();
                state.tab_mut().add_single(&selected, &tags);
            } else {
                state.tab_mut().options.tags_for_all.extend(tags);
            }
            state.errors.clear();
            show(&owner, &mut state);
        }
    });
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
    show_misc(window, state);
    window.set_quick(quick_namespaces_text(&tab.options.quick_namespaces).into());
    window.set_regexes(tab.options.regexes.join("\n").into());
    window.set_number_base(i32::try_from(tab.number_base).unwrap_or(1));
    window.set_number_step(i32::try_from(tab.number_step).unwrap_or(1));
    window.set_number_namespace(tab.number_namespace.as_str().into());
    show_rows(window, state);
    show_sidecars(window, state);
}

/// Publish literal misc fields even when clearing an already checked namespace.
fn show_misc(window: &FilenameTaggingWindow, state: &State) {
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
}

/// Read the tab's two-way fields.
fn read(window: &FilenameTaggingWindow, state: &mut State) {
    if state.example.is_some() {
        state.example = Some(example_path(&window.get_example()));
    }
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
    let quick_changed =
        window.get_quick().as_str() != quick_namespaces_text(&state.tab().options.quick_namespaces);
    let regexes_changed = window.get_regexes().as_str() != state.tab().options.regexes.join("\n");
    let (quick, mut errors) = if quick_changed {
        parse_quick_namespaces(&window.get_quick())
    } else {
        (state.tab().options.quick_namespaces.clone(), Vec::new())
    };
    let (regexes, regex_errors) = if regexes_changed {
        parse_regexes(&window.get_regexes())
    } else {
        (state.tab().options.regexes.clone(), Vec::new())
    };
    errors.extend(regex_errors);
    state.errors = errors;
    let tab = state.tab_mut();
    // (cleaned, as the reference's tag list keeps them)
    tab.options.tags_for_all = lines(&window.get_tags_all())
        .iter()
        .filter_map(|t| hydrus_core::tag::clean_tag_checked(t))
        .collect();
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
    if quick_changed || regexes_changed {
        let current = state.current;
        state.rules[current] = RuleEditor::new(&state.tab().options);
    }
}

/// An example path as typed: without the quotes round it or a leading
/// "file:///" (`ScheduleRefreshTags`).
fn example_path(text: &str) -> String {
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text);
    text.strip_prefix("file:///")
        .map_or_else(|| text.to_owned(), |rest| format!("/{rest}"))
}

/// Open the dialog on one tag service's filename tagging options alone
/// ("edit filename tagging options", for an import folder), showing the
/// tags of `example`; "apply" gives the options to `done`.
pub(crate) fn open_options(
    store: &Arc<Store>,
    service: (String, String),
    options: hydrus_parse::folders::FilenameTagging,
    example: String,
    slot: &Rc<RefCell<Option<FilenameTaggingWindow>>>,
    done: Rc<dyn Fn(hydrus_parse::folders::FilenameTagging)>,
) -> Result<FilenameTaggingWindow, String> {
    let (window, state) = build(
        store,
        vec![service],
        Vec::new(),
        slot,
        None,
        Rc::new(|_, _| {}),
    )?;
    window.set_window_title("edit filename tagging options".into());
    window.set_options_mode(true);
    window.set_example(example.as_str().into());
    {
        let mut state = state.borrow_mut();
        state.example = Some(example);
        let misc = std::array::from_fn(|i| {
            if i == 0 {
                (
                    options.add_filename.is_some(),
                    options
                        .add_filename
                        .clone()
                        .unwrap_or_else(|| "filename".into()),
                )
            } else {
                let index = DIRECTORIES[i - 1].1;
                let set = options.directories.iter().find(|(d, _)| *d == index);
                (
                    set.is_some(),
                    set.map(|(_, n)| n.clone()).unwrap_or_default(),
                )
            }
        });
        state.misc[0] = misc;
        state.rules[0] = RuleEditor::new(&options);
        state.tab_mut().options = options;
        show(&window, &mut state);
    }
    window.on_apply({
        let state = state.clone();
        let slot = slot.clone();
        let weak = window.as_weak();
        move || {
            if !state.borrow().can_edit() {
                return;
            }
            let options = state.borrow().tab().options.clone();
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            state.borrow_mut().closed = true;
            slot.borrow_mut().take();
            done(options);
        }
    });
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

/// Open the dialog on `paths` for the real tag services (`(key hex,
/// name)`), with its "sidecars" tab; "apply" gives the tags for each path
/// and the sidecar routers to `done`. It forgets itself from `slot` when
/// closed.
pub(crate) fn open(
    services: Vec<(String, String)>,
    paths: Vec<String>,
    slot: &Rc<RefCell<Option<FilenameTaggingWindow>>>,
    sidecars: Sidecars,
    done: Done,
) -> Result<FilenameTaggingWindow, String> {
    let store = sidecars.store.clone();
    let (window, _) = build(&store, services, paths, slot, Some(sidecars), done)?;
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}

/// The dialog, bound but not shown, and its state.
fn build(
    store: &Arc<Store>,
    services: Vec<(String, String)>,
    paths: Vec<String>,
    slot: &Rc<RefCell<Option<FilenameTaggingWindow>>>,
    sidecars: Option<Sidecars>,
    done: Done,
) -> Result<(FilenameTaggingWindow, Rc<RefCell<State>>), String> {
    let window =
        crate::app_title::new::<crate::FilenameTaggingWindow>().map_err(|e| e.to_string())?;
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
        rules: (0..count).map(|_| RuleEditor::default()).collect(),
        regex: None,
        closed: false,
        tags: crate::write_tag_window::Slot::default(),
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
        example: None,
        on_sidecars: false,
        routers: Vec::new(),
        sidecars,
    }));
    if state.borrow().services.is_empty() {
        return Err("there are no tag services".into());
    }
    let close = {
        let weak = window.as_weak();
        let slot = slot.clone();
        let state = state.clone();
        move || {
            if state.borrow().closed {
                return;
            }
            state.borrow_mut().closed = true;
            let tags = state
                .borrow()
                .tags
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong);
            if let Some(tags) = tags {
                tags.invoke_cancel();
            }
            let regex = state.borrow().regex.clone();
            if let Some(regex) = regex {
                regex.cancel();
            }
            let sidecars = state.borrow().sidecars.clone();
            if let Some(sidecars) = sidecars {
                sidecars.slots.cancel();
            }
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
            if !state.can_edit() {
                return;
            }
            let current = state.current;
            state.rules[current].input = window.get_regex_input().into();
            if let Some(i) = usize::try_from(i)
                .ok()
                .filter(|i| *i < state.services.len())
            {
                state.current = i;
            }
            state.on_sidecars = false;
            show(&window, &mut state);
        }
    });
    window.on_sidecars_chosen({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            if !state.can_edit() {
                return;
            }
            state.on_sidecars = state.sidecars.is_some();
            show_sidecars(&window, &state);
        }
    });
    window.on_edit_sidecars({
        let weak = window.as_weak();
        let state = state.clone();
        move || {
            let (sidecars, routers) = {
                let state = state.borrow();
                if !state.can_edit() {
                    return;
                }
                let Some(sidecars) = state.sidecars.clone() else {
                    return;
                };
                (sidecars, state.routers.clone())
            };
            if sidecars.slots.routers.borrow().is_some() {
                return;
            }
            let applied: Rc<dyn Fn(Vec<Router>)> = Rc::new({
                let weak = weak.clone();
                let state = state.clone();
                move |routers| {
                    let mut state = state.borrow_mut();
                    if state.closed {
                        return;
                    }
                    state.routers = routers;
                    if let Some(window) = weak.upgrade() {
                        show_sidecars(&window, &state);
                    }
                }
            });
            sidecars.slots.set_test_objects(
                state
                    .borrow()
                    .paths
                    .iter()
                    .cloned()
                    .map(crate::sidecar_editors::TestObject::File)
                    .collect(),
            );
            match crate::sidecars_window::open_routers(
                &sidecars.store,
                crate::sidecar_editors::Context::Import,
                routers,
                &sidecars.slots,
                applied,
            ) {
                Ok(w) => *sidecars.slots.routers.borrow_mut() = Some(w),
                Err(e) => eprintln!("could not open the sidecars: {e}"),
            }
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
            if !state.can_edit() {
                return;
            }
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
            if !state.can_edit() {
                return;
            }
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
            if !state.can_edit() {
                return;
            }
            let current = state.current;
            if let Some(row) = usize::try_from(i)
                .ok()
                .and_then(|i| state.misc[current].get_mut(i))
            {
                row.0 = on;
            }
            state.apply_misc();
            show_misc(&window, &state);
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
            if !state.can_edit() {
                return;
            }
            let current = state.current;
            if let Some(row) = usize::try_from(i)
                .ok()
                .and_then(|i| state.misc[current].get_mut(i))
            {
                namespace.as_str().clone_into(&mut row.1);
                // (typing a namespace ticks its box, as the reference's
                // does)
                if !row.0 && !row.1.is_empty() {
                    row.0 = true;
                }
            }
            state.apply_misc();
            show_misc(&window, &state);
            show_rows(&window, &state);
        }
    });
    window.on_apply({
        let state = state.clone();
        let close = close.clone();
        move || {
            if !state.borrow().can_edit() {
                return;
            }
            let routers = state.borrow().routers.clone();
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
            done(tags, routers);
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
    let alive: Rc<dyn Fn() -> bool> = Rc::new({
        let state = Rc::downgrade(&state);
        let weak = window.as_weak();
        move || {
            weak.upgrade()
                .is_some_and(|window| !window.get_rule_delete())
                && state.upgrade().is_some_and(|state| {
                    let state = state.borrow();
                    !state.closed
                        && state.tags.borrow().is_none()
                        && !state.on_sidecars
                        && !state
                            .sidecars
                            .as_ref()
                            .is_some_and(|sidecars| sidecars.slots.routers.borrow().is_some())
                })
        }
    });
    state.borrow_mut().regex = Some(crate::filename_regex_menu::bind(&window, store, alive));
    bind_tags(&window, &state, store);
    bind_rules(&window, &state);
    show(&window, &mut state.borrow_mut());
    Ok((window, state))
}
