//! The page's owned still/poster preview and its display interval. The selected
//! thumbnail only requests media; this canvas records its own displayed file.
//! Page/request identities retire late decodes and window close is terminal.
use crate::{MainWindow, thumbnails::Pixels};
use hydrus_core::{CanvasType, HashId, TimestampMs, pages::PageKey};
use hydrus_gui_model::viewing_statistics::Tracker;
use hydrus_store::Store;
use slint::{ComponentHandle as _, Timer, TimerMode};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

type Target = (PageKey, HashId);
type Source = Rc<dyn Fn() -> Option<Target>>;
/// The preview clock, retained by its canvas owner rather than a global clock.
pub type Clock = Rc<dyn Fn() -> i64>;
/// Decode backend; the owner can supply a bounded media worker for deterministic replay.
pub type Decoder = Arc<dyn Fn(&Store, HashId) -> Option<hydrus_media::Raster> + Send + Sync>;
struct Pending {
    file: HashId,
    started_ms: i64,
    generation: u128,
}
struct Request {
    file: HashId,
    generation: u128,
    decoder: Decoder,
}
// One held obsolete decode must not block its successor's presentation. Two
// workers preserve that behavior, with only the latest queued request retained.
struct Workers {
    requests: crossbeam_channel::Sender<Request>,
    queued: crossbeam_channel::Receiver<Request>,
    pixels: crossbeam_channel::Receiver<(u128, Option<Pixels>)>,
    alive: Arc<AtomicBool>,
}
impl Workers {
    fn new(store: &Arc<Store>) -> std::io::Result<Self> {
        let (requests, queued) = crossbeam_channel::bounded::<Request>(1);
        let (send, pixels) = crossbeam_channel::bounded(2);
        let workers = Self {
            requests,
            queued,
            pixels,
            alive: Arc::new(AtomicBool::new(true)),
        };
        for index in 0..2 {
            let receive = workers.queued.clone();
            let send = send.clone();
            let alive = workers.alive.clone();
            let store = Arc::downgrade(store);
            let thread = std::thread::Builder::new()
                .name(format!("preview-image-{index}"))
                .spawn(move || {
                    while let Ok(request) = receive.recv() {
                        if !alive.load(Ordering::Acquire) {
                            break;
                        }
                        let Some(store) = store.upgrade() else { break };
                        let pixels = (request.decoder)(&store, request.file)
                            .as_ref()
                            .map(Pixels::new);
                        drop(store);
                        if !alive.load(Ordering::Acquire)
                            || send.send((request.generation, pixels)).is_err()
                        {
                            break;
                        }
                    }
                })?;
            // The channel/retirement flag owns exit; GUI close must not join
            // a synchronous decoder that is still finishing.
            drop(thread);
        }
        Ok(workers)
    }
    fn clear_queued(&self) {
        while self.queued.try_recv().is_ok() {}
    }
    fn submit(&self, mut request: Request) -> bool {
        loop {
            match self.requests.try_send(request) {
                Ok(()) => return true,
                Err(crossbeam_channel::TrySendError::Full(latest)) => {
                    self.clear_queued();
                    request = latest;
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => return false,
            }
        }
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        self.clear_queued();
        // Dropping both endpoints releases idle receivers and blocked replies.
        // An in-progress synchronous decode exits without publishing afterward.
    }
}
struct State {
    window: slint::Weak<MainWindow>,
    store: Arc<Store>,
    source: Source,
    clock: RefCell<Clock>,
    decoder: RefCell<Decoder>,
    tracker: RefCell<Tracker>,
    requested: Cell<Option<Target>>,
    blocked: Cell<Option<Target>>,
    splitter_hidden: Cell<bool>,
    pending: RefCell<Option<Pending>>,
    workers: RefCell<Option<Workers>>,
    alive: Cell<bool>,
    timer: Timer,
}
impl State {
    fn time(&self) -> i64 {
        (self.clock.borrow())()
    }
    fn track(&self, file: Option<HashId>) {
        self.track_at(file, self.time());
    }
    fn track_at(&self, file: Option<HashId>, now_ms: i64) {
        if let Err(error) = self.tracker.borrow_mut().show(file, now_ms) {
            eprintln!("could not save preview viewing statistics: {error}");
        }
    }
    fn clear(&self, window: &MainWindow) {
        self.pending.borrow_mut().take();
        if let Some(workers) = self.workers.borrow().as_ref() {
            workers.clear_queued();
        }
        self.requested.set(None);
        self.track(None);
        window.set_preview_media(Default::default());
        window.set_preview_has_media(false);
        window.set_preview_loading(false);
    }
    fn eligible(&self, file: HashId) -> bool {
        use hydrus_core::media_viewer::{MediaViewerSettings, ShowAction};
        let Some((mime, resolution)) = crate::viewer::shape(&self.store, file) else {
            return false;
        };
        if resolution.is_some_and(|(width, height)| width == 0 || height == 0) {
            return false;
        }
        if mime.general_class() == Some(hydrus_core::Mime::GeneralImage) && resolution.is_none() {
            return false;
        }
        let shown = crate::viewer::shown(&self.store, file);
        if !shown.local && !shown.trashed {
            return false;
        }
        let settings: MediaViewerSettings = self
            .store
            .read(hydrus_store::settings::get)
            .unwrap_or_default();
        !matches!(
            settings.view(mime).preview_show_action,
            ShowAction::DoNotShow | ShowAction::DoNotShowOnActivationOpenExternally
        )
    }
    fn refresh(&self) {
        if !self.alive.get() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            self.close();
            return;
        };
        let target = (self.source)();
        if !window.window().is_visible() {
            self.clear(&window);
            return;
        }
        let hidden = window.get_preview_splitter_hidden();
        if hidden != self.splitter_hidden.replace(hidden) {
            self.clear(&window);
            // A splitter reveal does not restore its old file in Qt. The next
            // selection change supplies a fresh SetMedia; page show does restore.
            self.blocked.set(target);
        }
        if hidden {
            self.clear(&window);
            self.blocked.set(target);
            return;
        }
        if target != self.blocked.get() {
            self.blocked.set(None);
        }
        let target = target.filter(|target| Some(*target) != self.blocked.get());
        if target != self.requested.get() {
            self.clear(&window);
            self.requested.set(target);
            if let Some((_, file)) = target.filter(|(_, file)| self.eligible(*file)) {
                window.set_preview_loading(true);
                let generation = rand::random();
                *self.pending.borrow_mut() = Some(Pending {
                    file,
                    started_ms: self.time(),
                    generation,
                });
                let decoder = self.decoder.borrow().clone();
                let mut workers = self.workers.borrow_mut();
                if workers.is_none() {
                    match Workers::new(&self.store) {
                        Ok(created) => *workers = Some(created),
                        Err(error) => {
                            self.pending.borrow_mut().take();
                            window.set_preview_loading(false);
                            eprintln!("could not load preview: {error}");
                        }
                    }
                }
                if let Some(workers) = workers.as_ref()
                    && !workers.submit(Request {
                        file,
                        generation,
                        decoder,
                    })
                {
                    self.pending.borrow_mut().take();
                    window.set_preview_loading(false);
                }
            }
        }
        let mut ready = None;
        if let Some(workers) = self.workers.borrow().as_ref() {
            loop {
                match workers.pixels.try_recv() {
                    Ok((generation, pixels)) => {
                        if let Some(pending) = self.pending.borrow().as_ref()
                            && generation == pending.generation
                        {
                            ready = Some((pending.file, pending.started_ms, pixels));
                        }
                    }
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        if ready.is_none()
                            && let Some(pending) = self.pending.borrow().as_ref()
                        {
                            ready = Some((pending.file, pending.started_ms, None));
                        }
                        break;
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                }
            }
        }
        if let Some((file, started_ms, pixels)) = ready {
            self.pending.borrow_mut().take();
            window.set_preview_loading(false);
            if let Some(pixels) = pixels {
                window.set_preview_media(pixels.image());
                window.set_preview_has_media(true);
                // Successful presentation accepts the requested interval's
                // original SetMedia timestamp, as Qt does before decoding.
                self.track_at(Some(file), started_ms);
            }
        }
    }
    fn close(&self) {
        if !self.alive.replace(false) {
            return;
        }
        self.timer.stop();
        self.pending.borrow_mut().take();
        self.workers.borrow_mut().take();
        if let Err(error) = self.tracker.borrow_mut().close(self.time()) {
            eprintln!("could not save preview viewing statistics: {error}");
        }
        if let Some(window) = self.window.upgrade() {
            window.set_preview_media(Default::default());
            window.set_preview_has_media(false);
            window.set_preview_loading(false);
        }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.close();
    }
}

