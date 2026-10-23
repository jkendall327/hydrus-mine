//! The current network jobs window's position column for real engine jobs,
//! each held where oracle/record_network_job_waits.py held the reference
//! engine's job (fixtures/network_job_waits.json): its position, and the
//! status text where the engine words it as the reference does.
use hydrus_core::{
    bandwidth::{BandwidthType, GalleryTokenKind, Rule, Rules, Tracker},
    network::NetworkContext,
    time::TimestampMs,
};
use hydrus_gui::{headless, network_data_window as windows};
use hydrus_net::{BandwidthScope, Job, NetEngine, NetOptions, Request};
use hydrus_store::{
    Store,
    bandwidth::BandwidthSettings,
    network::{self, Approval},
    network_runtime::WaitReason,
    settings,
};
use slint::Model as _;
use std::io::{Read, Write};
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};

fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "waited too long for {what}"
        );
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn now() -> i64 {
    TimestampMs::now().millis() / 1000
}

/// A local server answering every request with "ok", or holding every
/// request open until the test ends.
fn server(hold: bool) -> (String, Arc<AtomicBool>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let done = Arc::new(AtomicBool::new(false));
    let stop = done.clone();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            if hold {
                held.push(stream);
            } else {
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                );
            }
            if stop.load(Ordering::Relaxed) {
                return;
            }
        }
    });
    (base, done)
}

/// A port nothing listens on (from this test's own allocation).
fn refused() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

/// A request, its scope, and the wait it must reach before the next starts.
struct Step {
    request: Request,
    scope: BandwidthScope,
    /// Run it to its end before the next (it is not looked at).
    finish: bool,
    ready: WaitReason,
}

fn step(url: String, ready: WaitReason) -> Step {
    Step {
        request: Request::get(url),
        scope: BandwidthScope::default(),
        finish: false,
        ready,
    }
}

/// Run `steps` on a fresh engine, then read the jobs window's rows for them:
/// (position, url, status) by url.
fn positions(
    options: NetOptions,
    prepare: impl FnOnce(&Store),
    before: impl FnOnce(&NetEngine),
    steps: Vec<Step>,
) -> Vec<(String, String, String)> {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    prepare(&store);
    let engine = Arc::new(NetEngine::new(store.clone(), options).unwrap());
    before(&engine);
    let watched: Vec<String> = steps
        .iter()
        .filter(|s| !s.finish)
        .map(|s| s.request.url.clone())
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let all_ready = Arc::new(AtomicBool::new(false));
    let worker = std::thread::spawn({
        let engine = engine.clone();
        let stop = stop.clone();
        let all_ready = all_ready.clone();
        move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let mut running = Vec::new();
                for step in steps {
                    let job = Job::scoped(step.scope);
                    let fetching = engine.clone();
                    let request = step.request;
                    let handle = job.clone();
                    let fetch = tokio::spawn(async move { fetching.fetch(&request, &job).await });
                    if step.finish {
                        while !fetch.is_finished() {
                            engine.publish_runtime().unwrap();
                            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                        }
                        let _ = fetch.await;
                        continue;
                    }
                    while handle.state().wait != step.ready {
                        assert!(!fetch.is_finished(), "{:?}", handle.state());
                        engine.publish_runtime().unwrap();
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                    }
                    running.push((handle, fetch));
                }
                all_ready.store(true, Ordering::Relaxed);
                while !stop.load(Ordering::Relaxed) {
                    engine.publish_runtime().unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
                for (job, fetch) in running {
                    job.cancel();
                    let _ = fetch.await;
                }
            });
        }
    });
    let slots = windows::Slots::default();
    let window = windows::open_jobs(store.clone(), &slots).unwrap();
    let rows = || -> Vec<(String, String, String)> {
        let rows = window.get_rows();
        (0..rows.row_count())
            .map(|r| {
                let cells = rows.row_data(r).unwrap().cells;
                let cell = |c: usize| cells.row_data(c).unwrap().to_string();
                (cell(0), cell(1), cell(2))
            })
            .collect()
    };
    until("the jobs to settle and be listed", || {
        window.invoke_refresh();
        all_ready.load(Ordering::Relaxed)
            && watched
                .iter()
                .all(|url| rows().iter().any(|(_, u, _)| u == url))
    });
    // (one more heartbeat after every job settled)
    std::thread::sleep(std::time::Duration::from_millis(100));
    window.invoke_refresh();
    let listed = rows()
        .into_iter()
        .filter(|(_, url, _)| watched.contains(url))
        .collect();
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    slots.close();
    drop(dir);
    listed
}

fn recorded(name: &str) -> (String, String) {
    let f = hydrus_testkit::fixture_json("network_job_waits.json");
    let case = &f["scenarios"][name];
    (
        case["position"].as_str().unwrap().to_owned(),
        case["status"].as_str().unwrap().to_owned(),
    )
}

/// The one row of a scenario: its position matches the reference's, and its
/// status too when `same_status`.
fn check(name: &str, rows: &[(String, String, String)], url: &str, same_status: bool) {
    let (position, status) = recorded(name);
    let row = rows
        .iter()
        .find(|(_, u, _)| u == url)
        .unwrap_or_else(|| panic!("{name}: {rows:?}"));
    assert_eq!(row.0, position, "{name}");
    if same_status {
        assert_eq!(row.2, status, "{name}");
    }
}

