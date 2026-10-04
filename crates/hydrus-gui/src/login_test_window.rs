//! Login result review and background execution retained by its owning editor.
use crate::LoginTestResultWindow;
use hydrus_net::{
    Job, NetEngine, NetOptions,
    login::{Execution, Outcome, TestResult},
};
use hydrus_parse::login::LoginScript;
use hydrus_store::{
    Store,
    network::{self, NetworkSettings},
    settings,
};
use slint::{ComponentHandle as _, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::Duration,
};
/// A result window whose data can be copied without truncating the preview.
pub type ResultSlot = Rc<RefCell<Option<LoginTestResultWindow>>>;
/// Close a result review without invoking a stale clipboard action.
pub fn cancel_result(slot: &ResultSlot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_action("close".into());
    }
}
/// Review one native execution result using the reference's 1024-character preview.
pub fn open_result(
    value: &TestResult,
    slot: &ResultSlot,
) -> Result<LoginTestResultWindow, slint::PlatformError> {
    cancel_result(slot);
    let window = LoginTestResultWindow::new()?;
    window.set_name(value.name.as_str().into());
    window.set_url(value.url.as_str().into());
    window.set_body(value.body.as_deref().unwrap_or_default().into());
    window.set_data(value.data.chars().take(1024).collect::<String>().into());
    window.set_variables(value.new_variables.join("\n").into());
    window.set_cookies(value.new_cookies.join("\n").into());
    window.set_result(value.result.as_str().into());
    let active = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
            if let Some(window) = weak.upgrade() {
                window.invoke_closed();
            }
        }
    });
    window.on_action({
        let weak = window.as_weak();
        let close = close.clone();
        let active = active.clone();
        let data = value.data.clone();
        move |action| {
            if !active.get() {
                return;
            }
            match action.as_str() {
                "close" => close(),
                "copy" => {
                    crate::copy_to_clipboard(&data);
                    if let Some(window) = weak.upgrade() {
                        window.set_copied(true);
                    }
                }
                _ => {}
            }
        }
    });
    window.window().on_close_requested(move || {
        close();
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
struct Running {
    job: Arc<Job>,
    _timer: Timer,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.job.cancel();
    }
}
/// Single background login run, discarded safely when its owner closes.
#[derive(Clone, Default)]
pub struct RunSlot(Rc<RefCell<Option<Running>>>);
impl std::fmt::Debug for RunSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRunSlot")
            .field("busy", &self.busy())
            .finish()
    }
}
/// Final results are delivered only to an owner that has not canceled its run.
pub type Completed = Rc<dyn Fn(Execution)>;
/// Current HTTP job status shown by the native editor.
pub type Progress = Rc<dyn Fn(String)>;
/// One completed step delivered while later steps/waits are still running.
pub type ResultReady = Rc<dyn Fn(TestResult)>;
enum RunEvent {
    Result(TestResult),
    Complete(Execution),
}
fn isolated(source: &Store) -> Result<(tempfile::TempDir, Arc<Store>), String> {
    let (options, classes, headers) = source
        .read(|conn| {
            let options: NetworkSettings = settings::get(conn)?;
            let classes: hydrus_core::url::UrlClassSettings = settings::get(conn)?;
            let headers = network::header_contexts(conn)?
                .into_iter()
                .map(|context| network::headers(conn, &context).map(|headers| (context, headers)))
                .collect::<hydrus_store::Result<Vec<_>>>()?;
            Ok((options, classes, headers))
        })
        .map_err(|e| e.to_string())?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let store = Store::open(dir.path()).map_err(|e| e.to_string())?;
    store
        .write_and_refresh(move |ctx| {
            settings::set(ctx.conn(), &options)?;
            settings::set(ctx.conn(), &classes)?;
            for (context, headers) in headers {
                for header in headers {
                    network::set_header(
                        ctx.conn(),
                        &context,
                        &header.name,
                        Some(&header.value),
                        Some(header.approval),
                        Some(&header.reason),
                    )?;
                }
            }
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    Ok((dir, store))
}
/// Request data owned by the worker; test mode isolates its session cookies.
#[derive(Clone)]
pub struct Input {
    pub source: Arc<Store>,
    pub script: LoginScript,
    pub domain: String,
    pub credentials: BTreeMap<String, String>,
    pub test: bool,
}
impl std::fmt::Debug for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRunInput")
            .field("script", &self.script.name)
            .field("domain", &self.domain)
            .field("test", &self.test)
            .finish_non_exhaustive()
    }
}
impl RunSlot {
    pub fn busy(&self) -> bool {
        self.0.borrow().is_some()
    }
    pub fn cancel(&self) {
        if let Some(running) = self.0.borrow().as_ref() {
            running.job.cancel();
        }
    }
    pub fn stop(&self) {
        self.0.borrow_mut().take();
    }
    /// Test runs use a fresh cookie store while retaining request preferences and
    /// custom headers; real domain runs share the existing persisted sessions.
    pub fn start(&self, input: Input, progress: Progress, completed: Completed) {
        self.start_with_results(input, progress, Rc::new(|_| {}), completed);
    }
    /// Stream each finished step on the GUI thread before the final completion.
    pub fn start_with_results(
        &self,
        input: Input,
        progress: Progress,
        result_ready: ResultReady,
        completed: Completed,
    ) {
        let Input {
            source,
            script,
            domain,
            credentials,
            test,
        } = input;
        self.stop();
        let job = Job::new();
        let (send, receive) = crossbeam_channel::unbounded();
        std::thread::spawn({
            let job = job.clone();
            move || {
                let run = || -> Result<Execution, String> {
                    let isolated = if test { Some(isolated(&source)?) } else { None };
                    let store = isolated
                        .as_ref()
                        .map_or_else(|| source.clone(), |(_, store)| store.clone());
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| e.to_string())?;
                    let options: NetworkSettings =
                        store.read(settings::get).map_err(|e| e.to_string())?;
                    let engine = {
                        let _entered = runtime.enter();
                        NetEngine::new(store.clone(), NetOptions::from_settings(&options))
                            .map_err(|e| e.to_string())?
                    };
                    let execution = runtime.block_on(hydrus_net::login::execute_with_results(
                        &engine,
                        &store,
                        &script,
                        &domain,
                        &credentials,
                        &job,
                        |result| {
                            let _ = send.send(RunEvent::Result(result.clone()));
                        },
                    ));
                    engine.save_bandwidth().map_err(|e| e.to_string())?;
                    Ok(execution)
                };
                let execution = run().unwrap_or_else(|error| Execution {
                    results: Vec::new(),
                    variables: BTreeMap::new(),
                    outcome: Outcome::Unusual(error),
                });
                let _ = send.send(RunEvent::Complete(execution));
            }
        });
        let timer = Timer::default();
        timer.start(TimerMode::Repeated, Duration::from_millis(50), {
            let slot = Rc::downgrade(&self.0);
            let job = job.clone();
            move || {
                let Some(slot) = slot.upgrade() else {
                    return;
                };
                loop {
                    match receive.try_recv() {
                        Ok(RunEvent::Result(result)) => result_ready(result),
                        Ok(RunEvent::Complete(execution)) => {
                            slot.borrow_mut().take();
                            completed(execution);
                            break;
                        }
                        Err(crossbeam_channel::TryRecvError::Empty) => {
                            let state = job.state();
                            progress(format!("{} — {} bytes", state.status, state.bytes_read));
                            break;
                        }
                        Err(crossbeam_channel::TryRecvError::Disconnected) => {
                            slot.borrow_mut().take();
                            completed(Execution {
                                results: Vec::new(),
                                variables: BTreeMap::new(),
                                outcome: Outcome::Unusual(
                                    "Login worker stopped before returning a result.".into(),
                                ),
                            });
                            break;
                        }
                    }
                }
            }
        });
        *self.0.borrow_mut() = Some(Running { job, _timer: timer });
    }
}

