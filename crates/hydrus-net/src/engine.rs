//! Sending requests as the reference's network jobs do.

use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::{Mutex, RwLock};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::io::AsyncWriteExt as _;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use hydrus_core::bandwidth::{BandwidthType, GalleryTokenKind, Manager, Tracker};
use hydrus_core::numbers::human_bytes_with_figures;
use hydrus_core::url::functions::{check_full_url, ensure_url_is_encoded};
use hydrus_core::url::pyurl::urljoin;
use hydrus_core::url::{UrlType, psl};
use hydrus_store::Store;
use hydrus_store::bandwidth::{BandwidthSettings, HistoryResets};
use hydrus_store::network::{self, Approval, NetworkContext};
use hydrus_store::network_runtime::{self, WaitReason};
use hydrus_store::settings::GuiFormatting;

use crate::cookies::{CookieChange, CookieUrl, cookie_header, set_cookie_changes};
use crate::error::{NetError, StatusOutcome, status_outcome};

/// The client options that shape requests (the reference's defaults).
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Whether clock gaps detect a wake (`do_sleep_check`).
    pub detect_sleep: bool,
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
            detect_sleep: true,
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
            detect_sleep: s.detect_sleep,
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
    /// Login-step HTTP requests bypass demand admission to avoid recursive login.
    pub for_login: bool,
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
            for_login: false,
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
#[derive(Debug, Default, Clone)]
pub struct Job {
    /// Serialize request replacement against commands targeting this handle.
    control: Arc<Mutex<()>>,
    state: Arc<Mutex<JobState>>,
    cancel: CancellationToken,
    override_bandwidth: Arc<AtomicBool>,
    wake: Arc<tokio::sync::Notify>,
    scope: Arc<Mutex<BandwidthScope>>,
    /// The data it has read, by second, for its speed (the reference's
    /// job's own `BandwidthTracker`).
    tracker: Arc<Mutex<Option<Tracker>>>,
    /// Why it was cancelled, if it was.
    cancel_reason: Arc<Mutex<Option<String>>>,
    /// Text returned by the last failed HTTP response, for parser test panels.
    error_text: Arc<Mutex<Option<String>>>,
    skip_wait: Arc<Mutex<Option<WaitReason>>>,
    override_gallery: Arc<AtomicBool>,
    auto_override_at: Arc<std::sync::atomic::AtomicI64>,
    auto_override_owners: Arc<Mutex<std::collections::HashSet<u64>>>,
}

/// What a job is doing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobState {
    pub status: String,
    pub created: i64,
    pub gallery: bool,
    pub tokens_ok: bool,
    pub one_shot: bool,
    /// Typed runtime phase, independent of the displayed status wording.
    pub wait: WaitReason,
    /// Whether this request currently observes startup bandwidth limits.
    pub obeys_bandwidth: bool,
    /// What its importer last said it is doing ("downloading file"), which
    /// the reference's importers report apart from the network job's own
    /// status (their status hooks).
    pub stage: String,
    /// How many times the importer has said so (to tell a new "404" from
    /// the last).
    pub stages: u64,
    /// The URL it is fetching, or last fetched.
    pub url: String,
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
    /// The server's decoded error document, when an HTTP response failed.
    pub fn error_text(&self) -> Option<String> {
        self.error_text.lock().clone()
    }
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A job whose requests count against `scope`.
    pub fn scoped(scope: BandwidthScope) -> Arc<Self> {
        Arc::new(Self {
            scope: Arc::new(Mutex::new(scope)),
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
        self.state_at(now())
    }

    fn state_at(&self, at: i64) -> JobState {
        let mut state = self.state.lock().clone();
        if let Some(tracker) = self.tracker.lock().as_mut() {
            state.speed = tracker.usage(BandwidthType::Data, Some(1), at);
        }
        state
    }

    /// Count `bytes` read towards the job's speed.
    fn report_read(&self, bytes: u64) {
        self.report_read_at(bytes, now());
    }

    fn report_read_at(&self, bytes: u64, now: i64) {
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

    /// Say what the job's importer is doing (it shows as the job's status
    /// too, until the engine says what the request is doing).
    pub fn set_status_text(&self, status: impl Into<String>) {
        let status = status.into();
        let mut state = self.state.lock();
        state.stage.clone_from(&status);
        state.stages += 1;
        state.status = status;
    }

    /// Say what the job's importer is doing, leaving the job's status as
    /// the engine left it (a request's end: "404", "error!").
    pub fn set_stage(&self, stage: impl Into<String>) {
        let mut state = self.state.lock();
        state.stage = stage.into();
        state.stages += 1;
    }

    /// Let this request skip bandwidth limits, waking a pending bandwidth wait.
    pub fn override_bandwidth(&self) {
        self.override_bandwidth.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }

    fn bandwidth_overridden(&self, at: i64) -> bool {
        let deadline = self.auto_override_at.load(Ordering::Relaxed);
        if deadline > 0 && at > deadline {
            // The reference calls OverrideBandwidth, which remains set when
            // its control later turns the automatic policy off.
            self.override_bandwidth.store(true, Ordering::Relaxed);
        }
        self.override_bandwidth.load(Ordering::Relaxed)
    }

    /// Skip only the current connection/server retry delay, leaving future retries intact.
    pub fn override_retry_wait(&self, reason: WaitReason) -> bool {
        if !matches!(reason, WaitReason::Connection | WaitReason::ServerBandwidth)
            || self.state.lock().wait != reason
        {
            return false;
        }
        *self.skip_wait.lock() = Some(reason);
        self.wake.notify_one();
        true
    }
    /// Force this request past the gallery token gate without consuming a token.
    pub fn override_gallery_wait(&self) {
        self.override_gallery.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }
    /// Apply/remove the control's five-second policy for this active request.
    pub fn auto_override_bandwidth(&self, enabled: bool) {
        self.auto_override_bandwidth_for(0, enabled);
    }
    /// Turning off one control leaves another control's policy in effect.
    pub fn auto_override_bandwidth_for(&self, owner: u64, enabled: bool) {
        let mut owners = self.auto_override_owners.lock();
        if enabled {
            owners.insert(owner);
        } else {
            owners.remove(&owner);
        }
        self.auto_override_at.store(
            if owners.is_empty() {
                0
            } else {
                self.state.lock().created.saturating_add(5)
            },
            Ordering::Relaxed,
        );
        self.bandwidth_overridden(now());
        self.wake.notify_one();
    }

    fn set_wait(&self, reason: WaitReason) {
        self.state.lock().wait = reason;
    }

    fn set_status(&self, status: impl Into<String>) {
        self.state.lock().status = status.into();
    }

    async fn sleep(&self, seconds: f64) -> Result<(), NetError> {
        if seconds <= 0.0 {
            return Ok(());
        }
        let reason = self.state.lock().wait;
        let delay = tokio::time::sleep(Duration::from_secs_f64(seconds));
        tokio::pin!(delay);
        loop {
            if self.skip_wait.lock().as_ref() == Some(&reason) {
                self.skip_wait.lock().take();
                return Ok(());
            }
            tokio::select! {
                () = &mut delay => return Ok(()),
                () = self.cancel.cancelled() => return Err(NetError::Cancelled),
                () = self.wake.notified() => {
                    // These gates re-check their predicates after every wake.
                    // Retry sleeps require their own one-shot override flag.
                    if matches!(reason, WaitReason::Bandwidth | WaitReason::Gallery | WaitReason::Domain) { return Ok(()); }
                }
            }
        }
    }
}

// Counters successfully saved together with the reset generations they observed.
type BandwidthCheckpoint = (Vec<(NetworkContext, Tracker)>, HistoryResets);
// Live request handles and the accounting contexts fixed at registration.
type RegisteredJobs = std::collections::BTreeMap<u64, (Job, Vec<NetworkContext>)>;

/// Makes requests with the client's cookies and headers.
#[derive(Debug, Clone)]
pub struct NetEngine {
    client: Arc<RwLock<reqwest::Client>>,
    store: Arc<Store>,
    options: Arc<RwLock<NetOptions>>,
    slots: Arc<RwLock<Arc<Semaphore>>>,
    domain_slots: Arc<Mutex<std::collections::HashMap<String, Arc<Semaphore>>>>,
    /// The bandwidth rules and usage, and when the usage was last saved.
    bandwidth: Arc<Mutex<(Manager, i64, HistoryResets)>>,
    bandwidth_settings: Arc<RwLock<BandwidthSettings>>,
    formatting: Arc<RwLock<GuiFormatting>>,
    /// Last successfully persisted counts and their reset generations. Serializes
    /// saves without holding the live bandwidth-manager lock across store I/O.
    saved_bandwidth: Arc<Mutex<BandwidthCheckpoint>>,
    /// When each domain last had serious errors (`DomainOK`).
    domain_errors: Arc<Mutex<std::collections::HashMap<String, Vec<i64>>>>,
    /// When the sleep check last ran, and (after a wake) when requests may
    /// go again, in ms.
    wake: Arc<Mutex<(Option<i64>, Option<i64>)>>,
    /// When it started, and what it has read since (the reference's
    /// session tracker, `GetMySessionTracker`).
    started: i64,
    session: Arc<Mutex<Tracker>>,
    jobs: Arc<Mutex<RegisteredJobs>>,
    next_job: Arc<AtomicU64>,
    recent_errors: Arc<Mutex<Vec<network_runtime::JobError>>>,
    epoch: String,
    login: Arc<Mutex<Option<Arc<Job>>>>,
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

// Removing registration on Drop also covers an aborted/dropped fetch future.
struct RegisteredJob<'a> {
    engine: &'a NetEngine,
    id: u64,
}
impl Drop for RegisteredJob<'_> {
    fn drop(&mut self) {
        self.engine.jobs.lock().remove(&self.id);
    }
}

// Engine clones own their current control; admission itself belongs to the store.
struct LoginOwner {
    lease: hydrus_store::login_runtime::Lease,
    control: Arc<Mutex<Option<Arc<Job>>>>,
}
impl Drop for LoginOwner {
    fn drop(&mut self) {
        self.control.lock().take();
    }
}

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

/// The HTTP client for these options: its timeouts, HTTPS checks and
/// proxies.
fn http_client(options: &NetOptions) -> Result<reqwest::Client, NetError> {
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
    builder.build().map_err(failed)
}

fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: String) {
    headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
    headers.push((name.to_owned(), value));
}