/// The page preview's lifetime; a retained callback cannot restart a closed owner.
#[derive(Clone)]
pub struct Monitor(Rc<State>);
impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewMonitor")
            .field("active", &self.0.alive.get())
            .field("requested", &self.0.requested.get())
            .finish_non_exhaustive()
    }
}
impl Monitor {
    pub(crate) fn bind(window: &MainWindow, store: Arc<Store>, source: Source) -> Self {
        // The window owns this retirement callback. Rebinding retires its prior
        // preview before any successor callback/pixels can be published.
        window.invoke_preview_retired();
        let state = Rc::new(State {
            window: window.as_weak(),
            tracker: RefCell::new(Tracker::new(store.clone(), CanvasType::Preview)),
            store,
            source,
            clock: RefCell::new(Rc::new(|| TimestampMs::now().0)),
            decoder: RefCell::new(Arc::new(crate::viewer::still)),
            requested: Cell::new(None),
            blocked: Cell::new(None),
            splitter_hidden: Cell::new(false),
            pending: RefCell::new(None),
            workers: RefCell::new(None),
            alive: Cell::new(true),
            timer: Timer::default(),
        });
        window.on_preview_retired({
            let state = Rc::downgrade(&state);
            move || {
                if let Some(state) = state.upgrade() {
                    state.close();
                }
            }
        });
        window.on_preview_presentation_changed({
            let state = Rc::downgrade(&state);
            move || {
                if let Some(state) = state.upgrade() {
                    state.refresh();
                }
            }
        });
        state
            .timer
            .start(TimerMode::Repeated, Duration::from_millis(25), {
                let state = Rc::downgrade(&state);
                move || {
                    if let Some(state) = state.upgrade() {
                        state.refresh();
                    }
                }
            });
        Self(state)
    }
    /// Refresh this visible canvas after a real page/selection/presentation change.
    pub fn refresh(&self) {
        self.0.refresh();
    }
    /// Supply the owned display clock before showing media (also used for replay).
    pub fn set_clock(&self, clock: Clock) {
        if self.0.alive.get() {
            *self.0.clock.borrow_mut() = clock;
        }
    }
    /// Supply a display decoder before requesting media; late replies stay request-owned.
    pub fn set_decoder(&self, decoder: Decoder) {
        if self.0.alive.get() {
            *self.0.decoder.borrow_mut() = decoder;
        }
    }
    /// Finish the displayed interval once after an accepted client close.
    pub fn close(&self) {
        self.0.close();
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;

    #[test]
    fn idle_preview_workers_do_not_retain_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let weak = Arc::downgrade(&store);
        let workers = Workers::new(&store).unwrap();
        drop(store);
        assert!(
            weak.upgrade().is_none(),
            "idle workers own only a weak store"
        );
        drop(workers);
    }

