//! Running URL queues: each works through its file seeds in order, one at
//! a time, as a reference "urls downloader" page does.
//!
//! A file that fails is marked as failed and the queue carries on, as in
//! the reference; a gallery page or watcher check that fails on the
//! network pauses the queue for the client's downloader network error
//! delay (90 minutes by default), as the reference's pages do.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Notify;

use hydrus_core::bandwidth::GalleryTokenKind;
use hydrus_core::import_options::{CallerType, ImportOptionsSlice};
use hydrus_core::network::NetworkContext;
use hydrus_core::subscriptions::CheckerDefaults;
use hydrus_core::url::UrlType;
use hydrus_core::watchers::{CheckerStatus, WatcherState};
use hydrus_net::{BandwidthScope, Job};
use hydrus_store::StoreError;
use hydrus_store::live::{JobKind, JobLive, QueueLive};
use hydrus_store::queues::{
    self, FileSeed, GallerySeed, GallerySeedMeta, NewGallerySeed, Queue, QueueKind, SeedStatus,
};
use hydrus_store::settings::Pauses;

use crate::gallery::{QueueSink, set_gallery_status};
use crate::seeds::new_url_seed;
use crate::work_slots::{Kind as WorkKind, Permit, Slots};
use crate::{Downloader, WorkError, now};

/// The default name of a new URL queue (the reference's page name).
pub const DEFAULT_URL_QUEUE_NAME: &str = "url import";

/// What a URL queue is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlQueueStatus {
    /// What the file work is doing now, if anything.
    pub files_status: String,
    /// File work is waiting out a network error until this time (seconds).
    pub delayed_until: Option<i64>,
    /// What its gallery work is doing, or last did (a simple downloader's
    /// `_gallery_status`).
    pub gallery_status: String,
}

#[derive(Debug, Default)]
struct Handle {
    /// Wakes the file work.
    wake: Notify,
    /// Wakes the gallery work (a gallery search's, or a URL list's
    /// gallery URLs), which runs beside the file work.
    gallery_wake: Notify,
    status: Mutex<UrlQueueStatus>,
    /// The file it is downloading.
    file_job: Mutex<Option<Arc<Job>>>,
    /// The gallery page (or watcher check) it is downloading.
    gallery_job: Mutex<Option<Arc<Job>>>,
    running: Mutex<bool>,
}

impl Handle {
    fn job(&self, kind: JobKind) -> &Mutex<Option<Arc<Job>>> {
        match kind {
            JobKind::File => &self.file_job,
            JobKind::Gallery => &self.gallery_job,
        }
    }
}

/// Runs the store's URL queues.
#[derive(Debug)]
pub struct QueueRunner {
    downloader: Arc<Downloader>,
    /// Queues only run once the runner is started.
    started: std::sync::atomic::AtomicBool,
    handles: Mutex<HashMap<i64, Arc<Handle>>>,
    work_slots: Arc<Slots>,
    /// Seconds to wait after a network failure.
    network_error_delay: std::sync::atomic::AtomicU64,
}

