//! Owned one-at-a-time cache warming. Presentation and its intervals stay separate.
use crate::image_cache::{Handle, Loading};
use hydrus_core::{
    HashId, Mime,
    media_viewer::{MediaViewerSettings, ShowAction},
};
use hydrus_store::{Store, image_cache::Policy, viewer_prefetch::Preferences};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub id: HashId,
    pub mime: Mime,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub action: ShowAction,
}
impl Candidate {
    pub fn identity(&self) -> (Mime, Option<u32>, Option<u32>) {
        (self.mime, self.width, self.height)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plan {
    pub candidates: Vec<Candidate>,
    pub preferences: Preferences,
    pub policy: Policy,
    pub normalise_icc: bool,
}
impl Plan {
    fn capture(store: &Store, files: &[HashId], preferences: Preferences) -> Option<Self> {
        let (policy, colour, settings) = store
            .read(|conn| {
                Ok((
                    hydrus_store::image_cache::load(conn)?,
                    hydrus_store::image_colour::load(conn)?,
                    hydrus_store::settings::get::<MediaViewerSettings>(conn)?,
                ))
            })
            .ok()?;
        let results = store
            .read(|conn| hydrus_store::media::load_basic(conn, files))
            .ok()?;
        let candidates = files
            .iter()
            .filter_map(|id| {
                let info = results
                    .iter()
                    .find(|result| result.hash_id == *id)?
                    .info
                    .as_ref()?;
                // Static-image capabilities admit the native raster action;
                // unsupported MPV/Qt-player choices fall back to external in Qt.
                if info.mime.general_class() != Some(Mime::GeneralImage)
                    || settings.view(info.mime).media_show_action != ShowAction::Native
                {
                    return None;
                }
                Some(Candidate {
                    id: *id,
                    mime: info.mime,
                    width: info.width,
                    height: info.height,
                    action: settings.view(info.mime).media_show_action,
                })
            })
            .collect();
        Some(Self {
            candidates,
            preferences,
            policy,
            normalise_icc: colour.normalise_icc,
        })
    }
    fn still_saved(&self, store: &Store) -> bool {
        store
            .read(|conn| {
                let settings = hydrus_store::settings::get::<MediaViewerSettings>(conn)?;
                Ok(self.candidates.iter().all(|candidate| {
                    settings.view(candidate.mime).media_show_action == candidate.action
                }) && hydrus_store::viewer_prefetch::load(conn)? == self.preferences
                    && hydrus_store::image_cache::load(conn)? == self.policy
                    && hydrus_store::image_colour::load(conn)?.normalise_icc == self.normalise_icc)
            })
            .unwrap_or(false)
    }
}
/// A captured pending renderer; waiting occurs exclusively on the warm worker.
pub(crate) struct Pending(Arc<Loading>);
impl std::fmt::Debug for Pending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Pending").field(&self.0).finish()
    }
}
impl Pending {
    pub fn new(loading: Arc<Loading>) -> Self {
        Self(loading)
    }
    fn wait(&self, valid: impl Fn() -> bool) {
        self.0.wait_while_active(valid);
    }
}
#[derive(Debug)]
pub(crate) enum Outcome {
    Decoded,
    Wait(Pending),
    Stop,
}
struct Queue {
    pending: Option<(u64, Plan)>,
}
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
    active: AtomicBool,
    generation: AtomicU64,
}
impl Shared {
    fn valid(&self, generation: u64) -> bool {
        self.active.load(Ordering::Acquire) && self.generation.load(Ordering::Acquire) == generation
    }
    fn retire(&self) {
        self.active.store(false, Ordering::Release);
        drop(
            self.queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pending
                .take(),
        );
        self.wake.notify_all();
    }
}
type Pass = Arc<dyn Fn(&Handle, &Store, &Plan, &dyn Fn() -> bool) -> Outcome + Send + Sync>;
struct Worker {
    shared: Arc<Shared>,
}
impl Worker {
    fn new(store: Weak<Store>, cache: Handle) -> std::io::Result<Self> {
        Self::with_pass(
            store,
            cache,
            Arc::new(|cache, store, plan, valid| cache.prefetch_once(store, plan, valid)),
        )
    }
    fn with_pass(store: Weak<Store>, cache: Handle, pass: Pass) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue { pending: None }),
            wake: Condvar::new(),
            active: AtomicBool::new(true),
            generation: AtomicU64::new(0),
        });
        let owned = shared.clone();
        let thread = std::thread::Builder::new()
            .name("viewer prefetch".into())
            .spawn(move || {
                loop {
                    let request = {
                        let mut queue = owned
                            .queue
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        while queue.pending.is_none() && owned.active.load(Ordering::Acquire) {
                            queue = owned
                                .wake
                                .wait(queue)
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                        }
                        if !owned.active.load(Ordering::Acquire) {
                            break;
                        }
                        queue.pending.take()
                    };
                    let Some((generation, plan)) = request else {
                        continue;
                    };
                    while owned.valid(generation) && cache.active() {
                        let Some(store) = store.upgrade() else {
                            owned.retire();
                            break;
                        };
                        if !plan.still_saved(&store) {
                            break;
                        }
                        let result = pass(&cache, &store, &plan, &|| owned.valid(generation));
                        // No Store ownership while awaiting another owner's held renderer.
                        drop(store);
                        match result {
                            Outcome::Decoded => {}
                            Outcome::Wait(pending) => {
                                pending.wait(|| owned.valid(generation) && cache.active());
                            }
                            Outcome::Stop => break,
                        }
                    }
                }
            })?;
        // Closing never joins a held disk decoder on the UI thread. The owned
        // active flag/closed transport ends idle work, and cache retirement is terminal.
        drop(thread);
        Ok(Self { shared })
    }
    fn request(&self, plan: Plan) {
        self.queue(plan, true);
    }
    fn resume(&self, plan: Plan) {
        self.queue(plan, false);
    }
    fn queue(&self, plan: Plan, fresh: bool) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.shared.active.load(Ordering::Acquire) {
            return;
        }
        let generation = if fresh {
            self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1
        } else {
            self.shared.generation.load(Ordering::Acquire)
        };
        queue.pending = Some((generation, plan));
        self.shared.wake.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.retire();
    }
}
struct State {
    store: Weak<Store>,
    cache: Handle,
    valid: Rc<dyn Fn() -> bool>,
    files: Rc<dyn Fn(Preferences) -> Vec<HashId>>,
    last: RefCell<Option<Plan>>,
    readiness: std::cell::Cell<u64>,
    worker: RefCell<Option<Worker>>,
    timer: slint::Timer,
    active: std::cell::Cell<bool>,
}
impl State {
    fn retire(&self) {
        self.active.set(false);
        self.timer.stop();
        drop(self.last.borrow_mut().take());
        if let Some(worker) = self.worker.borrow_mut().take() {
            worker.shared.retire();
        }
    }
    fn refresh(&self) {
        if !self.active.get() {
            return;
        }
        if !(self.valid)() || !self.cache.active() {
            self.retire();
            return;
        }
        let Some(store) = self.store.upgrade() else {
            self.retire();
            return;
        };
        let Ok(preferences) = store.read(hydrus_store::viewer_prefetch::load) else {
            return;
        };
        let Some(plan) = Plan::capture(&store, &(self.files)(preferences), preferences) else {
            return;
        };
        let readiness = self.cache.readiness();
        if self.last.borrow().as_ref() == Some(&plan) {
            if self.readiness.replace(readiness) != readiness
                && let Some(worker) = self.worker.borrow().as_ref()
            {
                worker.resume(plan);
            }
            return;
        }
        self.readiness.set(readiness);
        if self.worker.borrow().is_none() && !plan.candidates.is_empty() {
            match Worker::new(self.store.clone(), self.cache.clone()) {
                Ok(worker) => *self.worker.borrow_mut() = Some(worker),
                Err(error) => {
                    tracing::warn!(%error,"starting viewer prefetch worker");
                    return;
                }
            }
        }
        if let Some(worker) = self.worker.borrow().as_ref() {
            worker.request(plan.clone());
        }
        *self.last.borrow_mut() = Some(plan);
    }
}
impl Drop for State {
    fn drop(&mut self) {
        self.retire();
    }
}
/// Handles do not own a GUI component or Store; exact owner liveness is supplied.
#[derive(Clone)]
pub(crate) struct Control(Rc<State>);
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerPrefetch")
            .field("active", &self.0.active.get())
            .finish_non_exhaustive()
    }
}
impl Control {
    pub(crate) fn new(
        store: &Arc<Store>,
        cache: Handle,
        valid: Rc<dyn Fn() -> bool>,
        files: Rc<dyn Fn(Preferences) -> Vec<HashId>>,
    ) -> Self {
        let state = Rc::new(State {
            store: Arc::downgrade(store),
            cache,
            valid,
            files,
            last: RefCell::default(),
            readiness: std::cell::Cell::new(0),
            worker: RefCell::default(),
            timer: slint::Timer::default(),
            active: std::cell::Cell::new(true),
        });
        state
            .timer
            .start(slint::TimerMode::Repeated, Duration::from_millis(100), {
                let state = Rc::downgrade(&state);
                move || {
                    if let Some(state) = state.upgrade() {
                        state.refresh();
                    }
                }
            });
        Self(state)
    }
    pub(crate) fn refresh(&self) {
        self.0.refresh();
    }
    pub(crate) fn retire(&self) {
        self.0.retire();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;
    fn store_plan() -> (tempfile::TempDir, Arc<Store>, Handle, Plan) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let cache = Handle::standalone(&store);
        let plan = Plan {
            candidates: Vec::new(),
            preferences: store.read(hydrus_store::viewer_prefetch::load).unwrap(),
            policy: store.read(hydrus_store::image_cache::load).unwrap(),
            normalise_icc: store
                .read(hydrus_store::image_colour::load)
                .unwrap()
                .normalise_icc,
        };
        (dir, store, cache, plan)
    }
    fn target(mut plan: Plan, id: u32) -> Plan {
        plan.candidates.push(Candidate {
            id: HashId(id),
            mime: Mime::ImagePng,
            width: Some(10),
            height: Some(1),
            action: ShowAction::Native,
        });
        plan
    }
    #[test]
    fn held_worker_coalesces_latest_request_and_retirement_discards_queued_work() {
        let (_dir, store, cache, plan) = store_plan();
        let (started, seen) = crossbeam_channel::unbounded();
        let (release, held) = crossbeam_channel::bounded(1);
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let worker = Worker::with_pass(
            Arc::downgrade(&store),
            cache,
            Arc::new({
                let active = active.clone();
                let maximum = maximum.clone();
                move |_, _, plan, valid| {
                    let count = active.fetch_add(1, Ordering::AcqRel) + 1;
                    maximum.fetch_max(count, Ordering::AcqRel);
                    started.send(plan.candidates[0].id).unwrap();
                    held.recv().unwrap();
                    let _still_current = valid();
                    active.fetch_sub(1, Ordering::AcqRel);
                    Outcome::Stop
                }
            }),
        )
        .unwrap();
        worker.request(target(plan.clone(), 1));
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5)).unwrap(),
            HashId(1)
        );
        worker.request(target(plan.clone(), 2));
        worker.request(target(plan.clone(), 3));
        release.send(()).unwrap();
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5)).unwrap(),
            HashId(3),
            "the displaced pending target never gets a decoder"
        );
        worker.request(target(plan, 4));
        drop(worker);
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while active.load(Ordering::Acquire) != 0 {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(maximum.load(Ordering::Acquire), 1);
        assert!(
            seen.recv_timeout(Duration::from_secs(5)).is_err(),
            "retirement releases the last sender rather than running queued target4"
        );
    }
    #[test]
    fn finished_notifications_resume_the_same_captured_navigation_generation() {
        let (_dir, store, cache, plan) = store_plan();
        let (started, seen) = crossbeam_channel::unbounded();
        let worker = Worker::with_pass(
            Arc::downgrade(&store),
            cache,
            Arc::new(move |_, _, plan, _| {
                started.send(plan.candidates[0].id).unwrap();
                Outcome::Stop
            }),
        )
        .unwrap();
        let plan = target(plan, 1);
        worker.request(plan.clone());
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5)).unwrap(),
            HashId(1)
        );
        let generation = worker.shared.generation.load(Ordering::Acquire);
        worker.resume(plan);
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5)).unwrap(),
            HashId(1)
        );
        assert_eq!(worker.shared.generation.load(Ordering::Acquire), generation);
        drop(worker);
        assert!(seen.recv_timeout(Duration::from_secs(5)).is_err());
    }
    #[test]
    fn waiting_on_another_pending_renderer_holds_no_store_and_ends_on_owner_drop() {
        let (_dir, store, cache, plan) = store_plan();
        let weak = Arc::downgrade(&store);
        let pending = Arc::new(Loading::default());
        let (started, seen) = crossbeam_channel::bounded(1);
        let pass: Pass = Arc::new(move |_, _, _, _| {
            started.send(()).unwrap();
            Outcome::Wait(Pending::new(pending.clone()))
        });
        let worker = Worker::with_pass(weak.clone(), cache, pass).unwrap();
        worker.request(plan);
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(store);
        let deadline = Instant::now() + Duration::from_secs(5);
        while weak.strong_count() != 0 {
            assert!(Instant::now() < deadline, "pending warm work retains Store");
            std::thread::yield_now();
        }
        drop(worker);
        assert!(
            seen.recv_timeout(Duration::from_secs(5)).is_err(),
            "no readiness successor survives retirement"
        );
    }
    #[test]
    fn real_uncached_equality_decode_repeats_on_owned_readiness_and_retire_stops_it() {
        use hydrus_import::{FileImportOptions, FileImporter};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let imported = FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
            .import_path(
                &hydrus_testkit::fixture_path("image_cache/a.png"),
                &FileImportOptions::default(),
            )
            .unwrap();
        let id = store
            .read(|conn| hydrus_store::master::hash_id(conn, &imported.hash.unwrap()))
            .unwrap()
            .unwrap();
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &Policy {
                        bytes: 100,
                        timeout: 300,
                        percentage: 30,
                    },
                )?;
                hydrus_store::settings::set(
                    ctx.conn(),
                    &Preferences {
                        previous: 0,
                        next: 0,
                        percentage: 50,
                        ..Preferences::default()
                    },
                )
            })
            .unwrap();
        let preferences = store.read(hydrus_store::viewer_prefetch::load).unwrap();
        let plan = Plan::capture(&store, &[id], preferences).unwrap();
        let cache = Handle::standalone(&store);
        let (finished, seen) = crossbeam_channel::unbounded();
        let (release, held) = crossbeam_channel::bounded(1);
        let count = Arc::new(AtomicUsize::new(0));
        let worker = Worker::with_pass(
            Arc::downgrade(&store),
            cache.clone(),
            Arc::new(move |cache, store, plan, valid| {
                let outcome = cache.prefetch_once(store, plan, valid);
                assert!(matches!(outcome, Outcome::Decoded));
                let count = count.fetch_add(1, Ordering::AcqRel) + 1;
                finished.send(count).unwrap();
                if count == 2 {
                    held.recv().unwrap();
                }
                outcome
            }),
        )
        .unwrap();
        worker.request(plan);
        assert_eq!(seen.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        assert_eq!(seen.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
        assert!(
            !cache.contains(id),
            "strict cache admission still excludes equality while warm readiness repeats"
        );
        cache.retire();
        drop(worker);
        release.send(()).unwrap();
        assert!(
            seen.recv_timeout(Duration::from_secs(5)).is_err(),
            "no third admission after terminal owner retirement"
        );
    }
    #[test]
    fn actual_static_image_display_capabilities_filter_each_recorded_candidate_action() {
        use hydrus_import::{FileImportOptions, FileImporter};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let imported = FileImporter::new(store.clone(), hydrus_media::MediaTools::new())
            .import_path(
                &hydrus_testkit::fixture_path("image_cache/a.png"),
                &FileImportOptions::default(),
            )
            .unwrap();
        let id = store
            .read(|conn| hydrus_store::master::hash_id(conn, &imported.hash.unwrap()))
            .unwrap()
            .unwrap();
        let preferences = store.read(hydrus_store::viewer_prefetch::load).unwrap();
        let fixture = hydrus_testkit::fixture_json("viewer_prefetch.json");
        for case in fixture["candidates"].as_array().unwrap() {
            let action = ShowAction::from_code(case["action"].as_i64().unwrap()).unwrap();
            store
                .write(move |ctx| {
                    let mut saved = hydrus_store::settings::get::<MediaViewerSettings>(ctx.conn())?;
                    let mut view = saved.view(Mime::ImagePng);
                    view.media_show_action = action;
                    saved.media_view.insert(Mime::ImagePng.code(), view);
                    hydrus_store::settings::set(ctx.conn(), &saved)
                })
                .unwrap();
            let plan = Plan::capture(&store, &[id], preferences).unwrap();
            assert_eq!(
                !plan.candidates.is_empty(),
                case["expected"].as_bool().unwrap(),
                "{case}"
            );
        }
    }
    #[test]
    fn a_saved_candidate_display_action_change_invalidates_the_captured_pass() {
        let (_dir, store, _cache, plan) = store_plan();
        let plan = target(plan, 1);
        assert!(plan.still_saved(&store));
        store
            .write(|ctx| {
                let mut saved = hydrus_store::settings::get::<MediaViewerSettings>(ctx.conn())?;
                let mut view = saved.view(Mime::ImagePng);
                view.media_show_action = ShowAction::DoNotShow;
                saved.media_view.insert(Mime::ImagePng.code(), view);
                hydrus_store::settings::set(ctx.conn(), &saved)
            })
            .unwrap();
        assert!(!plan.still_saved(&store));
    }
}
