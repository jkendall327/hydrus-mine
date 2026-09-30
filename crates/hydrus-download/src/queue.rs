//! Running URL queues: each works through its file seeds in order, one at
//! a time, as a reference "urls downloader" page does.
//!
//! A network failure pauses the queue's file work for the client's
//! downloader network error delay (90 minutes by default), as the
//! reference's pages do, rather than burning through the rest of the queue.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::sync::Notify;

use hydrus_core::import_options::{CallerType, ImportOptionsSlice};
use hydrus_core::url::UrlType;
use hydrus_net::Job;
use hydrus_store::StoreError;
use hydrus_store::queues::{
    self, FileSeed, GallerySeed, GallerySeedMeta, NewGallerySeed, Queue, QueueKind, SeedStatus,
};

use crate::gallery::{FileSink, set_gallery_status};
use crate::seeds::new_url_seed;
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
}

#[derive(Debug, Default)]
struct Handle {
    wake: Notify,
    status: Mutex<UrlQueueStatus>,
    job: Mutex<Option<Arc<Job>>>,
    running: Mutex<bool>,
}

/// Runs the store's URL queues.
#[derive(Debug)]
pub struct QueueRunner {
    downloader: Arc<Downloader>,
    /// Queues only run once the runner is started.
    started: std::sync::atomic::AtomicBool,
    handles: Mutex<HashMap<i64, Arc<Handle>>>,
    /// Seconds to wait after a network failure.
    network_error_delay: u64,
}

impl QueueRunner {
    pub fn new(downloader: Arc<Downloader>, network_error_delay: u64) -> Arc<Self> {
        Arc::new(Self {
            downloader,
            started: std::sync::atomic::AtomicBool::new(false),
            handles: Mutex::default(),
            network_error_delay,
        })
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
            .read(|conn| queues::queues(conn, Some(QueueKind::Urls)))?;
        for queue in all {
            self.wake(queue.id);
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
    }

    pub fn status(&self, queue: i64) -> UrlQueueStatus {
        self.handles
            .lock()
            .get(&queue)
            .map(|h| h.status.lock().clone())
            .unwrap_or_default()
    }

    /// Stop what a queue is downloading now.
    pub fn cancel_current(&self, queue: i64) {
        if let Some(handle) = self.handles.lock().get(&queue)
            && let Some(job) = handle.job.lock().as_ref()
        {
            job.cancel();
        }
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

    async fn run(self: &Arc<Self>, queue_id: i64, handle: &Handle) {
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
            if !queue.gallery_paused {
                match store.read(|conn| queues::next_gallery_seed(conn, queue_id)) {
                    Ok(Some(gallery_seed)) => {
                        self.work_on_gallery_seed(gallery_seed, handle).await;
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
            let next = if queue.files_paused {
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
                // idle until more work arrives (or check back in a while)
                let _ =
                    tokio::time::timeout(Duration::from_secs(600), handle.wake.notified()).await;
                continue;
            };
            let did_work = self.work_on_file_seed(&queue, seed, handle).await;
            if did_work {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    /// `_WorkOnGallery`: one gallery page, its file seeds going to the same
    /// queue.
    async fn work_on_gallery_seed(&self, mut seed: GallerySeed, handle: &Handle) {
        let job = Job::new();
        *handle.job.lock() = Some(Arc::clone(&job));
        handle.status.lock().files_status = "reading a gallery page".into();
        let sink = FileSink {
            queue: seed.queue_id,
            max_new_urls: None,
        };
        let mut seen = BTreeSet::new();
        let result = self
            .downloader
            .work_on_gallery_url(&mut seed, &mut seen, &sink, &job)
            .await;
        *handle.job.lock() = None;
        match result {
            Ok(_) => {}
            Err(WorkError::Network(e)) => self.delay(handle, &e),
            Err(e) => set_gallery_status(&mut seed, SeedStatus::Error, e.to_string()),
        }
        if let Err(e) = self
            .downloader
            .store
            .write(move |ctx| queues::update_gallery_seed(ctx.conn(), &seed))
        {
            tracing::error!("saving a gallery seed: {e}");
        }
    }

    /// `_DelayWork`: wait out a network failure.
    fn delay(&self, handle: &Handle, e: &hydrus_net::NetError) {
        let until = now() + self.network_error_delay as i64;
        let mut status = handle.status.lock();
        status.delayed_until = Some(until);
        status.files_status = format!("{e} - waiting to retry");
    }

    /// `_WorkOnFiles`: one seed.
    async fn work_on_file_seed(&self, queue: &Queue, mut seed: FileSeed, handle: &Handle) -> bool {
        let lookup: Vec<&str> = std::iter::once(seed.data.as_str())
            .chain(seed.referral_url.as_deref())
            .collect();
        let options =
            match self
                .downloader
                .full_options(CallerType::PostUrls, &queue.options, &lookup)
            {
                Ok(o) => o,
                Err(e) => {
                    tracing::error!("import options: {e}");
                    return false;
                }
            };
        let job = Job::new();
        *handle.job.lock() = Some(Arc::clone(&job));
        handle.status.lock().files_status = "working".into();
        let result = self.downloader.work_on_url(&mut seed, &options, &job).await;
        *handle.job.lock() = None;
        let did_work = match result {
            Ok(did_work) => did_work,
            Err(WorkError::Network(e)) => {
                self.delay(handle, &e);
                false
            }
            Err(e) => {
                crate::seeds::set_status(&mut seed, SeedStatus::Error, e.to_string());
                false
            }
        };
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

/// A random 32-byte token (the reference's `HydrusData.GenerateKey`).
fn rand_token() -> [u8; 32] {
    rand::random()
}
