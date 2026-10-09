//! GUI-owned automatic maintenance, admitted by the live session idle monitor.
use crate::{MainWindow, session_autosave::Monitor};
use hydrus_store::{
    Store,
    maintenance::PurgeControl,
    maintenance_gates::{self, Worker},
};
use slint::ComponentHandle as _;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Weak, mpsc},
    thread::JoinHandle,
    time::Duration,
};

/// Observations of completed owned passes; retired replies cannot update these.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statistics {
    pub trash_passes: u64,
    pub deferred_passes: u64,
    pub trashed_files_removed: usize,
    pub physical_files_removed: usize,
    pub thumbnails_removed: usize,
    pub last_error: Option<String>,
}
#[derive(Debug)]
enum Completed {
    Trash(hydrus_store::trash::TrashReport),
    Deferred(hydrus_store::maintenance::PurgeReport),
}
#[derive(Debug)]
struct Pending {
    task: JoinHandle<()>,
    result: mpsc::Receiver<hydrus_store::Result<Completed>>,
}
struct Inner {
    window: slint::Weak<MainWindow>,
    store: Weak<Store>,
    monitor: Monitor,
    active: Rc<Cell<bool>>,
    live: Cell<bool>,
    shown: Cell<bool>,
    started_ms: i64,
    deadlines: [Cell<i64>; 2],
    pending: [RefCell<Option<Pending>>; 2],
    control: PurgeControl,
    statistics: RefCell<Statistics>,
    timer: slint::Timer,
    /// The idle state last published for the daemon, and when.
    published: Cell<Option<((bool, bool), i64)>>,
    /// The CPU-busy check, sampled once a minute, and its answer.
    cpu: RefCell<hydrus_store::idle_state::CpuBusy>,
    /// Per-core (busy, total) times given by a test instead of `/proc/stat`.
    cpu_times: RefCell<Option<CpuTimes>>,
    cpu_at: Cell<i64>,
    busy: Cell<bool>,
    /// The application-busy field's last look at the running jobs.
    app_busy: RefCell<hydrus_gui_model::status::AppBusy>,
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.control.cancel();
        self.timer.stop();
    }
}
type CpuTimes = Box<dyn FnMut() -> Vec<(u64, u64)>>;
/// One binding owns at most one pass of each kind, without retaining its window.
#[derive(Clone)]
pub struct Control(Rc<Inner>);
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaintenanceControl")
            .field("live", &self.0.live.get())
            .field("statistics", &self.0.statistics.borrow())
            .finish_non_exhaustive()
    }
}
/// The final binding clone retires work even if callbacks or a Control are retained.
#[derive(Debug)]
pub(crate) struct Owner(Control);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
fn index(worker: Worker) -> usize {
    match worker {
        Worker::Trash => 0,
        Worker::Deferred => 1,
    }
}
fn period(worker: Worker) -> i64 {
    match worker {
        Worker::Trash => 3_600_000,
        Worker::Deferred => 600_000,
    }
}
impl Control {
    pub(crate) fn bind(
        window: &MainWindow,
        store: &Arc<Store>,
        monitor: &Monitor,
        active: &Rc<Cell<bool>>,
    ) -> Self {
        let now = hydrus_core::TimestampMs::now().0;
        let control = Self(Rc::new(Inner {
            window: window.as_weak(),
            store: Arc::downgrade(store),
            monitor: monitor.clone(),
            active: active.clone(),
            live: Cell::new(true),
            shown: Cell::new(false),
            started_ms: now,
            deadlines: std::array::from_fn(|_| Cell::new(now.saturating_add(30_000))),
            pending: std::array::from_fn(|_| RefCell::new(None)),
            control: PurgeControl::default(),
            statistics: RefCell::new(Statistics::default()),
            timer: slint::Timer::default(),
            published: Cell::new(None),
            cpu: RefCell::default(),
            cpu_times: RefCell::new(None),
            cpu_at: Cell::new(i64::MIN),
            busy: Cell::new(false),
            app_busy: RefCell::default(),
        }));
        control
            .0
            .timer
            .start(slint::TimerMode::Repeated, Duration::from_secs(1), {
                let weak = Rc::downgrade(&control.0);
                move || {
                    if let Some(inner) = weak.upgrade() {
                        let owner = Self(inner);
                        if let Err(error) = owner.poll_at(hydrus_core::TimestampMs::now().0) {
                            eprintln!("automatic maintenance failed: {error}");
                        }
                    }
                }
            });
        control
    }
    pub(crate) fn owner(&self) -> Owner {
        Owner(self.clone())
    }
    /// Captured owner start time allows deterministic replay of real admissions.
    pub fn started_ms(&self) -> i64 {
        self.0.started_ms
    }
    /// Next pass boundary for this owner's existing native worker cadence.
    pub fn deadline(&self, worker: Worker) -> i64 {
        self.0.deadlines[index(worker)].get()
    }
    /// Owned pass reports; a cancelled/rebound owner's late reply is discarded.
    pub fn statistics(&self) -> Statistics {
        self.0.statistics.borrow().clone()
    }
    /// The cancellation this binding's passes share (for tests that act
    /// between a pass's writes).
    #[doc(hidden)]
    pub fn purge_control(&self) -> PurgeControl {
        self.0.control.clone()
    }
    /// Whether this binding still has a pass running off the GUI thread.
    pub fn running(&self, worker: Worker) -> bool {
        self.0.pending[index(worker)].borrow().is_some()
    }
    /// Permanent accepted-exit/rebind retirement, also performed on last-owner drop.
    pub fn retire(&self) {
        self.0.live.set(false);
        self.0.control.cancel();
        self.0.timer.stop();
        for pending in &self.0.pending {
            pending.borrow_mut().take();
        }
    }
    /// Publish the idle state for the daemon (on change, else every five
    /// seconds) and show it, with the CPU-busy check, in the status bar.
    fn publish_idle(&self, window: &MainWindow, store: &Store, now_ms: i64) {
        store.sleep_check();
        self.check_busy(store, now_ms);
        let idle = self.0.monitor.idle_at(now_ms);
        // Background work wants idle and a system that is not busy
        // (`GoodTimeToStartBackgroundWork`).
        let work_idle = idle && !self.0.busy.get();
        let due = self
            .0
            .published
            .get()
            .is_none_or(|(was, at)| was != (idle, work_idle) || now_ms - at >= 5_000);
        if due {
            if let Err(error) =
                hydrus_store::idle_state::publish_state(store.dir(), idle, work_idle, now_ms)
            {
                eprintln!("could not publish the idle state: {error}");
            }
            self.0.published.set(Some(((idle, work_idle), now_ms)));
        }
        let (idle_text, busy_text) = hydrus_gui_model::status::activity(idle, self.0.busy.get());
        window.set_status_idle(idle_text.into());
        window.set_status_busy(busy_text.into());
        let (idle_tip, busy_tip) =
            hydrus_gui_model::status::activity_tooltips(idle, self.0.busy.get());
        window.set_status_idle_tip(idle_tip.into());
        window.set_status_busy_tip(busy_tip.into());
        // the background jobs running: the daemon's downloader queues (if
        // what it last said is recent) and this client's maintenance passes
        let (app_text, app_tip) = {
            let mut app = self.0.app_busy.borrow_mut();
            let (text, tip) = app.status(now_ms / 1000, || {
                let daemon: hydrus_store::live::DaemonLive =
                    store.read(hydrus_store::settings::get).unwrap_or_default();
                let queues = if now_ms / 1000 - daemon.at <= 10 {
                    daemon.jobs as usize
                } else {
                    0
                };
                queues
                    + self
                        .0
                        .pending
                        .iter()
                        .filter(|p| p.borrow().is_some())
                        .count()
            });
            (text, tip.to_owned())
        };
        window.set_status_app_busy(app_text.into());
        window.set_status_app_busy_tip(app_tip.into());
        // (the reference's job name has no counterpart here, so no tooltip)
        window.set_status_db(store.db_activity().into());
    }
    /// Read the cores' (busy, total) times from `times` instead of the system,
    /// so tests can say how busy each core was.
    #[doc(hidden)]
    pub fn use_cpu_times(&self, times: impl FnMut() -> Vec<(u64, u64)> + 'static) {
        *self.0.cpu_times.borrow_mut() = Some(Box::new(times));
    }
    /// The CPU-busy check (`SystemBusy`), once a minute: busy when at least the
    /// saved number of cores ran above the saved percentage; never while idle
    /// mode is forced, and never when the core count is none.
    fn check_busy(&self, store: &Store, now_ms: i64) {
        if now_ms.saturating_sub(self.0.cpu_at.get()) < 60_000 {
            return;
        }
        self.0.cpu_at.set(now_ms);
        let config: hydrus_store::settings::GuiIdleSettings =
            store.read(hydrus_store::settings::get).unwrap_or_default();
        let busy = match config.busy_cpu_count {
            _ if self.0.monitor.forced_idle() => false,
            None => false,
            Some(count) => {
                let mut cpu = self.0.cpu.borrow_mut();
                let sample = match self.0.cpu_times.borrow_mut().as_mut() {
                    Some(times) => cpu.sample_times(&times(), config.busy_cpu_percent, count),
                    None => cpu.sample(config.busy_cpu_percent, count),
                };
                sample.unwrap_or(self.0.busy.get())
            }
        };
        self.0.busy.set(busy);
    }
    /// Sample current saved settings and live idle state immediately before each
    /// admission. Already admitted passes retain their policy until completion.
    /// Only completed threads are joined here; filesystem work/waits stay off UI.
    pub fn poll_at(&self, now_ms: i64) -> hydrus_store::Result<()> {
        if !self.0.live.get() || !self.0.active.get() {
            self.retire();
            return Ok(());
        }
        let Some(window) = self.0.window.upgrade() else {
            self.retire();
            return Ok(());
        };
        if !self.0.shown.get() {
            if !window.window().is_visible() {
                return Ok(());
            }
            self.0.shown.set(true);
        }
        let Some(store) = self.0.store.upgrade() else {
            self.retire();
            return Ok(());
        };
        self.publish_idle(&window, &store, now_ms);
        for worker in [Worker::Trash, Worker::Deferred] {
            let slot = &self.0.pending[index(worker)];
            if slot
                .borrow()
                .as_ref()
                .is_some_and(|pending| pending.task.is_finished())
            {
                let pending = slot.borrow_mut().take().expect("completed owned task");
                let joined = pending.task.join();
                let report = pending.result.try_recv().unwrap_or_else(|_| {
                    Err(hydrus_store::StoreError::Corrupt(
                        "maintenance worker returned without its result".into(),
                    ))
                });
                let mut stats = self.0.statistics.borrow_mut();
                match report {
                    Ok(Completed::Trash(report)) if joined.is_ok() => {
                        stats.trash_passes += 1;
                        stats.trashed_files_removed += report.total();
                    }
                    Ok(Completed::Deferred(report)) if joined.is_ok() => {
                        stats.deferred_passes += 1;
                        stats.physical_files_removed += report.files_deleted;
                        stats.thumbnails_removed += report.thumbnails_deleted;
                    }
                    Err(error) => stats.last_error = Some(error.to_string()),
                    _ => stats.last_error = Some("maintenance worker panicked".into()),
                }
            }
            if slot.borrow().is_some() || now_ms < self.deadline(worker) {
                continue;
            }
            // Qt checks this only at entry, not between file/thumbnail pairs.
            let idle = self.0.monitor.idle_at(now_ms) && !self.0.busy.get();
            let gates = store.read(maintenance_gates::load)?;
            self.0.deadlines[index(worker)].set(now_ms.saturating_add(period(worker)));
            if !gates.allows(worker, idle) {
                continue;
            }
            hydrus_core::debug_flags::report(hydrus_core::debug_flags::Flag::DaemonReport, || {
                let name = match worker {
                    Worker::Trash => "maintain_trash",
                    Worker::Deferred => "deferred_physical_deletes",
                };
                format!("{name} doing a job.")
            });
            let store = store.clone();
            let cancellation = self.0.control.clone();
            let (result, receiver) = mpsc::sync_channel(1);
            let task = std::thread::Builder::new()
                .name(format!("gui-maintenance-{worker:?}"))
                .spawn(move || {
                    let report = if cancellation.is_cancelled() {
                        Ok(match worker {
                            Worker::Trash => {
                                Completed::Trash(hydrus_store::trash::TrashReport::default())
                            }
                            Worker::Deferred => Completed::Deferred(
                                hydrus_store::maintenance::PurgeReport::default(),
                            ),
                        })
                    } else {
                        match worker {
                            Worker::Trash => hydrus_store::trash::maintain_trash_with_control(
                                &store,
                                256,
                                &cancellation,
                            )
                            .map(Completed::Trash),
                            Worker::Deferred => {
                                hydrus_store::maintenance::purge_deleted_media_with_control(
                                    &store,
                                    1024,
                                    &cancellation,
                                )
                                .map(Completed::Deferred)
                            }
                        }
                    };
                    let _ = result.send(report);
                })?;
            *slot.borrow_mut() = Some(Pending {
                task,
                result: receiver,
            });
        }
        Ok(())
    }
}
