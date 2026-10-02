//! Sending requests as the reference's network jobs do.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use hydrus_core::bandwidth::{BandwidthType, GalleryTokenKind, Manager, Tracker};
use hydrus_core::numbers::human_bytes;
use hydrus_core::time::timestamp_to_pretty_time_delta;
use hydrus_core::url::functions::{check_full_url, ensure_url_is_encoded};
use hydrus_core::url::pyurl::urljoin;
use hydrus_core::url::{UrlType, psl};
use hydrus_store::Store;
use hydrus_store::bandwidth::BandwidthSettings;
use hydrus_store::network::{self, Approval, NetworkContext};

use crate::cookies::{CookieChange, CookieUrl, cookie_header, set_cookie_changes};
use crate::error::{NetError, StatusOutcome, status_outcome};

/// The client options that shape requests (the reference's defaults).
#[derive(Debug, Clone)]
pub struct NetOptions {
    /// Seconds to wait for a connection; reading waits six times as long.
    pub network_timeout: u64,
    /// Seconds to wait before reconnecting, times the attempts so far.
    pub connection_error_wait_time: u64,
    /// Seconds to wait when a server says it is too busy (grows with each
    /// attempt), unless it says how long.
    pub serverside_bandwidth_wait_time: u64,
    pub max_connection_attempts: u32,
    /// Attempts for a GET that gets a retryable failure (a POST gets one).
    pub max_get_attempts: u32,
    pub max_jobs: usize,
    pub max_jobs_per_domain: usize,
    pub verify_https: bool,
    /// Apply the bandwidth rules and the gallery page waits (tests talking
    /// to a local server turn them off).
    pub obey_bandwidth: bool,
    /// After this many serious errors (connection failures, server errors)
    /// from a domain within `domain_error_window` seconds, requests to it
    /// wait until there are fewer; 0 never waits
    /// (`domain_network_infrastructure_error_number` and `_time_delta`).
    pub domain_error_number: usize,
    pub domain_error_window: i64,
    /// Proxies for http and https requests, and the hosts that skip them.
    pub http_proxy: Option<String>,
    pub https_proxy: Option<String>,
    pub no_proxy: Option<String>,
    /// Seconds requests wait after the computer wakes from sleep
    /// (`wake_delay_period`), for its network to come back.
    pub wake_delay: u64,
}

impl Default for NetOptions {
    fn default() -> Self {
        Self {
            network_timeout: 10,
            connection_error_wait_time: 15,
            serverside_bandwidth_wait_time: 60,
            max_connection_attempts: 5,
            max_get_attempts: 5,
            max_jobs: 15,
            max_jobs_per_domain: 3,
            verify_https: true,
            obey_bandwidth: true,
            domain_error_number: 3,
            domain_error_window: 600,
            http_proxy: None,
            https_proxy: None,
            no_proxy: None,
            wake_delay: 15,
        }
    }
}

impl NetOptions {
    /// The client's network options.
    pub fn from_settings(s: &hydrus_store::network::NetworkSettings) -> Self {
        Self {
            network_timeout: s.network_timeout,
            connection_error_wait_time: s.connection_error_wait_time,
            serverside_bandwidth_wait_time: s.serverside_bandwidth_wait_time,
            max_connection_attempts: s.max_connection_attempts,
            max_get_attempts: s.max_get_attempts,
            max_jobs: s.max_jobs,
            max_jobs_per_domain: s.max_jobs_per_domain,
            verify_https: s.verify_https,
            obey_bandwidth: true,
            domain_error_number: s.domain_error_number,
            domain_error_window: s.domain_error_window,
            http_proxy: s.http_proxy.clone(),
            https_proxy: s.https_proxy.clone(),
            no_proxy: s.no_proxy.clone(),
            wake_delay: s.wake_delay_period,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// A request to make.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: Method,
    /// The URL to fetch (already the API URL, if its class has one).
    pub url: String,
    /// The page the URL was found on, if known.
    pub referral_url: Option<String>,
    pub body: Option<Vec<u8>>,
    pub additional_headers: Vec<(String, String)>,
    /// Contexts beyond the global and domain ones (the downloader page,
    /// subscription or watcher it is for), whose headers also apply.
    pub extra_contexts: Vec<NetworkContext>,
    /// Other URLs whose sites' bandwidth this request counts against (the
    /// post page a file was found on).
    pub bandwidth_urls: Vec<String>,
    /// Stop obeying the bandwidth rules after waiting this many seconds
    /// (else the job's [`BandwidthScope`]'s).
    pub override_bandwidth_after: Option<u64>,
    /// A gallery page: it waits its turn per site, as the job's scope says.
    pub gallery_page: bool,
    /// Try once, whatever happens.
    pub one_shot: bool,
    /// Write the response here instead of keeping it in memory.
    pub destination: Option<PathBuf>,
}

impl Request {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            referral_url: None,
            body: None,
            additional_headers: Vec::new(),
            extra_contexts: Vec::new(),
            bandwidth_urls: Vec::new(),
            override_bandwidth_after: None,
            gallery_page: false,
            one_shot: false,
            destination: None,
        }
    }
}

/// What came back.
#[derive(Debug, Clone)]
pub struct Response {
    /// Where the request ended up, after redirects.
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    /// The body, unless it was written to the request's destination.
    pub body: Vec<u8>,
    pub bytes_read: u64,
    /// The server's `Last-Modified`, if sensible.
    pub last_modified: Option<i64>,
    pub server: Option<String>,
}

