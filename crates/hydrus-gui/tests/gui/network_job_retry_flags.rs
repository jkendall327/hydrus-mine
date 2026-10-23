//! The download control's cog acting on a live engine's job, as the
//! reference's `NetworkJobControl` acts on its job: the entries each wait
//! offers, and the job's flags after "override forced gallery wait times",
//! "reattempt request now", "reattempt connection now" and "scrub domain
//! errors" (`actions.retry` of oracle/fixtures/network_job_control.json).
use hydrus_core::{bandwidth::GalleryTokenKind, network::NetworkContext};
use hydrus_gui::{MainWindow, headless, network_data_window, network_job_control as gui};
use hydrus_gui_model::network_job_control::{self as model, Target};
use hydrus_net::{BandwidthScope, Job, NetEngine, NetOptions, Request};
use hydrus_store::{
    Store,
    bandwidth::BandwidthSettings,
    network_runtime::{JobControl, NetworkJob, Snapshot, WaitReason},
    settings,
};
use slint::Model as _;
use std::io::{Read, Write};
use std::{rc::Rc, sync::Arc};

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

/// The live gallery job and its control state, as the window reads them.
fn live(store: &Store) -> Option<(NetworkJob, JobControl)> {
    let snapshot = store.read(settings::get::<Snapshot>).ok()?;
    let job = snapshot.jobs.into_iter().find(|j| {
        j.contexts
            .contains(&NetworkContext::downloader_page(format!("{:016x}", 1)))
    })?;
    let control = snapshot.controls.into_iter().find(|c| c.id == job.id)?;
    control.gallery.then_some((job, control))
}

/// The gallery cog's job entries: (label, id).
fn entries(window: &MainWindow) -> Vec<(String, i32)> {
    window.invoke_control_menu(true);
    window
        .get_search_cog()
        .actions
        .iter()
        .map(|r| (r.label.to_string(), r.id))
        .collect()
}

fn click(window: &MainWindow, label: &str) {
    let mut id = None;
    until(label, || {
        id = entries(window)
            .into_iter()
            .find(|(l, _)| l == label)
            .map(|(_, id)| id);
        id.is_some()
    });
    window.invoke_control_action(true, id.unwrap());
}

