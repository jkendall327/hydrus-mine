//! End-to-end user flows, replayed against what the reference did
//! (`oracle/record_flow_*.py`, `oracle/flows.py`; `docs/rust/FLOWS.md`
//! says how to add one).
//!
//! A flow drives the headless GUI on a real store, as a user would, while
//! the work runs as `hydrus serve` runs it ([`Daemon`]), against the fake
//! site the recording was made on ([`Site`]). After each step it waits for
//! both to settle ([`wait_until_idle`]), reads what the user can see
//! ([`State`]: the subscriptions and their logs, the files with their tags
//! and URLs, the popups, the site's requests), normalised as the recorder
//! normalised it, and compares it with the recording ([`compare`]).
//!
//! Differences the flow is known to show are listed with the flow, each
//! with the `DIFFERENCES.md` entry that explains it; one that no longer
//! differs fails the flow too, so the list stays true.

mod subscription;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State as AxumState;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{Value as Json, json};
use slint::Model as _;
use std::sync::Mutex;

use hydrus_gui::MainWindow;
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use hydrus_store::queues::{self, FileSeed, GallerySeed};
use hydrus_store::subscriptions as subs;

// ---------------------------------------------------------------- the store

/// The `basic` fixture, imported as a migration would, as the recordings
/// start from it. Keep the directories for as long as the store is used.
pub fn basic_store() -> ([tempfile::TempDir; 2], Arc<Store>) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    ([legacy, native], store)
}

/// What `flows.install` did in the reference: the recorded domain manager
/// (the site's URL classes, parsers and GUG) installed as a migration
/// installs it, new network traffic unpaused, and no wait between a
/// subscription's gallery pages.
pub fn install(store: &Store, recording: &Json) {
    use hydrus_legacy::serialisable::SerialisableObject;
    let domain_manager =
        SerialisableObject::from_tuple_str(&recording["domain_manager"].to_string()).unwrap();
    let url_classes = hydrus_legacy::objects::domain::url_class_settings(&domain_manager).unwrap();
    let downloaders = hydrus_legacy::objects::parsers::downloaders(&domain_manager).unwrap();
    assert!(
        downloaders.unconverted.is_empty(),
        "{:?}",
        downloaders.unconverted
    );
    store
        .write_and_refresh(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &url_classes)?;
            hydrus_store::settings::set(ctx.conn(), &downloaders)?;
            let mut bandwidth: hydrus_store::bandwidth::BandwidthSettings =
                hydrus_store::settings::get(ctx.conn())?;
            bandwidth.gallery_page_wait_subscriptions = 0;
            hydrus_store::settings::set(ctx.conn(), &bandwidth)?;
            let mut pauses: hydrus_store::settings::Pauses =
                hydrus_store::settings::get(ctx.conn())?;
            pauses.network_traffic = false;
            pauses.subscriptions = false;
            hydrus_store::settings::set(ctx.conn(), &pauses)
        })
        .unwrap();
}

// ---------------------------------------------------------------- the site

/// The recording's fake site, served on its port in phases, noting every
/// request.
pub struct Site {
    phases: Vec<(HashMap<String, String>, HashMap<String, String>)>,
    pub base: String,
    phase: AtomicUsize,
    hits: Mutex<Vec<String>>,
    runtime: Mutex<Option<tokio::runtime::Runtime>>,
}

async fn serve(AxumState(site): AxumState<Arc<Site>>, uri: Uri) -> Response {
    let phase = site.phase.load(Ordering::SeqCst);
    let path = uri.path().to_owned();
    site.hits.lock().unwrap().push(path.clone());
    let (pages, files) = &site.phases[phase];
    if let Some(html) = pages.get(&path) {
        return ([("content-type", "text/html; charset=utf-8")], html.clone()).into_response();
    }
    if let Some(name) = files.get(&path) {
        let bytes = std::fs::read(hydrus_testkit::fixture_path(format!("media/{name}"))).unwrap();
        return ([("content-type", "image/jpeg")], bytes).into_response();
    }
    (
        StatusCode::NOT_FOUND,
        [("content-type", "text/html; charset=utf-8")],
        "not found",
    )
        .into_response()
}

fn string_map(value: &Json) -> HashMap<String, String> {
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect()
}