impl Response {
    /// The body as text, in the charset the server gave or a good guess.
    pub fn text(&self) -> String {
        crate::text::decode(&self.body, self.content_type.as_deref())
    }
}

/// What an importer's requests count against, as the reference's network
/// job factories set it: the downloader page, subscription or watcher
/// context, how long they wait for bandwidth before going anyway, and which
/// gallery pages wait their turn together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BandwidthScope {
    pub contexts: Vec<NetworkContext>,
    /// Seconds a request waits on the rules before ignoring them (`None`:
    /// as long as they say).
    pub override_after: Option<u64>,
    pub gallery_token: Option<GalleryTokenKind>,
}

/// A request in progress: what it is doing, and a way to cancel it.
#[derive(Debug, Default)]
pub struct Job {
    state: Mutex<JobState>,
    cancel: CancellationToken,
    scope: Mutex<BandwidthScope>,
    /// The data it has read, by second, for its speed (the reference's
    /// job's own `BandwidthTracker`).
    tracker: Mutex<Option<Tracker>>,
    /// Why it was cancelled, if it was.
    cancel_reason: Mutex<Option<String>>,
}

/// What a job is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobState {
    pub status: String,
    pub bytes_read: u64,
    pub bytes_total: Option<u64>,
    /// Bytes read in the last second, as the reference reckons a job's
    /// speed (filled in by [`Job::state`]).
    pub speed: u64,
    pub done: bool,
    /// Its last request failed or was cancelled (`HasError`).
    pub error: bool,
}

impl Job {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A job whose requests count against `scope`.
    pub fn scoped(scope: BandwidthScope) -> Arc<Self> {
        Arc::new(Self {
            scope: Mutex::new(scope),
            ..Self::default()
        })
    }

    pub fn scope(&self) -> BandwidthScope {
        self.scope.lock().clone()
    }

    /// Count the job's next requests against `scope` (a subscription works
    /// through its queries with one job).
    pub fn set_scope(&self, scope: BandwidthScope) {
        *self.scope.lock() = scope;
    }

    pub fn state(&self) -> JobState {
        let mut state = self.state.lock().clone();
        if let Some(tracker) = self.tracker.lock().as_mut() {
            state.speed = tracker.usage(BandwidthType::Data, Some(1), now());
        }
        state
    }

    /// Count `bytes` read towards the job's speed.
    fn report_read(&self, bytes: u64) {
        let now = now();
        self.tracker
            .lock()
            .get_or_insert_with(|| Tracker::new(now))
            .report_data(bytes, now);
    }

    pub fn cancel(&self) {
        self.cancel_because("cancelled!");
    }

    /// Cancel it, saying why (the reference's `Cancel(status_text)`): its
    /// status says so until its request stops ("Cancelled!").
    pub fn cancel_because(&self, why: &str) {
        *self.cancel_reason.lock() = Some(why.to_owned());
        self.set_status(why);
        self.cancel.cancel();
    }

    /// What a seed whose download was cancelled notes (the reference's
    /// `CancelledException` from `WaitUntilDone`).
    pub fn cancelled_note(&self) -> String {
        let reason = self.cancel_reason.lock();
        format!(
            "Download cancelled: {}",
            reason.as_deref().unwrap_or("cancelled!")
        )
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Say what the job is doing (the engine and its callers both do).
    pub fn set_status_text(&self, status: impl Into<String>) {
        self.set_status(status);
    }

    fn set_status(&self, status: impl Into<String>) {
        self.state.lock().status = status.into();
    }

    async fn sleep(&self, seconds: f64) -> Result<(), NetError> {
        if seconds <= 0.0 {
            return Ok(());
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs_f64(seconds)) => Ok(()),
            () = self.cancel.cancelled() => Err(NetError::Cancelled),
        }
    }
}

/// Makes requests with the client's cookies and headers.
#[derive(Debug)]
pub struct NetEngine {
    client: reqwest::Client,
    store: Arc<Store>,
    options: NetOptions,
    slots: Arc<Semaphore>,
    domain_slots: Mutex<std::collections::HashMap<String, Arc<Semaphore>>>,
    /// The bandwidth rules and usage, and when the usage was last saved.
    bandwidth: Mutex<(Manager, i64)>,
    bandwidth_settings: BandwidthSettings,
    /// When each domain last had serious errors (`DomainOK`).
    domain_errors: Mutex<std::collections::HashMap<String, Vec<i64>>>,
    /// When the sleep check last ran, and (after a wake) when requests may
    /// go again, in ms.
    wake: Mutex<(Option<i64>, Option<i64>)>,
}

/// Why one attempt failed, and so what happens next.
enum Failure {
    Fatal(NetError),
    ServersideBandwidth {
        message: String,
        retry_after: Option<u64>,
    },
    Reattempt(String),
    Broken,
    Connect(String),
    ReadTimeout,
}

impl From<NetError> for Failure {
    fn from(e: NetError) -> Self {
        Failure::Fatal(e)
    }
}

/// Where a response body goes.
enum Sink {
    Memory(Vec<u8>),
    File(tokio::fs::File),
}

/// What an attempt knows about its request.
struct Attempt<'a> {
    request: &'a Request,
    job: &'a Job,
    contexts: Vec<NetworkContext>,
    session: NetworkContext,
    sends_range: bool,
}

/// How much of the body has arrived so far (across ranged responses).
#[derive(Default)]
struct Progress {
    read: u64,
    total: Option<u64>,
    accurate: bool,
    empty_chunks: u32,
}

const MAX_REDIRECTS: usize = 30;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: String) {
    headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    headers.push((name.to_owned(), value));
}

