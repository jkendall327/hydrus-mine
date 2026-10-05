//! Options-owned call table. Every mutation updates its detached draft; parent
//! Apply alone writes registered calls. Pending questions capture identities.
use crate::external_call_window::{Slots, ask};
use crate::{OptionsWindow, TableRow};
use hydrus_core::external_calls::{ActualCall, Callable};
use hydrus_gui_model::{external_calls::Table, options::Editor};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};
pub(crate) struct Binding {
    pub show: Rc<dyn Fn()>,
    pub cancel: Rc<dyn Fn()>,
    pub has_open: Rc<dyn Fn() -> bool>,
}
fn defaults(
    slots: &Slots,
    active: &Rc<Cell<bool>>,
    calls: Vec<Callable>,
    accepted: Rc<dyn Fn(Vec<Callable>)>,
) -> Result<(), String> {
    let w = crate::ExternalDefaultsWindow::new().map_err(|e| e.to_string())?;
    let selected = Rc::new(RefCell::new(vec![false; calls.len()]));
    let alive = Rc::new(Cell::new(true));
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let selected = selected.clone();
        let names = calls.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
        move || {
            if let Some(w) = weak.upgrade() {
                w.set_rows(ModelRc::new(VecModel::from(
                    names
                        .iter()
                        .enumerate()
                        .map(|(i, name)| TableRow {
                            cells: ModelRc::new(VecModel::from(vec![name.as_str().into()])),
                            selected: selected.borrow()[i],
                        })
                        .collect::<Vec<_>>(),
                )));
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let slot = Rc::downgrade(&slots.defaults);
        let alive = alive.clone();
        move || {
            if !alive.replace(false) {
                return;
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    w.on_toggled({
        let selected = selected.clone();
        let alive = alive.clone();
        let active = active.clone();
        let show = show.clone();
        move |i| {
            if !alive.get() || !active.get() {
                return;
            }
            if let Ok(i) = usize::try_from(i)
                && let Some(v) = selected.borrow_mut().get_mut(i)
            {
                *v = !*v;
            }
            show();
        }
    });
    w.on_apply({
        let active = active.clone();
        let alive = alive.clone();
        let close = close.clone();
        move || {
            if !alive.get() || !active.get() {
                return;
            }
            let calls = calls
                .iter()
                .zip(selected.borrow().iter())
                .filter(|(_, on)| **on)
                .map(|(c, _)| c.clone())
                .collect();
            close();
            accepted(calls);
        }
    });
    w.on_cancel({
        let close = close.clone();
        move || {
            close();
        }
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    *slots.defaults.borrow_mut() = Some(w.clone_strong());
    show();
    w.show().map_err(|e| e.to_string())
}

struct DuplicateState {
    pending: std::collections::VecDeque<Callable>,
    added: Vec<[u8; 32]>,
}
fn duplicate_next(
    slots: &Slots,
    active: &Rc<Cell<bool>>,
    table: &Rc<RefCell<Table>>,
    state: &Rc<RefCell<DuplicateState>>,
    changed: &Rc<dyn Fn()>,
) {
    if !active.get() {
        return;
    }
    loop {
        let next = state.borrow_mut().pending.pop_front();
        let Some(call) = next else {
            let mut selected = table
                .borrow()
                .selected()
                .iter()
                .map(|c| c.key)
                .collect::<Vec<_>>();
            selected.extend_from_slice(&state.borrow().added);
            let mut table = table.borrow_mut();
            table.selection.select_many(&selected);
            let (column, ascending) = (table.sort_column, table.ascending);
            table.sort(column, ascending);
            drop(table);
            changed();
            return;
        };
        if let ActualCall::Process(process) = &call.call
            && let Some(issue) = process.import_warning()
        {
            let message = format!(
                "Hey, a call you are trying to import, \"{}\", seems to be a bit weird. Are you sure you want to import it? The problem is:\n\n{issue}",
                call.name
            );
            let done = Rc::new({
                let slots = slots.clone();
                let active = active.clone();
                let table = table.clone();
                let state = state.clone();
                let changed = changed.clone();
                move |yes| {
                    if !active.get() {
                        return;
                    }
                    if yes {
                        let key = table.borrow_mut().append(call.clone());
                        state.borrow_mut().added.push(key);
                        changed();
                        duplicate_next(&slots, &active, &table, &state, &changed);
                    }
                    // A declined later import aborts the remainder, retaining unselected prefix additions.
                }
            });
            let _ = ask(&slots.question, active, message, done);
            return;
        }
        let key = table.borrow_mut().append(call);
        state.borrow_mut().added.push(key);
        changed();
    }
}

pub(crate) fn bind(
    store: &Arc<Store>,
    w: &OptionsWindow,
    editor: &Rc<RefCell<Editor>>,
    active: &Rc<Cell<bool>>,
    slots: &Slots,
) -> Binding {
    let table = Rc::new(RefCell::new(Table::new(
        editor.borrow().edited_external_calls(),
    )));
    let has_open: Rc<dyn Fn() -> bool> = Rc::new({
        let child = Rc::downgrade(&slots.editor);
        let question = Rc::downgrade(&slots.question);
        let defaults = Rc::downgrade(&slots.defaults);
        let exchange = Rc::downgrade(&slots.exchange.0);
        move || {
            child.upgrade().is_some_and(|s| s.borrow().is_some())
                || question.upgrade().is_some_and(|s| s.borrow().is_some())
                || defaults.upgrade().is_some_and(|s| s.borrow().is_some())
                || exchange.upgrade().is_some_and(|s| s.borrow().is_some())
        }
    });
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let table = table.clone();
        let has_open = has_open.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                let table = table.borrow();
                w.set_external_call_rows(ModelRc::new(VecModel::from(
                    table
                        .manager
                        .calls
                        .iter()
                        .map(|c| TableRow {
                            cells: ModelRc::new(VecModel::from(vec![
                                c.name.as_str().into(),
                                c.pipeline.label().into(),
                                c.call.description().into(),
                            ])),
                            selected: table.selection.is_selected(c.key),
                        })
                        .collect::<Vec<_>>(),
                )));
                w.set_external_call_selected(!table.selection.is_empty());
                w.set_external_call_single(table.selection.one().is_some());
                w.set_external_call_sort_column(i32::try_from(table.sort_column).unwrap_or(0));
                w.set_external_call_ascending(table.ascending);
                w.set_external_call_child_open(has_open());
            }
        }
    });
    let changed: Rc<dyn Fn()> = Rc::new({
        let table = table.clone();
        let editor = editor.clone();
        let show = show.clone();
        move || {
            editor
                .borrow_mut()
                .set_external_calls(table.borrow().manager.clone());
            show();
        }
    });
    w.on_external_call_clicked({
        let table = table.clone();
        let active = active.clone();
        let has_open = has_open.clone();
        let show = show.clone();
        move |i, c, s| {
            if !active.get() || has_open() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                table.borrow_mut().click(i, c, s);
            }
            show();
        }
    });
    w.on_external_call_sort({
        let table = table.clone();
        let active = active.clone();
        let has_open = has_open.clone();
        let changed = changed.clone();
        move |i, ascending| {
            if !active.get() || has_open() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                table.borrow_mut().sort(i, ascending);
            }
            changed();
        }
    });
    w.on_external_call_activated({
        let weak = w.as_weak();
        move |i| {
            if let Some(w) = weak.upgrade() {
                w.invoke_external_call_clicked(i, false, false);
                w.invoke_external_call_action("edit".into());
            }
        }
    });
    w.on_external_call_action({
        let store = store.clone();
        let weak = w.as_weak();
        let slots = slots.clone();
        let active = active.clone();
        let has_open = has_open.clone();
        let table = table.clone();
        let show = show.clone();
        let changed = changed.clone();
        move |action| {
            if !active.get() || has_open() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let result: Result<(), String> = match action.as_str() {
                "add" | "edit" => {
                    let call = if action == "add" {
                        Some(Callable::new("new call"))
                    } else {
                        table.borrow().selection.one().and_then(|key| {
                            table
                                .borrow()
                                .manager
                                .calls
                                .iter()
                                .find(|c| c.key == key)
                                .cloned()
                        })
                    };
                    let Some(call) = call else {
                        return;
                    };
                    let editing = (action == "edit").then_some(call.key);
                    let done = Rc::new({
                        let table = table.clone();
                        let active = active.clone();
                        let changed = changed.clone();
                        let owner = weak.clone();
                        move |call| {
                            if !active.get() || owner.upgrade().is_none() {
                                return;
                            }
                            if let Some(key) = editing {
                                table.borrow_mut().replace(key, call);
                            } else {
                                table.borrow_mut().add(call);
                            }
                            changed();
                        }
                    });
                    crate::external_call_window::open(&store, &slots, call, done).map(|_| ())
                }
                "duplicate" => {
                    let state = Rc::new(RefCell::new(DuplicateState {
                        pending: table.borrow().selected().into(),
                        added: Vec::new(),
                    }));
                    duplicate_next(&slots, &active, &table, &state, &changed);
                    Ok(())
                }
                "delete" => {
                    let keys = table
                        .borrow()
                        .selected()
                        .iter()
                        .map(|c| c.key)
                        .collect::<Vec<_>>();
                    if keys.is_empty() {
                        return;
                    }
                    let done = Rc::new({
                        let active = active.clone();
                        let table = table.clone();
                        let changed = changed.clone();
                        move |yes| {
                            if active.get() && yes {
                                table.borrow_mut().delete(&keys);
                                changed();
                            }
                        }
                    });
                    ask(
                        &slots.question,
                        &active,
                        "Remove all selected?".into(),
                        done,
                    )
                    .map(|_| ())
                }
                "defaults" | "defaults-all" => {
                    let all = action == "defaults-all";
                    let done = Rc::new({
                        let slots = slots.clone();
                        let active = active.clone();
                        let table = table.clone();
                        let changed = changed.clone();
                        move |platform_only| {
                            if !active.get() {
                                return;
                            }
                            let calls =
                                hydrus_downloader_exchange::external_calls::defaults(platform_only)
                                    .unwrap_or_default();
                            let applied = Rc::new({
                                let table = table.clone();
                                let active = active.clone();
                                let changed = changed.clone();
                                move |calls| {
                                    if active.get() {
                                        table.borrow_mut().add_selected(calls);
                                        changed();
                                    }
                                }
                            });
                            if all {
                                applied(calls);
                            } else {
                                let _ = defaults(&slots, &active, calls, applied);
                            }
                        }
                    });
                    let question = ask(
                        &slots.question,
                        &active,
                        format!(
                            "Want to see the calls just for your platform ({}) or everything?",
                            if cfg!(windows) {
                                "Windows"
                            } else if cfg!(target_os = "macos") {
                                "macOS"
                            } else {
                                "Linux"
                            }
                        ),
                        done,
                    );
                    if let Ok(q) = &question {
                        q.set_yes_label("just for my platform".into());
                        q.set_no_label("no, show me everything".into());
                    }
                    question.map(|_| ())
                }
                "import" | "export" => {
                    let importing = action == "import";
                    let calls = if importing {
                        Vec::new()
                    } else {
                        table.borrow().selected()
                    };
                    let preview = Rc::new(|calls: Vec<Callable>| {
                        for c in &calls {
                            if let ActualCall::Process(p) = &c.call
                                && let Some(issue) = p.import_warning()
                            {
                                return Err(format!(
                                    "Inspect this call before importing: \"{}\". {issue}",
                                    c.name
                                ));
                            }
                        }
                        Ok(format!(
                            "{} external calls\n{}",
                            calls.len(),
                            calls
                                .iter()
                                .map(|c| format!("{}: {}", c.name, c.call.description()))
                                .collect::<Vec<_>>()
                                .join("\n")
                        ))
                    });
                    let applied = Rc::new({
                        let active = active.clone();
                        let table = table.clone();
                        let changed = changed.clone();
                        move |calls| {
                            if !active.get() {
                                return Err("The Options owner has closed.".into());
                            }
                            table.borrow_mut().add_selected(calls);
                            changed();
                            Ok(())
                        }
                    });
                    crate::downloader_interchange_window::open_external_calls(
                        &store,
                        &slots.exchange,
                        importing,
                        &calls,
                        preview,
                        applied,
                    )
                    .map(|_| ())
                }
                _ => return,
            };
            if let Err(error) = result {
                w.set_external_call_error(error.into());
            }
            show();
        }
    });
    // Descendant closure updates only its enabled state, not table rows/selection.
    let timer = Rc::new(slint::Timer::default());
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(30),
        {
            let weak = w.as_weak();
            let has_open = has_open.clone();
            let active = active.clone();
            move || {
                if active.get()
                    && let Some(w) = weak.upgrade()
                {
                    w.set_external_call_child_open(has_open());
                }
            }
        },
    );
    let cancel: Rc<dyn Fn()> = Rc::new({
        let slots = slots.clone();
        move || {
            timer.stop();
            slots.cancel();
        }
    });
    show();
    Binding {
        show,
        cancel,
        has_open,
    }
}