impl Site {
    /// Serve the recording's `site` (its port and phases), from phase 0.
    pub fn serve(recording: &Json) -> Arc<Self> {
        let port = u16::try_from(recording["site"]["port"].as_u64().unwrap()).unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let site = Arc::new(Self {
            phases: recording["site"]["phases"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (string_map(&p["pages"]), string_map(&p["files"])))
                .collect(),
            base: format!("http://127.0.0.1:{port}"),
            phase: AtomicUsize::new(0),
            hits: Mutex::default(),
            runtime: Mutex::new(None),
        });
        let app = Router::new().fallback(serve).with_state(Arc::clone(&site));
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind(("127.0.0.1", port)))
            .unwrap_or_else(|e| panic!("the recording's site port {port}: {e}"));
        runtime.spawn(async move { axum::serve(listener, app).await.unwrap() });
        *site.runtime.lock().unwrap() = Some(runtime);
        site
    }

    /// Run the GUI's timers until the site has been asked for something,
    /// within `seconds`: work the reference starts at once must start
    /// promptly here too.
    pub fn wait_for_a_request(&self, seconds: u64, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        while self.hits.lock().unwrap().is_empty() {
            assert!(
                Instant::now() < deadline,
                "{what}: nothing was requested within {seconds} seconds"
            );
            slint::platform::update_timers_and_animations();
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn set_phase(&self, phase: usize) {
        self.phase.store(phase, Ordering::SeqCst);
    }

    /// The paths requested since last asked, sorted (as the recorder
    /// takes them: the order of a query's pages and files can vary).
    pub fn take_hits(&self) -> Vec<String> {
        let mut hits = std::mem::take(&mut *self.hits.lock().unwrap());
        hits.sort();
        hits
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let runtime = self
            .runtime
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(runtime) = runtime {
            runtime.shutdown_timeout(Duration::from_secs(5));
        }
    }
}

// ---------------------------------------------------------------- the daemon

/// The work `hydrus serve` does for the client, in this process: the
/// download queues, the subscription runner, and the client's nudges
/// taken every second, as `serve` takes them.
pub struct Daemon {
    runtime: Option<tokio::runtime::Runtime>,
    state: Arc<hydrus_api::AppState>,
    store: Arc<Store>,
}

impl Daemon {
    pub fn start(store: &Arc<Store>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let state = hydrus_api::AppState::new(Arc::clone(store)).unwrap();
        {
            let _entered = runtime.enter();
            let downloads = state.downloads.clone().expect("the network engine starts");
            downloads.start_all().unwrap();
            let subscriptions = state.subscriptions.clone();
            if let Some(runner) = &subscriptions {
                runner.start();
            }
            runtime.spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    if let Err(e) = downloads.take_nudges(subscriptions.as_ref()) {
                        panic!("acting on nudged queues failed: {e}");
                    }
                }
            });
        }
        Self {
            runtime: Some(runtime),
            state,
            store: Arc::clone(store),
        }
    }

    /// Nothing running, nothing nudged and waiting to be taken, and no
    /// subscription due within the minute (one just made, or told to
    /// check now, is due at once).
    pub fn idle(&self) -> bool {
        let runner = self.state.subscriptions.as_ref().unwrap();
        if !runner.status().active.is_empty() {
            return false;
        }
        let downloads = self.state.downloads.as_ref().unwrap();
        if !downloads.live().is_empty() {
            return false;
        }
        if self.store.read(queues::any_nudged).unwrap() {
            return false;
        }
        let soon = hydrus_core::time::TimestampMs::now().millis() / 1000 + 60;
        let downloader = downloads.downloader();
        self.store
            .read(subs::subscriptions)
            .unwrap()
            .iter()
            .all(|sub| {
                downloader
                    .next_work_time(sub)
                    .unwrap()
                    .is_none_or(|t| t >= soon)
            })
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            let entered = runtime.enter();
            if let Some(runner) = &self.state.subscriptions {
                runner.shutdown();
                let _ = runtime.block_on(tokio::time::timeout(
                    Duration::from_secs(15),
                    runner.wait_stopped(),
                ));
            }
            drop(entered);
            runtime.shutdown_timeout(Duration::from_secs(5));
        }
    }
}

// ---------------------------------------------------------------- waiting