/// Parent-owned reference runtime domain text prompt.
pub type DomainSlot = Rc<RefCell<Option<crate::SessionDialog>>>;
/// Accepted text or prompt cancellation, delivered only while the prompt is owned.
pub type DomainAccepted = Rc<dyn Fn(Option<String>)>;
/// Parent closure discards the prompt without starting the next credential/request stage.
pub fn cancel_domain(slot: &DomainSlot) {
    let window = slot
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong);
    if let Some(window) = window {
        window.invoke_force_close();
    }
}
/// Ask the remembered test domain; unlike example-description prompts, Cancel aborts.
pub fn open_domain(
    initial: &str,
    slot: &DomainSlot,
    accepted: DomainAccepted,
) -> Result<crate::SessionDialog, slint::PlatformError> {
    open_text("Edit the domain.", initial, false, slot, accepted)
}
/// Parent-owned EnterText prompt with the reference's explicit blank-value policy.
pub fn open_text(
    message: &str,
    initial: &str,
    allow_blank: bool,
    slot: &DomainSlot,
    accepted: DomainAccepted,
) -> Result<crate::SessionDialog, slint::PlatformError> {
    if let Some(window) = slot.borrow().as_ref() {
        return Ok(window.clone_strong());
    }
    let window = crate::SessionDialog::new()?;
    window.set_window_title("Enter Text".into());
    window.set_message(message.into());
    window.set_name_ok_label("ok".into());
    window.set_asking_name(true);
    window.set_text(initial.into());
    let active = Rc::new(Cell::new(true));
    let close: Rc<dyn Fn()> = Rc::new({
        let weak = window.as_weak();
        let slot = Rc::downgrade(slot);
        let active = active.clone();
        move || {
            if !active.replace(false) {
                return;
            }
            if let Some(window) = weak.upgrade() {
                let _ = window.hide();
            }
            if let Some(slot) = slot.upgrade() {
                slot.borrow_mut().take();
            }
        }
    });
    let finish: DomainAccepted = Rc::new({
        let active = active.clone();
        let close = close.clone();
        move |value| {
            if !active.get() {
                return;
            }
            close();
            accepted(value);
        }
    });
    window.on_name_entered({
        let finish = finish.clone();
        move |value| {
            finish((allow_blank || !value.is_empty()).then(|| value.to_string()));
        }
    });
    window.on_cancelled({
        let finish = finish.clone();
        move || finish(None)
    });
    window.on_force_close(move || {
        close();
    });
    window.window().on_close_requested(move || {
        finish(None);
        slint::CloseRequestResponse::HideWindow
    });
    window.show()?;
    *slot.borrow_mut() = Some(window.clone_strong());
    Ok(window)
}