// leaf: audit-network-download-control-cog
#[test]
fn the_cog_s_retry_scrub_and_token_entries_clear_a_live_job_s_waits_as_the_reference_s_do() {
    let recorded = hydrus_testkit::fixture_json("network_job_control.json");
    // [waiting on connection error, waiting on serverside bandwidth,
    //  domain ok, tokens ok], after the four entries
    let retry: Vec<bool> = recorded["actions"]["retry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_bool().unwrap())
        .collect();
    let offered: Vec<&str> = recorded["menus"]["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["label"].as_str())
        .collect();
    for label in [
        model::CONNECTION_LABEL,
        model::SERVER_LABEL,
        model::DOMAIN_LABEL,
        model::GALLERY_LABEL,
    ] {
        assert!(offered.contains(&label), "{label}");
    }

    let _headless = headless::init();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
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
    // The site, the recording's example.com, reached through the local
    // server as its http proxy (scrubbing clears a second-level domain's
    // errors, and an IP address has none): a first gallery page (taking the site's token); then, for
    // the job under test, "509" (a serverside bandwidth wait), a connection
    // dropped before any answer (a connection error wait), and a request
    // held open until the test is done.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let base = "http://example.com";
    let (release, released) = std::sync::mpsc::channel::<()>();
    let (holding, held) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let read = |stream: &mut std::net::TcpStream| {
            let mut request = [0; 4096];
            assert!(stream.read(&mut request).unwrap() > 0);
        };
        let (mut first, _) = listener.accept().unwrap();
        read(&mut first);
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .unwrap();
        drop(first);
        let (mut limited, _) = listener.accept().unwrap();
        read(&mut limited);
        limited
            .write_all(b"HTTP/1.1 509 Bandwidth Limit Exceeded\r\nContent-Length: 4\r\nConnection: close\r\n\r\nslow")
            .unwrap();
        drop(limited);
        let (mut dropped, _) = listener.accept().unwrap();
        read(&mut dropped);
        drop(dropped);
        let (mut kept, _) = listener.accept().unwrap();
        read(&mut kept);
        holding.send(()).unwrap();
        released.recv().unwrap();
        let _ =
            kept.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    });
    // (one serious error marks the domain, as the reference's scenario has
    // the domain at its error limit)
    let engine = Arc::new(
        NetEngine::new(
            store.clone(),
            NetOptions {
                domain_error_number: 1,
                http_proxy: Some(proxy),
                no_proxy: None,
                ..NetOptions::default()
            },
        )
        .unwrap(),
    );
    let worker = std::thread::spawn({
        let engine = engine.clone();
        move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                let mut results = Vec::new();
                for path in ["/first", "/second"] {
                    let job = Job::scoped(BandwidthScope {
                        contexts: vec![NetworkContext::downloader_page(format!("{:016x}", 1))],
                        gallery_token: Some(GalleryTokenKind::DownloadPage),
                        ..BandwidthScope::default()
                    });
                    let mut request = Request::get(format!("{base}{path}"));
                    request.gallery_page = true;
                    let fetching = engine.clone();
                    let fetch = tokio::spawn(async move { fetching.fetch(&request, &job).await });
                    while !fetch.is_finished() {
                        engine.publish_runtime().unwrap();
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                    }
                    results.push(fetch.await.unwrap().map(|r| r.body));
                    engine.publish_runtime().unwrap();
                }
                results
            })
        }
    });

    let window = MainWindow::new().unwrap();
    let slots = network_data_window::Slots::default();
    let _owner = gui::bind(
        &window,
        store.clone(),
        Rc::new(|gallery| Some(Target { queue: 1, gallery })),
        slots,
    );

    // The second page waits on the token the first took.
    until("the token wait", || {
        live(&store).is_some_and(|(j, c)| j.wait == WaitReason::Gallery && !c.tokens_ok)
    });
    click(&window, model::GALLERY_LABEL);
    until("the serverside bandwidth wait", || {
        live(&store).is_some_and(|(j, c)| {
            j.wait == WaitReason::ServerBandwidth && c.waiting_on_serverside_bandwidth
        })
    });
    assert_eq!(live(&store).unwrap().1.tokens_ok, retry[3]);
    until("the token entry to go", || {
        !entries(&window)
            .iter()
            .any(|(l, _)| l == model::GALLERY_LABEL)
    });

    click(&window, model::SERVER_LABEL);
    until("the connection error wait", || {
        live(&store)
            .is_some_and(|(j, c)| j.wait == WaitReason::Connection && c.waiting_on_connection_error)
    });
    assert_eq!(
        live(&store).unwrap().1.waiting_on_serverside_bandwidth,
        retry[1]
    );

    click(&window, model::CONNECTION_LABEL);
    until("the held request", || held.try_recv().is_ok());
    until("the running job", || {
        live(&store).is_some_and(|(j, c)| {
            j.wait == WaitReason::Downloading && !c.waiting_on_connection_error
        })
    });
    assert_eq!(
        live(&store).unwrap().1.waiting_on_connection_error,
        retry[0]
    );

    // (the 509 and the dropped connection were serious errors on its domain)
    until("the domain errors", || {
        live(&store).is_some_and(|(_, c)| !c.domain_ok)
    });
    click(&window, model::DOMAIN_LABEL);
    until("the scrubbed domain", || {
        live(&store).is_some_and(|(_, c)| c.domain_ok)
    });
    let (_, control) = live(&store).unwrap();
    assert_eq!(
        vec![
            control.waiting_on_connection_error,
            control.waiting_on_serverside_bandwidth,
            control.domain_ok,
            control.tokens_ok,
        ],
        retry
    );
    // (the window's menu follows once its review has the new snapshot)
    let gone = [
        model::CONNECTION_LABEL,
        model::SERVER_LABEL,
        model::DOMAIN_LABEL,
        model::GALLERY_LABEL,
    ];
    until("the four entries to go", || {
        entries(&window)
            .iter()
            .all(|(l, _)| !gone.contains(&l.as_str()))
    });

    release.send(()).unwrap();
    let results = worker.join().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    server.join().unwrap();
}