impl NetEngine {
    /// Every fetch currently owned by this engine, and live usage before its
    /// periodic durable save. Locks only protect snapshots, never store I/O.
    pub fn runtime_snapshot(&self) -> network_runtime::Snapshot {
        let handles = self.jobs.lock();
        let jobs = handles
            .iter()
            .map(|(id, (job, contexts))| {
                let state = job.state();
                network_runtime::NetworkJob {
                    id: *id,
                    url: state.url,
                    status: state.status,
                    wait: state.wait,
                    bytes_read: state.bytes_read,
                    bytes_total: state.bytes_total,
                    speed: state.speed,
                    contexts: contexts.clone(),
                    obeys_bandwidth: state.obeys_bandwidth && !job.bandwidth_overridden(now()),
                }
            })
            .collect();
        drop(handles);
        let controls = self
            .jobs
            .lock()
            .iter()
            .map(|(id, (job, _))| {
                let state = job.state();
                network_runtime::JobControl {
                    id: *id,
                    created: state.created,
                    gallery: state.gallery,
                    domain_ok: state.one_shot || self.domain_ok(&state.url),
                    tokens_ok: state.tokens_ok || job.override_gallery.load(Ordering::Relaxed),
                    auto_override: job.auto_override_at.load(Ordering::Relaxed) > 0,
                }
            })
            .collect();
        let usage = self.bandwidth.lock().0.all_trackers();
        let errors = self.recent_errors.lock().clone();
        let login = self.login_process();
        network_runtime::Snapshot {
            epoch: self.epoch.clone(),
            at: now(),
            jobs,
            usage,
            controls,
            errors,
            login,
        }
    }

    /// Apply a command only to the live request and daemon that were reviewed.
    pub fn runtime_command(&self, command: &network_runtime::Command) -> bool {
        if command.action == network_runtime::JobAction::CancelLogin {
            return hydrus_store::login_runtime::cancel(
                &self.store,
                command.epoch.clone(),
                command.job,
            )
            .unwrap_or(false);
        }
        if command.epoch != self.epoch {
            return false;
        }
        let Some(job) = self
            .jobs
            .lock()
            .get(&command.job)
            .map(|(job, _)| job.clone())
        else {
            return false;
        };
        let _control = job.control.lock();
        // The request may have finished between lookup and taking its handle.
        // A replacement on the same reusable job cannot initialise under this guard.
        if !self.jobs.lock().contains_key(&command.job) {
            return false;
        }
        match command.action {
            network_runtime::JobAction::Cancel => job.cancel(),
            network_runtime::JobAction::CancelLogin => return false,
            network_runtime::JobAction::OverrideBandwidth => job.override_bandwidth(),
            network_runtime::JobAction::OverrideConnectionWait => {
                return job.override_retry_wait(WaitReason::Connection);
            }
            network_runtime::JobAction::OverrideServerBandwidthWait => {
                return job.override_retry_wait(WaitReason::ServerBandwidth);
            }
            network_runtime::JobAction::OverrideGalleryWait => job.override_gallery_wait(),
            network_runtime::JobAction::ScrubDomainErrors => {
                self.scrub_domain_errors(&job.state().url);
            }
            network_runtime::JobAction::AutoOverrideBandwidth(enabled) => {
                job.auto_override_bandwidth(enabled);
            }
            network_runtime::JobAction::AutoOverrideBandwidthFor { owner, enabled } => {
                job.auto_override_bandwidth_for(owner, enabled);
            }
        }
        true
    }

