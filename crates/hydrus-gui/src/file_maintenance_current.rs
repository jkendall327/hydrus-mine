//! One GUI incarnation's scheduled-work review and serial asynchronous runner.
use crate::{FileMaintenanceWindow, TableRow};
use hydrus_gui_model::file_maintenance_current as model;
use hydrus_store::{
    Store,
    file_maintenance::{self, JobType},
    popups,
};
use slint::{ComponentHandle as _, ModelRc, SharedString, Timer, TimerMode, VecModel};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

type Counts = BTreeMap<JobType, (u64, u64)>;
pub type Slot = Rc<RefCell<Option<FileMaintenanceWindow>>>;
enum Command {
    Refresh,
    Clear(Vec<JobType>),
    Force(Option<Vec<JobType>>),
}
enum Reply {
    Rows(Result<Counts, String>),
    Redownload(Vec<String>),
    Changed(bool),
    Error(String),
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
fn rows(store: &Store) -> Result<Counts, String> {
    store
        .read(|conn| file_maintenance::job_counts(conn, now()))
        .map_err(|e| e.to_string())
}
fn clear(store: &Store, jobs: Vec<JobType>, shutdown: &AtomicBool) -> Result<(), String> {
    let _lease = loop {
        if shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(lease) =
            hydrus_store::store::lock_file_maintenance(store.dir()).map_err(|e| e.to_string())?
        {
            break lease;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if shutdown.load(Ordering::Acquire) {
        return Ok(());
    }
    store
        .write(move |ctx| file_maintenance::cancel_jobs(ctx.conn(), &jobs))
        .map_err(|e| e.to_string())
}
fn force(
    store: &Arc<Store>,
    wanted: Option<&[JobType]>,
    shutdown: &AtomicBool,
    replies: &mpsc::Sender<Reply>,
    commands: &mpsc::Receiver<Command>,
    deferred: &mut VecDeque<Command>,
) -> Result<(), String> {
    let total: u64 = rows(store)?
        .iter()
        .filter(|(job, _)| wanted.is_none_or(|wanted| wanted.contains(job)))
        .map(|(_, &(due, _))| due)
        .sum();
    if total == 0 {
        let message = popups::Job::text("No file maintenance due!", now() as f64);
        return store
            .write(move |ctx| popups::add(ctx.conn(), &message, now()))
            .map_err(|e| e.to_string());
    }
    let mut job = popups::Job::new(false, true, now() as f64);
    job.status_title = Some("file maintenance".into());
    let key = job.key;
    store
        .write(move |ctx| popups::add(ctx.conn(), &job, now()))
        .map_err(|e| e.to_string())?;
    // Default MediaTools remain configured by FileImporter (including FFmpeg).
    let importer = hydrus_import::FileImporter::new(store.clone(), hydrus_media::MediaTools::new());
    let mut delivered = 0;
    let result = importer.run_file_maintenance_with_callbacks(
        u64::MAX,
        u64::MAX,
        &|job| wanted.is_none_or(|wanted| wanted.contains(&job)),
        &|| {
            !shutdown.load(Ordering::Acquire)
                && store
                    .read(|conn| popups::get(conn, &key, now()))
                    .ok()
                    .flatten()
                    .is_some_and(|job| !job.cancelled)
        },
        hydrus_import::maintenance::MaintenanceCallbacks {
            before_batch: &mut || {
                // The completed batch has released its local media/results, but
                // this actor still owns the exclusive physical-work lease.
                while let Ok(command) = commands.try_recv() {
                    if shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    match command {
                        Command::Clear(jobs) => {
                            store.write(move |ctx| {
                                file_maintenance::cancel_jobs(ctx.conn(), &jobs)
                            })?;
                            let _ = replies.send(Reply::Rows(rows(store)));
                        }
                        Command::Refresh => {
                            let _ = replies.send(Reply::Rows(rows(store)));
                        }
                        command @ Command::Force(_) => deferred.push_back(command),
                    }
                }
                Ok(())
            },
            before_job: &mut |done| {
                let _ = store.write(move |ctx| {
                    popups::update(ctx.conn(), &key, now(), |job| {
                        if !job.cancelled {
                            job.status_text_1 = Some(format!(
                                "{}/{}",
                                hydrus_core::numbers::human_int(done),
                                hydrus_core::numbers::human_int(total)
                            ));
                            job.popup_gauge_1 = Some((
                                i64::try_from(done).unwrap_or(i64::MAX),
                                i64::try_from(total).unwrap_or(i64::MAX),
                            ));
                        }
                    })
                });
            },
            committed: &mut |report| {
                if report.redownload.len() > delivered {
                    let _ =
                        replies.send(Reply::Redownload(report.redownload[delivered..].to_vec()));
                    delivered = report.redownload.len();
                }
                let _ = replies.send(Reply::Changed(report.done.keys().any(|job| {
                    matches!(
                        job,
                        JobType::ForceThumbnail | JobType::RefitThumbnail | JobType::FileMetadata
                    )
                })));
            },
        },
    );
    store
        .write(move |ctx| {
            popups::update(ctx.conn(), &key, now(), |job| {
                job.status_text_1 = Some("done!".into());
                job.popup_gauge_1 = None;
                job.finish_and_dismiss(Some(5), now());
            })
        })
        .map_err(|e| e.to_string())?;
    result.map(|_| ()).map_err(|e| e.to_string())
}

struct State {
    active: Cell<bool>,
    shutdown: Arc<AtomicBool>,
    commands: mpsc::Sender<Command>,
    replies: RefCell<mpsc::Receiver<Reply>>,
    queue: RefCell<model::Queue>,
    question: RefCell<Option<Vec<JobType>>>,
    slot: Slot,
    valid: Rc<dyn Fn() -> bool>,
    visible: Rc<dyn Fn() -> bool>,
    changed: Rc<dyn Fn(bool)>,
    redownload: Rc<dyn Fn(Vec<String>)>,
    store: Arc<Store>,
    timer: Timer,
}
impl Drop for State {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
    }
}
#[derive(Clone)]
pub struct Control(Rc<State>);
pub struct Owner(Control);
impl std::fmt::Debug for Owner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.0, f)
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileMaintenance")
            .field("active", &self.0.active.get())
            .finish_non_exhaustive()
    }
}
impl Control {
    pub fn owner(&self) -> Owner {
        Owner(self.clone())
    }
    pub fn bind(
        store: Arc<Store>,
        valid: Rc<dyn Fn() -> bool>,
        visible: Rc<dyn Fn() -> bool>,
        changed: Rc<dyn Fn(bool)>,
        redownload: Rc<dyn Fn(Vec<String>)>,
    ) -> Result<Self, String> {
        let (send, commands) = mpsc::channel();
        let (replies, recv) = mpsc::channel();
        let shutdown = Arc::new(AtomicBool::new(false));
        let stop = shutdown.clone();
        let kept = store.clone();
        std::thread::Builder::new()
            .name("file maintenance".into())
            .spawn(move || {
                let mut deferred = VecDeque::new();
                while let Some(command) = deferred.pop_front().or_else(|| commands.recv().ok()) {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    let result = match command {
                        Command::Refresh => Ok(()),
                        Command::Clear(jobs) => clear(&store, jobs, &stop),
                        Command::Force(wanted) => force(
                            &store,
                            wanted.as_deref(),
                            &stop,
                            &replies,
                            &commands,
                            &mut deferred,
                        ),
                    };
                    if let Err(error) = result {
                        let message = popups::Job::text(error.clone(), now() as f64);
                        let _ = store.write(move |ctx| popups::add(ctx.conn(), &message, now()));
                        let _ = replies.send(Reply::Error(error));
                    }
                    let _ = replies.send(Reply::Rows(rows(&store)));
                }
            })
            .map_err(|e| e.to_string())?;
        let state = Rc::new(State {
            active: Cell::new(true),
            shutdown,
            commands: send,
            replies: RefCell::new(recv),
            queue: RefCell::new(model::Queue::new()),
            question: RefCell::new(None),
            slot: Slot::default(),
            valid,
            visible,
            changed,
            redownload,
            store: kept,
            timer: Timer::default(),
        });
        let weak = Rc::downgrade(&state);
        state
            .timer
            .start(TimerMode::Repeated, Duration::from_millis(20), move || {
                let Some(state) = weak.upgrade() else { return };
                Control(state).poll();
            });
        Ok(Self(state))
    }
    pub fn slot(&self) -> Slot {
        self.0.slot.clone()
    }
    pub fn retire(&self) {
        if !self.0.active.replace(false) {
            return;
        }
        self.0.shutdown.store(true, Ordering::Release);
        // Wake an idle receiver even when retained Main callbacks still own us.
        let _ = self.0.commands.send(Command::Refresh);
        self.0.timer.stop();
        let window = self
            .0
            .slot
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong);
        if let Some(window) = window {
            window.invoke_close_clicked();
        }
    }
    pub fn poll(&self) {
        if !self.0.active.get() {
            return;
        }
        if !(self.0.valid)() {
            self.retire();
            return;
        }
        loop {
            let reply = self.0.replies.borrow().try_recv();
            match reply {
                Ok(Reply::Rows(Ok(counts))) => {
                    self.0.queue.borrow_mut().replace(counts);
                    self.paint();
                }
                Ok(Reply::Rows(Err(error)) | Reply::Error(error)) => self.error(error),
                Ok(Reply::Redownload(urls)) => (self.0.redownload)(urls),
                Ok(Reply::Changed(thumbnails)) => (self.0.changed)(thumbnails),
                Err(_) => break,
            }
        }
    }
    fn error(&self, error: String) {
        if let Some(window) = self.0.slot.borrow().as_ref() {
            window.set_error(error.into());
        }
    }
    fn paint(&self) {
        let slot = self.0.slot.borrow();
        let Some(window) = slot.as_ref() else { return };
        let queue = self.0.queue.borrow();
        window.set_rows(ModelRc::new(VecModel::from(
            queue
                .rows()
                .iter()
                .map(|row| TableRow {
                    cells: ModelRc::new(VecModel::from(vec![
                        SharedString::from(row.name.clone()),
                        SharedString::from(row.count_text()),
                    ])),
                    selected: row.selected,
                })
                .collect::<Vec<_>>(),
        )));
        window.set_can_clear(!queue.selected().is_empty());
        window.set_can_work(queue.can_work(true));
        window.set_can_all(queue.can_work(false));
        window.set_sort_column(i32::from(queue.numeric));
        window.set_ascending(queue.ascending);
    }
    pub fn open(&self) -> Result<FileMaintenanceWindow, String> {
        if !self.0.active.get() || !(self.0.valid)() || !(self.0.visible)() {
            return Err("The main window is unavailable.".into());
        }
        if let Some(window) = self.0.slot.borrow().as_ref() {
            window.show().map_err(|e| e.to_string())?;
            return Ok(window.clone_strong());
        }
        let window =
            crate::app_title::new::<crate::FileMaintenanceWindow>().map_err(|e| e.to_string())?;
        // A new Review frame starts without selection or stale count rows.
        // Keep this owner's column sort choice, but refetch actual counts.
        self.0.queue.borrow_mut().replace(BTreeMap::new());
        window.set_explanation(model::EXPLANATION.into());
        let weak = Rc::downgrade(&self.0);
        let owner = window.as_weak();
        let input = Rc::new(move || {
            let state = weak.upgrade()?;
            let window = owner.upgrade()?;
            let current = state
                .slot
                .borrow()
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.window(), window.window()));
            (current
                && state.active.get()
                && (state.valid)()
                && (state.visible)()
                && window.window().is_visible())
            .then_some(Control(state))
        });
        window.on_clicked({
            let input = input.clone();
            move |index, ctrl, shift| {
                let Some(control) = input() else { return };
                if control.0.question.borrow().is_some() {
                    return;
                }
                if let Ok(index) = usize::try_from(index) {
                    control.0.queue.borrow_mut().click(index, ctrl, shift);
                    control.paint();
                }
            }
        });
        window.on_sorted({
            let input = input.clone();
            move |column, ascending| {
                let Some(control) = input() else { return };
                if control.0.question.borrow().is_some() {
                    return;
                }
                {
                    let mut queue = control.0.queue.borrow_mut();
                    queue.numeric = column == 1;
                    queue.ascending = ascending;
                }
                control.paint();
            }
        });
        window.on_refresh_clicked({
            let input = input.clone();
            move || {
                if let Some(control) = input()
                    && control.0.question.borrow().is_none()
                {
                    let _ = control.0.commands.send(Command::Refresh);
                }
            }
        });
        window.on_clear_clicked({
            let input = input.clone();
            let owner = window.as_weak();
            move || {
                let Some(control) = input() else { return };
                if control.0.question.borrow().is_some() {
                    return;
                }
                let jobs = control.0.queue.borrow().selected();
                if jobs.is_empty() {
                    return;
                }
                *control.0.question.borrow_mut() = Some(jobs);
                if let Some(window) = owner.upgrade() {
                    window.set_question(model::CLEAR_QUESTION.into());
                }
            }
        });
        window.on_answered({
            let input = input.clone();
            let owner = window.as_weak();
            move |yes| {
                let Some(control) = input() else { return };
                let Some(jobs) = control.0.question.borrow_mut().take() else {
                    return;
                };
                if let Some(window) = owner.upgrade() {
                    window.set_question(SharedString::new());
                }
                if yes {
                    let _ = control.0.commands.send(Command::Clear(jobs));
                }
            }
        });
        window.on_work_clicked({
            let input = input.clone();
            move |all| {
                let Some(control) = input() else { return };
                if control.0.question.borrow().is_some() {
                    return;
                }
                let queue = control.0.queue.borrow();
                if !queue.can_work(!all) {
                    return;
                }
                let wanted = if all { None } else { Some(queue.selected()) };
                let _ = control.0.commands.send(Command::Force(wanted));
            }
        });
        let new_work = crate::file_maintenance_new::bind(
            &window,
            &self.0.store,
            Rc::new({
                let input = input.clone();
                move || input().is_some()
            }),
            Rc::new({
                let weak = Rc::downgrade(&self.0);
                move || {
                    if let Some(state) = weak.upgrade() {
                        let _ = state.commands.send(Command::Refresh);
                    }
                }
            }),
        );
        let close = Rc::new({
            let new_work = new_work.clone();
            let weak = Rc::downgrade(&self.0);
            let owner = window.as_weak();
            move || {
                new_work.retire();
                if let Some(window) = owner.upgrade() {
                    if let Some(state) = weak.upgrade() {
                        let current =
                            state.slot.borrow().as_ref().is_some_and(|current| {
                                std::ptr::eq(current.window(), window.window())
                            });
                        if current {
                            state.slot.borrow_mut().take();
                            state.question.borrow_mut().take();
                        }
                    }
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
            slint::CloseRequestResponse::HideWindow
        });
        *self.0.slot.borrow_mut() = Some(window.clone_strong());
        self.paint();
        if let Err(error) = window.show() {
            window.invoke_close_clicked();
            return Err(error.to_string());
        }
        let _ = self.0.commands.send(Command::Refresh);
        Ok(window)
    }
}
