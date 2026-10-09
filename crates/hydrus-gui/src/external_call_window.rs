//! Detached callable/command editors and their explicitly owned descendants.
use crate::{ExternalCallWindow, ExternalCommandWindow, ExternalRule, SessionDialog, TableRow};
use hydrus_core::external_calls::{
    ActualCall, Callable, Inputs, Parameter, Pipeline, Process, Rule, clean_arguments,
};
use hydrus_gui_model::{
    external_calls as model,
    external_command::{Paste, Queue as CommandState},
};
use hydrus_store::Store;
use slint::{ComponentHandle as _, ModelRc, VecModel};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

/// Explicit child inspection/cancellation slots retained by Options.
#[derive(Clone, Default)]
pub struct Slots {
    worker_running: Arc<std::sync::atomic::AtomicBool>,
    pub defaults: Rc<RefCell<Option<crate::ExternalDefaultsWindow>>>,
    pub editor: Rc<RefCell<Option<ExternalCallWindow>>>,
    pub command: Rc<RefCell<Option<ExternalCommandWindow>>>,
    pub question: Rc<RefCell<Option<SessionDialog>>>,
    pub strings: crate::string_processor_window::Slots,
    pub exchange: crate::downloader_interchange_window::Slots,
}
impl Slots {
    /// Whether a descendant currently blocks its parent's acceptance.
    pub fn has_open(&self) -> bool {
        self.defaults.borrow().is_some()
            || self.editor.borrow().is_some()
            || self.command.borrow().is_some()
            || self.question.borrow().is_some()
            || self.exchange.has_open()
            || self.strings.has_open()
    }
    /// Discard every descendant, invalidating callbacks on retained old handles.
    pub fn cancel(&self) {
        let defaults = self
            .defaults
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(w) = defaults {
            w.invoke_cancel();
        }
        self.exchange.cancel();
        self.strings.cancel_all();
        let question = self
            .question
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(w) = question {
            w.invoke_force_close();
        }
        let command = self
            .command
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(w) = command {
            w.invoke_cancel();
        }
        let editor = self
            .editor
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(w) = editor {
            w.invoke_cancel();
        }
    }
}
impl std::fmt::Debug for Slots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalCallSlots")
            .field("open", &self.has_open())
            .finish_non_exhaustive()
    }
}

pub(crate) fn ask(
    slot: &Rc<RefCell<Option<SessionDialog>>>,
    active: &Rc<Cell<bool>>,
    message: String,
    answer: Rc<dyn Fn(bool)>,
) -> Result<SessionDialog, String> {
    if slot.borrow().is_some() {
        return Err("A question is already open.".into());
    }
    let window = crate::app_title::new::<crate::SessionDialog>().map_err(|e| e.to_string())?;
    window.set_window_title("Are you sure?".into());
    window.set_message(message.into());
    let alive = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
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
    window.on_answered({
        let alive = alive.clone();
        let active = active.clone();
        let close = close.clone();
        let answer = answer.clone();
        move |yes| {
            if !alive.get() || !active.get() {
                return;
            }
            close();
            answer(yes);
        }
    });
    window.on_force_close({
        let close = close.clone();
        move || {
            close();
        }
    });
    let rejected: Rc<dyn Fn()> = Rc::new({
        let alive = alive.clone();
        let active = active.clone();
        let close = close.clone();
        move || {
            let notify = alive.get() && active.get();
            close();
            if notify {
                answer(false);
            }
        }
    });
    window.on_cancelled({
        let rejected = rejected.clone();
        move || {
            rejected();
        }
    });
    window.window().on_close_requested(move || {
        rejected();
        slint::CloseRequestResponse::HideWindow
    });
    *slot.borrow_mut() = Some(window.clone_strong());
    window.show().map_err(|e| e.to_string())?;
    Ok(window)
}
fn text_child(
    slot: &Rc<RefCell<Option<SessionDialog>>>,
    active: &Rc<Cell<bool>>,
    initial: String,
    accepted: Rc<dyn Fn(String)>,
) -> Result<(), String> {
    let w = crate::app_title::new::<crate::SessionDialog>().map_err(|e| e.to_string())?;
    w.set_window_title("Enter parameter".into());
    w.set_asking_name(true);
    w.set_text(initial.into());
    w.set_placeholder("-o".into());
    w.set_name_ok_label("ok".into());
    w.set_message("Edit the parameter. This should just be one thing, which you typically see separated by whitespace in a command. In a command like this:\n\nmy_program -o d a=virt profile=\"My Profile\" input_path\n\nThe parameters would be \"-o\", \"d\", \"a=virt\", \"profile=\"My Profile\"\", and \"input_path\" (or, likely for our purposes here, \"%path%\"). While you may need quotes _within_ a parameter, you should not, generally speaking, wrap a whole parameter in quotes here to avoid whitespace issues--that is handled for you, so do not worry about it; trying to add extra quotes may just break things.\n\nNo newlines or leading/trailing whitespace allowed in these parameter templates, and multiple whitespace is collapsed to single.\n\nYou can mix an input parameter's replacement token in amongst other text, or even have multiple tokens in the same parameter. \"%parameter1%-%parameter2%\" is fine. You cannot use the same token more than once per parameter, but you can use it in multiple parameters!".into());
    let alive = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let slot = Rc::downgrade(slot);
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
    w.on_name_entered({
        let alive = alive.clone();
        let active = active.clone();
        let close = close.clone();
        move |text| {
            if !alive.get() || !active.get() {
                return;
            }
            let Some(value) = clean_arguments(&[text.to_string()]).into_iter().next() else {
                return;
            };
            close();
            accepted(value);
        }
    });
    w.on_force_close({
        let close = close.clone();
        move || {
            close();
        }
    });
    w.on_cancelled({
        let close = close.clone();
        move || {
            close();
        }
    });
    w.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    *slot.borrow_mut() = Some(w.clone_strong());
    w.show().map_err(|e| e.to_string())
}

