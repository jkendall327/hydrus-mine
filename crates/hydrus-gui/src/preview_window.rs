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
    collections::HashMap,
    rc::{Rc, Weak},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

type Target = (PageKey, HashId);
/// A live page incarnation and its requested focus. Weak ownership prevents
/// preview snapshots from keeping closed/forgotten pages alive.
#[derive(Debug)]
pub(crate) struct SourcePage {
    pub key: PageKey,
    pub owner: Weak<RefCell<crate::SearchPage>>,
    pub file: Option<HashId>,
}
type Source = Rc<dyn Fn() -> SourcePage>;
type OwnerValid = Rc<dyn Fn(PageKey, &Weak<RefCell<crate::SearchPage>>) -> bool>;
// Preview snapshots are a rendering cache, not page/session data. Retain at
// most 64MiB across hidden pages; the currently shown frame may exceed this
// soft bound. Eviction keeps accepted identity and interval ownership intact.
const SNAPSHOT_BYTES: u64 = 64 * 1024 * 1024;
/// The preview clock, retained by its canvas owner rather than a global clock.
pub type Clock = Rc<dyn Fn() -> i64>;
/// Decode backend; the owner can supply a bounded media worker for deterministic replay.
pub type Decoder = Arc<dyn Fn(&Store, HashId) -> Option<hydrus_media::Raster> + Send + Sync>;
struct Pending {
    file: HashId,
    started_ms: i64,
    generation: u128,
    restore: bool,
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
struct Frame {
    image: slint::Image,
    bytes: u64,
    touched: u64,
}
struct Canvas {
    owner: Weak<RefCell<crate::SearchPage>>,
    tracker: RefCell<Tracker>,
    requested: Cell<Option<Target>>,
    accepted: Cell<Option<HashId>>,
    observed: Cell<Option<HashId>>,
    suspended: Cell<bool>,
    retry: Cell<bool>,
    failed: Cell<bool>,
    requested_started_ms: Cell<Option<i64>>,
    frame: RefCell<Option<Frame>>,
    blocked: Cell<Option<Target>>,
    splitter_hidden: Cell<bool>,
    pending: RefCell<Option<Pending>>,
}
impl Canvas {
    fn track(&self, file: Option<HashId>, now_ms: i64) {
        if let Err(error) = self.tracker.borrow_mut().show(file, now_ms) {
            eprintln!("could not save preview viewing statistics: {error}");
        }
    }
    fn clear(&self, now_ms: i64) {
        self.pending.borrow_mut().take();
        self.requested.set(None);
        self.accepted.set(None);
        self.suspended.set(false);
        self.retry.set(false);
        self.failed.set(false);
        self.requested_started_ms.set(None);
        self.frame.borrow_mut().take();
        self.track(None, now_ms);
    }
    fn close(&self, now_ms: i64) {
        self.pending.borrow_mut().take();
        self.frame.borrow_mut().take();
        if let Err(error) = self.tracker.borrow_mut().close(now_ms) {
            eprintln!("could not save preview viewing statistics: {error}");
        }
    }
}
struct State {
    window: slint::Weak<MainWindow>,
    store: Arc<Store>,
    source: Source,
    owner_valid: OwnerValid,
    clock: RefCell<Clock>,
    decoder: RefCell<Decoder>,
    canvases: RefCell<HashMap<PageKey, Rc<Canvas>>>,
    current: RefCell<Option<Rc<Canvas>>>,
    touch: Cell<u64>,
    workers: RefCell<Option<Workers>>,
    alive: Cell<bool>,
    timer: Timer,
}
impl State {
    fn time(&self) -> i64 {
        (self.clock.borrow())()
    }
    fn blank(window: &MainWindow) {
        window.set_preview_media(slint::Image::default());
        window.set_preview_has_media(false);
        window.set_preview_loading(false);
    }
    fn trim_frames(&self) {
        let current = self.current.borrow().clone();
        loop {
            let canvases = self.canvases.borrow();
            let bytes: u64 = canvases
                .values()
                .filter_map(|canvas| canvas.frame.borrow().as_ref().map(|frame| frame.bytes))
                .sum();
            if bytes <= SNAPSHOT_BYTES {
                break;
            }
            let oldest = canvases
                .values()
                .filter(|canvas| {
                    !current
                        .as_ref()
                        .is_some_and(|shown| Rc::ptr_eq(shown, canvas))
                })
                .filter_map(|canvas| {
                    canvas
                        .frame
                        .borrow()
                        .as_ref()
                        .map(|frame| (frame.touched, canvas.clone()))
                })
                .min_by_key(|(touched, _)| *touched)
                .map(|(_, canvas)| canvas);
            drop(canvases);
            let Some(oldest) = oldest else { break };
            oldest.frame.borrow_mut().take();
        }
    }
    fn present(&self, canvas: &Canvas, window: &MainWindow) {
        if let Some(frame) = canvas.frame.borrow_mut().as_mut() {
            let touched = self.touch.get().wrapping_add(1);
            self.touch.set(touched);
            frame.touched = touched;
            window.set_preview_media(frame.image.clone());
            window.set_preview_has_media(true);
        } else {
            window.set_preview_media(slint::Image::default());
            window.set_preview_has_media(false);
        }
        window.set_preview_loading(canvas.pending.borrow().is_some());
        self.trim_frames();
    }
    fn submit(&self, canvas: &Canvas, file: HashId, restore: bool) {
        let generation = rand::random();
        canvas.retry.set(false);
        canvas.failed.set(false);
        *canvas.pending.borrow_mut() = Some(Pending {
            file,
            started_ms: if restore {
                self.time()
            } else {
                canvas
                    .requested_started_ms
                    .get()
                    .unwrap_or_else(|| self.time())
            },
            generation,
            restore,
        });
        let decoder = self.decoder.borrow().clone();
        let mut workers = self.workers.borrow_mut();
        if workers.is_none() {
            match Workers::new(&self.store) {
                Ok(created) => *workers = Some(created),
                Err(error) => {
                    canvas.pending.borrow_mut().take();
                    canvas.failed.set(true);
                    eprintln!("could not load preview: {error}");
                    return;
                }
            }
        }
        if let Some(workers) = workers.as_ref() {
            // Retire a superseded queued request's logical loading state too.
            // The two running decodes keep their request generations.
            while let Ok(request) = workers.queued.try_recv() {
                for previous in self.canvases.borrow().values() {
                    if previous
                        .pending
                        .borrow()
                        .as_ref()
                        .is_some_and(|pending| pending.generation == request.generation)
                    {
                        previous.pending.borrow_mut().take();
                        previous.retry.set(true);
                    }
                }
            }
            if !workers.submit(Request {
                file,
                generation,
                decoder,
            }) {
                canvas.pending.borrow_mut().take();
                canvas.failed.set(true);
            }
        }
    }
    fn collect(&self) {
        let mut replies = Vec::new();
        let mut disconnected = false;
        if let Some(workers) = self.workers.borrow().as_ref() {
            loop {
                match workers.pixels.try_recv() {
                    Ok(reply) => replies.push(reply),
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                }
            }
        }
        for (generation, pixels) in replies {
            let canvas = self
                .canvases
                .borrow()
                .values()
                .find(|canvas| {
                    canvas
                        .pending
                        .borrow()
                        .as_ref()
                        .is_some_and(|pending| pending.generation == generation)
                })
                .cloned();
            let Some(canvas) = canvas else { continue };
            let Some(pending) = canvas.pending.borrow_mut().take() else {
                continue;
            };
            if canvas.owner.upgrade().is_none() {
                continue;
            }
            if let Some(pixels) = pixels {
                let bytes = pixels.byte_len();
                let image = pixels.image();
                *canvas.frame.borrow_mut() = Some(Frame {
                    image,
                    bytes,
                    touched: self.touch.get(),
                });
                if !pending.restore {
                    canvas.accepted.set(Some(pending.file));
                    canvas.track(Some(pending.file), pending.started_ms);
                }
            } else {
                canvas.failed.set(true);
            }
        }
        if disconnected {
            for canvas in self.canvases.borrow().values() {
                if canvas.pending.borrow_mut().take().is_some() {
                    canvas.failed.set(true);
                }
            }
        }
        self.trim_frames();
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
        let source = (self.source)();
        let hide = self
            .store
            .read(hydrus_store::page_layout::load)
            .unwrap_or_default()
            .hide_preview;
        let now = self.time();
        self.canvases.borrow_mut().retain(|key, canvas| {
            if canvas.owner.upgrade().is_none() || !(self.owner_valid)(*key, &canvas.owner) {
                canvas.close(now);
                false
            } else {
                true
            }
        });
        let canvas = {
            let mut canvases = self.canvases.borrow_mut();
            if canvases
                .get(&source.key)
                .is_some_and(|canvas| !Weak::ptr_eq(&canvas.owner, &source.owner))
                && let Some(retired) = canvases.remove(&source.key)
            {
                retired.close(now);
            }
            canvases
                .entry(source.key)
                .or_insert_with(|| {
                    Rc::new(Canvas {
                        owner: source.owner,
                        tracker: RefCell::new(Tracker::new(
                            self.store.clone(),
                            CanvasType::Preview,
                        )),
                        requested: Cell::new(None),
                        accepted: Cell::new(None),
                        observed: Cell::new(None),
                        suspended: Cell::new(false),
                        retry: Cell::new(false),
                        failed: Cell::new(false),
                        requested_started_ms: Cell::new(None),
                        frame: RefCell::new(None),
                        blocked: Cell::new(None),
                        splitter_hidden: Cell::new(window.get_preview_splitter_hidden()),
                        pending: RefCell::new(None),
                    })
                })
                .clone()
        };
        let changed = !self
            .current
            .borrow()
            .as_ref()
            .is_some_and(|shown| Rc::ptr_eq(shown, &canvas));
        if changed {
            if let Some(previous) = self.current.borrow_mut().replace(canvas.clone())
                && !hide
                && !previous.splitter_hidden.get()
            {
                if previous.pending.borrow_mut().take().is_some() {
                    previous.retry.set(true);
                }
                previous.track(None, now);
                previous.suspended.set(true);
                // Keep accepted identity/frame as PageHidden's restoration snapshot.
            }
            Self::blank(&window);
            if canvas.suspended.replace(false) {
                if hide || window.get_preview_splitter_hidden() {
                    // PageShown consumes the hidden restoration identity even
                    // when SetMedia rejects it; an empty current canvas stays empty.
                    canvas.clear(now);
                } else {
                    canvas.requested_started_ms.set(Some(now));
                    canvas.failed.set(false);
                    if let Some(file) = canvas.accepted.get() {
                        canvas.track(Some(file), now);
                    }
                }
            }
        }
        if !window.window().is_visible() && hide {
            // The global flag rejects clears even while the whole window is
            // hidden. Keep each accepted canvas/interval owned until close.
            self.collect();
            self.present(&canvas, &window);
            return;
        }
        if !window.window().is_visible() {
            // Window suspension ends current presentation; accepted close below
            // terminates every per-page tracker, including globally hidden pages.
            canvas.clear(now);
            canvas.observed.set(None);
            Self::blank(&window);
            return;
        }
        let hidden = window.get_preview_splitter_hidden();
        let focus_changed = canvas.observed.replace(source.file) != source.file;
        // Changing the preference alone does not resend an already rejected
        // SetMedia. The real focus source must change (or PageShown restore).
        let target = if hide || hidden || !focus_changed {
            canvas.requested.get()
        } else {
            source.file.map(|file| (source.key, file))
        };
        if hidden != canvas.splitter_hidden.replace(hidden) && !hide && hidden {
            canvas.clear(now);
            canvas.blocked.set(target);
        }
        if hidden && !hide {
            // An already collapsed splitter rejects SetMedia(None) too. Turning
            // global hide off cannot replay the earlier refused clear.
            self.collect();
            self.present(&canvas, &window);
            return;
        }
        if target != canvas.blocked.get() {
            canvas.blocked.set(None);
        }
        let target = target.filter(|target| Some(*target) != canvas.blocked.get());
        if target != canvas.requested.get() {
            canvas.clear(now);
            canvas.requested.set(target);
            canvas.requested_started_ms.set(Some(now));
            if let Some((_, file)) = target.filter(|(_, file)| self.eligible(*file)) {
                self.submit(&canvas, file, false);
            }
        } else if canvas.retry.get() && canvas.pending.borrow().is_none() {
            if let Some((_, file)) = canvas.requested.get() {
                self.submit(&canvas, file, false);
            }
        } else if canvas.accepted.get().is_some()
            && !canvas.failed.get()
            && canvas.frame.borrow().is_none()
            && canvas.pending.borrow().is_none()
        {
            // An evicted rendering snapshot is restored from its accepted file;
            // this is not a new SetMedia acceptance or viewing interval.
            if let Some(file) = canvas.accepted.get() {
                self.submit(&canvas, file, true);
            }
        }
        self.collect();
        self.present(&canvas, &window);
    }
    fn close(&self) {
        if !self.alive.replace(false) {
            return;
        }
        self.timer.stop();
        self.workers.borrow_mut().take();
        for canvas in self.canvases.borrow_mut().drain().map(|(_, canvas)| canvas) {
            canvas.close(self.time());
        }
        self.current.borrow_mut().take();
        if let Some(window) = self.window.upgrade() {
            Self::blank(&window);
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
            .field("pages", &self.0.canvases.borrow().len())
            .finish_non_exhaustive()
    }
}
impl Monitor {
    pub(crate) fn bind(
        window: &MainWindow,
        store: Arc<Store>,
        source: Source,
        owner_valid: OwnerValid,
    ) -> Self {
        // The window owns this retirement callback. Rebinding retires its prior
        // preview before any successor callback/pixels can be published.
        window.invoke_preview_retired();
        let state = Rc::new(State {
            window: window.as_weak(),
            store,
            source,
            owner_valid,
            clock: RefCell::new(Rc::new(|| TimestampMs::now().0)),
            decoder: RefCell::new(Arc::new(crate::viewer::still)),
            canvases: RefCell::new(HashMap::new()),
            current: RefCell::new(None),
            touch: Cell::new(0),
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
    /// Accepted media belongs to the shown live page, independently of focus.
    pub fn displayed_file(&self) -> Option<HashId> {
        self.0.current.borrow().as_ref().and_then(|canvas| {
            if canvas.suspended.get() {
                None
            } else {
                canvas.accepted.get()
            }
        })
    }
    /// Bytes retained by this owner's bounded preview rendering snapshots.
    pub fn resident_snapshot_bytes(&self) -> u64 {
        self.0
            .canvases
            .borrow()
            .values()
            .filter_map(|canvas| canvas.frame.borrow().as_ref().map(|frame| frame.bytes))
            .sum()
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