impl NetEngine {
    pub fn new(store: Arc<Store>, options: NetOptions) -> Result<Self, NetError> {
        let failed =
            |e: reqwest::Error| NetError::Network(format!("could not start the HTTP client: {e}"));
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(options.network_timeout))
            .read_timeout(Duration::from_secs(options.network_timeout * 6))
            .tls_danger_accept_invalid_certs(!options.verify_https);
        // (the hosts that skip the proxies only apply when there is one, as
        // in the reference)
        let no_proxy = options
            .no_proxy
            .as_deref()
            .and_then(reqwest::NoProxy::from_string);
        if let Some(proxy) = &options.http_proxy {
            builder = builder.proxy(
                reqwest::Proxy::http(proxy)
                    .map_err(failed)?
                    .no_proxy(no_proxy.clone()),
            );
        }
        if let Some(proxy) = &options.https_proxy {
            builder = builder.proxy(
                reqwest::Proxy::https(proxy)
                    .map_err(failed)?
                    .no_proxy(no_proxy),
            );
        }
        let client = builder.build().map_err(failed)?;
        let now = now();
        let (bandwidth_settings, usage) = store
            .read(|conn| {
                Ok((
                    hydrus_store::settings::get::<BandwidthSettings>(conn)?,
                    hydrus_store::bandwidth::usage(conn, now)?,
                ))
            })
            .map_err(|e| NetError::Io(e.to_string()))?;
        let mut manager = Manager::new(bandwidth_settings.rules.clone());
        manager.set_trackers(usage);
        Ok(Self {
            client,
            store,
            slots: Arc::new(Semaphore::new(options.max_jobs.max(1))),
            domain_slots: Mutex::default(),
            bandwidth: Mutex::new((manager, now)),
            bandwidth_settings,
            domain_errors: Mutex::default(),
            wake: Mutex::default(),
            options,
        })
    }

    /// `SleepCheck`, to call every 15 seconds or so: a minute or more since
    /// the last call means the computer slept, and requests then wait the
    /// wake delay for its network to come back.
    pub fn sleep_check(&self) {
        self.sleep_check_at(now_ms());
    }

    /// [`Self::sleep_check`] at a given time (ms since the epoch).
    #[doc(hidden)]
    pub fn sleep_check_at(&self, now: i64) {
        let mut wake = self.wake.lock();
        let (last, awake_at) = &mut *wake;
        if last.is_some_and(|t| now - t > 60_000) {
            let delay = i64::try_from(self.options.wake_delay).unwrap_or(i64::MAX);
            *awake_at = Some(now.saturating_add(delay.saturating_mul(1000)));
            tracing::info!("the computer seems to have just woken up; requests wait {delay} s");
        } else if awake_at.is_some_and(|t| now >= t) {
            *awake_at = None;
        }
        *last = Some(now);
    }

    fn just_woke(&self) -> bool {
        self.wake.lock().1.is_some_and(|t| now_ms() < t)
    }

    /// Whether requests to `url`'s domain may go (`DomainOK`): not if it or
    /// a parent domain has had too many serious errors recently.
    pub fn domain_ok(&self, url: &str) -> bool {
        let number = self.options.domain_error_number;
        if number == 0 {
            return true;
        }
        let Ok(domain) = hydrus_core::url::url_domain(url) else {
            return true;
        };
        let cutoff = now() - self.options.domain_error_window;
        let mut errors = self.domain_errors.lock();
        let mut ok = true;
        for domain in psl::all_applicable_domains(&domain) {
            if let Some(times) = errors.get_mut(&domain) {
                times.retain(|&t| t > cutoff);
                if times.is_empty() {
                    errors.remove(&domain);
                } else if times.len() >= number {
                    ok = false;
                }
            }
        }
        ok
    }

    /// Count a serious error against `url`'s domain and its parents.
    fn report_domain_error(&self, url: &str) {
        let Ok(domain) = hydrus_core::url::url_domain(url) else {
            return;
        };
        let now = now();
        let mut errors = self.domain_errors.lock();
        for domain in psl::all_applicable_domains(&domain) {
            errors.entry(domain).or_default().push(now);
        }
    }

    /// Wait while `url`'s domain is having trouble.
    async fn wait_for_domain(&self, url: &str, job: &Job) -> Result<(), NetError> {
        while !self.domain_ok(url) {
            job.set_status("This domain has had several serious errors recently. Waiting a bit.");
            job.sleep(10.0).await?;
        }
        Ok(())
    }

    pub fn bandwidth_settings(&self) -> &BandwidthSettings {
        &self.bandwidth_settings
    }

    /// The network contexts a request to `url` counts against: everything,
    /// and each level of its domain.
    pub fn contexts_for(url: &str) -> Vec<NetworkContext> {
        let mut contexts = vec![NetworkContext::global()];
        if let Ok(parts) = check_full_url(url) {
            contexts.extend(
                psl::all_applicable_domains(&parts.netloc)
                    .into_iter()
                    .map(NetworkContext::domain),
            );
        }
        contexts
    }

    /// Whether `contexts` have room for a request and a megabyte, ignoring
    /// rules over `threshold` seconds or less (a subscription asks before
    /// working on a query).
    pub fn can_do_work(&self, contexts: &[NetworkContext], threshold: u64) -> bool {
        !self.options.obey_bandwidth
            || self
                .bandwidth
                .lock()
                .0
                .can_do_work(contexts, threshold, now())
    }

    /// Seconds until `contexts`' rules all have room again.
    pub fn waiting_estimate(&self, contexts: &[NetworkContext]) -> u64 {
        if !self.options.obey_bandwidth {
            return 0;
        }
        self.bandwidth
            .lock()
            .0
            .waiting_estimate_and_context(contexts, now())
            .0
    }

    /// Keep the bandwidth usage that changed (done every minute as it
    /// changes; call it when stopping).
    pub fn save_bandwidth(&self) -> Result<(), NetError> {
        let dirty = {
            let mut b = self.bandwidth.lock();
            b.1 = now();
            b.0.take_dirty()
        };
        if dirty.is_empty() {
            return Ok(());
        }
        self.store
            .write(move |ctx| hydrus_store::bandwidth::save_usage(ctx.conn(), &dirty))
            .map_err(|e| NetError::Io(e.to_string()))
    }

    /// Save the usage if it hasn't been for a minute.
    fn maybe_save_bandwidth(&self) {
        let due = now() - self.bandwidth.lock().1 >= 60;
        if due && let Err(e) = self.save_bandwidth() {
            tracing::warn!("could not save bandwidth usage: {e}");
        }
    }

    /// Wait until the rules let a request start (counting it), or until
    /// it may go regardless (`ClientNetworkingJobs.TryToStartBandwidth`).
    async fn wait_for_bandwidth(
        &self,
        contexts: &[NetworkContext],
        obeys: bool,
        override_at: Option<i64>,
        job: &Job,
    ) -> Result<(), NetError> {
        loop {
            if job.is_cancelled() {
                return Err(NetError::Cancelled);
            }
            let now = now();
            // POSTs and overridden requests go at once (but still count)
            let obeys = obeys && override_at.is_none_or(|at| now <= at);
            let wait = {
                let mut b = self.bandwidth.lock();
                if !obeys {
                    b.0.report_request(contexts, now);
                    None
                } else if b.0.try_to_start_request(contexts, now) {
                    None
                } else {
                    Some(b.0.waiting_estimate_and_context(contexts, now))
                }
            };
            let Some((seconds, whose)) = wait else {
                self.maybe_save_bandwidth();
                return Ok(());
            };
            let seconds = i64::try_from(seconds).unwrap_or(i64::MAX);
            let (until, what) = match override_at {
                Some(at) if at - now < seconds => (at, "overriding bandwidth"),
                _ => (now.saturating_add(seconds), "bandwidth free"),
            };
            let when = match timestamp_to_pretty_time_delta(until, now, "") {
                t if t == "now" => "imminently".to_owned(),
                t => t,
            };
            job.set_status(format!(
                "{what} {when}\u{2026} ({})",
                whose.to_human_string()
            ));
            // as the reference's job sleeps; shorter waits are retried as
            // its engine loop retries them
            let mut sleep = match seconds {
                s if s > 1200 => 30.0,
                s if s > 120 => 10.0,
                s if s > 10 => 0.8,
                _ => 0.1,
            };
            // (woken for an override, as the reference wakes the job)
            if let Some(at) = override_at {
                sleep = f64::min(sleep, ((at - now) as f64 + 1.0).max(0.1));
            }
            job.sleep(sleep).await?;
        }
    }

    /// Wait while all new network traffic is paused (looked at every couple
    /// of seconds, so a switch from the command line takes effect).
    async fn wait_while_paused(&self, job: &Job) -> Result<(), NetError> {
        loop {
            let paused = self
                .store
                .read(hydrus_store::settings::get::<hydrus_store::settings::Pauses>)
                .is_ok_and(|p| p.network_traffic);
            if !paused {
                return Ok(());
            }
            job.set_status("all new network traffic is paused\u{2026}");
            job.sleep(2.0).await?;
        }
    }

    /// Wait while a custom header for these contexts awaits approval: the
    /// reference holds such a job while it asks in a popup (`IsValid`);
    /// here the header is approved through the Client API.
    async fn wait_for_header_approval(
        &self,
        contexts: &[NetworkContext],
        job: &Job,
    ) -> Result<(), NetError> {
        loop {
            let pending = self
                .store
                .read(|conn| {
                    for context in contexts {
                        if let Some(h) = network::headers(conn, context)?
                            .into_iter()
                            .find(|h| h.approval == Approval::Pending)
                        {
                            return Ok(Some(h.name));
                        }
                    }
                    Ok(None)
                })
                .map_err(|e| NetError::Io(e.to_string()))?;
            let Some(name) = pending else {
                return Ok(());
            };
            job.set_status(format!(
                "waiting for the custom header \"{name}\" to be approved\u{2026}"
            ));
            job.sleep(5.0).await?;
        }
    }

    /// Wait for this site's turn to fetch a gallery page of `kind`
    /// (`ClientNetworkingBandwidth.TryToConsumeAGalleryToken`).
    async fn wait_for_gallery_token(
        &self,
        second_level_domain: &str,
        kind: GalleryTokenKind,
        job: &Job,
    ) -> Result<(), NetError> {
        let s = &self.bandwidth_settings;
        let delay = match kind {
            GalleryTokenKind::DownloadPage => s.gallery_page_wait_pages,
            GalleryTokenKind::Subscription => s.gallery_page_wait_subscriptions,
            GalleryTokenKind::Watcher => s.watcher_page_wait,
        };
        loop {
            let now = now();
            let result = self.bandwidth.lock().0.try_to_consume_gallery_token(
                second_level_domain,
                kind,
                delay,
                now,
            );
            match result {
                Ok(()) => {
                    job.set_status("gallery token ok - starting soon");
                    return Ok(());
                }
                Err(next) => {
                    let when = match timestamp_to_pretty_time_delta(next, now, "") {
                        t if t == "now" => "checking".to_owned(),
                        t => t,
                    };
                    job.set_status(format!("waiting to start: {when}"));
                    job.sleep(0.8).await?;
                }
            }
        }
    }

    pub fn options(&self) -> &NetOptions {
        &self.options
    }

    /// Make a request, retrying as the reference does, and report progress
    /// on `job`.
    pub async fn fetch(&self, request: &Request, job: &Job) -> Result<Response, NetError> {
        {
            let mut state = job.state.lock();
            state.done = false;
            state.error = false;
        }
        let result = self.fetch_inner(request, job).await;
        if let Err(e) = &result
            && e.is_infrastructure()
        {
            self.report_domain_error(&request.url);
        }
        let mut state = job.state.lock();
        state.done = true;
        state.error = result.is_err();
        // (as the reference's job ends: done, an error status as the server
        // gave it, cancelled, or the error)
        state.status = match &result {
            Ok(_) => "done!".into(),
            Err(NetError::Status { code, .. }) => {
                let reason = reqwest::StatusCode::from_u16(*code)
                    .ok()
                    .and_then(|s| s.canonical_reason())
                    .unwrap_or("");
                format!("{code} - {reason}")
            }
            Err(NetError::Cancelled) => "Cancelled!".into(),
            Err(e) => format!("Error: {e}"),
        };
        result
    }

    async fn fetch_inner(&self, request: &Request, job: &Job) -> Result<Response, NetError> {
        let parts = check_full_url(&request.url)
            .map_err(|e| NetError::Network(format!("Invalid URL {}: {e}", request.url)))?;
        let domain = parts.netloc.clone();
        let registrable = psl::second_level_domain(&domain);
        let scope = job.scope();
        let mut contexts = vec![NetworkContext::global()];
        contexts.extend(
            psl::all_applicable_domains(&domain)
                .into_iter()
                .map(NetworkContext::domain),
        );
        for extra in request.extra_contexts.iter().chain(&scope.contexts) {
            if !contexts.contains(extra) {
                contexts.push(extra.clone());
            }
        }
        // (a file counts against the post page it was found on, too)
        for url in &request.bandwidth_urls {
            for c in Self::contexts_for(url) {
                if !contexts.contains(&c) {
                    contexts.push(c);
                }
            }
        }
        let session = {
            let registrable = NetworkContext::domain(registrable.clone());
            self.store
                .read(|conn| network::session_for(conn, &registrable))
                .map_err(|e| NetError::Io(e.to_string()))?
        };
        let snapshot = self.store.snapshot();
        let sends_range = snapshot
            .url_classes
            .class_for(&request.url)
            .is_none_or(|c| matches!(c.url_type, UrlType::File | UrlType::Unknown));
        let attempt = Attempt {
            request,
            job,
            contexts,
            session,
            sends_range,
        };

        self.wait_while_paused(job).await?;
        self.wait_for_header_approval(&attempt.contexts, job)
            .await?;
        while self.just_woke() {
            job.set_status("looks like computer just woke up, waiting a bit");
            job.sleep(5.0).await?;
        }
        if self.options.obey_bandwidth {
            let override_at = request
                .override_bandwidth_after
                .or(scope.override_after)
                .map(|s| now().saturating_add(i64::try_from(s).unwrap_or(i64::MAX)));
            self.wait_for_bandwidth(
                &attempt.contexts,
                request.method == Method::Get,
                override_at,
                job,
            )
            .await?;
        }
        if !request.one_shot {
            self.wait_for_domain(&request.url, job).await?;
        }
        let gallery_token = scope
            .gallery_token
            .filter(|_| request.gallery_page && self.options.obey_bandwidth);

        job.set_status("waiting for a slot");
        let _slot = tokio::select! {
            slot = Arc::clone(&self.slots).acquire_owned() => slot.expect("never closed"),
            () = job.cancel.cancelled() => return Err(NetError::Cancelled),
        };
        let domain_slots = Arc::clone(
            self.domain_slots
                .lock()
                .entry(registrable.clone())
                .or_insert_with(|| {
                    Arc::new(Semaphore::new(self.options.max_jobs_per_domain.max(1)))
                }),
        );
        let _domain_slot = tokio::select! {
            slot = domain_slots.acquire_owned() => slot.expect("never closed"),
            () = job.cancel.cancelled() => return Err(NetError::Cancelled),
        };

        if let Some(kind) = gallery_token {
            self.wait_for_gallery_token(&registrable, kind, job).await?;
        }
        let mut connection_attempt: u32 = 1;
        let mut request_attempt: u32 = 1;
        let max_requests = match request.method {
            Method::Get => self.options.max_get_attempts,
            Method::Post => 1,
        };
        loop {
            if job.is_cancelled() {
                return Err(NetError::Cancelled);
            }
            let failure = match self.attempt(&attempt).await {
                Ok(response) => return Ok(response),
                Err(failure) => failure,
            };
            let can_retry_request = |n: u32| !request.one_shot && n <= max_requests;
            match failure {
                Failure::Fatal(e) => return Err(e),
                Failure::ServersideBandwidth {
                    message,
                    retry_after,
                } => {
                    request_attempt += 1;
                    if !can_retry_request(request_attempt) {
                        return Err(NetError::Bandwidth(format!(
                            "Server reported very limited bandwidth: {message}"
                        )));
                    }
                    let seconds = retry_after.map_or_else(
                        || {
                            let rating = f64::from(connection_attempt + request_attempt - 1);
                            1.25_f64.powf(rating)
                                * self.options.serverside_bandwidth_wait_time as f64
                        },
                        |s| s as f64,
                    );
                    job.set_status("server reported limited bandwidth - retrying");
                    job.sleep(seconds).await?;
                }
                Failure::Reattempt(message) => {
                    request_attempt += 1;
                    if !can_retry_request(request_attempt) {
                        return Err(NetError::Infrastructure(format!(
                            "Ran out of reattempts on this error: {message}"
                        )));
                    }
                    self.wait_on_connection_error(job, connection_attempt, &message)
                        .await?;
                }
                Failure::Broken => {
                    request_attempt += 1;
                    if !can_retry_request(request_attempt) {
                        return Err(NetError::StreamTimeout(
                            "Unable to complete request--it broke mid-way!".into(),
                        ));
                    }
                    self.wait_on_connection_error(
                        job,
                        connection_attempt,
                        "connection broke mid-request",
                    )
                    .await?;
                }
                Failure::ReadTimeout => {
                    request_attempt += 1;
                    if !can_retry_request(request_attempt) {
                        return Err(NetError::StreamTimeout(
                            "Connection successful, but reading response timed out!".into(),
                        ));
                    }
                    self.wait_on_connection_error(job, connection_attempt, "read timed out")
                        .await?;
                }
                Failure::Connect(fail_text) => {
                    connection_attempt += 1;
                    request_attempt = 1;
                    if request.one_shot || connection_attempt > self.options.max_connection_attempts
                    {
                        return Err(NetError::Connection(fail_text));
                    }
                    self.wait_on_connection_error(job, connection_attempt, "connection failed")
                        .await?;
                }
            }
        }
    }

    async fn wait_on_connection_error(
        &self,
        job: &Job,
        connection_attempt: u32,
        status: &str,
    ) -> Result<(), NetError> {
        let seconds = u64::from(connection_attempt - 1) * self.options.connection_error_wait_time;
        if seconds > 0 {
            job.set_status(format!("{status} - retrying in {seconds} seconds"));
        }
        job.sleep(seconds as f64).await
    }

    /// The headers every hop of an attempt sends (cookies are added per
    /// hop).
    fn headers(&self, a: &Attempt<'_>, bytes_read: u64) -> Result<HeaderMap, NetError> {
        let mut headers: Vec<(String, String)> = vec![("Accept".into(), "*/*".into())];
        let custom = self
            .store
            .read(|conn| {
                a.contexts
                    .iter()
                    .map(|c| network::headers(conn, c))
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| NetError::Io(e.to_string()))?;
        for header in custom.into_iter().flatten() {
            if header.approval == Approval::Approved {
                set_header(&mut headers, &header.name, header.value);
            }
        }
        let snapshot = self.store.snapshot();
        let classes = &snapshot.url_classes;
        if let Some(class) = classes.class_for(&a.request.url) {
            for (name, value) in &class.header_overrides {
                set_header(&mut headers, name, value.clone());
            }
        }
        if a.sends_range {
            set_header(&mut headers, "Range", format!("bytes={bytes_read}-"));
        }
        if let Some(referral) =
            classes.referral_url(&a.request.url, a.request.referral_url.as_deref())
        {
            let referral = if referral.chars().all(|c| u32::from(c) < 256) {
                referral
            } else {
                ensure_url_is_encoded(&referral, true, classes.settings().collapse_leading_slashes)
            };
            set_header(&mut headers, "referer", referral);
        }
        for (name, value) in &a.request.additional_headers {
            set_header(&mut headers, name, value.clone());
        }
        let mut map = HeaderMap::new();
        for (name, value) in headers {
            let header_name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| NetError::Network(format!("Invalid header name {name:?}")))?;
            let header_value =
                HeaderValue::from_bytes(&value.chars().map(|c| c as u8).collect::<Vec<u8>>())
                    .ok()
                    .filter(|_| value.chars().all(|c| u32::from(c) < 256))
                    .ok_or_else(|| {
                        NetError::Network(format!("Invalid header value for {name}: {value:?}"))
                    })?;
            map.insert(header_name, header_value);
        }
        Ok(map)
    }

    fn session_cookies(&self, session: &NetworkContext) -> Result<Vec<network::Cookie>, NetError> {
        self.store
            .read(|conn| network::cookies(conn, session))
            .map_err(|e| NetError::Io(e.to_string()))
    }

    fn keep_cookies(
        &self,
        session: &NetworkContext,
        response: &reqwest::Response,
        url: &reqwest::Url,
    ) -> Result<(), NetError> {
        let set_cookies: Vec<&str> = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect();
        if set_cookies.is_empty() {
            return Ok(());
        }
        let cookie_url = CookieUrl::new(
            url.scheme(),
            url.host_str().unwrap_or_default(),
            url.path(),
            "",
        );
        let changes = set_cookie_changes(&set_cookies, &cookie_url, now());
        let session = session.clone();
        self.store
            .write(move |ctx| {
                for change in &changes {
                    match change {
                        CookieChange::Set(cookie) => {
                            network::set_cookie(ctx.conn(), &session, cookie)?;
                        }
                        CookieChange::Clear { domain, path, name } => {
                            network::clear_cookie(ctx.conn(), &session, domain, path, name)?;
                        }
                    }
                }
                Ok(())
            })
            .map_err(|e| NetError::Io(e.to_string()))
    }

    /// Send the request, following redirects as `requests` does, keeping
    /// each hop's cookies in the job's session.
    async fn send(
        &self,
        a: &Attempt<'_>,
        headers: &HeaderMap,
    ) -> Result<(reqwest::Response, String), Failure> {
        let mut url = a.request.url.clone();
        let mut method = a.request.method;
        let mut body = a.request.body.clone();
        let mut headers = headers.clone();
        for _ in 0..=MAX_REDIRECTS {
            let parsed = reqwest::Url::parse(&url)
                .map_err(|e| NetError::Network(format!("Invalid URL {url}: {e}")))?;
            let mut hop_headers = headers.clone();
            let cookie_url = CookieUrl::new(
                parsed.scheme(),
                parsed.host_str().unwrap_or_default(),
                parsed.path(),
                "",
            );
            if let Some(cookies) =
                cookie_header(&self.session_cookies(&a.session)?, &cookie_url, now())
                && let Ok(value) = HeaderValue::from_str(&cookies)
            {
                hop_headers.insert(reqwest::header::COOKIE, value);
            }
            let mut builder = self
                .client
                .request(
                    match method {
                        Method::Get => reqwest::Method::GET,
                        Method::Post => reqwest::Method::POST,
                    },
                    parsed.clone(),
                )
                .headers(hop_headers);
            if let Some(body) = &body {
                builder = builder.body(body.clone());
            }
            a.job.set_status("sending request\u{2026}");
            let response = tokio::select! {
                r = builder.send() => r.map_err(|e| send_failure(&e))?,
                () = a.job.cancel.cancelled() => return Err(NetError::Cancelled.into()),
            };
            if let Some(len) = body.as_ref().map(Vec::len) {
                tracing::trace!(bytes = len, "request body sent");
            }
            self.keep_cookies(&a.session, &response, &parsed)?;
            let status = response.status().as_u16();
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .map(|l| String::from_utf8_lossy(l.as_bytes()).into_owned());
            let Some(location) = location.filter(|_| matches!(status, 301 | 302 | 303 | 307 | 308))
            else {
                return Ok((response, url));
            };
            let mut next = if location.starts_with("//") {
                format!("{}:{location}", parsed.scheme())
            } else {
                urljoin(&url, &location)
            };
            if !next.contains('#')
                && let Some(fragment) = parsed.fragment()
            {
                next = format!("{next}#{fragment}");
            }
            // `requests`' method rules
            if (status == 303 || status == 302 || (status == 301 && method == Method::Post))
                && method != Method::Get
            {
                method = Method::Get;
            }
            if !matches!(status, 307 | 308) {
                body = None;
                headers.remove(reqwest::header::CONTENT_TYPE);
                headers.remove(reqwest::header::CONTENT_LENGTH);
                headers.remove(reqwest::header::TRANSFER_ENCODING);
            }
            let next_host = reqwest::Url::parse(&next)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned));
            if next_host.as_deref() != parsed.host_str() {
                headers.remove(reqwest::header::AUTHORIZATION);
            }
            url = next;
        }
        Err(NetError::Network(format!("Exceeded {MAX_REDIRECTS} redirects.")).into())
    }

    async fn attempt(&self, a: &Attempt<'_>) -> Result<Response, Failure> {
        let mut sink =
            match &a.request.destination {
                None => Sink::Memory(Vec::new()),
                Some(path) => Sink::File(tokio::fs::File::create(path).await.map_err(|e| {
                    NetError::Io(format!("could not write {}: {e}", path.display()))
                })?),
            };
        let mut progress = Progress {
            accurate: true,
            ..Progress::default()
        };
        {
            let mut state = a.job.state.lock();
            state.bytes_read = 0;
            state.bytes_total = None;
        }
        let headers = self.headers(a, 0)?;
        let (mut response, mut final_url) = self.send(a, &headers).await?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let compressed = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|v| !v.as_bytes().eq_ignore_ascii_case(b"identity"));
        if status >= 400 {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(retry_after_seconds);
            self.read_body(a, response, &mut sink, &mut progress, false)
                .await?;
            let body = match &sink {
                Sink::Memory(bytes) => bytes.clone(),
                Sink::File(_) => Vec::new(),
            };
            return Err(match status_outcome(status, &body) {
                StatusOutcome::Fail(e) => Failure::Fatal(e),
                StatusOutcome::ServersideBandwidth(message) => Failure::ServersideBandwidth {
                    message,
                    retry_after,
                },
                StatusOutcome::Reattempt(message) => Failure::Reattempt(message),
            });
        }
        if !compressed {
            progress.total = response.content_length();
        }
        a.job.set_status("downloading\u{2026}");
        let mut last_modified;
        let mut server;
        loop {
            last_modified = response
                .headers()
                .get(reqwest::header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_last_modified);
            server = response
                .headers()
                .get(reqwest::header::SERVER)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let more = self
                .read_body(
                    a,
                    response,
                    &mut sink,
                    &mut progress,
                    a.request.method == Method::Get,
                )
                .await?;
            if !more {
                break;
            }
            a.job.set_status("downloading next part\u{2026}");
            let headers = self.headers(a, progress.read)?;
            let (next, url) = self.send(a, &headers).await?;
            if next.status().as_u16() >= 400 {
                return Err(NetError::Network(format!(
                    "Ranged response failed {}",
                    next.status().as_u16()
                ))
                .into());
            }
            response = next;
            final_url = url;
        }
        let bytes = match sink {
            Sink::Memory(bytes) => bytes,
            Sink::File(mut file) => {
                file.flush()
                    .await
                    .map_err(|e| NetError::Io(e.to_string()))?;
                Vec::new()
            }
        };
        Ok(Response {
            url: final_url,
            status,
            content_type,
            body: bytes,
            bytes_read: progress.read,
            last_modified,
            server,
        })
    }

    /// Count `bytes` against the request's contexts, and wait while a speed
    /// limit is used up.
    async fn report_data(&self, a: &Attempt<'_>, bytes: u64) -> Result<(), NetError> {
        if !self.options.obey_bandwidth {
            return Ok(());
        }
        let mut last_failed = None;
        loop {
            let now = now();
            {
                let mut b = self.bandwidth.lock();
                if last_failed.is_none() {
                    b.0.report_data(&a.contexts, bytes, now);
                }
                // (it won't have changed within the same second)
                if last_failed != Some(now) {
                    if b.0.can_continue_download(&a.contexts, now) {
                        return Ok(());
                    }
                    last_failed = Some(now);
                }
            }
            a.job.sleep(0.1).await?;
        }
    }

    /// Read a response into the sink. Whether a ranged download has more to
    /// fetch (only asked for successful GETs).
    async fn read_body(
        &self,
        a: &Attempt<'_>,
        mut response: reqwest::Response,
        sink: &mut Sink,
        progress: &mut Progress,
        may_continue: bool,
    ) -> Result<bool, Failure> {
        let read_before = progress.read;
        let mut read_here: u64 = 0;
        let mut expected_here: Option<u64> = None;
        if let Some(range) = response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes "))
            && let Some((span, size)) = range.split_once('/')
        {
            if span != "*"
                && let Some((start, end)) = span.split_once('-')
                && let Ok(start) = start.trim().parse::<u64>()
            {
                if start != progress.read {
                    return Err(NetError::Network(format!(
                        "This server delivered an undesired Range response! We asked for Range \"bytes={}-\" and got Content-Range \"bytes {range}\" back!",
                        progress.read
                    ))
                    .into());
                }
                if let Ok(end) = end.trim().parse::<u64>() {
                    expected_here = Some(end.saturating_sub(start) + 1);
                }
            }
            // the whole file's size, which a partial response's
            // Content-Length is not
            if size != "*"
                && let Ok(size) = size.trim().parse::<u64>()
            {
                progress.total = Some(size);
            }
        }
        {
            let mut state = a.job.state.lock();
            state.bytes_total = progress.total;
        }
        loop {
            let chunk = tokio::select! {
                c = response.chunk() => c,
                () = a.job.cancel.cancelled() => return Err(NetError::Cancelled.into()),
            };
            let chunk = match chunk {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(e) if e.is_timeout() => return Err(Failure::ReadTimeout),
                Err(_) => return Err(Failure::Broken),
            };
            match sink {
                Sink::Memory(bytes) => bytes.extend_from_slice(&chunk),
                Sink::File(file) => file
                    .write_all(&chunk)
                    .await
                    .map_err(|e| NetError::Io(e.to_string()))?,
            }
            let n = chunk.len() as u64;
            progress.read += n;
            read_here += n;
            a.job.state.lock().bytes_read = progress.read;
            a.job.report_read(n);
            self.report_data(a, n).await?;
            if progress.accurate {
                if let Some(total) = progress.total
                    && progress.read > total
                {
                    return Err(NetError::Network(format!(
                        "Too much data: Was expecting {}, but the server continued responding!",
                        human_bytes(total)
                    ))
                    .into());
                }
                if let Some(expected) = expected_here
                    && read_here > expected
                {
                    return Err(NetError::Network(format!(
                        "Too much data: Was expecting {} in this range chunk, but the server continued responding!",
                        human_bytes(expected)
                    ))
                    .into());
                }
            }
        }
        let more = may_continue
            && a.sends_range
            && progress.accurate
            && progress.total.is_some_and(|total| progress.read < total);
        if more {
            if progress.read > read_before {
                progress.empty_chunks = 0;
            } else {
                progress.empty_chunks += 1;
                if progress.empty_chunks > 2 {
                    return Err(NetError::Network(
                        "The server appeared to want to send this URL in ranged chunks, but we got several empty chunks in a row!".into(),
                    )
                    .into());
                }
            }
        }
        Ok(more)
    }
}