fn command_show(w: &ExternalCommandWindow, state: &CommandState) {
    w.set_rows(ModelRc::new(VecModel::from(
        state
            .arguments
            .iter()
            .enumerate()
            .map(|(i, a)| TableRow {
                cells: ModelRc::new(VecModel::from(vec![a.as_str().into()])),
                selected: state.selection.is_selected(i),
            })
            .collect::<Vec<_>>(),
    )));
    w.set_selected(!state.selection.is_empty());
    w.set_single(state.selection.one().is_some());
    w.set_full_template(state.full_template(w.get_executable().as_str()).into());
}
fn command_action(
    action: &str,
    w: &ExternalCommandWindow,
    state: &Rc<RefCell<CommandState>>,
    active: &Rc<Cell<bool>>,
    question: &Rc<RefCell<Option<SessionDialog>>>,
    show: &Rc<dyn Fn()>,
) {
    let weak = w.as_weak();
    match action {
        "copy" => {
            crate::copy_to_clipboard(w.get_full_template().as_str());
            w.set_feedback("Copied!".into());
            w.set_feedback_generation(w.get_feedback_generation() + 1);
        }
        "paste" => match crate::clipboard_text()
            .and_then(|text| text.ok_or_else(|| "No text on the clipboard!".to_owned()))
        {
            Ok(text) => {
                let paste = Paste::parse(&text);
                let message = paste.question();
                let executable = paste.executable;
                let arguments = paste.arguments;
                let done: Rc<dyn Fn(bool)> = Rc::new({
                    let weak = weak.clone();
                    let state = state.clone();
                    let show = show.clone();
                    move |yes| {
                        if yes && let Some(w) = weak.upgrade() {
                            w.set_executable(executable.as_str().into());
                            let mut state = state.borrow_mut();
                            *state = CommandState::new(arguments.clone());
                            w.set_feedback("Pasted!".into());
                            w.set_feedback_generation(w.get_feedback_generation() + 1);
                        }
                        show();
                    }
                });
                if let Err(e) = ask(question, active, message, done) {
                    w.set_error(e.into());
                }
            }
            Err(e) => match ask(question, active, e, Rc::new(|_| {})) {
                Ok(notice) => {
                    notice.set_window_title("Error".into());
                    notice.set_notice_only(true);
                }
                Err(error) => {
                    w.set_error(error.into());
                }
            },
        },
        "copy-selected" => {
            if let Some(text) = state.borrow().copy_selected() {
                crate::copy_to_clipboard(&text);
            }
        }
        "select-all" => {
            state.borrow_mut().select_all();
        }
        "toggle-current" => {
            state.borrow_mut().toggle_current();
        }
        "add" | "edit" => {
            let editing = if action == "edit" {
                state.borrow().selection.selected_order().first().copied()
            } else {
                None
            };
            if action == "edit" && editing.is_none() {
                return;
            }
            let initial = editing
                .and_then(|i| state.borrow().arguments.get(i).cloned())
                .unwrap_or_default();
            let done: Rc<dyn Fn(String)> = Rc::new({
                let state = state.clone();
                let show = show.clone();
                move |value| {
                    if let Some(i) = editing {
                        if let Some(row) = state.borrow_mut().arguments.get_mut(i) {
                            *row = value;
                        }
                    } else {
                        state.borrow_mut().arguments.push(value);
                    }
                    show();
                }
            });
            if let Err(e) = text_child(question, active, initial, done) {
                w.set_error(e.into());
            }
        }
        "delete" => {
            let selected = {
                let state = state.borrow();
                (0..state.arguments.len())
                    .filter(|i| state.selection.is_selected(*i))
                    .collect::<Vec<_>>()
            };
            if selected.is_empty() {
                return;
            }
            let message = format!(
                "Remove {} selected?",
                hydrus_core::numbers::human_int(selected.len() as u64)
            );
            let done: Rc<dyn Fn(bool)> = Rc::new({
                let state = state.clone();
                let show = show.clone();
                move |yes| {
                    if yes {
                        let mut state = state.borrow_mut();
                        state.delete(&selected);
                    }
                    show();
                }
            });
            if let Err(e) = ask(question, active, message, done) {
                w.set_error(e.into());
            }
        }
        "up" | "down" => {
            state.borrow_mut().reorder(action == "down");
        }
        _ => return,
    }
    show();
}

