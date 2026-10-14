//! Native-only network features, driven from the window through the store's
//! IPC to a running `NetEngine` daemon and back. The reference has no
//! counterpart for these (its jobs panel has no cancel or override, and its
//! bandwidth panel offers no typed domain), so there is nothing to record:
//! each test proves the whole path with a real engine fetching from a
//! loopback server.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use axum::{Router, routing::get};
use hydrus_core::{
    bandwidth::{BandwidthType, Rule, Rules},
    network::NetworkContext,
};
use hydrus_gui::{headless, network_data_window as windows};
use hydrus_net::{Job, NetEngine, NetError, NetOptions, Request, Response};
use hydrus_store::{
    Store,
    bandwidth::BandwidthSettings,
    network_runtime::{self, Snapshot, WaitReason},
    settings,
};
use slint::Model as _;

/// The windows share one process-wide clock that the expiry test holds still;
/// every test here that opens a window takes this first.
static CLOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn clock() -> std::sync::MutexGuard<'static, ()> {
    CLOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < Duration::from_secs(15));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A real engine, its heartbeat worker (what the application runs) and a
/// loopback server, over a real store.
struct Daemon {
    runtime: tokio::runtime::Runtime,
    engine: Arc<NetEngine>,
    store: Arc<Store>,
    url: String,
    stop: Arc<AtomicBool>,
    heartbeat: Option<std::thread::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}

type Fetch = tokio::task::JoinHandle<Result<Response, NetError>>;

impl Daemon {
    fn start(obey_bandwidth: bool) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        // Requests name a dotted host and reach the loopback server as its
        // proxy, so the typed-domain window (which takes no port) can name
        // the context the engine counts them against.
        let proxy = format!("http://{}", listener.local_addr().unwrap());
        let url = "http://files.example.com/echo".to_owned();
        let app = Router::new().route("/echo", get(|| async { "ok" }));
        runtime.spawn(async move { axum::serve(listener, app).await.unwrap() });
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        // (in the store, so a settings reload keeps it)
        let network = hydrus_store::network::NetworkSettings {
            http_proxy: Some(proxy),
            no_proxy: None,
            network_timeout: 5,
            ..hydrus_store::network::NetworkSettings::default()
        };
        store
            .write({
                let network = network.clone();
                move |ctx| settings::set(ctx.conn(), &network)
            })
            .unwrap();
        let engine = Arc::new(
            NetEngine::new(
                store.clone(),
                NetOptions {
                    obey_bandwidth,
                    ..NetOptions::from_settings(&network)
                },
            )
            .unwrap(),
        );
        let mut daemon = Self {
            runtime,
            engine,
            store,
            url,
            stop: Arc::default(),
            heartbeat: None,
            _dir: dir,
        };
        daemon.start_heartbeat();
        daemon
    }

    fn start_heartbeat(&mut self) {
        self.stop.store(false, Ordering::SeqCst);
        let (engine, stop) = (self.engine.clone(), self.stop.clone());
        self.heartbeat = Some(std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let _ = engine.publish_runtime();
                std::thread::sleep(Duration::from_millis(20));
            }
        }));
    }

    fn stop_heartbeat(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.heartbeat.take() {
            thread.join().unwrap();
        }
    }

    fn fetch(&self) -> Fetch {
        let engine = self.engine.clone();
        let request = Request::get(self.url.clone());
        self.runtime
            .spawn(async move { engine.fetch(&request, &Job::new()).await })
    }

    fn finished(&self, fetch: Fetch) -> Result<Response, NetError> {
        until(|| fetch.is_finished());
        self.runtime.block_on(fetch).unwrap()
    }

    fn set<S: settings::Setting + Send + 'static>(&self, value: S) {
        self.store
            .write(move |ctx| settings::set(ctx.conn(), &value))
            .unwrap();
    }

    fn snapshot(&self) -> Snapshot {
        self.store.read(settings::get::<Snapshot>).unwrap()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop_heartbeat();
    }
}

fn refresh_until(jobs: &hydrus_gui::NetworkJobsWindow, mut condition: impl FnMut() -> bool) {
    until(|| {
        jobs.invoke_refresh();
        condition()
    });
}