    /// Publish a heartbeat and consume GUI commands through local store IPC.
    /// Call from a blocking worker at most a few times per second.
    pub fn publish_runtime(&self) -> Result<(), NetError> {
        let seen = self.bandwidth.lock().2.clone();
        let mut snapshot = self.runtime_snapshot();
        let (commands, resets) = self
            .store
            .write(move |ctx| {
                let resets = hydrus_store::settings::get::<HistoryResets>(ctx.conn())?;
                snapshot
                    .usage
                    .retain(|(c, _)| resets.generation(c) == seen.generation(c));
                hydrus_store::settings::set(ctx.conn(), &snapshot)?;
                Ok((network_runtime::take_commands(ctx.conn())?, resets))
            })
            .map_err(|e| NetError::Io(e.to_string()))?;
        let reset_usage = {
            let mut bandwidth = self.bandwidth.lock();
            let changed = resets.changed_since(&bandwidth.2);
            let reset_usage = !changed.is_empty();
            bandwidth.0.delete_history(&changed);
            // Never regress a generation when concurrent heartbeat calls overlap.
            for context in changed {
                bandwidth.2.0.retain(|(c, _)| c != &context);
                bandwidth
                    .2
                    .0
                    .push((context.clone(), resets.generation(&context)));
            }
            reset_usage
        };
        if reset_usage {
            for (job, _) in self.jobs.lock().values() {
                job.wake.notify_one();
            }
        }
        for command in commands {
            self.runtime_command(&command);
        }
        Ok(())
    }