// leaf: audit-network-jobs-waits
#[test]
fn real_jobs_are_listed_at_the_reference_engine_s_position_for_their_wait() {
    let _headless = headless::init();
    let no_rules = |store: &Store| {
        store
            .write(|ctx| {
                settings::set(
                    ctx.conn(),
                    &BandwidthSettings {
                        rules: Vec::new(),
                        ..BandwidthSettings::default()
                    },
                )
            })
            .unwrap();
    };

    // A custom header awaiting approval: validation. (The reference asks
    // in a popup, and says so in the status; native waits for the Client
    // API's approval and says that.)
    let url = "https://headers.example.com/file".to_owned();
    let rows = positions(
        NetOptions::default(),
        |store| {
            no_rules(store);
            store
                .write(|ctx| {
                    network::set_header(
                        ctx.conn(),
                        &NetworkContext::domain("headers.example.com"),
                        "X-Test",
                        Some("value"),
                        Some(Approval::Pending),
                        Some("fixture"),
                    )
                })
                .unwrap();
        },
        |_| {},
        vec![step(url.clone(), WaitReason::Headers)],
    );
    check("headers", &rows, &url, false);

    // All new traffic paused: bandwidth.
    let url = "https://paused.example.com/file".to_owned();
    let rows = positions(
        NetOptions::default(),
        |store| {
            no_rules(store);
            store
                .write(|ctx| {
                    settings::set(
                        ctx.conn(),
                        &settings::Pauses {
                            network_traffic: true,
                            ..settings::Pauses::default()
                        },
                    )
                })
                .unwrap();
        },
        |_| {},
        vec![step(url.clone(), WaitReason::Paused)],
    );
    check("paused", &rows, &url, true);

    // The global rules used up: bandwidth.
    let url = "https://bandwidth.example.com/file".to_owned();
    let rows = positions(
        NetOptions::default(),
        |store| {
            let mut used = Tracker::new(now());
            used.report_requests(1, now());
            store
                .write(move |ctx| {
                    settings::set(
                        ctx.conn(),
                        &BandwidthSettings {
                            rules: vec![(
                                NetworkContext::global(),
                                Rules::new([Rule::new(BandwidthType::Requests, Some(3600), 1)]),
                            )],
                            ..BandwidthSettings::default()
                        },
                    )?;
                    hydrus_store::bandwidth::save_usage(
                        ctx.conn(),
                        &[(NetworkContext::global(), used)],
                    )
                })
                .unwrap();
        },
        |_| {},
        vec![step(url.clone(), WaitReason::Bandwidth)],
    );
    check("bandwidth", &rows, &url, false);

    // A request in progress (running), and a second on its domain, whose one
    // slot it holds (free work slot).
    let (base, held) = server(true);
    let (first, second) = (format!("{base}/held"), format!("{base}/held2"));
    let rows = positions(
        NetOptions {
            max_jobs_per_domain: 1,
            ..NetOptions::default()
        },
        no_rules,
        |_| {},
        vec![
            step(first.clone(), WaitReason::Downloading),
            step(second.clone(), WaitReason::Slot),
        ],
    );
    held.store(true, Ordering::Relaxed);
    check("running", &rows, &first, false);
    check("domain_slot", &rows, &second, true);

    // A gallery page after another took the site's token: free work slot.
    let (base, answered) = server(false);
    let gallery = |path: &str, finish: bool, ready: WaitReason| {
        let mut request = Request::get(format!("{base}{path}"));
        request.gallery_page = true;
        Step {
            request,
            scope: BandwidthScope {
                gallery_token: Some(GalleryTokenKind::DownloadPage),
                ..BandwidthScope::default()
            },
            finish,
            ready,
        }
    };
    let url = format!("{base}/page2");
    let rows = positions(
        NetOptions::default(),
        no_rules,
        |_| {},
        vec![
            gallery("/page1", true, WaitReason::Downloading),
            gallery("/page2", false, WaitReason::Gallery),
        ],
    );
    answered.store(true, Ordering::Relaxed);
    check("gallery_token", &rows, &url, false);

    // A domain with a recent serious error (a refused connection, with one
    // error the limit): free work slot.
    let base = refused();
    let url = format!("{base}/again");
    let mut failing = step(format!("{base}/first"), WaitReason::Engine);
    failing.finish = true;
    let rows = positions(
        NetOptions {
            domain_error_number: 1,
            max_connection_attempts: 1,
            ..NetOptions::default()
        },
        no_rules,
        |_| {},
        vec![failing, step(url.clone(), WaitReason::Domain)],
    );
    check("domain_errors", &rows, &url, true);

    // Just woke from sleep: free work slot.
    let url = "https://woke.example.com/file".to_owned();
    let rows = positions(
        NetOptions::default(),
        no_rules,
        NetEngine::simulate_wake,
        vec![step(url.clone(), WaitReason::Wake)],
    );
    check("woke", &rows, &url, true);

    // A refused connection waiting to retry: running.
    let url = format!("{}/refused", refused());
    let rows = positions(
        NetOptions::default(),
        no_rules,
        |_| {},
        vec![step(url.clone(), WaitReason::Connection)],
    );
    check("connection", &rows, &url, false);
}
