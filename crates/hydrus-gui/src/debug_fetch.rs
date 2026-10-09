//! Help > Debug GET requests with ordinary network policy and captured byte results.
use crate::{DebugFetchWindow, MainWindow};
use hydrus_download::popups::Working;
use hydrus_net::{Job, NetEngine, NetOptions, Request, Response};
use hydrus_store::{Store, network::NetworkSettings, settings};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

struct Running {
    job: Arc<Job>,
    popup: Arc<Working>,
    receive: crossbeam_channel::Receiver<Result<Response, String>>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.job.cancel();
    }
}
struct Child {
    window: DebugFetchWindow,
    response: Option<Response>,
}
struct State {
    main: slint::Weak<MainWindow>,
    store: Arc<Store>,
    binding: Rc<Cell<bool>>,
    active: Cell<bool>,
    next: Cell<u64>,
    timer: slint::Timer,
    running: RefCell<BTreeMap<u64, Running>>,
    children: RefCell<BTreeMap<u64, Child>>,
    published: RefCell<Rc<dyn Fn()>>,
}
impl State {
    fn live(&self) -> bool {
        self.active.get() && self.binding.get() && self.main.upgrade().is_some()
    }
    fn valid_child(&self, id: u64) -> bool {
        self.live()
            && self
                .main
                .upgrade()
                .is_some_and(|main| main.get_question().is_empty())
            && self
                .children
                .borrow()
                .get(&id)
                .is_some_and(|child| child.window.window().is_visible())
    }
    fn close(&self, id: u64) {
        let child = self.children.borrow_mut().remove(&id);
        if let Some(child) = child {
            let _ = child.window.hide();
        }
    }
    fn retire(&self) {
        self.active.set(false);
        self.timer.stop();
        let running = std::mem::take(&mut *self.running.borrow_mut());
        for run in running.values() {
            run.job.cancel();
            run.popup.finish_and_dismiss();
        }
        drop(running);
        let children = std::mem::take(&mut *self.children.borrow_mut());
        for child in children.values() {
            let _ = child.window.hide();
        }
    }
    fn show(
        self: &Rc<Self>,
        id: u64,
        response: Option<Response>,
    ) -> Result<(), slint::PlatformError> {
        if !self.live() {
            return Ok(());
        }
        let window = crate::app_title::new::<crate::DebugFetchWindow>()?;
        if let Some(response) = &response {
            window.set_input_mode(false);
            window.set_message(
                format!(
                    "Request complete. Length of response is {}.",
                    hydrus_core::numbers::human_bytes(response.body.len() as u64)
                )
                .into(),
            );
        }
        window.on_cancelled({
            let state = Rc::downgrade(self);
            move || {
                if let Some(state) = state.upgrade() {
                    state.close(id);
                }
            }
        });
        window.window().on_close_requested({
            let state = Rc::downgrade(self);
            move || {
                if let Some(state) = state.upgrade() {
                    state.close(id);
                }
                slint::CloseRequestResponse::KeepWindowShown
            }
        });
        window.on_accepted({
            let state = Rc::downgrade(self);
            move || {
                let Some(state) = state.upgrade().filter(|s| s.valid_child(id)) else {
                    return;
                };
                let url = state
                    .children
                    .borrow()
                    .get(&id)
                    .filter(|child| child.response.is_none())
                    .map(|child| child.window.get_url().to_string());
                if let Some(url) = url.filter(|url| !url.is_empty()) {
                    state.close(id);
                    state.start(id, url);
                }
            }
        });
        window.on_choice({
            let state = Rc::downgrade(self);
            move |choice| {
                let Some(state) = state.upgrade().filter(|s| s.valid_child(id)) else {
                    return;
                };
                let response = state
                    .children
                    .borrow()
                    .get(&id)
                    .and_then(|child| child.response.clone());
                let Some(response) = response else { return };
                if !matches!(choice, 0 | 1) {
                    return;
                }
                state.close(id);
                if choice == 1 {
                    crate::copy_to_clipboard(&response.text());
                } else if let Some(path) = crate::pick_debug_response()
                    && state.live()
                    && let Err(error) = std::fs::write(&path, &response.body)
                {
                    hydrus_download::popups::show_exception(&state.store, error.to_string());
                    (state.published.borrow().clone())();
                }
            }
        });
        self.children.borrow_mut().insert(
            id,
            Child {
                window: window.clone_strong(),
                response,
            },
        );
        if let Err(error) = window.show() {
            self.close(id);
            return Err(error);
        }
        if !window.get_input_mode() {
            window.invoke_focus_result();
        }
        Ok(())
    }
    fn start(self: &Rc<Self>, id: u64, url: String) {
        if !self.live() {
            return;
        }
        let job = Job::new();
        // Native popup stop cancels this actual Job; the shared network cog remains separate.
        let popup = Working::new(&self.store, "debug network job", true);
        popup.set_network_job(Some(job.clone()));
        popup.show();
        let (send, receive) = crossbeam_channel::bounded(1);
        let store = self.store.clone();
        let worker_job = job.clone();
        let worker_popup = popup.clone();
        let spawn = std::thread::Builder::new()
            .name("debug-url-fetch".into())
            .spawn(move || {
                let run = || -> Result<Response, String> {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|e| e.to_string())?;
                    let network: NetworkSettings =
                        store.read(settings::get).map_err(|e| e.to_string())?;
                    runtime.block_on(async {
                        let engine =
                            NetEngine::new(store.clone(), NetOptions::from_settings(&network))
                                .map_err(|e| e.to_string())?;
                        worker_popup.keep_up();
                        let result = engine
                            .fetch(&Request::get(url), &worker_job)
                            .await
                            .map_err(|e| e.to_string());
                        let accounting = engine.save_bandwidth().map_err(|e| e.to_string());
                        match (result, accounting) {
                            (Ok(response), Ok(())) => Ok(response),
                            (Err(error), _) | (_, Err(error)) => Err(error),
                        }
                    })
                };
                let result = run();
                worker_popup.finish_and_dismiss_after(3);
                let _ = send.send(result);
            });
        if let Err(error) = spawn {
            popup.finish_and_dismiss_after(3);
            hydrus_download::popups::show_exception(&self.store, error.to_string());
        } else {
            self.running.borrow_mut().insert(
                id,
                Running {
                    job,
                    popup,
                    receive,
                },
            );
            if !self.timer.running() {
                let state = Rc::downgrade(self);
                self.timer.start(
                    slint::TimerMode::Repeated,
                    Duration::from_millis(50),
                    move || {
                        if let Some(state) = state.upgrade() {
                            state.tick();
                        }
                    },
                );
            }
        }
        (self.published.borrow().clone())();
    }
    fn tick(self: &Rc<Self>) {
        if !self.live() {
            self.retire();
            return;
        }
        let mut finished = Vec::new();
        for (&id, run) in self.running.borrow().iter() {
            if run.popup.is_cancelled() {
                run.job.cancel();
            }
            match run.receive.try_recv() {
                Ok(result) => finished.push((
                    id,
                    if run.job.is_cancelled() {
                        Err(hydrus_net::NetError::Cancelled.to_string())
                    } else {
                        result
                    },
                )),
                Err(crossbeam_channel::TryRecvError::Disconnected) => finished.push((
                    id,
                    Err("Network worker stopped before returning a response.".into()),
                )),
                Err(crossbeam_channel::TryRecvError::Empty) => (),
            }
        }
        for (id, result) in finished {
            self.running.borrow_mut().remove(&id);
            match result {
                Ok(response) => {
                    if let Err(error) = self.show(id, Some(response)) {
                        hydrus_download::popups::show_exception(&self.store, error.to_string());
                    }
                }
                Err(error) => hydrus_download::popups::show_exception(&self.store, error),
            }
        }
        if self.running.borrow().is_empty() {
            self.timer.stop();
        }
        (self.published.borrow().clone())();
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.retire();
    }
}
/// Retained public controls cannot bypass retirement; Bound clones share one lease.
#[derive(Clone)]
pub struct Control(Rc<State>);
pub(crate) struct Owner(Control);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugFetch")
            .field("running", &self.running())
            .finish_non_exhaustive()
    }
}
impl Control {
    pub(crate) fn new(main: &MainWindow, store: Arc<Store>, binding: Rc<Cell<bool>>) -> Self {
        Self(Rc::new(State {
            main: main.as_weak(),
            store,
            binding,
            active: Cell::new(true),
            next: Cell::new(0),
            timer: slint::Timer::default(),
            running: RefCell::default(),
            children: RefCell::default(),
            published: RefCell::new(Rc::new(|| {})),
        }))
    }
    pub(crate) fn owner(&self) -> Owner {
        Owner(self.clone())
    }
    pub(crate) fn set_published(&self, refresh: Rc<dyn Fn()>) {
        *self.0.published.borrow_mut() = refresh;
    }
    /// Permanently dispose requests and result children for this binding.
    pub fn retire(&self) {
        self.0.retire();
    }
    /// Open the reference URL prompt only for a visible current main owner.
    pub fn open(&self) {
        if !self.0.live()
            || !self
                .0
                .main
                .upgrade()
                .is_some_and(|main| main.window().is_visible() && main.get_question().is_empty())
        {
            return;
        }
        let id = self.0.next.get();
        self.0.next.set(id + 1);
        if let Err(error) = self.0.show(id, None) {
            hydrus_download::popups::show_exception(&self.0.store, error.to_string());
        }
    }
    /// Number of admitted HTTP requests still owned by this binding.
    pub fn running(&self) -> usize {
        self.0.running.borrow().len()
    }
    /// Current input/result children, for native owner and input inspection.
    pub fn windows(&self) -> Vec<DebugFetchWindow> {
        self.0
            .children
            .borrow()
            .values()
            .map(|child| child.window.clone_strong())
            .collect()
    }
    /// Receive actual worker results and forward the current popup stop state.
    pub fn tick(&self) {
        self.0.tick();
    }
}