fn select_only_row(jobs: &hydrus_gui::NetworkJobsWindow, wait: &str) {
    refresh_until(jobs, || jobs.get_rows().row_count() == 1);
    jobs.invoke_row_clicked(0, false, false);
    assert!(jobs.get_can_act());
    assert!(
        jobs.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(2)
            .unwrap()
            .contains(wait),
        "status column should show the wait"
    );
}

// leaf: audit-network-jobs-cancel
#[test]
fn cancelling_a_selected_request_stops_the_engines_fetch_and_clears_the_row() {
    let _clock = clock();
    let _headless = headless::init();
    let daemon = Daemon::start(false);
    daemon.set(settings::Pauses {
        network_traffic: true,
        ..settings::Pauses::default()
    });
    let slots = windows::Slots::default();
    let fetch = daemon.fetch();
    let jobs = windows::open_jobs(daemon.store.clone(), &slots).unwrap();
    select_only_row(&jobs, "");
    assert_eq!(daemon.snapshot().jobs[0].wait, WaitReason::Paused);
    jobs.invoke_cancel_jobs();
    assert!(matches!(daemon.finished(fetch), Err(NetError::Cancelled)));
    refresh_until(&jobs, || jobs.get_rows().row_count() == 0);
    assert!(!jobs.get_can_act());
    jobs.invoke_close_clicked();
}

// leaf: audit-network-jobs-override
#[test]
fn overriding_a_selected_request_releases_the_bandwidth_wait_and_counts_usage() {
    let _clock = clock();
    let _headless = headless::init();
    let daemon = Daemon::start(true);
    daemon.set(BandwidthSettings {
        rules: vec![(
            NetworkContext::global(),
            Rules::new([Rule::new(BandwidthType::Requests, Some(3600), 1)]),
        )],
        ..BandwidthSettings::default()
    });
    assert!(daemon.engine.reload_settings().unwrap());
    daemon.runtime.block_on(daemon.fetch()).unwrap().unwrap();
    let fetch = daemon.fetch();
    let slots = windows::Slots::default();
    let jobs = windows::open_jobs(daemon.store.clone(), &slots).unwrap();
    refresh_until(&jobs, || {
        daemon
            .snapshot()
            .jobs
            .first()
            .is_some_and(|j| j.wait == WaitReason::Bandwidth)
            && jobs.get_rows().row_count() == 1
    });
    jobs.invoke_row_clicked(0, false, false);
    assert!(!fetch.is_finished(), "the rule holds the second request");
    jobs.invoke_override_jobs();
    daemon.finished(fetch).unwrap();
    daemon.engine.save_bandwidth().unwrap();
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let (_, mut global) = daemon
        .store
        .read(|conn| hydrus_store::bandwidth::usage(conn, now))
        .unwrap()
        .into_iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap();
    assert_eq!(global.usage(BandwidthType::Requests, None, now), 2);
    refresh_until(&jobs, || jobs.get_rows().row_count() == 0);
    jobs.invoke_close_clicked();
}

// leaf: audit-network-jobs-expiry
#[test]
fn a_silent_daemon_disables_the_command_controls_after_five_seconds() {
    let _clock = clock();
    let _headless = headless::init();
    let mut daemon = Daemon::start(false);
    daemon.set(settings::Pauses {
        network_traffic: true,
        ..settings::Pauses::default()
    });
    let slots = windows::Slots::default();
    let fetch = daemon.fetch();
    let jobs = windows::open_jobs(daemon.store.clone(), &slots).unwrap();
    select_only_row(&jobs, "");
    daemon.stop_heartbeat();
    let at = daemon.snapshot().at;
    // The last heartbeat is what the window judges against; hold its clock.
    windows::hold_now(Some(at + 5));
    refresh_until(&jobs, || jobs.get_rows().row_count() == 1);
    assert!(jobs.get_can_act(), "five seconds old is still current");
    windows::hold_now(Some(at + 6));
    jobs.invoke_refresh();
    until(|| !jobs.get_can_act());
    assert!(jobs.get_status().contains("offline"));
    jobs.invoke_cancel_jobs();
    jobs.invoke_override_jobs();
    assert!(
        daemon
            .store
            .write(|ctx| network_runtime::take_commands(ctx.conn()))
            .unwrap()
            .is_empty(),
        "an expired window sends nothing to a daemon that may be gone"
    );
    windows::hold_now(None);
    // A daemon that returns re-enables the controls on the same request.
    daemon.start_heartbeat();
    refresh_until(&jobs, || {
        jobs.get_can_act() || jobs.get_rows().row_count() == 1
    });
    jobs.invoke_row_clicked(0, false, false);
    until(|| jobs.get_can_act());
    jobs.invoke_cancel_jobs();
    assert!(matches!(daemon.finished(fetch), Err(NetError::Cancelled)));
    jobs.invoke_close_clicked();
}

