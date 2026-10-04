//! Downloader controls over the daemon's epoch-scoped protocol. Every menu holds
//! the reviewed request ID, rather than accidentally acting on its replacement.
use crate::{JobCogMenu, MainWindow, MenuRow, NetworkErrorWindow, network_data_window};
use crossbeam_channel::{Receiver, Sender};
use hydrus_gui_model::{
    network_data::Review,
    network_job_control::{self as model, Action, Control, Target},
};
use hydrus_store::{
    Store,
    network_runtime::{self, Command, JobAction},
};
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::Duration};

fn now() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}
enum Operation {
    Command(Command),
    Stop,
}
struct Worker {
    send: Sender<Operation>,
    receive: Receiver<Result<Review, String>>,
}
impl Worker {
    fn start(store: Arc<Store>) -> Self {
        let (send, requests) = crossbeam_channel::unbounded();
        let (events, receive) = crossbeam_channel::unbounded();
        std::thread::spawn(move || {
            loop {
                match requests.recv_timeout(Duration::from_millis(250)) {
                    Ok(Operation::Stop)
                    | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    Ok(Operation::Command(command)) => {
                        if let Err(error) =
                            store.write(move |ctx| network_runtime::send(ctx.conn(), command))
                        {
                            let _ = events.send(Err(error.to_string()));
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                }
                if events.send(Review::load(&store, now())).is_err() {
                    break;
                }
            }
        });
        Self { send, receive }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.send.send(Operation::Stop);
    }
}

/// The error dialog's owner. Showing/copying after an owner clear is a no-op.
#[derive(Clone, Default)]
pub struct Errors(Rc<RefCell<Option<NetworkErrorWindow>>>);
impl Errors {
    /// Close an error popup when its owning window is force-closed.
    pub fn cancel(&self) {
        let window = self.0.borrow_mut().take();
        if let Some(window) = window {
            let _ = window.hide();
        }
    }
    pub fn show(&self, text: &str) -> Result<NetworkErrorWindow, String> {
        if let Some(window) = self.0.borrow_mut().take() {
            let _ = window.hide();
        }
        let window = NetworkErrorWindow::new().map_err(|e| e.to_string())?;
        window.set_error_text(text.into());
        let close = Rc::new({
            let owner = Rc::downgrade(&self.0);
            move || {
                if let Some(owner) = owner.upgrade()
                    && let Some(window) = owner.borrow_mut().take()
                {
                    let _ = window.hide();
                }
            }
        });
        window.on_close_clicked({
            let close = close.clone();
            move || close()
        });
        window.window().on_close_requested(move || {
            close();
            slint::CloseRequestResponse::KeepWindowShown
        });
        LAST_ERROR.with(|last| *last.borrow_mut() = Some(window.as_weak()));
        *self.0.borrow_mut() = Some(window.clone_strong());
        window.show().map_err(|e| e.to_string())?;
        Ok(window)
    }
}
impl Drop for Errors {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1
            && let Some(window) = self.0.borrow_mut().take()
        {
            let _ = window.hide();
        }
    }
}
thread_local! { static LAST_ERROR: RefCell<Option<slint::Weak<NetworkErrorWindow>>> = const { RefCell::new(None) }; }
pub fn last_error() -> Option<NetworkErrorWindow> {
    LAST_ERROR.with(|last| last.borrow().as_ref().and_then(slint::Weak::upgrade))
}

struct Held {
    epoch: String,
    job: Option<u64>,
    actions: HashMap<i32, Action>,
}
struct State {
    window: slint::Weak<MainWindow>,
    store: Arc<Store>,
    target: Rc<dyn Fn(bool) -> Option<Target>>,
    owner_key: Rc<dyn Fn() -> String>,
    owner_alive: RefCell<Rc<dyn Fn(&str) -> bool>>,
    targets: RefCell<HashMap<(String, bool), Target>>,
    rules: network_data_window::Slots,
    errors: Errors,
    worker: Worker,
    timer: Timer,
    review: RefCell<Option<Review>>,
    controls: RefCell<HashMap<(String, bool), Control>>,
    held: RefCell<HashMap<(String, bool), Held>>,
}
fn item(label: String, id: i32) -> MenuRow {
    MenuRow {
        label: label.into(),
        id,
        checkable: false,
        checked: false,
    }
}
fn action_id(action: &Action) -> i32 {
    match action {
        Action::CopyUrl(_) => 1,
        Action::Job(JobAction::OverrideConnectionWait) => 2,
        Action::Job(JobAction::ScrubDomainErrors) => 3,
        Action::Job(JobAction::OverrideServerBandwidthWait) => 4,
        Action::Job(JobAction::OverrideBandwidth) => 5,
        Action::Job(JobAction::OverrideGalleryWait) => 6,
        _ => -1,
    }
}
impl State {
    fn key(&self, gallery: bool) -> (String, bool) {
        ((self.owner_key)(), gallery)
    }
    fn owner_for(&self, target: Target) -> Option<(String, bool)> {
        self.targets
            .borrow()
            .iter()
            .find(|(_, owned)| **owned == target)
            .map(|(key, _)| key.clone())
            .or_else(|| {
                ((self.target)(target.gallery) == Some(target)).then(|| self.key(target.gallery))
            })
    }
    fn poll(&self) {
        for result in self.worker.receive.try_iter() {
            match result {
                Ok(review) => *self.review.borrow_mut() = Some(review),
                Err(error) => {
                    if let Some(window) = self.window.upgrade() {
                        window.set_note(error.into());
                    }
                }
            }
        }
        let review = self.review.borrow();
        let Some(review) = review.as_ref() else {
            return;
        };
        let mut controls = self.controls.borrow_mut();
        controls.retain(|key, control| {
            if (self.owner_alive.borrow())(&key.0) {
                return true;
            }
            if control.auto_override
                && let Some(target) = self.targets.borrow().get(key)
                && let Some((job, _)) = target.job(&review.runtime, now())
            {
                let _ = self.worker.send.send(Operation::Command(Command {
                    epoch: review.runtime.epoch.clone(),
                    job: job.id,
                    action: JobAction::AutoOverrideBandwidth(false),
                }));
            }
            self.targets.borrow_mut().remove(key);
            self.held.borrow_mut().remove(key);
            false
        });
        for gallery in [false, true] {
            if let Some(target) = (self.target)(gallery) {
                let key = self.key(gallery);
                if (self.owner_alive.borrow())(&key.0) {
                    let previous = self.targets.borrow().get(&key).copied();
                    if previous.is_some_and(|old| old != target)
                        && controls.get(&key).is_some_and(|c| c.auto_override)
                        && let Some((job, _)) =
                            previous.and_then(|old| old.job(&review.runtime, now()))
                    {
                        let _ = self.worker.send.send(Operation::Command(Command {
                            epoch: review.runtime.epoch.clone(),
                            job: job.id,
                            action: JobAction::AutoOverrideBandwidth(false),
                        }));
                    }
                    self.targets.borrow_mut().insert(key.clone(), target);
                    controls.entry(key).or_default();
                }
            } else {
                let key = self.key(gallery);
                let previous = self.targets.borrow_mut().remove(&key);
                if controls.get(&key).is_some_and(|c| c.auto_override)
                    && let Some((job, _)) = previous.and_then(|old| old.job(&review.runtime, now()))
                {
                    let _ = self.worker.send.send(Operation::Command(Command {
                        epoch: review.runtime.epoch.clone(),
                        job: job.id,
                        action: JobAction::AutoOverrideBandwidth(false),
                    }));
                }
            }
        }
        for (key, control) in controls.iter_mut() {
            let Some(target) = self.targets.borrow().get(key).copied() else {
                continue;
            };
            if let Some(command) = control.sync(target, &review.runtime, now()) {
                let _ = self.worker.send.send(Operation::Command(command));
            }
        }
        if let Some(window) = self.window.upgrade() {
            for gallery in [false, true] {
                let control = controls.get(&self.key(gallery));
                let mut menu = if gallery {
                    window.get_search_cog()
                } else {
                    window.get_file_cog()
                };
                menu.auto_override = control.is_some_and(|c| c.auto_override);
                menu.has_error = control.is_some_and(|c| c.error().is_some());
                if gallery {
                    window.set_search_cog(menu);
                } else {
                    window.set_file_cog(menu);
                }
            }
        }
    }
    fn menu(&self, gallery: bool) {
        let Some(target) = (self.target)(gallery) else {
            return;
        };
        let review = self.review.borrow();
        let Some(review) = review.as_ref() else {
            return;
        };
        let controls = self.controls.borrow();
        let Some(control) = controls.get(&self.key(gallery)) else {
            return;
        };
        let job = target.job(&review.runtime, now());
        let menu = model::cog(review, job, control.auto_override, now());
        let mut actions = HashMap::new();
        let url = menu.url.map_or_else(String::new, |entry| {
            actions.insert(1, entry.action);
            entry.label
        });
        let rules = menu
            .rules
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let id = i32::try_from(i).unwrap_or(i32::MAX).saturating_add(100);
                actions.insert(id, entry.action);
                item(entry.label, id)
            })
            .collect::<Vec<_>>();
        let rows = menu
            .actions
            .into_iter()
            .map(|entry| {
                let id = action_id(&entry.action);
                actions.insert(id, entry.action);
                item(entry.label, id)
            })
            .collect::<Vec<_>>();
        self.held.borrow_mut().insert(
            self.key(gallery),
            Held {
                epoch: review.runtime.epoch.clone(),
                job: job.map(|(j, _)| j.id),
                actions,
            },
        );
        let menu = JobCogMenu {
            url: url.into(),
            rules: ModelRc::new(VecModel::from(rules)),
            actions: ModelRc::new(VecModel::from(rows)),
            auto_override: control.auto_override,
            has_error: control.error().is_some(),
        };
        if let Some(window) = self.window.upgrade() {
            if gallery {
                window.set_search_cog(menu);
            } else {
                window.set_file_cog(menu);
            }
        }
    }
    fn action(&self, gallery: bool, id: i32) {
        let Some(_) = (self.target)(gallery) else {
            return;
        };
        if !(self.owner_alive.borrow())(&(self.owner_key)()) {
            return;
        }
        if id == 7 {
            self.controls
                .borrow_mut()
                .entry(self.key(gallery))
                .or_default()
                .flip_auto_override();
            self.poll();
            self.menu(gallery);
            return;
        }
        if matches!(id, 8 | 9) {
            let text = self
                .controls
                .borrow()
                .get(&self.key(gallery))
                .and_then(|c| c.error())
                .map(str::to_owned);
            if let Some(text) = text {
                if id == 8 {
                    let _ = self.errors.show(&text);
                } else {
                    crate::copy_to_clipboard(&text);
                }
            }
            return;
        }
        let held = self.held.borrow();
        let Some(held) = held.get(&self.key(gallery)) else {
            return;
        };
        match held.actions.get(&id) {
            Some(Action::CopyUrl(url)) => crate::copy_to_clipboard(url),
            Some(Action::Rules(context)) => {
                let _ = network_data_window::open_rules(
                    self.store.clone(),
                    &self.rules,
                    context.clone(),
                );
            }
            Some(Action::Job(action)) => {
                if let Some(job) = held.job
                    && self.review.borrow().as_ref().is_some_and(|review| {
                        review.runtime.fresh(now())
                            && review.runtime.epoch == held.epoch
                            && review.runtime.jobs.iter().any(|j| j.id == job)
                    })
                {
                    let _ = self.worker.send.send(Operation::Command(Command {
                        epoch: held.epoch.clone(),
                        job,
                        action: *action,
                    }));
                }
            }
            _ => {}
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.timer.stop();
        if let Some(review) = self.review.borrow().as_ref() {
            for (key, control) in self.controls.borrow().iter() {
                if control.auto_override
                    && let Some(target) = self.targets.borrow().get(key)
                    && let Some((job, _)) = target.job(&review.runtime, now())
                {
                    let _ = self.worker.send.send(Operation::Command(Command {
                        epoch: review.runtime.epoch.clone(),
                        job: job.id,
                        action: JobAction::AutoOverrideBandwidth(false),
                    }));
                }
            }
        }
    }
}
/// Keep this owner alive as long as the downloader page controls are alive.
#[derive(Clone)]
pub struct Binding(Rc<State>);
impl Binding {
    /// Retire closed page controls, stopping their pending auto overrides.
    pub fn set_owner_alive(&self, alive: Rc<dyn Fn(&str) -> bool>) {
        *self.0.owner_alive.borrow_mut() = alive;
    }
    /// Explicit owner behavior, matching ClearError before a new test-data fetch.
    pub fn clear_error(&self, target: Target) {
        self.0.poll();
        let Some(key) = self.0.owner_for(target) else {
            return;
        };
        self.0
            .controls
            .borrow_mut()
            .entry(key)
            .or_default()
            .clear_error();
        self.0.poll();
    }
    pub fn set_error(&self, target: Target, text: String) {
        self.0.poll();
        let Some(key) = self.0.owner_for(target) else {
            return;
        };
        self.0
            .controls
            .borrow_mut()
            .entry(key)
            .or_default()
            .set_error(text);
        self.0.poll();
    }
}
pub fn bind(
    window: &MainWindow,
    store: Arc<Store>,
    target: Rc<dyn Fn(bool) -> Option<Target>>,
    rules: network_data_window::Slots,
) -> Binding {
    bind_owned(
        window,
        store,
        target,
        Rc::new(|| "standalone".into()),
        rules,
    )
}

