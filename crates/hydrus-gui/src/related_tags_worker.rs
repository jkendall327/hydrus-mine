//! One fallibly started worker per Manage Tags owner; newest requests replace pending work.
use hydrus_store::{
    Store,
    related_tags::{Query, Report},
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicU64, Ordering},
};
type Reply = Result<Report, String>;
type TaggedReply = (u64, Reply);
struct Pending {
    request: Option<(u64, Query)>,
    closed: bool,
}
struct Shared {
    pending: Mutex<Pending>,
    ready: Condvar,
    generation: AtomicU64,
    result: Mutex<Option<TaggedReply>>,
}
pub(crate) struct Worker {
    shared: Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker").finish_non_exhaustive()
    }
}
impl Worker {
    pub(crate) fn new(store: &Arc<Store>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            pending: Mutex::new(Pending {
                request: None,
                closed: false,
            }),
            ready: Condvar::new(),
            generation: AtomicU64::new(0),
            result: Mutex::new(None),
        });
        let state = shared.clone();
        let store = store.clone();
        let thread = std::thread::Builder::new()
            .name("related-tags".into())
            .spawn(move || {
                loop {
                    let Ok(mut pending) = state.pending.lock() else {
                        return;
                    };
                    while pending.request.is_none() && !pending.closed {
                        let Ok(next) = state.ready.wait(pending) else {
                            return;
                        };
                        pending = next;
                    }
                    if pending.closed {
                        return;
                    }
                    let Some((id, query)) = pending.request.take() else {
                        continue;
                    };
                    drop(pending);
                    let cancelled = || state.generation.load(Ordering::Acquire) != id;
                    let result = hydrus_store::related_tags::query(&store, &query, &cancelled)
                        .map_err(|error| error.to_string());
                    if !cancelled()
                        && let Ok(mut latest) = state.result.lock()
                    {
                        *latest = Some((id, result));
                    }
                }
            })?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    pub(crate) fn request(&self, query: Query) -> u64 {
        let id = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Ok(mut pending) = self.shared.pending.lock() {
            if pending.closed {
                return id;
            }
            pending.request = Some((id, query));
            self.shared.ready.notify_one();
        }
        id
    }
    pub(crate) fn poll(&self) -> Option<Reply> {
        let (id, result) = self.shared.result.lock().ok()?.take()?;
        (id == self.shared.generation.load(Ordering::Acquire)).then_some(result)
    }

    pub(crate) fn close(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut pending) = self.shared.pending.lock() {
            pending.closed = true;
            pending.request = None;
            self.shared.ready.notify_one();
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.close();
        // Detach only after cancellation; the owned worker leaves its read loop
        // without blocking the UI on shutdown. No replacement thread is started.
        self.thread.take();
    }
}