// leaf: audit-network-bandwidth-domain
#[test]
fn a_typed_domain_override_limits_the_engines_next_request_to_that_domain() {
    let _clock = clock();
    let _headless = headless::init();
    let daemon = Daemon::start(true);
    let slots = windows::Slots::default();
    let domain = match NetEngine::contexts_for(&daemon.url).pop() {
        Some(context) if context != NetworkContext::global() => context,
        other => panic!("no domain context for the loopback URL: {other:?}"),
    };
    let name = domain.data.clone();
    let review = windows::open_bandwidth(daemon.store.clone(), &slots).unwrap();
    review.set_domain(name.clone().into());
    // (the click does nothing until the window's first review has loaded)
    until(|| {
        review.invoke_domain_clicked();
        windows::last_rules().is_some()
    });
    let edit = windows::last_rules().unwrap();
    edit.set_requests(true);
    edit.set_amount("1".into());
    edit.set_seconds("3600".into());
    edit.set_monthly(false);
    edit.invoke_add_rule();
    edit.invoke_apply_clicked();
    until(|| windows::last_rules().is_none());
    // The engine picks the saved rule up as the application's settings poll does.
    assert!(daemon.engine.reload_settings().unwrap());
    daemon.runtime.block_on(daemon.fetch()).unwrap().unwrap();
    let held = daemon.fetch();
    let jobs = windows::open_jobs(daemon.store.clone(), &slots).unwrap();
    refresh_until(&jobs, || {
        daemon
            .snapshot()
            .jobs
            .first()
            .is_some_and(|j| j.wait == WaitReason::Bandwidth && j.contexts.contains(&domain))
    });
    assert!(!held.is_finished());
    // The rule the window saved for the typed domain is the one in force.
    let saved = daemon.engine.bandwidth_settings();
    let (_, rules) = saved.rules.iter().find(|(c, _)| c == &domain).unwrap();
    assert!(
        rules
            .rules()
            .iter()
            .any(|r| r.kind == BandwidthType::Requests
                && r.max_allowed == 1
                && r.time_delta == Some(3600))
    );
    held.abort();
    review.invoke_close_clicked();
    jobs.invoke_close_clicked();
}

fn package_text() -> String {
    hydrus_testkit::fixture_json("downloader_interchange.json")["reference"].to_string()
}

/// The recorded classes and parsers of `parser_auto_links.json` as one package.
fn linkable_package_text() -> String {
    let fixture = hydrus_testkit::fixture_json("parser_auto_links.json");
    let wrap = |key: &str| {
        fixture[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|object| serde_json::json!([2, object]))
            .collect::<Vec<_>>()
    };
    let mut objects = wrap("classes");
    objects.extend(wrap("parsers"));
    serde_json::json!([26, 3, objects]).to_string()
}

fn staged_package(
    store: &Arc<Store>,
    slots: &hydrus_gui::downloader_interchange_window::Slots,
) -> hydrus_gui::DownloaderExchangeWindow {
    staged_text(store, slots, package_text())
}

fn staged_text(
    store: &Arc<Store>,
    slots: &hydrus_gui::downloader_interchange_window::Slots,
    text: String,
) -> hydrus_gui::DownloaderExchangeWindow {
    let import = hydrus_gui::downloader_interchange_window::package(store, slots, true).unwrap();
    import.set_text(text.into());
    import.invoke_action("review".into());
    assert!(import.get_ready(), "{}", import.get_error());
    import.invoke_action("accept".into());
    assert!(import.get_question_asks(), "{}", import.get_question());
    import
}