/// Run the GUI's timers until the daemon is idle and `also` holds, both
/// for a moment and a half together (as `flows.wait_until_idle` waits).
pub fn wait_until_idle(daemon: &Daemon, also: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut quiet_since: Option<Instant> = None;
    while Instant::now() < deadline {
        slint::platform::update_timers_and_animations();
        if daemon.idle() && also() {
            let since = *quiet_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(1500) {
                return;
            }
        } else {
            quiet_since = None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for the client and its daemon to go idle");
}

// ---------------------------------------------------------------- the end state

/// What makes texts comparable across runs, as `flows.normaliser` does it:
/// the site is `BASE`, dates `TIME`, and a time just past ("now", "5
/// seconds ago") `DELTA`.
pub struct Normaliser {
    base: String,
    date_time: regex::Regex,
    delta: regex::Regex,
}

impl Normaliser {
    pub fn new(site: &Site) -> Self {
        Self {
            base: site.base.clone(),
            date_time: regex::Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap(),
            delta: regex::Regex::new(
                r"\b(\d+ (year|month|day|hour|minute|second)s?( \d+ (year|month|day|hour|minute|second)s?)*( ago)?|just now|now)\b",
            )
            .unwrap(),
        }
    }

    pub fn text(&self, text: &str) -> String {
        let text = text.replace(&self.base, "BASE");
        let text = self.date_time.replace_all(&text, "TIME");
        self.delta.replace_all(&text, "DELTA").into_owned()
    }
}

/// What the user can see after a step, shaped as the recorder writes it.
pub struct State<'a> {
    pub store: &'a Store,
    pub ui: &'a MainWindow,
    pub site: &'a Site,
    pub n: &'a Normaliser,
}

impl State<'_> {
    fn file_seed(&self, f: &FileSeed) -> Json {
        json!({
            "data": self.n.text(&f.data),
            "status": f.status.code(),
            "note": self.n.text(&f.note),
            "tags": f.meta.tags,
            "sha256": f.meta.hash("sha256"),
        })
    }

    fn gallery_seed(&self, g: &GallerySeed) -> Json {
        json!({
            "url": self.n.text(&g.url),
            "status": g.status.code(),
            "note": self.n.text(&g.note),
        })
    }

    /// `flows.subscriptions_state`.
    pub fn subscriptions(&self) -> Json {
        let mut out = Vec::new();
        for sub in self.store.read(subs::subscriptions).unwrap() {
            let mut queries = Vec::new();
            for q in self.store.read(|c| subs::queries(c, sub.id)).unwrap() {
                let files = self
                    .store
                    .read(|c| queues::file_seeds(c, q.queue_id))
                    .unwrap();
                let searches = self
                    .store
                    .read(|c| queues::gallery_seeds(c, q.queue_id))
                    .unwrap();
                queries.push(json!({
                    "text": q.state.query_text,
                    "paused": q.state.paused,
                    "dead": q.state.dead,
                    "checked": q.state.last_check_time > 0,
                    "check_now": q.state.check_now,
                    "files": files.iter().map(|f| self.file_seed(f)).collect::<Vec<_>>(),
                    "searches": searches.iter().map(|g| self.gallery_seed(g)).collect::<Vec<_>>(),
                }));
            }
            out.push(json!({
                "name": sub.name,
                "downloader": sub.settings.gug_name,
                "paused": sub.settings.paused,
                "queries": queries,
            }));
        }
        Json::Array(out)
    }

    /// `flows.files_state`, for the files in `subscriptions`' logs.
    pub fn files(&self, subscriptions: &Json) -> Json {
        use hydrus_core::service::ServiceType;
        let mut hashes = BTreeSet::new();
        for s in subscriptions.as_array().unwrap() {
            for q in s["queries"].as_array().unwrap() {
                for f in q["files"].as_array().unwrap() {
                    if let Some(h) = f["sha256"].as_str() {
                        hashes.insert(h.to_owned());
                    }
                }
            }
        }
        let snapshot = self.store.snapshot();
        let mut out = serde_json::Map::new();
        for hash in hashes {
            let record = self
                .store
                .read(|conn| {
                    let sha =
                        hydrus_core::Sha256::from_slice(&hex::decode(&hash).unwrap()).unwrap();
                    let id = hydrus_store::master::hash_id(conn, &sha)?.unwrap();
                    let batch = hydrus_store::media::load(conn, &snapshot.services, None, &[id])?;
                    let result = &batch.results[0];
                    let mut tags = BTreeMap::new();
                    for service in snapshot.services.all() {
                        if service.service_type() != ServiceType::LocalTag {
                            continue;
                        }
                        let mut current: Vec<String> = result
                            .tags
                            .get(&service.id)
                            .and_then(|t| t.by_status.get(&hydrus_core::ContentStatus::Current))
                            .map(|t| {
                                t.iter()
                                    .map(|t| batch.tags[t].as_str().to_owned())
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !current.is_empty() {
                            current.sort();
                            tags.insert(service.name.clone(), current);
                        }
                    }
                    let mut urls: Vec<String> =
                        result.urls.iter().map(|u| self.n.text(u)).collect();
                    urls.sort();
                    let mut domains: Vec<String> = result
                        .current
                        .iter()
                        .filter_map(|c| snapshot.services.get(c.service).ok())
                        .filter(|s| {
                            matches!(
                                s.service_type(),
                                ServiceType::LocalFileDomain | ServiceType::LocalFileTrashDomain
                            )
                        })
                        .map(|s| s.name.clone())
                        .collect();
                    domains.sort();
                    Ok(json!({
                        "tags": tags,
                        "urls": urls,
                        "inbox": result.inbox,
                        "in": domains,
                    }))
                })
                .unwrap();
            out.insert(hash, record);
        }
        Json::Object(out)
    }

    /// The popups the main window shows, oldest first (`flows.Popups.take`).
    pub fn popups(&self) -> Json {
        let popups = self.ui.get_popups();
        Json::Array(
            (0..popups.row_count())
                .map(|i| {
                    let p = popups.row_data(i).unwrap();
                    json!({
                        "title": self.n.text(&p.title),
                        "text_1": self.n.text(&p.text_1),
                        "text_2": self.n.text(&p.text_2),
                        "files": (!p.files.is_empty()).then(|| p.files.to_string()),
                        "error": !p.traceback.is_empty(),
                    })
                })
                .collect(),
        )
    }

    /// A step's state: `step` and `dialog` (what the flow saw in its
    /// dialogs) as given, then what the user sees.
    pub fn step(&self, step: &str, dialog: &Json) -> Json {
        let subscriptions = self.subscriptions();
        json!({
            "step": step,
            "dialog": dialog,
            "files": self.files(&subscriptions),
            "subscriptions": subscriptions,
            "popups": self.popups(),
            "hits": self.site.take_hits(),
        })
    }
}

// ---------------------------------------------------------------- comparing

/// Every place two JSON values differ: a path (`/steps/0/files/…`) and
/// both values.
pub fn differences(path: &str, ours: &Json, theirs: &Json, out: &mut Vec<(String, String)>) {
    match (ours, theirs) {
        (Json::Object(a), Json::Object(b)) => {
            let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for k in keys {
                differences(
                    &format!("{path}/{k}"),
                    a.get(k).unwrap_or(&Json::Null),
                    b.get(k).unwrap_or(&Json::Null),
                    out,
                );
            }
        }
        (Json::Array(a), Json::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                differences(&format!("{path}/{i}"), x, y, out);
            }
        }
        _ if ours == theirs => {}
        _ => out.push((
            path.to_owned(),
            format!("hydrus-rs: {ours}\n    reference: {theirs}"),
        )),
    }
}

/// A difference a flow is known to show: paths starting with `path`, and
/// why (the `DIFFERENCES.md` entry).
pub struct Known {
    pub path: &'static str,
    pub why: &'static str,
}

/// Compare a flow's steps with the recording's. Every difference must be
/// [`Known`], and every known difference must still show.
pub fn compare(ours: &[Json], recording: &Json, known: &[Known]) {
    let theirs = recording["steps"].as_array().unwrap();
    assert_eq!(
        ours.iter().map(|s| s["step"].clone()).collect::<Vec<_>>(),
        theirs.iter().map(|s| s["step"].clone()).collect::<Vec<_>>(),
        "the flow's steps"
    );
    let mut found = Vec::new();
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        differences(
            &format!("/steps/{i}({})", b["step"].as_str().unwrap()),
            a,
            b,
            &mut found,
        );
    }
    let unexpected: Vec<String> = found
        .iter()
        .filter(|(path, _)| !known.iter().any(|k| path.contains(k.path)))
        .map(|(path, what)| format!("{path}\n    {what}"))
        .collect();
    let gone: Vec<String> = known
        .iter()
        .filter(|k| !found.iter().any(|(path, _)| path.contains(k.path)))
        .map(|k| format!("{} ({})", k.path, k.why))
        .collect();
    assert!(
        unexpected.is_empty() && gone.is_empty(),
        "the flow differs from the reference:\n{}\n\nknown differences no longer shown: {gone:?}",
        unexpected.join("\n")
    );
}
