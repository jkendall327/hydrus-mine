//! One binding's decoded full-resolution images. A private Owner retires admission
//! even when callbacks/workers retain handles; current presentation owns its Arc
//! independently. Cache locks never cover disk I/O, decode, or GUI callbacks.
use crate::MainWindow;
use hydrus_core::HashId;
use hydrus_gui_model::image_cache::Cache;
use hydrus_media::Raster;
use hydrus_store::{Store, image_cache::Policy};
use slint::ComponentHandle as _;
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Condvar, Mutex, Weak},
    time::{Duration, Instant},
};

type Identity = (hydrus_core::Mime, Option<u32>, Option<u32>);
fn estimate(info: &hydrus_store::media::FileInfo) -> u64 {
    estimate_resolution(info.width, info.height)
}
fn estimate_resolution(width: Option<u32>, height: Option<u32>) -> u64 {
    let (width, height) = match (width, height) {
        (Some(width), Some(height)) => (width, height),
        _ => (100, 100),
    };
    u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(3)
}
#[derive(Default)]
struct Loading {
    identity: Option<Identity>,
    result: Mutex<Option<Option<Arc<Raster>>>>,
    ready: Condvar,
}
impl Loading {
    fn bytes(&self) -> Option<u64> {
        self.result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .and_then(|r| r.as_ref().map(|r| r.data().len() as u64))
    }
    fn finish(&self, raster: Option<Arc<Raster>>) {
        *self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(raster);
        self.ready.notify_all();
    }
    fn wait(&self) -> Option<Arc<Raster>> {
        let mut result = self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while result.is_none() {
            result = self
                .ready
                .wait(result)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        result.as_ref().and_then(Clone::clone)
    }
}
struct Data {
    cache: Cache<Arc<Loading>>,
    normalise_icc: bool,
    retired: bool,
}
struct Shared {
    data: Mutex<Data>,
    started: Instant,
    clock: Mutex<Option<Arc<dyn Fn() -> Duration + Send + Sync>>>,
}
/// A thread-safe transport to this binding's cache, without extending admission lifetime.
#[derive(Clone)]
pub(crate) struct Handle(Arc<Shared>);
impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let data = self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f.debug_struct("ImageCacheHandle")
            .field("retired", &data.retired)
            .field("bytes", &data.cache.bytes())
            .finish()
    }
}
impl Handle {
    fn now(&self) -> Duration {
        let clock = self
            .0
            .clock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        clock.map_or_else(|| self.0.started.elapsed(), |clock| clock())
    }
    pub(crate) fn retire(&self) {
        let mut data = self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        data.retired = true;
        data.cache.clear();
    }
    fn refresh(&self, policy: Policy, normalise_icc: bool) {
        let now = self.now();
        let mut data = self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if data.retired {
            return;
        }
        if data.normalise_icc != normalise_icc {
            data.cache.clear();
            data.normalise_icc = normalise_icc;
        }
        if data.cache.policy() != policy {
            data.cache.set_policy(policy, now);
        }
    }
    /// Loads on the caller's existing worker. An excluded image is still rendered,
    /// but no future navigation retains it through the cache.
    pub(crate) fn load(
        &self,
        store: &Store,
        id: HashId,
        normalise_icc: bool,
    ) -> Option<Arc<Raster>> {
        self.refresh_saved(store)?;
        if self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retired
        {
            return None;
        }
        if self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .normalise_icc
            != normalise_icc
        {
            return None;
        }
        let result = store
            .read(|conn| hydrus_store::media::load_basic(conn, &[id]))
            .ok()?
            .into_iter()
            .next()?;
        let info = result.info.as_ref()?;
        let full_image = info.mime.general_class() == Some(hydrus_core::Mime::GeneralImage);
        if !full_image {
            return crate::viewer::still_with_icc(store, id, normalise_icc).map(Arc::new);
        }
        let estimated_bytes = estimate(info);
        self.render(
            id,
            estimated_bytes,
            normalise_icc,
            Some((info.mime, info.width, info.height)),
            || crate::viewer::full_still_with_icc(store, &result, normalise_icc).map(Arc::new),
        )
        .or_else(|| {
            if self
                .0
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retired
            {
                return None;
            }
            crate::viewer::thumbnail_still_with_icc(store, &result, normalise_icc).map(Arc::new)
        })
    }
    fn render(
        &self,
        id: HashId,
        estimated_bytes: u64,
        normalise_icc: bool,
        identity: Option<Identity>,
        decode: impl FnOnce() -> Option<Arc<Raster>>,
    ) -> Option<Arc<Raster>> {
        let now = self.now();
        let (loading, created) = {
            let mut data = self
                .0
                .data
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if data.retired || data.normalise_icc != normalise_icc {
                return None;
            }
            data.cache
                .remove_if(id, |loading| loading.identity != identity);
            if let Some(loading) = data.cache.get(id, now, |loading| loading.bytes()) {
                (loading, false)
            } else {
                let loading = Arc::new(Loading {
                    identity,
                    ..Loading::default()
                });
                data.cache.insert(id, loading.clone(), estimated_bytes, now);
                (loading, true)
            }
        };
        if created {
            // Complete waiters even if a decoder unwinds, without holding its lock.
            struct Completion(Option<Arc<Loading>>);
            impl Drop for Completion {
                fn drop(&mut self) {
                    if let Some(loading) = self.0.take() {
                        loading.finish(None);
                    }
                }
            }
            let mut completion = Completion(Some(loading.clone()));
            let raster = decode();
            // Completion mutates only this pending renderer, never a new epoch's entry.
            if raster.is_none() {
                let mut data = self
                    .0
                    .data
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                data.cache
                    .remove_if(id, |current| Arc::ptr_eq(current, &loading));
            }
            completion.0.take();
            loading.finish(raster.clone());
            raster
        } else {
            loading.wait()
        }
    }
    pub(crate) fn refresh_saved(&self, store: &Store) -> Option<()> {
        let policy = store.read(hydrus_store::image_cache::load).ok()?;
        let colour = store.read(hydrus_store::image_colour::load).ok()?;
        self.refresh(policy, colour.normalise_icc);
        Some(())
    }
    pub(crate) fn prefetch_allowed(&self, store: &Store, id: HashId) -> bool {
        if self.refresh_saved(store).is_none() {
            return false;
        }
        let Ok(results) = store.read(|conn| hydrus_store::media::load_basic(conn, &[id])) else {
            return false;
        };
        let Some(info) = results.first().and_then(|result| result.info.as_ref()) else {
            return false;
        };
        if info.mime.general_class() != Some(hydrus_core::Mime::GeneralImage) {
            return false;
        }
        let data = self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        !data.retired && data.cache.admits(estimate(info))
    }
    pub(crate) fn standalone(store: &Store) -> Self {
        let policy = store
            .read(hydrus_store::image_cache::load)
            .unwrap_or_default();
        let colour = store
            .read(hydrus_store::image_colour::load)
            .unwrap_or_default();
        Self(Arc::new(Shared {
            data: Mutex::new(Data {
                cache: Cache::new(policy),
                normalise_icc: colour.normalise_icc,
                retired: false,
            }),
            started: Instant::now(),
            clock: Mutex::default(),
        }))
    }
    pub(crate) fn maintain(&self) {
        let now = self.now();
        self.0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cache
            .maintain(now);
    }
    pub(crate) fn load_saved(&self, store: &Store, id: HashId) -> Option<Arc<Raster>> {
        let policy = store.read(hydrus_store::image_cache::load).ok()?;
        let colour = store.read(hydrus_store::image_colour::load).ok()?;
        self.refresh(policy, colour.normalise_icc);
        self.load(store, id, colour.normalise_icc)
    }
    /// Future decoded copies can only be reused while their cache renderer is present.
    pub(crate) fn contains(&self, id: HashId) -> bool {
        let data = self
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        !data.retired && data.cache.keys().contains(&id)
    }
}
struct State {
    cache: Handle,
    store: Weak<Store>,
    window: slint::Weak<MainWindow>,
    binding_active: Rc<Cell<bool>>,
    timer: slint::Timer,
    alive: Cell<bool>,
}
impl State {
    fn refresh(&self) {
        if !self.alive.get() {
            return;
        }
        if !self.binding_active.get() || self.window.upgrade().is_none() {
            self.retire();
            return;
        }
        let Some(store) = self.store.upgrade() else {
            self.retire();
            return;
        };
        match store.read(|conn| {
            Ok((
                hydrus_store::image_cache::load(conn)?,
                hydrus_store::image_colour::load(conn)?,
            ))
        }) {
            Ok((policy, colour)) => {
                self.cache.refresh(policy, colour.normalise_icc);
                self.cache.maintain();
            }
            Err(error) => eprintln!("could not refresh decoded-image cache: {error}"),
        }
    }
    fn retire(&self) {
        self.alive.set(false);
        self.timer.stop();
        self.cache.retire();
    }
}
/// Owned diagnostics and maintenance; retained handles never revive a retired binding.
#[derive(Clone)]
pub struct Control(Rc<State>);
impl std::fmt::Debug for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageCache")
            .field("active", &self.0.alive.get())
            .field("bytes", &self.bytes())
            .finish()
    }
}
#[derive(Debug)]
pub(crate) struct Owner(Control);
impl Drop for Owner {
    fn drop(&mut self) {
        self.0.retire();
    }
}
impl Control {
    pub(crate) fn bind(
        window: &MainWindow,
        store: &Arc<Store>,
        binding_active: Rc<Cell<bool>>,
    ) -> Self {
        let state = Rc::new(State {
            cache: Handle::standalone(store),
            store: Arc::downgrade(store),
            window: window.as_weak(),
            binding_active,
            timer: slint::Timer::default(),
            alive: Cell::new(true),
        });
        state
            .timer
            .start(slint::TimerMode::Repeated, Duration::from_millis(100), {
                let weak = Rc::downgrade(&state);
                move || {
                    if let Some(state) = weak.upgrade() {
                        state.refresh();
                    }
                }
            });
        Self(state)
    }
    pub(crate) fn handle(&self) -> Handle {
        self.0.cache.clone()
    }
    pub(crate) fn owner(&self) -> Rc<Owner> {
        Rc::new(Owner(self.clone()))
    }
    /// Read saved settings and maintain this live cache without changing presentation.
    pub fn refresh(&self) {
        self.0.refresh();
    }
    /// Permanently release entries and prevent retained handles from admitting decodes.
    pub fn retire(&self) {
        self.0.retire();
    }
    /// Cache-owned bytes; current canvas and resize buffers are counted separately.
    pub fn bytes(&self) -> u64 {
        self.0
            .cache
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cache
            .bytes()
    }
    /// Cache-owned renderer identities, oldest access first.
    pub fn keys(&self) -> Vec<HashId> {
        self.0
            .cache
            .0
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cache
            .keys()
    }
    /// An owned monotonic clock for exact idle-expiry replay, separate from view time.
    pub fn set_clock(&self, clock: Arc<dyn Fn() -> Duration + Send + Sync>) {
        if self.0.alive.get() {
            *self
                .0
                .cache
                .0
                .clock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(clock);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    fn raster(channels: u8) -> Arc<Raster> {
        Arc::new(Raster::new(10, 1, channels, vec![23; 10 * usize::from(channels)]).unwrap())
    }
    fn cache() -> (tempfile::TempDir, Arc<Store>, Handle) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &Policy {
                        bytes: 100,
                        timeout: 300,
                        percentage: 50,
                    },
                )
            })
            .unwrap();
        let cache = Handle::standalone(&store);
        (dir, store, cache)
    }
    #[test]
    fn actual_qt_pending_estimate_uses_full_unknown_resolution_fallback() {
        let fixture = hydrus_testkit::fixture_json("image_cache.json");
        for case in fixture["metadata_estimates"].as_array().unwrap() {
            let resolution = &case["resolution"];
            let width = resolution[0]
                .as_u64()
                .map(|value| u32::try_from(value).unwrap());
            let height = resolution[1]
                .as_u64()
                .map(|value| u32::try_from(value).unwrap());
            assert_eq!(
                estimate_resolution(width, height),
                case["estimated_bytes"].as_u64().unwrap()
            );
        }
    }
    #[test]
    fn shared_pending_decode_has_one_estimate_and_access_updates_loaded_bytes() {
        let (_dir, _store, cache) = cache();
        let clock = Arc::new(AtomicU64::new(0));
        *cache.0.clock.lock().unwrap() = Some(Arc::new({
            let clock = clock.clone();
            move || Duration::from_millis(clock.load(Ordering::Acquire))
        }));
        let (started, wait) = crossbeam_channel::bounded(1);
        let (release, held) = crossbeam_channel::bounded(1);
        let decode = std::thread::spawn({
            let cache = cache.clone();
            move || {
                cache
                    .render(HashId(1), 30, true, None, || {
                        started.send(()).unwrap();
                        held.recv().unwrap();
                        Some(raster(4))
                    })
                    .unwrap()
            }
        });
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(cache.0.data.lock().unwrap().cache.bytes(), 30);
        // Obtain the actual admitted renderer while decode is held, as another consumer does.
        let pending = cache
            .0
            .data
            .lock()
            .unwrap()
            .cache
            .get(HashId(1), Duration::ZERO, |loading| loading.bytes())
            .unwrap();
        assert!(pending.bytes().is_none());
        release.send(()).unwrap();
        let source = decode.join().unwrap();
        let same = pending.wait().unwrap();
        assert!(Arc::ptr_eq(&source, &same));
        assert_eq!(
            cache.0.data.lock().unwrap().cache.bytes(),
            30,
            "completion alone does not recount Qt's renderer"
        );
        let next = cache
            .render(HashId(1), 30, true, None, || {
                panic!("a loaded shared renderer must not decode again")
            })
            .unwrap();
        assert!(Arc::ptr_eq(&source, &next));
        assert_eq!(cache.0.data.lock().unwrap().cache.bytes(), 40);
        clock.store(300_000, Ordering::Release);
        cache.maintain();
        assert!(
            cache.contains(HashId(1)),
            "exact idle threshold remains cached"
        );
        clock.store(300_001, Ordering::Release);
        cache.maintain();
        assert!(!cache.contains(HashId(1)));
        assert_eq!(
            source.data().len(),
            40,
            "eviction releases only cache ownership"
        );
    }
    #[test]
    fn late_old_icc_decode_cannot_replace_new_renderer_or_revive_retired_admission() {
        let (_dir, _store, cache) = cache();
        let (started, wait) = crossbeam_channel::bounded(1);
        let (release, held) = crossbeam_channel::bounded(1);
        let decode = std::thread::spawn({
            let cache = cache.clone();
            move || {
                cache
                    .render(HashId(1), 30, true, None, || {
                        started.send(()).unwrap();
                        held.recv().unwrap();
                        Some(raster(4))
                    })
                    .unwrap()
            }
        });
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        let policy = cache.0.data.lock().unwrap().cache.policy();
        cache.refresh(policy, false);
        let current = cache
            .render(HashId(1), 30, false, None, || Some(raster(3)))
            .unwrap();
        release.send(()).unwrap();
        let old = decode.join().unwrap();
        let retained = cache
            .render(HashId(1), 30, false, None, || {
                panic!("old completion cannot displace successor")
            })
            .unwrap();
        assert!(Arc::ptr_eq(&current, &retained));
        assert!(!Arc::ptr_eq(&old, &retained));
        cache.retire();
        cache.refresh(policy, true);
        assert!(cache.0.data.lock().unwrap().cache.keys().is_empty());
        assert!(
            cache
                .render(HashId(2), 30, true, None, || panic!(
                    "retired worker cannot begin decode"
                ))
                .is_none()
        );
        assert_eq!(old.data().len(), 40);
        assert_eq!(current.data().len(), 30);
    }
    #[test]
    fn held_failed_old_renderer_cleanup_does_not_remove_successor_and_excluded_images_redraw() {
        let (_dir, _store, cache) = cache();
        let (started, wait) = crossbeam_channel::bounded(1);
        let (release, held) = crossbeam_channel::bounded(1);
        let decode = std::thread::spawn({
            let cache = cache.clone();
            move || {
                cache.render(HashId(1), 30, true, None, || {
                    started.send(()).unwrap();
                    held.recv().unwrap();
                    None
                })
            }
        });
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        let policy = cache.0.data.lock().unwrap().cache.policy();
        cache.refresh(policy, false);
        let successor = cache
            .render(HashId(1), 30, false, None, || Some(raster(3)))
            .unwrap();
        release.send(()).unwrap();
        assert!(decode.join().unwrap().is_none());
        assert!(Arc::ptr_eq(
            &successor,
            &cache
                .render(HashId(1), 30, false, None, || panic!(
                    "successor must remain"
                ))
                .unwrap()
        ));
        let first = cache
            .render(HashId(2), 50, false, None, || Some(raster(3)))
            .unwrap();
        let second = cache
            .render(HashId(2), 50, false, None, || Some(raster(3)))
            .unwrap();
        assert!(
            !Arc::ptr_eq(&first, &second),
            "strict equality renders on demand on every request"
        );
        assert!(!cache.contains(HashId(2)));
    }

    #[test]
    fn changed_resolution_replaces_cached_source_without_invalidating_external_presentation() {
        let (_dir, _store, cache) = cache();
        let first = cache
            .render(
                HashId(1),
                30,
                true,
                Some((hydrus_core::Mime::ImagePng, Some(10), Some(1))),
                || Some(raster(3)),
            )
            .unwrap();
        let replacement = cache
            .render(
                HashId(1),
                33,
                true,
                Some((hydrus_core::Mime::ImagePng, Some(11), Some(1))),
                || Some(raster(4)),
            )
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &replacement));
        let reused = cache
            .render(
                HashId(1),
                33,
                true,
                Some((hydrus_core::Mime::ImagePng, Some(11), Some(1))),
                || panic!("same metadata reuses decoded source"),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&replacement, &reused));
        assert_eq!(first.data().len(), 30);
        assert_eq!(cache.0.data.lock().unwrap().cache.bytes(), 40);
    }
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    #[test]
    fn retired_pending_renderer_completes_external_source_without_repopulating_owner() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let cache = Handle::standalone(&store);
        let (started, wait) = crossbeam_channel::bounded(1);
        let (release, held) = crossbeam_channel::bounded(1);
        let decode = std::thread::spawn({
            let cache = cache.clone();
            move || {
                cache
                    .render(HashId(1), 30, true, None, || {
                        started.send(()).unwrap();
                        held.recv().unwrap();
                        Some(Arc::new(Raster::new(10, 1, 4, vec![23; 40]).unwrap()))
                    })
                    .unwrap()
            }
        });
        wait.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(cache.contains(HashId(1)));
        cache.retire();
        assert!(!cache.contains(HashId(1)));
        release.send(()).unwrap();
        let current = decode.join().unwrap();
        assert_eq!(current.data().len(), 40);
        assert!(cache.0.data.lock().unwrap().cache.keys().is_empty());
        assert!(
            cache
                .render(HashId(1), 30, true, None, || panic!(
                    "a retained worker handle cannot revive admission"
                ))
                .is_none()
        );
    }
}