impl QueueRunner {
    fn network_error_delay(&self) -> u64 {
        self.network_error_delay
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Pick up changed network and downloader options (the downloader's,
    /// its network engine's, and the wait after a network error); whether
    /// they had changed.
    pub fn reload_settings(&self) -> Result<bool, WorkError> {
        let changed = self.downloader.reload_settings()?;
        self.network_error_delay.store(
            self.downloader
                .network_settings()
                .downloader_network_error_delay,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.work_slots.changed.notify_waiters();
        Ok(changed)
    }

    pub fn new(downloader: Arc<Downloader>, network_error_delay: u64) -> Arc<Self> {
        Arc::new(Self {
            downloader,
            started: std::sync::atomic::AtomicBool::new(false),
            handles: Mutex::default(),
            work_slots: Arc::default(),
            network_error_delay: std::sync::atomic::AtomicU64::new(network_error_delay),
        })
    }

    // A pending task owns no permit. After a release/settings wake or one
    // second, return to the caller to re-read pauses, seeds and owner state.
    async fn acquire_work_slot(
        &self,
        kind: WorkKind,
        handle: &Handle,
        job_kind: JobKind,
    ) -> Option<Permit> {
        match self
            .downloader
            .store
            .read(hydrus_store::settings::get::<hydrus_store::settings::ImportWorkSlots>)
        {
            Ok(settings) => {
                if let Some(permit) = self.work_slots.acquire(kind, &settings) {
                    let mut status = handle.status.lock();
                    let text = match job_kind {
                        JobKind::File => &mut status.files_status,
                        JobKind::Gallery => &mut status.gallery_status,
                    };
                    if text == "pending" {
                        text.clear();
                    }
                    return Some(permit);
                }
            }
            Err(error) => tracing::error!(%error, "reading importing work slots"),
        }
        match job_kind {
            JobKind::File => handle.status.lock().files_status = "pending".into(),
            JobKind::Gallery => handle.status.lock().gallery_status = "pending".into(),
        }
        let _ =
            tokio::time::timeout(Duration::from_secs(1), self.work_slots.changed.notified()).await;
        None
    }

    // Closing/removing an owner cancels its network work; await ordinary cleanup
    // before releasing its permit. Non-network imports may finish their current
    // blocking work, and remain counted until that work actually ends.
    async fn while_owner_open<F: std::future::Future>(
        &self,
        queue_id: i64,
        handle: &Handle,
        kind: JobKind,
        work: F,
    ) -> F::Output {
        tokio::pin!(work);
        let period = Duration::from_millis(250);
        let mut poll = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
        loop {
            tokio::select! {
                output = &mut work => return output,
                _ = poll.tick() => {
                    let closed = self.downloader.store.read(|conn| queues::queue(conn, queue_id)).ok().is_some_and(|queue| queue.is_none_or(|queue| queue.page_closed));
                    if closed {
                        if let Some(job) = handle.job(kind).lock().as_ref() {
                            job.cancel_because("The import page was closed.");
                        }
                        return work.await;
                    }
                }
            }
        }
    }

    pub fn downloader(&self) -> &Arc<Downloader> {
        &self.downloader
    }

    /// Start every URL queue in the store, and any made from now on.
    pub fn start_all(self: &Arc<Self>) -> Result<(), StoreError> {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let all = self
            .downloader
            .store
            .read(|conn| queues::queues(conn, None))?;
        for queue in all {
            if matches!(
                queue.kind,
                QueueKind::Urls
                    | QueueKind::Watcher
                    | QueueKind::Gallery
                    | QueueKind::LocalImport
                    | QueueKind::SimpleDownloader
            ) {
                self.wake(queue.id);
            }
        }
        Ok(())
    }

    fn handle(&self, queue: i64) -> Arc<Handle> {
        Arc::clone(self.handles.lock().entry(queue).or_default())
    }

    /// Make sure a queue is being worked on (once the runner is started),
    /// and nudge it.
    pub fn wake(self: &Arc<Self>, queue: i64) {
        if !self.started.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let handle = self.handle(queue);
        {
            let mut running = handle.running.lock();
            if !*running {
                *running = true;
                let runner = Arc::clone(self);
                let task_handle = Arc::clone(&handle);
                tokio::spawn(async move {
                    runner.run(queue, &task_handle).await;
                    *task_handle.running.lock() = false;
                });
            }
        }
        handle.wake.notify_one();
        handle.gallery_wake.notify_one();
    }

    /// Look at a queue another process changed (nudged): woken if it is
    /// one this runs (URL lists, gallery searches and watchers), a URL
    /// list taking the URLs typed into its page first.
    pub fn nudged(self: &Arc<Self>, queue: i64) {
        let store = &self.downloader.store;
        let kind = store
            .read(|conn| queues::queue(conn, queue))
            .ok()
            .flatten()
            .map(|q| q.kind);
        if !matches!(
            kind,
            Some(
                QueueKind::Urls
                    | QueueKind::Watcher
                    | QueueKind::Gallery
                    | QueueKind::LocalImport
                    | QueueKind::SimpleDownloader
            )
        ) {
            return;
        }
        if kind == Some(QueueKind::Urls) {
            match store.write(move |ctx| queues::take_url_requests(ctx.conn(), queue)) {
                Ok(typed) if !typed.is_empty() => {
                    // (as the page's URL box takes them, `_PendURLs`: full
                    // URLs only, encoded)
                    let collapse = store
                        .snapshot()
                        .url_classes
                        .settings()
                        .collapse_leading_slashes;
                    let urls: Vec<String> = typed
                        .iter()
                        .map(|url| url.trim())
                        .filter(|url| hydrus_core::url::functions::check_full_url(url).is_ok())
                        .map(|url| hydrus_core::url::ensure_url_is_encoded(url, true, collapse))
                        .collect();
                    if let Err(e) = self.pend_urls(queue, &urls, &BTreeSet::new(), &[]) {
                        tracing::error!(queue, "adding a page's URLs: {e}");
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::error!(queue, "reading a page's URLs: {e}"),
            }
        }
        self.wake(queue);
    }

    pub fn status(&self, queue: i64) -> UrlQueueStatus {
        self.handles
            .lock()
            .get(&queue)
            .map(|h| h.status.lock().clone())
            .unwrap_or_default()
    }

    /// Stop a queue's current download of this kind, as its page's cancel
    /// button does.
    pub fn cancel(&self, queue: i64, kind: JobKind) {
        if let Some(handle) = self.handles.lock().get(&queue)
            && let Some(job) = handle.job(kind).lock().as_ref()
        {
            job.cancel_because("Cancelled by user.");
        }
    }

    /// What each running queue is doing now, for its page.
    pub fn live(&self) -> Vec<(i64, QueueLive)> {
        let job_live = |job: &Mutex<Option<Arc<Job>>>| job.lock().as_ref().map(|job| live(job));
        let mut out: Vec<(i64, QueueLive)> = self
            .handles
            .lock()
            .iter()
            .filter(|(_, handle)| *handle.running.lock())
            .map(|(&queue, handle)| {
                let status = handle.status.lock().clone();
                (
                    queue,
                    QueueLive {
                        files_status: status.files_status,
                        gallery_status: status.gallery_status,
                        file_job: job_live(&handle.file_job),
                        gallery_job: job_live(&handle.gallery_job),
                    },
                )
            })
            .collect();
        out.sort_by_key(|(queue, _)| *queue);
        out
    }

    /// The URL queue a URL should go to (`GetOrMakeURLImportPage`): the one
    /// with this page key, else those with this name, else any, preferring
    /// ones with the same import options; a new one if none fits.
    pub fn url_queue_for(
        &self,
        name: Option<&str>,
        page_key: Option<&[u8]>,
        options: Option<&ImportOptionsSlice>,
    ) -> Result<Queue, StoreError> {
        let store = &self.downloader.store;
        let all = store.read(|conn| queues::queues(conn, Some(QueueKind::Urls)))?;
        let mut candidates: Vec<Queue> = if let Some(key) =
            page_key.filter(|k| all.iter().any(|q| q.page_key.as_deref() == Some(*k)))
        {
            all.into_iter()
                .filter(|q| q.page_key.as_deref() == Some(key))
                .collect()
        } else if let Some(name) = name {
            all.into_iter().filter(|q| q.name == name).collect()
        } else {
            all
        };
        if let Some(options) = options {
            candidates.retain(|q| q.options == *options);
        }
        if let Some(queue) = candidates.into_iter().next() {
            return Ok(queue);
        }
        let name = name.unwrap_or(DEFAULT_URL_QUEUE_NAME).to_owned();
        let options = options.cloned().unwrap_or_default();
        let key = page_key.map(<[u8]>::to_vec);
        store.write(move |ctx| {
            let id = queues::create_queue(
                ctx.conn(),
                QueueKind::Urls,
                &name,
                key.as_deref(),
                &options,
                now(),
            )?;
            queues::queue(ctx.conn(), id).map(|q| q.expect("just made"))
        })
    }

    /// Add URLs to a URL queue (`URLsImport.PendURLs`): post, file and
    /// unrecognised URLs become file seeds (a failed one is retried),
    /// gallery-like ones become gallery pages to read. Returns how many
    /// were added.
    pub fn pend_urls(
        self: &Arc<Self>,
        queue: i64,
        urls: &[String],
        filterable_tags: &BTreeSet<String>,
        additional_tags: &[(String, BTreeSet<String>)],
    ) -> Result<usize, StoreError> {
        let snapshot = self.downloader.store.snapshot();
        let classes = &snapshot.url_classes;
        let mut file_seeds = Vec::new();
        let mut gallery_seeds = Vec::new();
        for url in urls.iter().filter(|u| u.chars().count() > 1) {
            let url_type = classes.class_for(url).map(|c| c.url_type);
            if matches!(url_type, None | Some(UrlType::File | UrlType::Post)) {
                let mut seed = new_url_seed(classes, url);
                seed.meta
                    .external_filterable_tags
                    .clone_from(filterable_tags);
                seed.meta.external_additional_tags = additional_tags.to_vec();
                file_seeds.push(seed);
            } else {
                gallery_seeds.push(NewGallerySeed {
                    url: classes.normalise(url, true).unwrap_or_else(|_| url.clone()),
                    can_generate_more_pages: false,
                    referral_url: None,
                    meta: GallerySeedMeta {
                        external_filterable_tags: filterable_tags.clone(),
                        external_additional_tags: additional_tags.to_vec(),
                        run_token: hex::encode(rand_token()),
                        ..GallerySeedMeta::default()
                    },
                });
            }
        }
        let added = self.downloader.store.write(move |ctx| {
            let mut n = queues::add_gallery_seeds(ctx.conn(), queue, &gallery_seeds, None, now())?;
            n += queues::add_file_seeds(ctx.conn(), queue, &file_seeds, true, now())?;
            Ok(n)
        })?;
        self.wake(queue);
        Ok(added)
    }

    /// Download missing or damaged files again from their URLs, as the
    /// reference's integrity checks call `ImportURL(url, "missing files
    /// redownloader")`: each URL, cleaned and normalised as `_ImportURL`
    /// does, goes to the URL queue of that name. A URL that isn't a full
    /// URL, or that a parser should read but none can, is left out (the
    /// reference shows its error). Returns how many were added.
    pub fn redownload(self: &Arc<Self>, urls: &[String]) -> Result<usize, StoreError> {
        if urls.is_empty() {
            return Ok(0);
        }
        let snapshot = self.downloader.store.snapshot();
        let classes = &snapshot.url_classes;
        let collapse = classes.settings().collapse_leading_slashes;
        let mut cleaned = Vec::new();
        for url in urls {
            if hydrus_core::url::functions::check_full_url(url).is_err() {
                tracing::warn!(url, "a missing file's URL could not be parsed at all");
                continue;
            }
            let url = hydrus_core::url::ensure_url_is_encoded(url, true, collapse);
            let Ok(url) = classes.normalise(&url, true) else {
                continue;
            };
            let capability = classes.parse_capability(&url);
            if matches!(
                capability.url_type,
                UrlType::Gallery | UrlType::Post | UrlType::Watchable
            ) && let Err(reason) = &capability.parser
            {
                tracing::warn!(
                    url,
                    "This URL was recognised as a \"{}\" but it cannot be parsed: {reason}",
                    capability.match_name
                );
                continue;
            }
            cleaned.push(url);
        }
        let queue = self.url_queue_for(
            Some(hydrus_import::maintenance::REDOWNLOAD_PAGE_NAME),
            None,
            None,
        )?;
        self.pend_urls(queue.id, &cleaned, &BTreeSet::new(), &[])
    }

    /// Watch a thread (`MultipleWatcherImport.AddURL` on the page from
    /// `GetOrMakeMultipleWatcherPage`): a new watcher on the watcher page
    /// with this page key, else this name, else any, preferring pages with
    /// the same import options; nothing new if the page already watches it.
    /// Returns the watcher's queue, and whether it is new.
    pub fn watch(
        self: &Arc<Self>,
        url: &str,
        name: Option<&str>,
        page_key: Option<&[u8]>,
        options: Option<&ImportOptionsSlice>,
        filterable_tags: &BTreeSet<String>,
        additional_tags: &[(String, BTreeSet<String>)],
    ) -> Result<(Queue, bool), StoreError> {
        let store = &self.downloader.store;
        let snapshot = store.snapshot();
        let url = snapshot
            .url_classes
            .normalise(url, true)
            .unwrap_or_else(|_| url.to_owned());
        let all = store.read(|conn| queues::queues(conn, Some(QueueKind::Watcher)))?;
        let mut pages: Vec<(Option<Vec<u8>>, String, ImportOptionsSlice)> = Vec::new();
        for q in &all {
            let page = (q.page_key.clone(), q.name.clone(), q.options.clone());
            if !pages.contains(&page) {
                pages.push(page);
            }
        }
        if let Some(key) = page_key.filter(|k| pages.iter().any(|p| p.0.as_deref() == Some(*k))) {
            pages.retain(|p| p.0.as_deref() == Some(key));
        } else if let Some(name) = name {
            pages.retain(|p| p.1 == name);
        }
        if let Some(options) = options {
            pages.retain(|p| p.2 == *options);
        }
        let (key, name, options) = pages.into_iter().next().unwrap_or_else(|| {
            (
                page_key.map(<[u8]>::to_vec),
                name.unwrap_or(DEFAULT_WATCHER_PAGE_NAME).to_owned(),
                options.cloned().unwrap_or_default(),
            )
        });
        if let Some(existing) = all.into_iter().find(|q| {
            q.page_key == key
                && q.name == name
                && q.options == options
                && watcher_state(q).is_some_and(|w| w.url == url)
        }) {
            self.wake(existing.id);
            return Ok((existing, false));
        }
        let checkers: CheckerDefaults = store.read(hydrus_store::settings::get)?;
        let mut state = WatcherState::new(url, checkers.watchers, now());
        state.external_filterable_tags.clone_from(filterable_tags);
        state.external_additional_tags = additional_tags.to_vec();
        let extra = serde_json::to_value(&state).expect("plain data serialises");
        let queue = store.write(move |ctx| {
            let id = queues::create_queue(
                ctx.conn(),
                QueueKind::Watcher,
                &name,
                key.as_deref(),
                &options,
                now(),
            )?;
            queues::set_queue_extra(ctx.conn(), id, &extra)?;
            queues::queue(ctx.conn(), id).map(|q| q.expect("just made"))
        })?;
        self.wake(queue.id);
        Ok((queue, true))
    }

    /// Check a watcher's thread now (`WatcherImport.CheckNow`).
    pub fn check_watcher_now(self: &Arc<Self>, queue: i64) -> Result<(), StoreError> {
        hydrus_store::watchers::check_now(&self.downloader.store, queue, now())?;
        self.wake(queue);
        Ok(())
    }

    /// `_CheckWatchableURL`: read the thread for new files, then time the
    /// next check.
    async fn check_watcher(&self, queue: &Queue, mut state: WatcherState, handle: &Handle) {
        let store = &self.downloader.store;
        let job = Job::scoped(bandwidth_scope(queue.kind, queue.id));
        *handle.gallery_job.lock() = Some(Arc::clone(&job));
        handle.status.lock().gallery_status = "checking".into();
        let new_seed = NewGallerySeed {
            url: state.url.clone(),
            can_generate_more_pages: false,
            referral_url: None,
            meta: GallerySeedMeta {
                external_filterable_tags: state.external_filterable_tags.clone(),
                external_additional_tags: state.external_additional_tags.clone(),
                run_token: hex::encode(rand_token()),
                ..GallerySeedMeta::default()
            },
        };
        let queue_id = queue.id;
        let seed = store.write(move |ctx| {
            // a check that never finished (say, the client was closed)
            while let Some(mut old) = queues::next_gallery_seed(ctx.conn(), queue_id)? {
                set_gallery_status(&mut old, SeedStatus::Vetoed, "check never finished".into());
                queues::update_gallery_seed(ctx.conn(), &old)?;
            }
            queues::add_gallery_seeds(ctx.conn(), queue_id, &[new_seed], None, now())?;
            queues::next_gallery_seed(ctx.conn(), queue_id)
        });
        let mut seed = match seed {
            Ok(Some(seed)) => seed,
            Ok(None) => {
                *handle.gallery_job.lock() = None;
                return;
            }
            Err(e) => {
                *handle.gallery_job.lock() = None;
                tracing::error!(queue_id, "adding a watcher's check: {e}");
                return;
            }
        };
        let mut sink = QueueSink {
            queue: queue_id,
            max_new_urls: None,
        };
        let mut seen = BTreeSet::new();
        let result = self
            .downloader
            .work_on_gallery_url(&mut seed, &mut seen, &mut sink, &job)
            .await;
        *handle.gallery_job.lock() = None;
        match result {
            Ok(outcome) => {
                if let Some(title) = outcome.title {
                    title
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .clone_into(&mut state.subject);
                }
                if outcome.result_404 {
                    state.checking_paused = true;
                    state.status = CheckerStatus::NotFound;
                }
                if seed.status == SeedStatus::Error {
                    state.checking_paused = true;
                }
            }
            Err(WorkError::Network(e)) => {
                state.delay(self.network_error_delay() as i64, &e.to_string(), now());
                set_gallery_status(&mut seed, SeedStatus::Error, e.to_string());
            }
            Err(e) => set_gallery_status(&mut seed, SeedStatus::Error, e.to_string()),
        }
        let t = now();
        state.check_now = false;
        state.last_check_time = t;
        let saved = seed.clone();
        let files = store.write(move |ctx| {
            queues::update_gallery_seed(ctx.conn(), &saved)?;
            queues::file_seeds(ctx.conn(), queue_id)
        });
        let times = match files {
            Ok(files) => seed_times(&files),
            Err(e) => {
                tracing::error!(queue_id, "saving a watcher's check: {e}");
                return;
            }
        };
        state.update_next_check_time(&times, t);
        // `_Compact`: the gallery log keeps its latest 500 checks
        let before = t - 2 * state.checker.death_file_velocity_period();
        let compacted = store.write(move |ctx| {
            let galleries = queues::gallery_seeds(ctx.conn(), queue_id)?;
            let entries: Vec<(bool, i64)> = galleries
                .iter()
                .map(|g| (g.status == SeedStatus::Unknown, g.created))
                .collect();
            let drop: Vec<i64> =
                hydrus_core::subscriptions::compact_gallery_log(&entries, 500, before)
                    .into_iter()
                    .map(|i| galleries[i].id)
                    .collect();
            queues::remove_gallery_seeds_by_id(ctx.conn(), &drop)
        });
        if let Err(e) = compacted.and_then(|()| save_watcher_state(store, queue_id, &state)) {
            tracing::error!(queue_id, "saving a watcher: {e}");
        }
        handle.status.lock().gallery_status.clear();
    }

    /// Work a queue: watcher checks and files, or a gallery search's
    /// (or URL list's) gallery pages and files at once, as the
    /// reference's `_WorkOnFiles` and `_WorkOnGallery` run, so files from
    /// the first pages import while later pages are read.
    async fn run(self: &Arc<Self>, queue_id: i64, handle: &Handle) {
        let kind = self
            .downloader
            .store
            .read(|conn| queues::queue(conn, queue_id))
            .ok()
            .flatten()
            .map(|q| q.kind);
        match kind {
            None => {}
            Some(QueueKind::Watcher) => {
                tokio::join!(
                    self.run_files(queue_id, handle),
                    self.run_watcher_checks(queue_id, handle)
                );
            }
            Some(QueueKind::SimpleDownloader) => {
                tokio::join!(
                    self.run_files(queue_id, handle),
                    self.run_simple_jobs(queue_id, handle)
                );
            }
            Some(_) => {
                tokio::join!(
                    self.run_files(queue_id, handle),
                    self.run_gallery(queue_id, handle)
                );
            }
        }
    }

    /// A gallery search's (or URL list's) gallery work: each gallery page
    /// waiting, its files going to the queue (and waking the file work),
    /// up to a search's file limit.
    async fn run_gallery(self: &Arc<Self>, queue_id: i64, handle: &Handle) {
        loop {
            let store = &self.downloader.store;
            let queue = match store.read(|conn| queues::queue(conn, queue_id)) {
                Ok(Some(q)) => q,
                Ok(None) => return,
                Err(e) => {
                    tracing::error!(queue_id, "reading an import queue: {e}");
                    return;
                }
            };
            let pauses = store
                .read(hydrus_store::settings::get::<Pauses>)
                .unwrap_or_default();
            let delayed_until = handle.status.lock().delayed_until;
            if let Some(until) = delayed_until {
                let wait = until - now();
                if wait > 0 {
                    tokio::time::sleep(Duration::from_secs(wait as u64)).await;
                    continue;
                }
            }
            let search = gallery_search(&queue);
            // (`CheckCanDoGalleryWork`: no more pages once the file limit is hit)
            let over_limit = search
                .as_ref()
                .is_some_and(|s| s.file_limit.is_some_and(|l| s.num_new_urls_found >= l));
            // (a closed page's queue waits: "page is closed")
            if !queue.gallery_paused && !queue.page_closed && !over_limit && pauses.galleries_run()
            {
                match store.read(|conn| queues::next_gallery_seed(conn, queue_id)) {
                    Ok(Some(gallery_seed)) => {
                        let kind = if queue.kind == QueueKind::Gallery {
                            WorkKind::GallerySearch
                        } else {
                            WorkKind::Misc
                        };
                        let Some(permit) =
                            self.acquire_work_slot(kind, handle, JobKind::Gallery).await
                        else {
                            continue;
                        };
                        self.while_owner_open(
                            queue_id,
                            handle,
                            JobKind::Gallery,
                            self.work_on_gallery_seed(gallery_seed, search, handle),
                        )
                        .await;
                        drop(permit);
                        // (the page's files are ready to import)
                        handle.wake.notify_one();
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::error!(queue_id, "reading an import queue: {e}");
                        return;
                    }
                }
            }
            // idle until more work arrives, or a while (less while paused
            // globally: the switch may be flipped from the command line)
            let wait = if pauses.paged_importers || pauses.gallery_searches {
                30
            } else {
                600
            };
            let _ = tokio::time::timeout(Duration::from_secs(wait), handle.gallery_wake.notified())
                .await;
        }
    }

    /// A simple downloader's page work: each page waiting, parsed by its
    /// formula for files to download (`_WorkOnGallery`).
    async fn run_simple_jobs(self: &Arc<Self>, queue_id: i64, handle: &Handle) {
        loop {
            let store = &self.downloader.store;
            let queue = match store.read(|conn| queues::queue(conn, queue_id)) {
                Ok(Some(q)) => q,
                Ok(None) => return,
                Err(e) => {
                    tracing::error!(queue_id, "reading an import queue: {e}");
                    return;
                }
            };
            let pauses = store
                .read(hydrus_store::settings::get::<Pauses>)
                .unwrap_or_default();
            let delayed_until = handle.status.lock().delayed_until;
            if let Some(until) = delayed_until {
                let wait = until - now();
                if wait > 0 {
                    tokio::time::sleep(Duration::from_secs(wait as u64)).await;
                    continue;
                }
            }
            let job = queues::SimpleDownloader::of(&queue).and_then(|s| s.pending.first().cloned());
            if !queue.gallery_paused
                && !queue.page_closed
                && pauses.galleries_run()
                && let Some(job) = job
            {
                let Some(permit) = self
                    .acquire_work_slot(WorkKind::Misc, handle, JobKind::Gallery)
                    .await
                else {
                    continue;
                };
                let error = self
                    .while_owner_open(
                        queue_id,
                        handle,
                        JobKind::Gallery,
                        self.work_on_simple_job(queue_id, job, handle),
                    )
                    .await;
                drop(permit);
                handle.wake.notify_one();
                // (a failed page waits a while before the next)
                let pause = if error { 5 } else { 1 };
                tokio::time::sleep(Duration::from_secs(pause)).await;
                continue;
            }
            let wait = if pauses.paged_importers || pauses.gallery_searches {
                30
            } else {
                600
            };
            let _ = tokio::time::timeout(Duration::from_secs(wait), handle.gallery_wake.notified())
                .await;
        }
    }

    /// One page of a simple downloader: fetched, parsed by its formula,
    /// the files found queued (with the page as their referrer), and the
    /// page logged with what came of it; whether it failed.
    async fn work_on_simple_job(
        &self,
        queue_id: i64,
        job: queues::SimpleJob,
        handle: &Handle,
    ) -> bool {
        handle.status.lock().gallery_status = format!("checking {}", job.url);
        let net_job = Job::scoped(bandwidth_scope(QueueKind::Gallery, queue_id));
        *handle.gallery_job.lock() = Some(Arc::clone(&net_job));
        let mut request = hydrus_net::Request::get(job.url.clone());
        request.override_bandwidth_after = Some(30);
        let fetched = self.downloader.net.fetch(&request, &net_job).await;
        *handle.gallery_job.lock() = None;
        let (status, note, error) = match fetched {
            Ok(response) => match job.formula.file_urls(&job.url, &response.text()) {
                Ok(urls) => {
                    let classes = &self.downloader.store.snapshot().url_classes;
                    let seeds: Vec<queues::NewFileSeed> = urls
                        .iter()
                        .map(|url| {
                            let mut seed = new_url_seed(classes, url);
                            seed.referral_url = Some(job.url.clone());
                            seed
                        })
                        .collect();
                    let total = seeds.len();
                    let added = self.downloader.store.write(move |ctx| {
                        queues::add_file_seeds(ctx.conn(), queue_id, &seeds, false, now())
                    });
                    match added {
                        Ok(new) => {
                            let mut note = format!(
                                "page checked OK with formula \"{}\" - {} new urls",
                                job.formula.name,
                                hydrus_core::numbers::human_int(new as u64)
                            );
                            if total > new {
                                note.push_str(&format!(
                                    " ({} already in queue)",
                                    hydrus_core::numbers::human_int((total - new) as u64)
                                ));
                            }
                            (SeedStatus::SuccessfulAndNew, note, false)
                        }
                        Err(e) => (SeedStatus::Error, e.to_string(), true),
                    }
                }
                Err(e) => (SeedStatus::Error, e.to_string(), true),
            },
            Err(hydrus_net::NetError::Status {
                kind: hydrus_net::StatusKind::NotFound,
                ..
            }) => (SeedStatus::Vetoed, "page 404".to_owned(), true),
            Err(e) => {
                self.delay(handle, &e);
                (SeedStatus::Error, e.to_string(), true)
            }
        };
        note.lines()
            .next()
            .unwrap_or_default()
            .clone_into(&mut handle.status.lock().gallery_status);
        let url = job.url.clone();
        let saved = self.downloader.store.write(move |ctx| {
            let conn = ctx.conn();
            let page = queues::NewGallerySeed {
                url: url.clone(),
                can_generate_more_pages: false,
                referral_url: None,
                meta: queues::GallerySeedMeta::default(),
            };
            if queues::add_gallery_seeds(conn, queue_id, &[page], None, now())? > 0
                && let Some(mut seed) = queues::gallery_seeds(conn, queue_id)?.pop()
            {
                crate::gallery::set_gallery_status(&mut seed, status, note);
                queues::update_gallery_seed(conn, &seed)?;
            }
            // (the job done, wherever it is now)
            queues::update_simple_downloader(conn, queue_id, |state| {
                if let Some(i) = state.pending.iter().position(|j| *j == job) {
                    state.pending.remove(i);
                }
            })
        });
        if let Err(e) = saved {
            tracing::error!(queue_id, "saving a simple downloader's page: {e}");
        }
        error
    }

    /// A watcher's checks run independently from its files and use their own
    /// category, so waiting for a checker permit does not hold up file imports.
    async fn run_watcher_checks(self: &Arc<Self>, queue_id: i64, handle: &Handle) {
        let mut first_pass = true;
        loop {
            let store = &self.downloader.store;
            let queue = match store.read(|conn| queues::queue(conn, queue_id)) {
                Ok(Some(queue)) => queue,
                Ok(None) => return,
                Err(error) => {
                    tracing::error!(queue_id, %error, "reading a watcher");
                    return;
                }
            };
            let Some(mut state) = watcher_state(&queue) else {
                return;
            };
            if first_pass {
                first_pass = false;
                if let Ok(seeds) = store.read(|conn| queues::file_seeds(conn, queue_id)) {
                    state.update_next_check_time(&seed_times(&seeds), now());
                    if let Err(error) = save_watcher_state(store, queue_id, &state) {
                        tracing::error!(queue_id, %error, "saving a watcher");
                    }
                }
            }
            let pauses = store
                .read(hydrus_store::settings::get::<Pauses>)
                .unwrap_or_default();
            if state.check_due(now()) && pauses.watchers_run() && !queue.page_closed {
                let Some(permit) = self
                    .acquire_work_slot(WorkKind::WatcherCheck, handle, JobKind::Gallery)
                    .await
                else {
                    continue;
                };
                self.while_owner_open(
                    queue_id,
                    handle,
                    JobKind::Gallery,
                    self.check_watcher(&queue, state, handle),
                )
                .await;
                drop(permit);
                handle.wake.notify_one();
                continue;
            }
            let mut wait = if !queue.page_closed
                && !state.checking_paused
                && state.status == CheckerStatus::Ok
            {
                (state.next_check_time.max(state.no_work_until) + 1 - now()).clamp(1, 600)
            } else {
                600
            };
            if pauses.paged_importers || pauses.watcher_checkers {
                wait = wait.min(30);
            }
            let _ = tokio::time::timeout(
                Duration::from_secs(wait as u64),
                handle.gallery_wake.notified(),
            )
            .await;
        }
    }

    /// A queue's file work, independent from watcher checks.
    async fn run_files(self: &Arc<Self>, queue_id: i64, handle: &Handle) {
        loop {
            let store = &self.downloader.store;
            let queue = match store.read(|conn| queues::queue(conn, queue_id)) {
                Ok(Some(q)) => q,
                Ok(None) => return,
                Err(e) => {
                    tracing::error!(queue_id, "reading an import queue: {e}");
                    return;
                }
            };
            // (the global pause switches, looked at on each pass)
            let pauses = store
                .read(hydrus_store::settings::get::<Pauses>)
                .unwrap_or_default();
            let delayed_until = handle.status.lock().delayed_until;
            if let Some(until) = delayed_until {
                let wait = until - now();
                if wait > 0 {
                    // a wake (e.g. new URLs) doesn't cut a network error delay short
                    tokio::time::sleep(Duration::from_secs(wait as u64)).await;
                    continue;
                }
                handle.status.lock().delayed_until = None;
            }
            let watcher = if queue.kind == QueueKind::Watcher {
                let Some(state) = watcher_state(&queue) else {
                    return;
                };
                Some(state)
            } else {
                None
            };
            let files_blocked = watcher
                .as_ref()
                .is_some_and(|w| !w.can_do_network_work(now()));
            let next = if queue.files_paused
                || queue.page_closed
                || files_blocked
                || !pauses.files_run()
            {
                None
            } else {
                match store.read(|conn| queues::next_file_seed(conn, queue_id)) {
                    Ok(next) => next,
                    Err(e) => {
                        tracing::error!(queue_id, "reading an import queue: {e}");
                        return;
                    }
                }
            };
            let Some(seed) = next else {
                handle.status.lock().files_status.clear();
                // idle until more work arrives, the next check, or a while
                let mut wait = 600;
                if let Some(w) = &watcher
                    && !w.checking_paused
                    && w.status == CheckerStatus::Ok
                {
                    let due = w.next_check_time.max(w.no_work_until) + 1;
                    wait = (due - now()).clamp(1, 600);
                }
                // while paused globally, look again soon: the switch may be
                // flipped from the command line
                if pauses.paged_importers
                    || pauses.file_queues
                    || pauses.gallery_searches
                    || pauses.watcher_checkers
                {
                    wait = wait.min(30);
                }
                let _ =
                    tokio::time::timeout(Duration::from_secs(wait as u64), handle.wake.notified())
                        .await;
                continue;
            };
            let kind = match queue.kind {
                QueueKind::Gallery => WorkKind::GalleryFiles,
                QueueKind::Watcher => WorkKind::WatcherFiles,
                _ => WorkKind::Misc,
            };
            let Some(permit) = self.acquire_work_slot(kind, handle, JobKind::File).await else {
                continue;
            };
            let did_work = self
                .while_owner_open(
                    queue_id,
                    handle,
                    JobKind::File,
                    self.work_on_file_seed(&queue, seed, handle),
                )
                .await;
            drop(permit);
            if did_work {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    /// `_WorkOnGallery`: one gallery page, its file seeds going to the same
    /// queue, up to a gallery search's file limit.
    async fn work_on_gallery_seed(
        &self,
        mut seed: GallerySeed,
        mut search: Option<GallerySearch>,
        handle: &Handle,
    ) {
        // (only URL and gallery queues read gallery pages here)
        let job = Job::scoped(bandwidth_scope(QueueKind::Gallery, seed.queue_id));
        *handle.gallery_job.lock() = Some(Arc::clone(&job));
        let queue = seed.queue_id;
        let mut sink = QueueSink {
            queue,
            max_new_urls: search.as_ref().and_then(|s| {
                s.file_limit
                    .map(|l| l.saturating_sub(s.num_new_urls_found) as usize)
            }),
        };
        let mut seen = BTreeSet::new();
        let result = self
            .downloader
            .work_on_gallery_url(&mut seed, &mut seen, &mut sink, &job)
            .await;
        *handle.gallery_job.lock() = None;
        let mut pause_gallery = false;
        match result {
            Ok(outcome) => {
                if let Some(s) = &mut search {
                    s.num_new_urls_found += outcome.num_urls_added as u64;
                    s.num_urls_found += outcome.num_urls_total as u64;
                }
            }
            Err(WorkError::Network(e)) => self.delay(handle, &e),
            Err(e) => {
                set_gallery_status(&mut seed, SeedStatus::Error, e.to_string());
                // (a gallery search stops at an error)
                pause_gallery = search.is_some();
            }
        }
        let extra = search.map(|s| serde_json::to_value(&s).expect("plain data serialises"));
        if let Err(e) = self.downloader.store.write(move |ctx| {
            queues::update_gallery_seed(ctx.conn(), &seed)?;
            if let Some(extra) = &extra {
                queues::set_queue_extra(ctx.conn(), queue, extra)?;
            }
            if pause_gallery {
                queues::set_paused(ctx.conn(), queue, None, Some(true))?;
            }
            Ok(())
        }) {
            tracing::error!("saving a gallery seed: {e}");
        }
    }

    /// Start gallery searches (see
    /// [`hydrus_store::gallery::create_gallery_searches`]) and set them
    /// working.
    pub fn search_gallery(
        self: &Arc<Self>,
        page_name: Option<&str>,
        gug_key: &str,
        gug_name: &str,
        queries: &[String],
        file_limit: Option<Option<u64>>,
    ) -> Result<Vec<Queue>, GallerySearchError> {
        let how = hydrus_store::gallery::NewSearches {
            page_name,
            gug_key,
            gug_name,
            file_limit,
            ..Default::default()
        };
        let made = hydrus_store::gallery::create_gallery_searches(
            &self.downloader.store,
            &self.downloader.definitions(),
            &how,
            queries,
            now(),
        )?;
        for queue in &made.queues {
            self.wake(queue.id);
        }
        Ok(made.queues)
    }

    /// Start working on queues made elsewhere (say, by the command line)
    /// since the runner started.
    pub fn start_new(self: &Arc<Self>) -> Result<(), StoreError> {
        if !self.started.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        let all = self
            .downloader
            .store
            .read(|conn| queues::queues(conn, None))?;
        for queue in all {
            let known = self.handles.lock().contains_key(&queue.id);
            if !known
                && matches!(
                    queue.kind,
                    QueueKind::Urls
                        | QueueKind::Watcher
                        | QueueKind::Gallery
                        | QueueKind::SimpleDownloader
                )
            {
                self.wake(queue.id);
            }
        }
        Ok(())
    }

    /// `_DelayWork`: wait out a network failure.
    fn delay(&self, handle: &Handle, e: &hydrus_net::NetError) {
        let until = now() + self.network_error_delay() as i64;
        let mut status = handle.status.lock();
        status.delayed_until = Some(until);
        status.files_status = format!("{e} - waiting to retry");
    }

    /// `_WorkOnFiles`: one seed.
    async fn work_on_file_seed(&self, queue: &Queue, mut seed: FileSeed, handle: &Handle) -> bool {
        let lookup: Vec<&str> = std::iter::once(seed.data.as_str())
            .chain(seed.referral_url.as_deref())
            .collect();
        let caller = match queue.kind {
            QueueKind::Watcher => CallerType::WatcherUrls,
            QueueKind::LocalImport => CallerType::LocalImport,
            _ => CallerType::PostUrls,
        };
        let options = match self
            .downloader
            .full_options(caller, &queue.options, &lookup)
        {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("import options: {e}");
                return false;
            }
        };
        // (`CheckImporterCanDoFileWorkBecauseLocationsProblem`: the files
        // pause, and the seed waits)
        if let Err(e) = options.locations.check_ready_to_import() {
            let id = queue.id;
            if let Err(e) = self
                .downloader
                .store
                .write(move |ctx| queues::set_paused(ctx.conn(), id, Some(true), None))
            {
                tracing::error!("pausing an import queue: {e}");
            }
            tracing::warn!(queue = %queue.name, "{e} The queue's files are paused.");
            handle.status.lock().files_status = e.into();
            return false;
        }
        if let Some(local) = queues::LocalImport::of(queue) {
            return self.work_on_path_seed(seed, options, local, handle).await;
        }
        let job = Job::scoped(bandwidth_scope(queue.kind, queue.id));
        *handle.file_job.lock() = Some(Arc::clone(&job));
        handle.status.lock().files_status = "working".into();
        let did_work = self.downloader.work_on_url(&mut seed, &options, &job).await;
        *handle.file_job.lock() = None;
        if let Err(e) = self
            .downloader
            .store
            .write(move |ctx| queues::update_file_seed(ctx.conn(), &seed))
        {
            tracing::error!("saving a file seed: {e}");
        }
        did_work
    }
}

impl QueueRunner {
    /// `HDDImport._WorkOnFiles`: import a local import's file from its
    /// path, then delete it if the import says to (to the recycle bin, if
    /// the options say) once it is in the database.
    async fn work_on_path_seed(
        &self,
        mut seed: FileSeed,
        options: crate::FullImportOptions,
        local: queues::LocalImport,
        handle: &Handle,
    ) -> bool {
        handle.status.lock().files_status = "importing".into();
        let downloader = Arc::clone(&self.downloader);
        let worked = tokio::task::spawn_blocking(move || {
            let folders: hydrus_store::settings::FolderSettings = downloader
                .store
                .read(hydrus_store::settings::get)
                .unwrap_or_default();
            if let Err(e) = downloader.import_path_seed(
                &mut seed,
                &options,
                folders.copy_import_files_to_temp_dir,
            ) {
                tracing::error!(path = %seed.data, "importing a file: {e}");
            }
            if seed.status.is_successful() {
                route_local_metadata(&downloader, &local.routers, &seed);
                if local.delete_after_success {
                    delete_imported(&downloader, &local.routers, &seed.data, folders);
                }
            }
            downloader
                .store
                .write(move |ctx| queues::update_file_seed(ctx.conn(), &seed))
        })
        .await;
        handle.status.lock().files_status.clear();
        match worked {
            Ok(Ok(())) => true,
            Ok(Err(e)) => {
                tracing::error!("saving a file seed: {e}");
                true
            }
            Err(e) => {
                tracing::error!("importing a file: {e}");
                false
            }
        }
    }
}

/// `HDDImport._WorkOnFiles`: an imported file's metadata, read by the
/// import's routers (from its sidecars, mostly).
fn route_local_metadata(
    downloader: &Downloader,
    routers: &[hydrus_parse::sidecar::Router],
    seed: &FileSeed,
) {
    if routers.is_empty() {
        return;
    }
    let Some(hash) = crate::folders::seed_hash(seed) else {
        return;
    };
    let store = &downloader.store;
    let Ok(Some(hash_id)) = store.read(|conn| hydrus_store::master::hash_id(conn, &hash)) else {
        return;
    };
    let mut media = crate::folders::StoreMedia {
        store,
        hash_id,
        now: now(),
    };
    for router in routers {
        if let Err(e) = hydrus_parse::sidecar::work(router, &seed.data, &mut media) {
            crate::popups::show_error(
                store,
                format!(
                    "Trying to run metadata routing on the file \"{}\" threw an error!",
                    seed.data
                ),
                e.to_string(),
            );
        }
    }
}

/// `HDDImport._WorkOnFiles`'s "delete after success": the file, and any
/// sidecars its routers might have read.
fn delete_imported(
    downloader: &Downloader,
    routers: &[hydrus_parse::sidecar::Router],
    path: &str,
    folders: hydrus_store::settings::FolderSettings,
) {
    let mut paths = vec![path.to_owned()];
    for router in routers {
        for sidecar in router.possible_sidecar_paths(path) {
            if !paths.contains(&sidecar) && std::path::Path::new(&sidecar).exists() {
                paths.push(sidecar);
            }
        }
    }
    for path in paths {
        if let Err(e) = hydrus_store::paths::delete_or_recycle(&path, folders.delete_to_recycle_bin)
        {
            crate::popups::show_error(
                &downloader.store,
                format!("While attempting to delete {path}, the following error occurred:"),
                e.to_string(),
            );
        }
    }
}

/// A random 32-byte token (the reference's `HydrusData.GenerateKey`).
fn rand_token() -> [u8; 32] {
    rand::random()
}

use hydrus_store::watchers::seed_times;
/// A watcher queue's state.
pub use hydrus_store::watchers::{DEFAULT_WATCHER_PAGE_NAME, watcher_state};

fn save_watcher_state(
    store: &hydrus_store::Store,
    queue: i64,
    state: &WatcherState,
) -> Result<(), StoreError> {
    let extra = serde_json::to_value(state).expect("plain data serialises");
    store.write(move |ctx| queues::set_queue_extra(ctx.conn(), queue, &extra))
}

pub use hydrus_store::gallery::{DEFAULT_GALLERY_PAGE_NAME, GallerySearchError};

pub use hydrus_core::gallery::GallerySearch;

/// A gallery search queue's state.
pub fn gallery_search(queue: &Queue) -> Option<GallerySearch> {
    (queue.kind == QueueKind::Gallery)
        .then(|| serde_json::from_value(queue.extra.clone()).ok())
        .flatten()
}

/// What a queue's requests count against, as the reference's importers make
/// their network jobs: the downloader page (a URL queue, a gallery search)
/// or the watcher, whose gallery pages wait their turn per site with the
/// rest of their kind.
fn bandwidth_scope(kind: QueueKind, queue: i64) -> BandwidthScope {
    let key = format!("{queue:016x}");
    match kind {
        QueueKind::Watcher => BandwidthScope {
            contexts: vec![NetworkContext::watcher_page(key)],
            override_after: None,
            gallery_token: Some(GalleryTokenKind::Watcher),
        },
        _ => BandwidthScope {
            contexts: vec![NetworkContext::downloader_page(key)],
            override_after: None,
            gallery_token: Some(GalleryTokenKind::DownloadPage),
        },
    }
}

/// A download as the client shows it (`NetworkJob.GetStatus`).
pub(crate) fn live(job: &Job) -> JobLive {
    let state = job.state();
    JobLive {
        url: state.url,
        status: state.status,
        speed: state.speed,
        bytes_read: state.bytes_read,
        bytes_to_read: state.bytes_total,
        done: state.done,
        error: state.error,
    }
}
