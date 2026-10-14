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
    let _headless = headless::init();
    let daemon = Daemon::start(false);
    daemon.set(settings::Pauses {
        network_traffic: true,
        ..settings::Pauses::default()
    });
    let slots = windows::Slots::default();
    let fetch = daemon.fetch();
    let jobs = windows::open_jobs(daemon.store.clone(), &slots).unwrap();
    select_only_row(&jobs, "all new network traffic is paused");
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
    let _headless = headless::init();
    let mut daemon = Daemon::start(false);
    daemon.set(settings::Pauses {
        network_traffic: true,
        ..settings::Pauses::default()
    });
    let slots = windows::Slots::default();
    let fetch = daemon.fetch();
    let jobs = windows::open_jobs(daemon.store.clone(), &slots).unwrap();
    select_only_row(&jobs, "all new network traffic is paused");
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
    let _headless = headless::init();
    let daemon = Daemon::start(true);
    let slots = windows::Slots::default();
    let domain = match NetEngine::contexts_for(&daemon.url).pop() {
        Some(context) if context != NetworkContext::global() => context,
        other => panic!("no domain context for the loopback URL: {other:?}"),
    };
    let name = domain.data.clone();
    let review = windows::open_bandwidth(daemon.store.clone(), &slots).unwrap();
    // Text that is not a bare domain opens nothing and says why.
    for bad in ["localhost", "http://files.example.com", "files.example.com/x", "a b.com", ""] {
        review.set_domain(bad.into());
        review.invoke_domain_clicked();
        assert!(windows::last_rules().is_none(), "{bad:?}");
        assert!(
            review
                .get_status()
                .contains("Enter a domain without a scheme or path."),
            "{bad:?}"
        );
    }
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
    // The inherited default domain rule (one request a second) would let the
    // request through after a second; only the typed rule holds it longer.
    let start = std::time::Instant::now();
    until(|| start.elapsed() > Duration::from_millis(2500));
    assert!(!held.is_finished(), "the typed one-an-hour rule still holds it");
    assert_eq!(daemon.snapshot().jobs[0].wait, WaitReason::Bandwidth);
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

/// The reference's package, as the recording imported it.
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

/// What the store holds, in the recording's terms: generators with their keys
/// and nested members, class and parser names, and parser links by name.
fn state(store: &Store) -> serde_json::Value {
    use hydrus_core::url::{AnyGug, UrlClassSettings};
    use hydrus_parse::Downloaders;
    let downloaders: Downloaders = store.read(settings::get).unwrap();
    let classes: UrlClassSettings = store.read(settings::get).unwrap();
    let mut gugs = downloaders.gugs.gugs.iter().collect::<Vec<_>>();
    gugs.sort_by_key(|g| g.name().to_owned());
    let mut class_names = classes
        .url_classes
        .iter()
        .map(|c| c.name.clone())
        .collect::<Vec<_>>();
    class_names.sort();
    let mut parser_names = downloaders
        .parsers
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>();
    parser_names.sort();
    let mut links = classes
        .parser_links
        .iter()
        .filter_map(|(class, parser)| {
            let class = classes
                .url_classes
                .iter()
                .find(|c| hex::encode(&c.key) == *class)?;
            let parser = downloaders.parser(parser.as_ref()?)?;
            Some(serde_json::json!([class.name, parser.name]))
        })
        .collect::<Vec<_>>();
    links.sort_by_key(ToString::to_string);
    serde_json::json!({
        "gugs": gugs.iter().map(|g| match g {
            AnyGug::Nested(n) => serde_json::json!({"name": n.name, "members": n.gugs}),
            _ => serde_json::json!({"name": g.name()}),
        }).collect::<Vec<_>>(),
        "classes": class_names,
        "parsers": parser_names,
        "links": links,
    })
}

fn names(state: &serde_json::Value, key: &str) -> Vec<String> {
    state[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().or(v.as_str()).unwrap().to_owned())
        .collect()
}

// leaf: audit-network-exchange-commit
// The import's message, duplicate rule, new keys and parser links are
// replayed from oracle/fixtures/package_import.json (the reference's
// ReviewDownloaderImport on the same packages). The conflict-safe save at
// the end is native-only: the reference writes each object as it goes.
#[test]
fn a_package_imported_in_the_window_matches_the_recorded_import_and_a_stale_review_saves_nothing() {
    use hydrus_core::url::UrlClassSettings;
    use hydrus_parse::Downloaders;
    let recording = hydrus_testkit::fixture_json("package_import.json");
    let steps = recording["steps"].as_array().unwrap();
    let _headless = headless::init();
    let slots = hydrus_gui::downloader_interchange_window::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let texts = [package_text(), package_text(), linkable_package_text()];
    let before: Downloaders = store.read(settings::get).unwrap();
    assert!(before.parsers.is_empty() && before.gugs.gugs.is_empty());
    for (step, text) in steps.iter().zip(texts) {
        let label = step["step"].as_str().unwrap();
        let import = staged_text(&store, &slots, text);
        // The final question, word for word (the reference lists the objects
        // it is about to add and link).
        assert_eq!(
            import.get_question().as_str(),
            step["questions"][0].as_str().unwrap(),
            "{label}"
        );
        if label == "package" {
            // nothing is written until the question is answered
            let unchanged: Downloaders = store.read(settings::get).unwrap();
            assert!(unchanged.parsers.is_empty(), "{label}");
        }
        import.invoke_action("yes".into());
        assert_eq!(
            import.get_question().as_str(),
            step["notices"][0].as_str().unwrap(),
            "{label}: {}",
            import.get_error()
        );
        import.invoke_action("ok".into());
        let recorded = &step["state"];
        let ours = state(&store);
        for key in ["gugs", "classes", "parsers"] {
            assert_eq!(names(&ours, key), names(recorded, key), "{label}: {key}");
        }
        assert_eq!(ours["links"], recorded["links"], "{label}: links");
    }
    // Imported generators get new keys, as the recording's do.
    let saved: Downloaders = store.read(settings::get).unwrap();
    let single = saved
        .gugs
        .gugs
        .iter()
        .find(|g| g.name() == "exchange gallery")
        .unwrap();
    assert_ne!(single.key(), "11".repeat(32));

    // Native-only: another editor saves between the review and the answer,
    // and nothing is saved.
    let dir = tempfile::tempdir().unwrap();
    let other = Store::open(dir.path()).unwrap();
    let stale = staged_text(&other, &slots, package_text());
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