fn command_open(
    slots: &Slots,
    process: &Process,
    accepted: &Rc<dyn Fn(String, Vec<String>)>,
) -> Result<ExternalCommandWindow, String> {
    let w = crate::app_title::new::<crate::ExternalCommandWindow>().map_err(|e| e.to_string())?;
    w.set_executable(process.executable.as_str().into());
    w.set_mac_delete_key(cfg!(target_os = "macos"));
    let state = Rc::new(RefCell::new(CommandState::new(process.arguments.clone())));
    let active = Rc::new(Cell::new(true));
    let timer = Rc::new(slint::Timer::default());
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(30),
        {
            let weak = w.as_weak();
            let question = Rc::downgrade(&slots.question);
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_child_open(question.upgrade().is_some_and(|s| s.borrow().is_some()));
                }
            }
        },
    );
    let show: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        let child = Rc::downgrade(&slots.question);
        move || {
            if let Some(w) = weak.upgrade() {
                command_show(&w, &state.borrow());
                w.set_child_open(child.upgrade().is_some_and(|s| s.borrow().is_some()));
            }
        }
    });
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let slot = Rc::downgrade(&slots.command);
        let question = Rc::downgrade(&slots.question);
        let active = active.clone();
        let timer = timer.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            timer.stop();
            if let Some(q) = question.upgrade().and_then(|s| {
                s.borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            }) {
                q.invoke_cancelled();
            }
            if let Some(w) = weak.upgrade() {
                w.set_feedback("".into());
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    w.on_clicked({
        let state = state.clone();
        let active = active.clone();
        let show = show.clone();
        let question = slots.question.clone();
        move |i, c, s| {
            if !active.get() || question.borrow().is_some() {
                return;
            }
            if let Ok(i) = usize::try_from(i) {
                let mut state = state.borrow_mut();
                state.click(i, c, s);
            }
            show();
        }
    });
    w.on_navigated({
        let state = state.clone();
        let active = active.clone();
        let question = slots.question.clone();
        let show = show.clone();
        move |destination, control, shift| {
            if !active.get() || question.borrow().is_some() {
                return;
            }
            state
                .borrow_mut()
                .navigate(destination.as_str(), control, shift);
            show();
        }
    });
    w.on_changed({
        let show = show.clone();
        move || {
            show();
        }
    });
    w.on_action({
        let weak = w.as_weak();
        let state = state.clone();
        let active = active.clone();
        let show = show.clone();
        let question = slots.question.clone();
        move |action| {
            if !active.get() || question.borrow().is_some() {
                return;
            }
            if let Some(w) = weak.upgrade() {
                command_action(action.as_str(), &w, &state, &active, &question, &show);
            }
        }
    });
    // Acceptance questions follow the reference order: empty path, then which.
    w.on_apply({
        let weak=w.as_weak();let active=active.clone();let state=state.clone();let close=close.clone();let question=slots.question.clone();let accepted=accepted.clone();
        move|| {
            if !active.get() || question.borrow().is_some(){return;}
            let Some(w)=weak.upgrade()else{return;};
            let executable=w.get_executable().to_string();
            let args=clean_arguments(&state.borrow().arguments);
            let finish:Rc<dyn Fn(bool)>=Rc::new({
                let close=close.clone();let accepted=accepted.clone();let executable=executable.clone();
                move|yes|{if yes{close();accepted(executable.clone(),args.clone());}}
            });
            if executable.is_empty() {
                let next:Rc<dyn Fn(bool)>=Rc::new({
                    let question=question.clone();let active=active.clone();let finish=finish.clone();
                    move|yes|{if yes{let _=ask(&question,&active,"Hey, I looked for the executable path \"\" but did not see it with a \"which\" call. You sure you are good?".into(),finish.clone());}}
                });
                let _=ask(&question,&active,"Hey, you really need to put an exe name/path in the path box. Are you sure you want to save this?".into(),next);
            }else if model::executable_path(&executable).is_none(){
                let _=ask(&question,&active,format!("Hey, I looked for the executable path \"{executable}\" but did not see it with a \"which\" call. You sure you are good?"),finish);
            }else{finish(true);}
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
    *slots.command.borrow_mut() = Some(w.clone_strong());
    show();
    w.show().map_err(|e| e.to_string())?;
    Ok(w)
}

thread_local! {
    /// The last test value typed for each kind of input, which the next
    /// editor starts with (the reference's `PARAM_TYPES_TO_LAST_SEEN_VALUES`).
    static LAST_SEEN: RefCell<std::collections::BTreeMap<Parameter, String>> =
        RefCell::default();
}

struct WorkerLease(Arc<std::sync::atomic::AtomicBool>);
impl Drop for WorkerLease {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

struct RuleDraft {
    rule: Rule,
    enabled: bool,
    input: String,
}
struct State {
    original: Callable,
    process: Process,
    rules: Vec<RuleDraft>,
    pipeline: Pipeline,
}
fn rules_for(pipeline: Pipeline, process: &Process) -> Vec<RuleDraft> {
    pipeline
        .parameters()
        .iter()
        .map(|p| {
            let rule = process.rules.iter().find(|r| r.parameter == *p).cloned();
            RuleDraft {
                enabled: rule.is_some(),
                rule: rule.unwrap_or_else(|| Rule::new(*p)),
                input: LAST_SEEN
                    .with(|seen| seen.borrow().get(p).cloned())
                    .unwrap_or_else(|| p.example().into()),
            }
        })
        .collect()
}
fn read(w: &ExternalCallWindow, state: &State) -> Callable {
    let mut call = state.original.clone();
    call.name = w.get_name().to_string();
    call.pipeline = if w.get_pipeline() == 0 {
        Pipeline::File
    } else {
        Pipeline::Url
    };
    call.call = if w.get_call_type() == 0 {
        let mut process = state.process.clone();
        process.timeout_seconds = if w.get_long_lived() {
            15
        } else {
            u64::try_from(w.get_timeout()).unwrap_or(15).max(1)
        };
        process.long_lived = w.get_long_lived();
        process.hide_terminal = w.get_hide_terminal();
        process.text = w.get_output_text();
        process.rules = state
            .rules
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.rule.clone())
            .collect();
        ActualCall::Process(process)
    } else if call.pipeline == Pipeline::File {
        ActualCall::DefaultFile
    } else {
        ActualCall::DefaultUrl
    };
    call
}
fn inputs(state: &State) -> Inputs {
    state
        .rules
        .iter()
        .map(|r| (r.rule.parameter, vec![r.input.clone()]))
        .collect()
}
fn preview(w: &ExternalCallWindow, state: &State) {
    let call = read(w, state);
    w.set_preview(call.call.preview(&inputs(state)).into());
    w.set_validity(
        match &call.call {
            ActualCall::Process(p) => p
                .validate()
                .err()
                .unwrap_or_else(|| "Everything looks good!".into()),
            _ => "Everything looks good!".into(),
        }
        .into(),
    );
}
fn show(w: &ExternalCallWindow, state: &State) {
    let pipeline = if w.get_pipeline() == 1 {
        Pipeline::Url
    } else {
        Pipeline::File
    };
    w.set_pipeline_description(pipeline.description().into());
    w.set_default_launch_description(pipeline.default_launch_description().into());
    w.set_command_template(
        format!(
            "{} {}",
            state.process.executable,
            state.process.arguments.join(" ")
        )
        .into(),
    );
    w.set_rules(ModelRc::new(VecModel::from(
        state
            .rules
            .iter()
            .map(|r| ExternalRule {
                label: r.rule.parameter.label().into(),
                enabled: r.enabled
                    || w.get_call_type() == 1
                        && matches!(
                            (state.pipeline, r.rule.parameter),
                            (Pipeline::File, Parameter::Path) | (Pipeline::Url, Parameter::Url)
                        ),
                token: r.rule.token.as_str().into(),
                summary: r.rule.processor.summary().into(),
                value: r.input.as_str().into(),
            })
            .collect::<Vec<_>>(),
    )));
    preview(w, state);
}

/// Open a detached named-call draft. Apply returns it with its original key;
/// the list owner alone decides whether this is an edit or a fresh-key addition.
pub fn open(
    store: &Arc<Store>,
    slots: &Slots,
    call: Callable,
    applied: Rc<dyn Fn(Callable)>,
) -> Result<ExternalCallWindow, String> {
    if slots.has_open() {
        return Err("An external-call child is already open.".into());
    }
    let w = crate::app_title::new::<crate::ExternalCallWindow>().map_err(|e| e.to_string())?;
    w.set_name(call.name.as_str().into());
    w.set_pipeline(i32::from(call.pipeline == Pipeline::Url));
    w.set_call_type(i32::from(!matches!(call.call, ActualCall::Process(_))));
    let process = if let ActualCall::Process(p) = &call.call {
        p.clone()
    } else {
        Process::default()
    };
    w.set_timeout(i32::try_from(process.timeout_seconds).unwrap_or(15));
    w.set_long_lived(process.long_lived);
    w.set_hide_terminal(process.hide_terminal);
    w.set_output_text(process.text);
    let state = Rc::new(RefCell::new(State {
        rules: rules_for(call.pipeline, &process),
        pipeline: call.pipeline,
        original: call,
        process,
    }));
    let active = Rc::new(Cell::new(true));
    let timer = Rc::new(slint::Timer::default());
    let publish_timer = Rc::new(slint::Timer::default());
    let worker_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let slot = Rc::downgrade(&slots.editor);
        let command = Rc::downgrade(&slots.command);
        let question = Rc::downgrade(&slots.question);
        let strings = slots.strings.clone();
        let timer = timer.clone();
        let publish_timer = publish_timer.clone();
        let worker_cancel = worker_cancel.clone();
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            worker_cancel.store(true, std::sync::atomic::Ordering::Release);
            timer.stop();
            publish_timer.stop();
            strings.cancel_all();
            if let Some(q) = question.upgrade().and_then(|s| {
                s.borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            }) {
                q.invoke_cancelled();
            }
            if let Some(c) = command.upgrade().and_then(|s| {
                s.borrow()
                    .as_ref()
                    .map(slint::ComponentHandle::clone_strong)
            }) {
                c.invoke_cancel();
            }
            if let Some(w) = weak.upgrade() {
                let _ = w.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    let blocked: Rc<dyn Fn() -> bool> = Rc::new({
        let command = slots.command.clone();
        let question = slots.question.clone();
        let strings = slots.strings.clone();
        let active = active.clone();
        move || {
            !active.get()
                || command.borrow().is_some()
                || question.borrow().is_some()
                || strings.has_open()
        }
    });
    let refresh: Rc<dyn Fn()> = Rc::new({
        let weak = w.as_weak();
        let state = state.clone();
        move || {
            if let Some(w) = weak.upgrade() {
                show(&w, &state.borrow());
            }
        }
    });
    // Only lifecycle flags are polled; parameter rows are not recreated while typing.
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(30),
        {
            let weak = w.as_weak();
            let blocked = blocked.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_child_open(blocked());
                }
            }
        },
    );
    w.on_changed({
        let weak = w.as_weak();
        let state = state.clone();
        let blocked = blocked.clone();
        let refresh = refresh.clone();
        move || {
            if blocked() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            let pipeline = if w.get_pipeline() == 0 {
                Pipeline::File
            } else {
                Pipeline::Url
            };
            if state.borrow().pipeline != pipeline {
                let mut state = state.borrow_mut();
                state.pipeline = pipeline;
                state.rules = rules_for(pipeline, &Process::default());
                w.set_call_type(0);
            }
            refresh();
        }
    });
    w.on_rule_enabled({
        let state = state.clone();
        let blocked = blocked.clone();
        let refresh = refresh.clone();
        move |i, on| {
            if blocked() {
                return;
            }
            if let Ok(i) = usize::try_from(i)
                && let Some(rule) = state.borrow_mut().rules.get_mut(i)
            {
                rule.enabled = on;
            }
            refresh();
        }
    });
    w.on_rule_token({
        let weak = w.as_weak();
        let state = state.clone();
        let blocked = blocked.clone();
        move |i, text| {
            if blocked() {
                return;
            }
            if let Ok(i) = usize::try_from(i)
                && let Some(rule) = state.borrow_mut().rules.get_mut(i)
            {
                rule.rule.token = text.to_string();
            }
            if let Some(w) = weak.upgrade() {
                preview(&w, &state.borrow());
            }
        }
    });
    w.on_test_input({
        let weak = w.as_weak();
        let state = state.clone();
        let blocked = blocked.clone();
        move |i, text| {
            if blocked() {
                return;
            }
            if let Ok(i) = usize::try_from(i)
                && let Some(rule) = state.borrow_mut().rules.get_mut(i)
            {
                rule.input = text.to_string();
                let parameter = rule.rule.parameter;
                LAST_SEEN.with(|seen| seen.borrow_mut().insert(parameter, text.to_string()));
            }
            if let Some(w) = weak.upgrade() {
                preview(&w, &state.borrow());
                w.set_test_status("".into());
            }
        }
    });
    w.on_rule_process({
        let store = store.clone();
        let strings = slots.strings.clone();
        let state = state.clone();
        let refresh = refresh.clone();
        let blocked = blocked.clone();
        let close = close.clone();
        move |i| {
            if blocked() {
                return;
            }
            let Ok(i) = usize::try_from(i) else {
                return;
            };
            let Some((processor, input)) = state
                .borrow()
                .rules
                .get(i)
                .map(|r| (r.rule.processor.clone(), r.input.clone()))
            else {
                close();
                return;
            };
            let applied = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                move |processor| {
                    if let Some(rule) = state.borrow_mut().rules.get_mut(i) {
                        rule.rule.processor = processor;
                    }
                    refresh();
                }
            });
            if let Ok(editor) = crate::string_processor_window::open(
                &store,
                &processor,
                vec![input],
                &strings,
                applied,
            ) {
                *strings.processor.borrow_mut() = Some(editor);
            }
        }
    });
    w.on_command_edit({
        let weak = w.as_weak();
        let slots = Slots {
            worker_running: slots.worker_running.clone(),
            defaults: Rc::default(),
            editor: Rc::default(),
            command: slots.command.clone(),
            question: slots.question.clone(),
            strings: slots.strings.clone(),
            exchange: crate::downloader_interchange_window::Slots::default(),
        };
        let state = state.clone();
        let blocked = blocked.clone();
        let refresh = refresh.clone();
        move || {
            if blocked() {
                return;
            }
            let process = state.borrow().process.clone();
            let applied: Rc<dyn Fn(String, Vec<String>)> = Rc::new({
                let state = state.clone();
                let refresh = refresh.clone();
                move |executable, arguments| {
                    let mut state = state.borrow_mut();
                    state.process.executable = executable;
                    state.process.arguments = arguments;
                    drop(state);
                    refresh();
                }
            });
            if let Err(e) = command_open(&slots, &process, &applied)
                && let Some(w) = weak.upgrade()
            {
                w.set_test_status(e.into());
            }
        }
    });
    w.on_show_path({
        let weak = w.as_weak();
        let question = slots.question.clone();
        let blocked = blocked.clone();
        let active = active.clone();
        move || {
            if blocked() { return; }
            let paths = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
            let message = format!("As hydrus sees it, your PATH is as follows. Any executable you specify with just a name, rather than a full path, needs to exist in one of these locations. You should be very very careful in ever editing your PATH. Ask a chatbot if you need to learn more. Recall that if you ever do change it, you need to restart hydrus (in a new terminal if needed) to see the changes here.\n\n{paths}");
            if let Ok(q) = ask(&question, &active, message, Rc::new(|_| {})) {
                q.set_yes_label("ok".into()); q.set_no_label("close".into());
            } else if let Some(w) = weak.upgrade() { w.set_test_status("Could not show PATH.".into()); }
        }
    });
    w.on_test({
        let worker_cancel = worker_cancel.clone();
        let worker_running = slots.worker_running.clone();
        let publish = publish_timer.clone();
        let weak = w.as_weak();
        let state = state.clone();
        let active = active.clone();
        let blocked = blocked.clone();
        move |availability| {
            if blocked() {
                return;
            }
            let Some(w) = weak.upgrade() else {
                return;
            };
            if w.get_testing() {
                return;
            }
            let call = read(&w, &state.borrow()).call;
            let inputs = inputs(&state.borrow());
            // An OS launch call opens the example path or URL for real, as
            // the reference's does.
            if !availability
                && let Some(parameter) = match call {
                    ActualCall::DefaultFile => Some(Parameter::Path),
                    ActualCall::DefaultUrl => Some(Parameter::Url),
                    ActualCall::Process(_) => None,
                }
            {
                match inputs.get(&parameter).and_then(|values| values.first()) {
                    Some(target) => {
                        crate::launch(target);
                        w.set_test_status("Looks good!".into());
                    }
                    None => w.set_test_status(
                        format!(
                            "ExecutableException: The expected input parameter \"{}\" was not in the call arguments!",
                            parameter.label()
                        )
                        .into(),
                    ),
                }
                return;
            }
            if worker_running
                .compare_exchange(
                    false,
                    true,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                )
                .is_err()
            {
                w.set_test_status(
                    "A cancelled test call is still stopping. Try again shortly.".into(),
                );
                return;
            }
            let lease = WorkerLease(worker_running.clone());
            w.set_testing(true);
            w.set_test_status(
                if matches!(&call,ActualCall::Process(p) if p.long_lived) && !availability {
                    "Testing… (forcing a 15 second timeout for testing purposes)"
                } else {
                    "Testing…"
                }
                .into(),
            );
            let (send, receive) = std::sync::mpsc::channel();
            let worker_cancel = worker_cancel.clone();
            let started = std::thread::Builder::new()
                .name("external-call-test".into())
                .spawn(move || {
                    let _lease = lease;
                    let result = if availability {
                        Ok(if model::available(&call) {
                            "Availability test worked!"
                        } else {
                            "Availability test failed!"
                        }
                        .to_owned())
                    } else {
                        model::test_call_cancellable(&call, &inputs, &worker_cancel)
                            .map(|()| "Looks good!".into())
                    };
                    let _ = send.send(result);
                });
            if let Err(error) = started {
                w.set_testing(false);
                w.set_test_status(
                    format!("Could not start the external-call test worker: {error}").into(),
                );
                return;
            }
            let retained = Rc::downgrade(&publish);
            let active = active.clone();
            let weak = weak.clone();
            publish.start(
                slint::TimerMode::Repeated,
                std::time::Duration::from_millis(20),
                move || {
                    if !active.get() {
                        if let Some(timer) = retained.upgrade() {
                            timer.stop();
                        }
                        return;
                    }
                    if let Ok(result) = receive.try_recv() {
                        if let Some(w) = weak.upgrade() {
                            w.set_test_status(result.unwrap_or_else(|e| e).into());
                            w.set_testing(false);
                        }
                        if let Some(timer) = retained.upgrade() {
                            timer.stop();
                        }
                    }
                },
            );
        }
    });
    w.on_apply({
        let weak = w.as_weak();
        let state = state.clone();
        let blocked = blocked.clone();
        let active = active.clone();
        let close = close.clone();
        let question = slots.question.clone();
        move || {
            if blocked() { return; }
            let Some(w) = weak.upgrade() else { return; };
            if w.get_testing() { return; }
            let call = read(&w, &state.borrow());
            let invalid = matches!(&call.call, ActualCall::Process(p) if p.validate().is_err());
            let done: Rc<dyn Fn(bool)> = Rc::new({
                let applied = applied.clone();
                let close = close.clone();
                move |yes| { if yes { close(); applied(call.clone()); } }
            });
            if invalid {
                let _ = ask(&question, &active, "Hey, it looks like something is not quite right here. Are you sure you want to save this?".into(), done);
            } else { done(true); }
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
    *slots.editor.borrow_mut() = Some(w.clone_strong());
    refresh();
    w.show().map_err(|e| e.to_string())?;
    Ok(w)
}