    pub fn new(store: Arc<Store>, options: NetOptions) -> Result<Self, NetError> {
        let client = http_client(&options)?;
        let now = now();
        let (bandwidth_settings, usage, history_resets, formatting) = store
            .read(|conn| {
                Ok((
                    hydrus_store::settings::get::<BandwidthSettings>(conn)?,
                    hydrus_store::bandwidth::usage(conn, now)?,
                    hydrus_store::settings::get::<HistoryResets>(conn)?,
                    hydrus_store::settings::get::<GuiFormatting>(conn)?,
                ))
            })
            .map_err(|e| NetError::Io(e.to_string()))?;
        let mut manager = Manager::new(bandwidth_settings.rules.clone());
        manager.set_trackers(usage.clone());
        let saved_bandwidth = (usage, history_resets.clone());
        let epoch = hydrus_store::login_runtime::next_epoch(&store)
            .map_err(|error| NetError::Io(error.to_string()))?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            login: Arc::new(Mutex::default()),
            store,
            slots: Arc::new(RwLock::new(Arc::new(Semaphore::new(
                options.max_jobs.max(1),
            )))),
            domain_slots: Arc::new(Mutex::default()),
            bandwidth: Arc::new(Mutex::new((manager, now, history_resets))),
            bandwidth_settings: Arc::new(RwLock::new(bandwidth_settings)),
            formatting: Arc::new(RwLock::new(formatting)),
            saved_bandwidth: Arc::new(Mutex::new(saved_bandwidth)),
            domain_errors: Arc::new(Mutex::default()),
            wake: Arc::new(Mutex::default()),
            started: now,
            session: Arc::new(Mutex::new(Tracker::new(now))),
            jobs: Arc::new(Mutex::default()),
            next_job: Arc::new(AtomicU64::new(1)),
            recent_errors: Arc::new(Mutex::default()),
            epoch,
            options: Arc::new(RwLock::new(options)),
        })
    }

    /// Use these options from now on, as the reference reads its options
    /// as it goes: requests made from now are made with them (a request
    /// already going keeps its slot and connection).
    pub fn set_options(&self, options: NetOptions) -> Result<(), NetError> {
        let old = self.options();
        if old == options {
            return Ok(());
        }
        let connection = |o: &NetOptions| {
            (
                o.network_timeout,
                o.verify_https,
                o.http_proxy.clone(),
                o.https_proxy.clone(),
                o.no_proxy.clone(),
            )
        };
        if connection(&old) != connection(&options) {
            *self.client.write() = http_client(&options)?;
        }
        if old.max_jobs != options.max_jobs {
            *self.slots.write() = Arc::new(Semaphore::new(options.max_jobs.max(1)));
        }
        if old.max_jobs_per_domain != options.max_jobs_per_domain {
            self.domain_slots.lock().clear();
        }
        *self.options.write() = options;
        Ok(())
    }

    /// Pick up the store's network options and bandwidth rules if they
    /// have changed (the options window, say, changed them); whether
    /// they had.
    pub fn reload_settings(&self) -> Result<bool, NetError> {
        let (network, bandwidth, formatting) = self
            .store
            .read(|conn| {
                Ok((
                    hydrus_store::settings::get::<hydrus_store::network::NetworkSettings>(conn)?,
                    hydrus_store::settings::get::<BandwidthSettings>(conn)?,
                    hydrus_store::settings::get::<GuiFormatting>(conn)?,
                ))
            })
            .map_err(|e| NetError::Io(e.to_string()))?;
        let mut options = NetOptions::from_settings(&network);
        // (whether bandwidth is obeyed isn't an option the store keeps)
        options.obey_bandwidth = self.options.read().obey_bandwidth;
        let mut changed = options != self.options();
        self.set_options(options)?;
        if bandwidth != *self.bandwidth_settings.read() {
            changed = true;
            self.bandwidth
                .lock()
                .0
                .set_all_rules(bandwidth.rules.clone());
            *self.bandwidth_settings.write() = bandwidth;
        }
        if formatting != *self.formatting.read() {
            *self.formatting.write() = formatting;
            changed = true;
            // Repaint waiting labels through their existing predicate loop.
            // Accounting, tokens, deadlines and cancellation remain owned by jobs.
            let jobs = self
                .jobs
                .lock()
                .values()
                .map(|(job, _)| job.clone())
                .collect::<Vec<_>>();
            for job in jobs {
                if matches!(
                    job.state.lock().wait,
                    WaitReason::Bandwidth | WaitReason::Gallery
                ) {
                    job.wake.notify_one();
                }
            }
        }
        Ok(changed)
    }

    fn wait_time(&self, value: i64, now: i64, imminent: &str, no_prefix: bool) -> String {
        if self.formatting.read().iso {
            return hydrus_core::time::timestamp_to_iso(Some(value));
        }
        let delta = value.abs_diff(now);
        if delta <= 2 {
            return imminent.into();
        }
        let span =
            hydrus_core::time::pretty_time_delta(i64::try_from(delta).unwrap_or(i64::MAX), false);
        if value < now {
            format!("{span} ago")
        } else if no_prefix {
            span
        } else {
            format!("in {span}")
        }
    }

    /// When it started, the data it has read since, and in the last second
    /// (the main window's status bar's bandwidth, `REPEATINGBandwidth`).
    pub fn session_usage(&self) -> (i64, u64, u64) {
        let now = now();
        let since = u64::try_from(now - self.started).unwrap_or(0).max(1);
        let mut session = self.session.lock();
        (
            self.started,
            session.usage(BandwidthType::Data, Some(since), now),
            session.usage(BandwidthType::Data, Some(1), now),
        )
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
        let options = self.options.read();
        if !options.detect_sleep {
            *awake_at = None;
            return;
        }
        if last.is_some_and(|t| now - t > 60_000) {
            let delay = i64::try_from(options.wake_delay).unwrap_or(i64::MAX);
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
        let number = self.options.read().domain_error_number;
        if number == 0 {
            return true;
        }
        let Ok(domain) = hydrus_core::url::url_domain(url) else {
            return true;
        };
        let cutoff = now() - self.options.read().domain_error_window;
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

    /// Clear errors for the registrable domain and its parents, as the reference does.
    pub fn scrub_domain_errors(&self, url: &str) {
        let Ok(domain) = hydrus_core::url::url_domain(url) else {
            return;
        };
        let registrable = psl::second_level_domain(&domain);
        let domains = psl::all_applicable_domains(&registrable);
        {
            let mut errors = self.domain_errors.lock();
            for d in &domains {
                errors.remove(d);
            }
        }
        for (job, contexts) in self.jobs.lock().values() {
            if contexts
                .iter()
                .any(|c| c.kind == 2 && domains.contains(&c.data))
            {
                job.wake.notify_one();
            }
        }
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
            job.set_wait(WaitReason::Domain);
            job.set_status("This domain has had several serious errors recently. Waiting a bit.");
            job.sleep(10.0).await?;
        }
        Ok(())
    }

    pub fn bandwidth_settings(&self) -> BandwidthSettings {
        self.bandwidth_settings.read().clone()
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
        !self.options.read().obey_bandwidth
            || self
                .bandwidth
                .lock()
                .0
                .can_do_work(contexts, threshold, now())
    }

    /// Seconds until `contexts`' rules all have room again.
    pub fn waiting_estimate(&self, contexts: &[NetworkContext]) -> u64 {
        if !self.options.read().obey_bandwidth {
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
        let mut saved = self.saved_bandwidth.lock();
        let (current, seen) = {
            let mut bandwidth = self.bandwidth.lock();
            bandwidth.1 = now();
            (bandwidth.0.all_trackers(), bandwidth.2.clone())
        };
        let previous: Vec<_> = saved
            .0
            .iter()
            .filter(|(context, _)| saved.1.generation(context) == seen.generation(context))
            .cloned()
            .collect();
        let to_save = current.clone();
        let previous_seen = seen.clone();
        self.store
            .write(move |ctx| {
                hydrus_store::bandwidth::save_usage_deltas_after_resets(
                    ctx.conn(),
                    &to_save,
                    &previous,
                    &previous_seen,
                    now(),
                )
            })
            .map_err(|e| NetError::Io(e.to_string()))?;
        *saved = (current, seen);
        Ok(())
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
            let auto_at = job.auto_override_at.load(Ordering::Relaxed);
            let override_at = override_at.or((auto_at > 0).then_some(auto_at));
            // POSTs and overridden requests go at once (but still count)
            let obeys =
                obeys && !job.bandwidth_overridden(now) && override_at.is_none_or(|at| now <= at);
            job.state.lock().obeys_bandwidth = obeys;
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
            let when = self.wait_time(until, now, "imminently", false);
            job.set_wait(WaitReason::Bandwidth);
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
            job.set_wait(WaitReason::Paused);
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
            job.set_wait(WaitReason::Headers);
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
        let delay = {
            let s = self.bandwidth_settings.read();
            match kind {
                GalleryTokenKind::DownloadPage => s.gallery_page_wait_pages,
                GalleryTokenKind::Subscription => s.gallery_page_wait_subscriptions,
                GalleryTokenKind::Watcher => s.watcher_page_wait,
            }
        };
        loop {
            if job.override_gallery.load(Ordering::Relaxed) {
                job.state.lock().tokens_ok = true;
                job.set_status("gallery token overridden - starting soon");
                return Ok(());
            }
            let now = now();
            let result = self.bandwidth.lock().0.try_to_consume_gallery_token(
                second_level_domain,
                kind,
                delay,
                now,
            );
            match result {
                Ok(()) => {
                    job.state.lock().tokens_ok = true;
                    job.set_status("gallery token ok - starting soon");
                    return Ok(());
                }
                Err(next) => {
                    let when = self.wait_time(next, now, "checking", true);
                    job.set_wait(WaitReason::Gallery);
                    job.set_status(format!("waiting to start: {when}"));
                    job.sleep(0.8).await?;
                }
            }
        }
    }

    pub fn options(&self) -> NetOptions {
        self.options.read().clone()
    }

    /// Make a request, retrying as the reference does, and report progress
    /// on `job`.
    pub async fn fetch(&self, request: &Request, job: &Job) -> Result<Response, NetError> {
        let id;
        {
            let _control = job.control.lock();
            job.error_text.lock().take();
            {
                let mut state = job.state.lock();
                state.done = false;
                state.error = false;
                state.created = now();
                state.gallery = request.gallery_page;
                state.one_shot = request.one_shot;
                state.tokens_ok = !(request.gallery_page
                    && self.options.read().obey_bandwidth
                    && job.scope().gallery_token.is_some());
                state.status = "initialising…".into();
                state.bytes_read = 0;
                state.bytes_total = None;
                state.speed = 0;
                state.url.clone_from(&request.url);
                state.obeys_bandwidth = self.options.read().obey_bandwidth
                    && request.method == Method::Get
                    && !request.for_login;
            }
            *job.tracker.lock() = Some(Tracker::new(now()));
            job.override_bandwidth.store(false, Ordering::Relaxed);
            job.override_gallery.store(false, Ordering::Relaxed);
            job.auto_override_at.store(0, Ordering::Relaxed);
            job.auto_override_owners.lock().clear();
            job.skip_wait.lock().take();
            job.set_wait(WaitReason::Engine);
            id = self.next_job.fetch_add(1, Ordering::Relaxed);
            let mut contexts = Self::contexts_for(&request.url);
            for context in request.extra_contexts.iter().chain(&job.scope().contexts) {
                if !contexts.contains(context) {
                    contexts.push(context.clone());
                }
            }
            for url in &request.bandwidth_urls {
                for context in Self::contexts_for(url) {
                    if !contexts.contains(&context) {
                        contexts.push(context);
                    }
                }
            }
            self.jobs.lock().insert(id, (job.clone(), contexts));
        }
        let _registered = RegisteredJob { engine: self, id };
        let result = self.fetch_inner(request, job).await;
        if let Ok(response) = &result
            && response.url != request.url
        {
            hydrus_core::debug_flags::report_network(|| {
                format!("Network Jobs Redirect: {} -> {}", request.url, response.url)
            });
        }
        if let Err(e) = &result
            && e.is_infrastructure()
        {
            self.report_domain_error(&request.url);
        }
        if let Err(error) = &result
            && !matches!(error, NetError::Cancelled)
        {
            let contexts = self
                .jobs
                .lock()
                .get(&id)
                .map_or_else(Vec::new, |(_, contexts)| contexts.clone());
            let text = match job.error_text() {
                Some(body) if !body.is_empty() => body,
                _ => error.to_string(),
            };
            hydrus_core::debug_flags::report_network(|| {
                format!("Network error should follow:\n{error}\n{text}")
            });
            let text = if text.chars().count() > 1024 {
                tracing::debug!(error_text = %text.chars().take(512 * 1024).collect::<String>(), "server error detail");
                format!(
                    "The server's error text was too long to display. The first part follows, while a larger chunk has been written to the log.\n{}",
                    text.chars().take(256).collect::<String>()
                )
            } else {
                text
            };
            let mut errors = self.recent_errors.lock();
            errors.push(network_runtime::JobError {
                id,
                url: request.url.clone(),
                contexts,
                gallery: request.gallery_page,
                text,
            });
            if errors.len() > 128 {
                errors.remove(0);
            }
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

    async fn wait_invalid_login(
        domain: &str,
        request: &Request,
        job: &Job,
        error: &str,
    ) -> Result<(), NetError> {
        let subscription = request
            .extra_contexts
            .iter()
            .chain(&job.scope().contexts)
            .any(|context| context.kind == hydrus_core::network::CONTEXT_SUBSCRIPTION);
        if subscription {
            job.cancel_because(&format!(r#"This job's network context "web domain: {domain}" seems to have an invalid login. The error was: {error}"#));
            return Err(NetError::Cancelled);
        }
        job.set_wait(WaitReason::Login);
        job.set_status(error);
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(60)) => Ok(()),
            () = job.cancel.cancelled() => Err(NetError::Cancelled),
        }
    }

    fn login_process(&self) -> Option<network_runtime::LoginProcess> {
        let mut process = hydrus_store::login_runtime::current(&self.store)
            .ok()
            .flatten()?;
        let control = self.login.lock().clone();
        if process.epoch == self.epoch
            && let Some(control) = control
        {
            process.status = control.state().stage;
        }
        Some(process)
    }

    fn try_login_owner(
        &self,
        domain: &str,
        script: &str,
        control: &Arc<Job>,
    ) -> Result<Option<LoginOwner>, NetError> {
        let process = network_runtime::LoginProcess {
            id: self.next_job.fetch_add(1, Ordering::Relaxed),
            domain: domain.into(),
            script: script.into(),
            epoch: self.epoch.clone(),
            status: control.state().stage,
        };
        let lease = hydrus_store::login_runtime::try_acquire(&self.store, process)
            .map_err(|error| NetError::Io(error.to_string()))?;
        Ok(lease.map(|lease| {
            *self.login.lock() = Some(control.clone());
            LoginOwner {
                lease,
                control: self.login.clone(),
            }
        }))
    }

    // Neither the store writer nor an in-memory lock is held across HTTP or waits.
    #[allow(clippy::too_many_arguments)]
    async fn execute_login_owned(
        &self,
        owner: LoginOwner,
        script: &hydrus_parse::login::LoginScript,
        domain: &str,
        credentials: &std::collections::BTreeMap<String, String>,
        control: &Job,
        result_ready: impl FnMut(&crate::login::TestResult),
    ) -> Result<(crate::login::Execution, bool), NetError> {
        let execution = crate::login::execute_with_results(
            self,
            &self.store,
            script,
            domain,
            credentials,
            control,
            result_ready,
        );
        tokio::pin!(execution);
        let mut ticks = tokio::time::interval(Duration::from_millis(50));
        let execution = loop {
            tokio::select! {
                result = &mut execution => break result,
                _ = ticks.tick() => {
                    match owner.lease.pulse(control.state().stage) {
                        Ok(true) => { control.cancel(); },
                        Ok(false) => {},
                        Err(error) => { control.cancel(); return Err(NetError::Io(error.to_string())); },
                    }
                }
            }
        };
        // Save before releasing admission so a queued downloader sees the outcome.
        let outcome = execution.outcome.clone();
        let domain = domain.to_owned();
        let key = script.key.clone();
        let applied = self
            .store
            .write_and_refresh(move |ctx| {
                let mut manager = hydrus_store::logins::load(ctx.conn())?;
                let applied = manager
                    .domains
                    .get_mut(&domain)
                    .is_some_and(|login| outcome.update_domain(login, &key, now()));
                if applied {
                    hydrus_store::logins::save(ctx.conn(), &manager)?;
                }
                Ok(applied)
            })
            .map_err(|error| NetError::Io(error.to_string()))?;
        Ok((execution, applied))
    }

    /// Execute a manual or forced login through the same per-store lease as demand.
    /// Test editors use an isolated store and may keep their direct execution path.
    pub async fn run_login_with_results(
        &self,
        script: &hydrus_parse::login::LoginScript,
        domain: &str,
        credentials: &std::collections::BTreeMap<String, String>,
        control: &Arc<Job>,
        result_ready: impl FnMut(&crate::login::TestResult),
    ) -> Result<crate::login::Execution, NetError> {
        let owner = loop {
            if control.is_cancelled() {
                return Err(NetError::Cancelled);
            }
            if let Some(owner) = self.try_login_owner(domain, &script.name, control)? {
                break owner;
            }
            control.set_wait(WaitReason::Login);
            control.set_status("waiting in login queue…");
            control.sleep(0.05).await?;
        };
        self.execute_login_owned(owner, script, domain, credentials, control, result_ready)
            .await
            .map(|(execution, _)| execution)
    }

    fn wait_for_login<'a>(
        &'a self,
        domain: &'a str,
        request: &'a Request,
        job: &'a Job,
    ) -> futures_util::future::BoxFuture<'a, Result<(), NetError>> {
        Box::pin(async move {
            use crate::login::{Demand, Outcome, demand};
            loop {
                if job.is_cancelled() {
                    return Err(NetError::Cancelled);
                }
                if self
                    .login_process()
                    .is_some_and(|active| active.domain == domain)
                {
                    job.set_wait(WaitReason::Login);
                    job.set_status("waiting in login queue…");
                    job.sleep(0.05).await?;
                    continue;
                }
                match demand(&self.store, domain, now()).map_err(NetError::Io)? {
                    Demand::None => return Ok(()),
                    Demand::Blocked { error, .. } => {
                        Self::wait_invalid_login(domain, request, job, &error).await?;
                    }
                    Demand::Ready(login) => {
                        job.set_wait(WaitReason::Login);
                        job.set_status("waiting in login queue…");
                        let control = Job::new();
                        let Some(owner) =
                            self.try_login_owner(&login.domain, &login.script.name, &control)?
                        else {
                            job.sleep(0.05).await?;
                            continue;
                        };
                        let expected_domain = login.domain;
                        let expected_key = login.script.key;
                        let Demand::Ready(login) =
                            demand(&self.store, domain, now()).map_err(NetError::Io)?
                        else {
                            continue;
                        };
                        if login.domain != expected_domain || login.script.key != expected_key {
                            continue;
                        }
                        job.set_status("logging in…");
                        // The guard exists before spawning, including an unpolled
                        // worker dropped with its runtime. Cancelling the trigger
                        // detaches the process; only its reviewed owner cancels it.
                        let mut task = tokio::spawn({
                            let engine = self.clone();
                            async move {
                                let (execution, applied) = engine
                                    .execute_login_owned(
                                        owner,
                                        &login.script,
                                        &login.domain,
                                        &login.credentials,
                                        &control,
                                        |_| {},
                                    )
                                    .await?;
                                Ok::<_, NetError>(applied.then_some(execution.outcome))
                            }
                        });
                        let outcome = tokio::select! {
                            result = &mut task => result.map_err(|error| NetError::Network(error.to_string()))??,
                            () = job.cancel.cancelled() => return Err(NetError::Cancelled),
                        };
                        let reason = match outcome {
                            Some(
                                Outcome::Verification(error)
                                | Outcome::FinalCookies(error)
                                | Outcome::Network(error)
                                | Outcome::Unusual(error),
                            ) => Some(error),
                            Some(Outcome::Cancelled) => {
                                Some("User cancelled the login process.".into())
                            }
                            _ => None,
                        };
                        if let Some(reason) = reason {
                            Self::wait_invalid_login(
                                domain,
                                request,
                                job,
                                &format!("The domain \"{domain}\" cannot log in: {reason}"),
                            )
                            .await?;
                        }
                    }
                }
            }
        })
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
            job.set_wait(WaitReason::Wake);
            job.set_status("looks like computer just woke up, waiting a bit");
            job.sleep(5.0).await?;
        }
        if self.options.read().obey_bandwidth {
            let override_at = request
                .override_bandwidth_after
                .or(scope.override_after)
                .map(|s| now().saturating_add(i64::try_from(s).unwrap_or(i64::MAX)));
            self.wait_for_bandwidth(
                &attempt.contexts,
                request.method == Method::Get && !request.for_login,
                override_at,
                job,
            )
            .await?;
        }
        if !request.one_shot {
            self.wait_for_domain(&request.url, job).await?;
        }
        if !request.for_login {
            self.wait_for_login(&domain, request, job).await?;
        }
        let gallery_token = scope
            .gallery_token
            .filter(|_| request.gallery_page && self.options.read().obey_bandwidth);

        job.set_wait(WaitReason::Engine);
        job.set_status("waiting for a slot");
        let _slot = tokio::select! {
            slot = Arc::clone(&self.slots.read()).acquire_owned() => slot.expect("never closed"),
            () = job.cancel.cancelled() => return Err(NetError::Cancelled),
        };
        let domain_slots = Arc::clone(
            self.domain_slots
                .lock()
                .entry(registrable.clone())
                .or_insert_with(|| {
                    Arc::new(Semaphore::new(
                        self.options.read().max_jobs_per_domain.max(1),
                    ))
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
            Method::Get => self.options.read().max_get_attempts,
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
                                * self.options.read().serverside_bandwidth_wait_time as f64
                        },
                        |s| s as f64,
                    );
                    job.set_wait(WaitReason::ServerBandwidth);
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
                    if request.one_shot
                        || connection_attempt > self.options.read().max_connection_attempts
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
        let seconds =
            u64::from(connection_attempt - 1) * self.options.read().connection_error_wait_time;
        if seconds > 0 {
            job.set_wait(WaitReason::Connection);
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
                .read()
                .clone()
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
            a.job.set_wait(WaitReason::Downloading);
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
            *a.job.error_text.lock() = Some(crate::text::decode(&body, content_type.as_deref()));
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
        a.job.set_wait(WaitReason::Downloading);
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
            a.job.set_wait(WaitReason::Downloading);
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
        if !self.options.read().obey_bandwidth {
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
                if a.job.bandwidth_overridden(now)
                    || (last_failed != Some(now) && b.0.can_continue_download(&a.contexts, now))
                {
                    a.job.set_wait(WaitReason::Downloading);
                    return Ok(());
                }
                last_failed = Some(now);
            }
            a.job.set_wait(WaitReason::Bandwidth);
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
            self.session.lock().report_data(n, now());
            self.report_data(a, n).await?;
            if progress.accurate {
                if let Some(total) = progress.total
                    && progress.read > total
                {
                    return Err(NetError::Network(format!(
                        "Too much data: Was expecting {}, but the server continued responding!",
                        human_bytes_with_figures(total, self.formatting.read().figures)
                    ))
                    .into());
                }
                if let Some(expected) = expected_here
                    && read_here > expected
                {
                    return Err(NetError::Network(format!(
                        "Too much data: Was expecting {} in this range chunk, but the server continued responding!",
                        human_bytes_with_figures(expected, self.formatting.read().figures)
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

#[cfg(test)]
mod reload_tests {
    use super::*;
    use hydrus_store::network::NetworkSettings;

    #[test]
    fn saved_formatting_replays_actual_backend_waits_and_reopens_without_changing_rules() {
        let fixture = hydrus_testkit::fixture_json("gui_format_backend.json");
        let (dir, store, engine) = engine();
        let original_options = engine.options();
        let original_slots = engine.slots.read().clone();
        let original_rules = engine.bandwidth_settings();
        let now = fixture["now"].as_i64().unwrap();
        for event in fixture["events"].as_array().unwrap() {
            let formatting: GuiFormatting = serde_json::from_value(event["saved"].clone()).unwrap();
            store
                .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &formatting))
                .unwrap();
            engine.reload_settings().unwrap();
            let outputs = &event["consumers"];
            assert_eq!(
                format!(
                    "bandwidth free {}… (global)",
                    engine.wait_time(now + 90, now, "imminently", false)
                ),
                outputs["bandwidth"]
            );
            assert_eq!(
                format!(
                    "overriding bandwidth {}… (global)",
                    engine.wait_time(now + 45, now, "imminently", false)
                ),
                outputs["override"]
            );
            assert_eq!(
                format!(
                    "waiting to start: {}",
                    engine.wait_time(now + 90, now, "checking", true)
                ),
                outputs["gallery"]
            );
            assert_eq!(
                format!(
                    "bandwidth free {}… (global)",
                    engine.wait_time(now + 2, now, "imminently", false)
                ),
                outputs["imminently"]
            );
            assert_eq!(
                format!(
                    "waiting to start: {}",
                    engine.wait_time(now + 2, now, "checking", true)
                ),
                outputs["checking"]
            );
            assert_eq!(
                format!(
                    "bandwidth free {}… (global)",
                    engine.wait_time(now + 3, now, "imminently", false)
                ),
                outputs["bandwidth_three"]
            );
            assert_eq!(
                format!(
                    "waiting to start: {}",
                    engine.wait_time(now + 3, now, "checking", true)
                ),
                outputs["gallery_three"]
            );
            assert!(!engine.reload_settings().unwrap());
            assert_eq!(engine.options(), original_options);
            assert!(Arc::ptr_eq(&original_slots, &engine.slots.read()));
            assert_eq!(engine.bandwidth_settings(), original_rules);
            let reopened = Store::open(dir.path()).unwrap();
            let restarted = NetEngine::new(reopened, original_options.clone()).unwrap();
            assert_eq!(*restarted.formatting.read(), *engine.formatting.read());
        }
    }

    #[tokio::test]
    async fn formatting_refresh_keeps_a_bandwidth_wait_and_its_usage_and_cancel_reason() {
        use hydrus_core::bandwidth::{Rule, Rules};
        let (_dir, store, engine) = engine();
        store
            .write(|ctx| {
                let rules = BandwidthSettings {
                    rules: vec![(
                        NetworkContext::global(),
                        Rules::new([Rule::new(BandwidthType::Requests, Some(3600), 1)]),
                    )],
                    ..Default::default()
                };
                hydrus_store::settings::set(ctx.conn(), &rules)
            })
            .unwrap();
        engine.reload_settings().unwrap();
        let contexts = vec![NetworkContext::global()];
        let started = now();
        engine.bandwidth.lock().0.report_request(&contexts, started);
        let trackers = engine.bandwidth.lock().0.all_trackers();
        let (seconds, _) = engine
            .bandwidth
            .lock()
            .0
            .waiting_estimate_and_context(&contexts, started);
        let target = started + i64::try_from(seconds).unwrap();
        let job = Job::new();
        engine
            .jobs
            .lock()
            .insert(1, ((*job).clone(), contexts.clone()));
        let waiting = engine.wait_for_bandwidth(&contexts, true, None, &job);
        tokio::pin!(waiting);
        tokio::select! { result = &mut waiting => panic!("bandwidth wait completed: {result:?}"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        let before = job.state.lock().clone();
        assert_eq!(before.wait, WaitReason::Bandwidth);
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &GuiFormatting {
                        iso: true,
                        figures: 2,
                    },
                )
            })
            .unwrap();
        engine.reload_settings().unwrap();
        tokio::select! { result = &mut waiting => panic!("formatting bypassed bandwidth: {result:?}"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        let mut after = job.state.lock().clone();
        assert_eq!(
            after.status,
            format!(
                "bandwidth free {}… (global)",
                hydrus_core::time::timestamp_to_iso(Some(target))
            )
        );
        after.status = before.status.clone();
        assert_eq!(after, before);
        assert_eq!(engine.bandwidth.lock().0.all_trackers(), trackers);
        job.cancel_because("formatting regression");
        assert!(matches!(waiting.await, Err(NetError::Cancelled)));
        assert!(job.cancelled_note().contains("formatting regression"));
    }

    #[tokio::test]
    async fn formatting_refresh_wakes_a_pending_gallery_label_and_preserves_its_token_and_job() {
        let (_dir, store, engine) = engine();
        store
            .write(|ctx| {
                let mut rules: BandwidthSettings = hydrus_store::settings::get(ctx.conn())?;
                rules.gallery_page_wait_pages = 3600;
                hydrus_store::settings::set(ctx.conn(), &rules)
            })
            .unwrap();
        engine.reload_settings().unwrap();
        let kind = GalleryTokenKind::DownloadPage;
        let started = now();
        engine
            .bandwidth
            .lock()
            .0
            .try_to_consume_gallery_token("format.example", kind, 3600, started)
            .unwrap();
        let target = engine
            .bandwidth
            .lock()
            .0
            .try_to_consume_gallery_token("format.example", kind, 3600, started)
            .unwrap_err();
        let job = Job::new();
        job.state.lock().bytes_read = 1536;
        job.state.lock().bytes_total = Some(243_200);
        engine.jobs.lock().insert(1, ((*job).clone(), Vec::new()));
        let waiting = engine.wait_for_gallery_token("format.example", kind, &job);
        tokio::pin!(waiting);
        tokio::select! { result = &mut waiting => panic!("pending token completed: {result:?}"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        let before = job.state.lock().clone();
        assert_eq!(before.wait, WaitReason::Gallery);
        assert!(!before.tokens_ok);
        store
            .write(|ctx| {
                hydrus_store::settings::set(
                    ctx.conn(),
                    &GuiFormatting {
                        iso: true,
                        figures: 6,
                    },
                )
            })
            .unwrap();
        assert!(engine.reload_settings().unwrap());
        tokio::select! { result = &mut waiting => panic!("formatting consumed token: {result:?}"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        let mut after = job.state.lock().clone();
        assert_eq!(
            after.status,
            format!(
                "waiting to start: {}",
                hydrus_core::time::timestamp_to_iso(Some(target))
            )
        );
        after.status = before.status.clone();
        assert_eq!(after, before, "formatting changed work state");
        assert_eq!(
            engine
                .bandwidth
                .lock()
                .0
                .try_to_consume_gallery_token("format.example", kind, 3600, now())
                .unwrap_err(),
            target
        );
        job.cancel();
        assert!(matches!(waiting.await, Err(NetError::Cancelled)));
    }

    #[test]
    fn job_speed_samples_the_current_second_and_expires_at_rollover() {
        let job = Job::new();
        job.report_read_at(400, 100);
        job.report_read_at(600, 101);
        job.state.lock().bytes_read = 1000;
        assert_eq!(job.state_at(100).speed, 400);
        assert_eq!(job.state_at(101).speed, 600);
        assert_eq!(job.state_at(102).speed, 0);
        assert_eq!(job.state_at(102).bytes_read, 1000);
    }

    fn engine() -> (tempfile::TempDir, Arc<Store>, NetEngine) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let settings: NetworkSettings = store.read(hydrus_store::settings::get).unwrap();
        let mut options = NetOptions::from_settings(&settings);
        options.obey_bandwidth = false;
        let engine = NetEngine::new(Arc::clone(&store), options).unwrap();
        (dir, store, engine)
    }

    #[test]
    fn automatic_control_override_has_strict_five_second_boundary_and_sticks() {
        // Recorded from the reference's NetworkJobControl
        // (oracle/record_network_job_control.py): a job that obeys bandwidth
        // rules still does at created + 5 s, not after, and "override
        // bandwidth rules for this job" stops it obeying.
        let recorded = hydrus_testkit::fixture_json("network_job_control.json");
        let created = recorded["now"].as_i64().unwrap();
        let obeys = |name: &str| recorded["actions"][name].as_bool().unwrap();
        let job = Job::new();
        job.state.lock().created = created;
        job.auto_override_bandwidth(true);
        assert_eq!(!job.bandwidth_overridden(created + 5), obeys("auto_at_five"));
        assert_eq!(
            !job.bandwidth_overridden(created + 6),
            obeys("auto_after_five")
        );
        // Turning the policy off later leaves the override in place.
        job.auto_override_bandwidth(false);
        assert!(job.bandwidth_overridden(created + 7));
        let manual = Job::new();
        manual.override_bandwidth();
        assert_eq!(
            !manual.bandwidth_overridden(created),
            obeys("obeys_after_override")
        );
    }

    #[test]
    fn independent_auto_override_owners_do_not_disable_each_other() {
        let job = Job::new();
        job.state.lock().created = now();
        job.auto_override_bandwidth_for(1, true);
        job.auto_override_bandwidth_for(2, true);
        job.auto_override_bandwidth_for(1, false);
        assert!(job.auto_override_at.load(Ordering::Relaxed) > 0);
        job.auto_override_bandwidth_for(2, false);
        assert_eq!(job.auto_override_at.load(Ordering::Relaxed), 0);
        assert!(!job.override_bandwidth.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn runtime_domain_scrub_wakes_a_registered_domain_gate_and_preserves_unrelated_errors() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let engine = NetEngine::new(
            store,
            NetOptions {
                domain_error_number: 1,
                ..NetOptions::default()
            },
        )
        .unwrap();
        let url = "https://sub.example.com/controlled";
        // The reference scrub clears a registrable domain and its parents,
        // so a subdomain job is blocked by this parent's recorded error.
        engine.report_domain_error("https://example.com/parent-error");
        engine.report_domain_error("https://other.example.net/unrelated");
        assert!(!engine.domain_ok(url));
        let job = Job::new();
        {
            let mut state = job.state.lock();
            state.url = url.into();
            state.created = now();
        }
        let contexts = NetEngine::contexts_for(url);
        engine.jobs.lock().insert(42, ((*job).clone(), contexts));
        let mut waiting = Box::pin(engine.wait_for_domain(url, &job));
        tokio::select! { _ = &mut waiting => panic!("domain gate unexpectedly passed"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        assert_eq!(job.state().wait, WaitReason::Domain);
        assert!(engine.runtime_command(&network_runtime::Command {
            epoch: engine.epoch.clone(),
            job: 42,
            action: network_runtime::JobAction::ScrubDomainErrors
        }));
        tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .unwrap()
            .unwrap();
        assert!(engine.domain_ok(url));
        assert!(!engine.domain_ok("https://other.example.net/unrelated"));
    }

    #[tokio::test]
    async fn connection_retry_skip_is_consumed_once_and_unrelated_wakes_do_not_skip() {
        let job = Job::new();
        job.set_wait(WaitReason::Connection);
        let mut sleep = Box::pin(job.sleep(60.0));
        job.override_bandwidth();
        tokio::select! { _ = &mut sleep => panic!("bandwidth override skipped connection delay"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        assert!(!job.override_retry_wait(WaitReason::ServerBandwidth));
        assert!(job.override_retry_wait(WaitReason::Connection));
        tokio::time::timeout(Duration::from_secs(1), sleep)
            .await
            .unwrap()
            .unwrap();
        let mut next = Box::pin(job.sleep(60.0));
        tokio::select! { _ = &mut next => panic!("skip leaked to the next retry"), () = tokio::time::sleep(Duration::from_millis(10)) => {} }
        job.cancel();
        assert_eq!(next.await.unwrap_err(), NetError::Cancelled);
    }

    #[test]
    fn sleep_detection_obeys_threshold_delay_and_disabled_pending_wait() {
        // Replays Controller.SleepCheck as recorded in system_sleep_options.json
        // (oracle/record_system_sleep_options.py): the check runs at `T` with
        // its last check `gap_ms` earlier, then every 15 s through the delay
        // and once more just after it.
        let recorded = hydrus_testkit::fixture_json("system_sleep_options.json");
        let (_dir, _store, engine) = engine();
        let t = 1_700_000_000_000_i64;
        let set = |enabled: bool, delay: u64| {
            let mut options = engine.options();
            options.detect_sleep = enabled;
            options.wake_delay = delay;
            engine.set_options(options).unwrap();
        };
        for case in recorded["cases"].as_array().unwrap() {
            let delay = case["delay"].as_u64().unwrap();
            let gap = case["gap_ms"].as_i64().unwrap();
            set(case["enabled"].as_bool().unwrap(), delay);
            *engine.wake.lock() = (Some(t - gap), None);
            engine.sleep_check_at(t);
            let (last, awake_at) = *engine.wake.lock();
            assert_eq!(awake_at.is_some(), case["detected"], "{case}");
            assert_eq!(awake_at.unwrap_or(0), case["deadline_ms"], "{case}");
            assert_eq!(last == Some(t), case["last_check_touched"], "{case}");
            let delay_ms = i64::try_from(delay * 1000).unwrap();
            for elapsed in (15_000..=delay_ms).step_by(15_000) {
                engine.sleep_check_at(t + elapsed);
            }
            engine.sleep_check_at(t + delay_ms + 1);
            assert_eq!(engine.wake.lock().1.is_some(), case["after_delay"], "{case}");
        }
        // A wait still pending when the option is turned off ends at the next check.
        let pending = &recorded["disabled_pending"];
        set(true, 60);
        *engine.wake.lock() = (Some(t - 61_000), None);
        engine.sleep_check_at(t);
        assert_eq!(engine.wake.lock().1.is_some(), pending["before"]);
        set(false, 60);
        engine.sleep_check_at(t);
        assert_eq!(engine.wake.lock().1.is_some(), pending["after"]);
    }

    #[test]
    fn changed_options_are_picked_up() {
        let (_dir, store, engine) = engine();
        assert!(!engine.reload_settings().unwrap(), "nothing changed");
        assert!(
            engine
                .bandwidth
                .lock()
                .0
                .all_rules()
                .iter()
                .any(|(_, rules)| !rules.is_empty()),
            "(hydrus's default rules, to begin with)"
        );
        let slots = Arc::clone(&engine.slots.read());
        engine
            .domain_slots
            .lock()
            .insert("example.com".into(), Arc::new(Semaphore::new(3)));
        store
            .write(|ctx| {
                let mut network: NetworkSettings = hydrus_store::settings::get(ctx.conn())?;
                network.max_jobs = 2;
                network.max_jobs_per_domain = 1;
                network.max_connection_attempts = 9;
                network.http_proxy = Some("http://127.0.0.1:1".into());
                hydrus_store::settings::set(ctx.conn(), &network)?;
                let mut bandwidth: BandwidthSettings = hydrus_store::settings::get(ctx.conn())?;
                bandwidth.gallery_page_wait_pages = 61;
                bandwidth.rules = vec![];
                hydrus_store::settings::set(ctx.conn(), &bandwidth)
            })
            .unwrap();
        assert!(engine.reload_settings().unwrap());
        let options = engine.options();
        assert_eq!(
            (
                options.max_jobs,
                options.max_jobs_per_domain,
                options.max_connection_attempts
            ),
            (2, 1, 9)
        );
        assert_eq!(options.http_proxy.as_deref(), Some("http://127.0.0.1:1"));
        // (bandwidth is still not obeyed: that isn't the store's to say)
        assert!(!options.obey_bandwidth);
        // new slots for the new number; the domains' made anew as needed
        assert!(!Arc::ptr_eq(&slots, &engine.slots.read()));
        assert_eq!(engine.slots.read().available_permits(), 2);
        assert!(engine.domain_slots.lock().is_empty());
        assert_eq!(engine.bandwidth_settings().gallery_page_wait_pages, 61);
        assert!(
            engine
                .bandwidth
                .lock()
                .0
                .all_rules()
                .iter()
                .all(|(_, rules)| rules.is_empty()),
            "the rules are the store's"
        );
        assert!(!engine.reload_settings().unwrap(), "and not again");
    }

    #[test]
    fn options_unchanged_keep_their_slots() {
        let (_dir, _store, engine) = engine();
        let slots = Arc::clone(&engine.slots.read());
        let mut options = engine.options();
        options.max_get_attempts += 1;
        engine.set_options(options).unwrap();
        assert!(Arc::ptr_eq(&slots, &engine.slots.read()));
    }
}