/// Page identity keeps this widget's policy when its highlighted importer changes.
pub fn bind_owned(
    window: &MainWindow,
    store: Arc<Store>,
    target: Rc<dyn Fn(bool) -> Option<Target>>,
    owner_key: Rc<dyn Fn() -> String>,
    rules: network_data_window::Slots,
) -> Binding {
    let state = Rc::new(State {
        window: window.as_weak(),
        worker: Worker::start(store.clone()),
        store,
        target,
        owner_key,
        owner_alive: RefCell::new(Rc::new(|_| true)),
        targets: RefCell::default(),
        rules,
        errors: Errors::default(),
        timer: Timer::default(),
        review: RefCell::default(),
        controls: RefCell::default(),
        held: RefCell::default(),
    });
    window.on_control_menu({
        let state = Rc::downgrade(&state);
        move |gallery| {
            if let Some(s) = state.upgrade() {
                s.poll();
                s.menu(gallery);
            }
        }
    });
    window.on_control_action({
        let state = Rc::downgrade(&state);
        move |gallery, id| {
            if let Some(s) = state.upgrade() {
                s.action(gallery, id);
            }
        }
    });
    state
        .timer
        .start(TimerMode::Repeated, Duration::from_millis(100), {
            let state = Rc::downgrade(&state);
            move || {
                if let Some(s) = state.upgrade() {
                    s.poll();
                }
            }
        });
    Binding(state)
}