/// What a failed send means.
fn send_failure(e: &reqwest::Error) -> Failure {
    let chain = error_chain(e);
    if e.is_connect() {
        let lower = chain.to_lowercase();
        return Failure::Connect(if lower.contains("certificate") {
            format!("Problem with SSL Verification: {chain}")
        } else if lower.contains("tls") || lower.contains("ssl") {
            format!("Problem with SSL: {chain}")
        } else {
            "Could not connect!".to_owned()
        });
    }
    if e.is_timeout() {
        return Failure::ReadTimeout;
    }
    if e.is_body() || e.is_decode() {
        return Failure::Broken;
    }
    if e.is_request() {
        // the connection went away before a response
        return Failure::Connect("Could not connect!".to_owned());
    }
    Failure::Fatal(NetError::Network(chain))
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}

/// `Retry-After`: seconds, or a date (then between a minute and a day).
fn retry_after_seconds(value: &str) -> Option<u64> {
    if let Some(seconds) = hydrus_core::numbers::py_int(value) {
        return u64::try_from(seconds).ok();
    }
    let when = httpdate::parse_http_date(value.trim()).ok()?;
    let when = when.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((when - now()).clamp(60, 86400) as u64)
}

/// A `Last-Modified` time, if it parses and is not absurdly early.
fn parse_last_modified(value: &str) -> Option<i64> {
    let when = httpdate::parse_http_date(value.trim()).ok()?;
    let when = when.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    (when > 86400 * 7).then_some(when)
}