// leaf: audit-network-exchange-commit
// Native-only: the reference's import has no conflict-safe save (it writes
// each object as it goes); the reference's duplicate rules are replayed in
// the model tests against oracle/fixtures/downloader_interchange.json.
#[test]
fn a_package_imported_in_the_window_saves_new_keys_members_and_links_and_a_stale_review_saves_nothing()
 {
    use hydrus_core::url::{AnyGug, UrlClassSettings};
    use hydrus_parse::Downloaders;
    let _clock = clock();
    let _headless = headless::init();
    let slots = hydrus_gui::downloader_interchange_window::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let import = staged_package(&store, &slots);
    // nothing is written until the question is answered
    assert!(
        store
            .read::<Downloaders>(settings::get)
            .unwrap()
            .parsers
            .is_empty()
    );
    import.invoke_action("yes".into());
    assert!(
        import.get_question().starts_with("Successfully added"),
        "{} {}",
        import.get_question(),
        import.get_error()
    );
    import.invoke_action("ok".into());
    let saved: Downloaders = store.read(settings::get).unwrap();
    let single = saved
        .gugs
        .gugs
        .iter()
        .find(|g| g.name() == "exchange gallery")
        .expect("the package's gallery generator");
    assert_ne!(
        single.key(),
        "11".repeat(32),
        "imported objects get new keys"
    );
    let nested = saved
        .gugs
        .gugs
        .iter()
        .find_map(|g| match g {
            AnyGug::Nested(n) if n.name == "exchange nested" => Some(n),
            _ => None,
        })
        .expect("the nested generator");
    assert_eq!(
        nested.gugs,
        vec![(single.key().to_owned(), single.name().to_owned())],
        "nested members follow their new keys"
    );
    let classes: UrlClassSettings = store.read(settings::get).unwrap();
    assert!(!classes.url_classes.is_empty());
    // A package whose parser examples match its classes gets those links.
    let dir = tempfile::tempdir().unwrap();
    let linked_store = Store::open(dir.path()).unwrap();
    let linked = staged_text(&linked_store, &slots, linkable_package_text());
    linked.invoke_action("yes".into());
    linked.invoke_action("ok".into());
    let downloaders: Downloaders = linked_store.read(settings::get).unwrap();
    let classes: UrlClassSettings = linked_store.read(settings::get).unwrap();
    let direct = classes
        .url_classes
        .iter()
        .find(|c| c.name == "direct")
        .expect("the direct class");
    let direct_parser = downloaders
        .parsers
        .iter()
        .find(|p| p.name == "direct parser")
        .expect("the direct parser");
    assert!(
        classes
            .parser_links
            .contains(&(hex::encode(&direct.key), Some(direct_parser.key.clone()))),
        "{:?}",
        classes.parser_links
    );
    // The same package again adds only what is new (the nested generator).
    let again = staged_package(&store, &slots);
    again.invoke_action("yes".into());
    again.invoke_action("ok".into());
    let twice: Downloaders = store.read(settings::get).unwrap();
    assert_eq!(twice.parsers.len(), saved.parsers.len());
    assert_eq!(twice.gugs.gugs.len(), saved.gugs.gugs.len() + 1);

    // Another editor saves between the review and the answer: nothing is saved.
    let dir = tempfile::tempdir().unwrap();
    let other = Store::open(dir.path()).unwrap();
    let stale = staged_package(&other, &slots);
    other
        .write_and_refresh(|ctx| {
            let mut classes: UrlClassSettings = settings::get(ctx.conn())?;
            classes.collapse_leading_slashes = true;
            settings::set(ctx.conn(), &classes)
        })
        .unwrap();
    stale.invoke_action("yes".into());
    assert!(!stale.get_error().is_empty(), "the stale save is refused");
    let kept: Downloaders = other.read(settings::get).unwrap();
    assert!(kept.parsers.is_empty() && kept.gugs.gugs.is_empty());
    let classes: UrlClassSettings = other.read(settings::get).unwrap();
    assert!(classes.url_classes.is_empty() && classes.collapse_leading_slashes);
}