    #[test]
    fn retiring_pool_drops_queued_decoder_and_releases_running_store_after_decode() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let weak_store = Arc::downgrade(&store);
        let workers = Workers::new(&store).unwrap();
        let (entered, entries) = crossbeam_channel::bounded(2);
        let (release, released) = crossbeam_channel::bounded(2);
        let held: Decoder = Arc::new(move |_, _| {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
            None
        });
        for generation in 1..=2 {
            assert!(workers.submit(Request {
                file: HashId(1),
                generation,
                decoder: held.clone(),
            }));
            entries.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let queued_resource = Arc::new(AtomicBool::new(false));
        let weak_resource = Arc::downgrade(&queued_resource);
        let queued_calls = Arc::new(AtomicBool::new(false));
        assert!(workers.submit(Request {
            file: HashId(2),
            generation: 3,
            decoder: Arc::new({
                let called = queued_calls.clone();
                move |_, _| {
                    queued_resource.store(true, Ordering::Release);
                    called.store(true, Ordering::Release);
                    None
                }
            }),
        }));
        drop(store);
        assert!(
            weak_store.upgrade().is_some(),
            "two held decodes own the store"
        );
        drop(workers);
        assert!(
            weak_resource.upgrade().is_none(),
            "queued decoder is dropped on retirement"
        );
        release.send(()).unwrap();
        release.send(()).unwrap();
        let started = std::time::Instant::now();
        while weak_store.upgrade().is_some() {
            assert!(started.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            !queued_calls.load(Ordering::Acquire),
            "retired queued work never executes"
        );
    }
}
