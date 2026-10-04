//! The engine against a local server that scripts responses and reports
//! what it was sent.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};

use hydrus_core::url::{UrlClass, UrlClassSettings, UrlType};
use hydrus_net::{Job, NetEngine, NetError, NetOptions, Request, StatusKind};
use hydrus_store::Store;
use hydrus_store::network::{self, Approval, NetworkContext};

#[tokio::test]
async fn runtime_registry_cancellation_ipc_and_dropped_fetch() {
    use hydrus_store::{
        network_runtime::{self, Command, JobAction, Snapshot, WaitReason},
        settings,
    };
    let s = setup(|_| Vec::new()).await;
    s.store
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
    let job = Job::new();
    let request = Request::get(format!("{}/echo", s.base));
    let mut fetch = Box::pin(s.engine.fetch(&request, &job));
    tokio::select! { _ = &mut fetch => panic!("paused job ran"), () = tokio::time::sleep(std::time::Duration::from_millis(30)) => {} }
    let snapshot = s.engine.runtime_snapshot();
    assert_eq!(snapshot.jobs.len(), 1);
    assert_eq!(snapshot.jobs[0].wait, WaitReason::Paused);
    assert!(
        snapshot.jobs[0]
            .contexts
            .contains(&NetworkContext::global())
    );
    assert!(!s.engine.runtime_command(&Command {
        epoch: "previous daemon".into(),
        job: snapshot.jobs[0].id,
        action: JobAction::Cancel
    }));
    assert!(!job.is_cancelled());
    let command = Command {
        epoch: snapshot.epoch.clone(),
        job: snapshot.jobs[0].id,
        action: JobAction::Cancel,
    };
    s.store
        .write(move |ctx| network_runtime::send(ctx.conn(), command))
        .unwrap();
    s.engine.publish_runtime().unwrap();
    assert_eq!(fetch.await.unwrap_err(), NetError::Cancelled);
    assert!(s.engine.runtime_snapshot().jobs.is_empty());
    s.engine.publish_runtime().unwrap();
    assert!(
        s.store
            .read(settings::get::<Snapshot>)
            .unwrap()
            .jobs
            .is_empty()
    );
    let job = Job::new();
    let mut abandoned = Box::pin(s.engine.fetch(&request, &job));
    tokio::select! { _ = &mut abandoned => panic!("paused job ran"), () = tokio::time::sleep(std::time::Duration::from_millis(30)) => {} }
    assert_eq!(s.engine.runtime_snapshot().jobs.len(), 1);
    drop(abandoned);
    assert!(s.engine.runtime_snapshot().jobs.is_empty());
    assert!(!s.engine.runtime_command(&Command {
        epoch: snapshot.epoch,
        job: snapshot.jobs[0].id,
        action: JobAction::Cancel
    }));
}

#[tokio::test]
async fn runtime_override_releases_a_live_bandwidth_wait_and_counts_usage() {
    use hydrus_core::bandwidth::{BandwidthType, Rule, Rules};
    use hydrus_store::{
        bandwidth::BandwidthSettings,
        network_runtime::{Command, JobAction, WaitReason},
        settings,
    };
    let s = setup(|_| Vec::new()).await;
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &BandwidthSettings {
                    rules: vec![(
                        NetworkContext::global(),
                        Rules::new([Rule::new(BandwidthType::Requests, Some(3600), 1)]),
                    )],
                    ..BandwidthSettings::default()
                },
            )
        })
        .unwrap();
    let engine = NetEngine::new(
        s.store.clone(),
        NetOptions {
            obey_bandwidth: true,
            ..s.engine.options()
        },
    )
    .unwrap();
    let request = Request::get(format!("{}/echo", s.base));
    engine.fetch(&request, &Job::new()).await.unwrap();
    let job = Job::new();
    let mut fetch = Box::pin(engine.fetch(&request, &job));
    tokio::select! { _ = &mut fetch => panic!("bandwidth-limited job ran"), () = tokio::time::sleep(std::time::Duration::from_millis(30)) => {} }
    let snapshot = engine.runtime_snapshot();
    assert_eq!(snapshot.jobs[0].wait, WaitReason::Bandwidth);
    assert!(engine.runtime_command(&Command {
        epoch: snapshot.epoch,
        job: snapshot.jobs[0].id,
        action: JobAction::OverrideBandwidth
    }));
    tokio::time::timeout(std::time::Duration::from_secs(1), fetch)
        .await
        .unwrap()
        .unwrap();
    engine.save_bandwidth().unwrap();
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    let (_, mut global) = s
        .store
        .read(|conn| hydrus_store::bandwidth::usage(conn, now))
        .unwrap()
        .into_iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap();
    assert_eq!(global.usage(BandwidthType::Requests, None, now), 2);
    // Rule writes reach the running engine without destroying its usage.
    s.store
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
    assert!(engine.reload_settings().unwrap());
    engine.fetch(&request, &Job::new()).await.unwrap();
    let mut global = engine
        .runtime_snapshot()
        .usage
        .into_iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap()
        .1;
    assert_eq!(global.usage(BandwidthType::Requests, None, now), 3);
}

#[tokio::test]
async fn runtime_live_transfer_progress_and_usage_precede_the_durable_save() {
    use hydrus_core::bandwidth::BandwidthType;
    use hydrus_store::{
        bandwidth::BandwidthSettings,
        network_runtime::{Command, JobAction, WaitReason},
        settings,
    };
    let s = setup(|_| Vec::new()).await;
    s.store
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
    let engine = NetEngine::new(
        s.store.clone(),
        NetOptions {
            obey_bandwidth: true,
            ..s.engine.options()
        },
    )
    .unwrap();
    let request = Request::get(format!("{}/progressive", s.base));
    let job = Job::new();
    let mut fetch = Box::pin(engine.fetch(&request, &job));
    let start = std::time::Instant::now();
    loop {
        tokio::select! { _ = &mut fetch => panic!("finished before its live progress was read"), () = tokio::time::sleep(std::time::Duration::from_millis(10)) => {} }
        if job.state().bytes_read > 0 {
            break;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }
    let snapshot = engine.runtime_snapshot();
    let live = &snapshot.jobs[0];
    assert_eq!(live.wait, WaitReason::Downloading);
    assert_eq!(live.bytes_read, 512);
    assert_eq!(live.bytes_total, Some(2048));
    assert!(
        s.store
            .read(|conn| hydrus_store::bandwidth::usage(conn, snapshot.at))
            .unwrap()
            .is_empty()
    );
    let mut global = snapshot
        .usage
        .iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap()
        .1
        .clone();
    assert_eq!(global.usage(BandwidthType::Data, None, snapshot.at), 512);
    assert!(engine.runtime_command(&Command {
        epoch: snapshot.epoch,
        job: live.id,
        action: JobAction::Cancel
    }));
    assert_eq!(fetch.await.unwrap_err(), NetError::Cancelled);
    assert!(engine.runtime_snapshot().jobs.is_empty());
    engine.save_bandwidth().unwrap();
    let mut saved = s
        .store
        .read(|conn| hydrus_store::bandwidth::usage(conn, snapshot.at))
        .unwrap()
        .into_iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap()
        .1;
    assert_eq!(saved.usage(BandwidthType::Data, None, snapshot.at), 512);
}

#[derive(Default)]
struct Server {
    hits: Mutex<HashMap<String, usize>>,
    total: AtomicUsize,
}

type Mutex<T> = parking_lot::Mutex<T>;

impl Server {
    fn hit(&self, key: &str) -> usize {
        self.total.fetch_add(1, Ordering::SeqCst);
        let mut hits = self.hits.lock();
        let n = hits.entry(key.to_owned()).or_default();
        *n += 1;
        *n
    }
}

fn headers_json(headers: &HeaderMap) -> String {
    let mut pairs: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_owned(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

const FILE: &[u8; 1000] = &[7u8; 1000];

async fn echo(headers: HeaderMap) -> String {
    headers_json(&headers)
}

async fn login() -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/home")
        .header(header::SET_COOKIE, "sid=abc123; Path=/; HttpOnly")
        .header(header::SET_COOKIE, "short=1; Max-Age=0")
        .body(Body::empty())
        .unwrap()
}

async fn flaky(State(s): State<Arc<Server>>, Path(kind): Path<String>) -> Response {
    let n = s.hit(&kind);
    match kind.as_str() {
        "503" if n <= 2 => (StatusCode::SERVICE_UNAVAILABLE, "busy").into_response(),
        "429" if n <= 1 => Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(header::RETRY_AFTER, "0")
            .body(Body::from("slow down"))
            .unwrap(),
        "429long" if n <= 1 => Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(header::RETRY_AFTER, "60")
            .body(Body::from("slow down"))
            .unwrap(),
        "always429" => Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header(header::RETRY_AFTER, "0")
            .body(Body::from("slow down"))
            .unwrap(),
        "404" => (StatusCode::NOT_FOUND, "not here").into_response(),
        "500" => (StatusCode::INTERNAL_SERVER_ERROR, "broken").into_response(),
        _ => (StatusCode::OK, format!("ok after {n}")).into_response(),
    }
}

/// Serves `FILE` in 400-byte ranged pieces.
async fn ranged(State(s): State<Arc<Server>>, headers: HeaderMap) -> Response {
    s.hit("ranged");
    let start: usize = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.strip_suffix('-'))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let end = (start + 400).min(FILE.len());
    Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{}", end - 1, FILE.len()),
        )
        .header(header::CONTENT_TYPE, "image/png")
        .body(Body::from(FILE[start..end].to_vec()))
        .unwrap()
}

async fn redirect_loop(Path(n): Path<u32>) -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, format!("/loop/{}", n + 1))
        .body(Body::empty())
        .unwrap()
}

async fn whole_uri(uri: axum::http::Uri) -> String {
    uri.to_string()
}

async fn slow() -> &'static str {
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    "late"
}

async fn progressive() -> Response {
    let stream = futures_util::stream::unfold(0u8, |part| async move {
        if part >= 4 {
            return None;
        }
        if part > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Some((
            Ok::<_, std::io::Error>(bytes::Bytes::from(vec![part; 512])),
            part + 1,
        ))
    });
    Response::builder()
        .header(header::CONTENT_LENGTH, "2048")
        .body(Body::from_stream(stream))
        .unwrap()
}

struct Setup {
    engine: NetEngine,
    store: Arc<Store>,
    base: String,
    server: Arc<Server>,
    _dir: tempfile::TempDir,
}

async fn setup(make_classes: impl FnOnce(&str) -> Vec<UrlClass>) -> Setup {
    let server = Arc::new(Server::default());
    let app = Router::new()
        .route("/echo", get(echo))
        .route("/home", get(echo))
        .route("/login", get(login))
        .route("/flaky/{kind}", any(flaky))
        .route("/file.png", get(ranged))
        .route("/loop/{n}", get(redirect_loop))
        .route("/slow", get(slow))
        .route("/progressive", get(progressive))
        .route("/uri", get(whole_uri))
        .with_state(Arc::clone(&server));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let url_classes = make_classes(&base);
    if !url_classes.is_empty() {
        let settings = UrlClassSettings {
            url_classes,
            ..UrlClassSettings::default()
        };
        store
            .write_and_refresh(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
            .unwrap();
    }
    let options = NetOptions {
        connection_error_wait_time: 0,
        serverside_bandwidth_wait_time: 0,
        network_timeout: 2,
        // the local test server takes requests as fast as they come
        obey_bandwidth: false,
        ..NetOptions::default()
    };
    Setup {
        engine: NetEngine::new(Arc::clone(&store), options).unwrap(),
        store,
        base,
        server,
        _dir: dir,
    }
}

fn post_class(base: &str) -> UrlClass {
    let host = base.trim_start_matches("http://").to_owned();
    UrlClass {
        name: "local echo".into(),
        url_type: UrlType::Post,
        preferred_scheme: "http".into(),
        domain_mask: hydrus_core::url::DomainMask::new(vec![host], vec![], false, false),
        path_components: vec![(hydrus_core::url::StringMatch::fixed("echo"), None)],
        parameters: Vec::new(),
        header_overrides: vec![("X-Class".into(), "yes".into())],
        example_url: format!("{base}/echo"),
        ..UrlClass::default()
    }
}

#[tokio::test]
async fn sends_the_clients_headers() {
    let s = setup(|_| Vec::new()).await;
    let domain = s.base.trim_start_matches("http://").to_owned();
    s.store
        .write(move |ctx| {
            network::set_header(
                ctx.conn(),
                &NetworkContext::domain(domain),
                "X-Token",
                Some("t"),
                Some(Approval::Approved),
                None,
            )?;
            // (denied headers are simply left out)
            network::set_header(
                ctx.conn(),
                &NetworkContext::global(),
                "X-Denied",
                Some("no"),
                Some(Approval::Denied),
                None,
            )
        })
        .unwrap();
    let mut request = Request::get(format!("{}/echo", s.base));
    request.referral_url = Some("https://example.com/gallery".into());
    let response = s.engine.fetch(&request, &Job::new()).await.unwrap();
    let text = response.text();
    for expected in [
        "user-agent: Mozilla/5.0 (compatible; Hydrus Client)",
        "accept: image/jpeg,image/png,image/*;q=0.9,*/*;q=0.8",
        "cache-control: no-transform",
        "x-token: t",
        "referer: https://example.com/gallery",
        // an unclassified URL might be a file
        "range: bytes=0-",
    ] {
        assert!(text.contains(expected), "{expected} missing from\n{text}");
    }
    assert!(!text.contains("x-denied"), "{text}");
}

#[tokio::test]
async fn url_class_overrides_and_no_range_for_posts() {
    let s = setup(|base| vec![post_class(base)]).await;
    let response = s
        .engine
        .fetch(&Request::get(format!("{}/echo", s.base)), &Job::new())
        .await
        .unwrap();
    let text = response.text();
    assert!(text.contains("x-class: yes"), "{text}");
    assert!(!text.contains("range:"), "{text}");
}

#[tokio::test]
async fn keeps_cookies_through_redirects() {
    let s = setup(|_| Vec::new()).await;
    let job = Job::new();
    let response = s
        .engine
        .fetch(&Request::get(format!("{}/login", s.base)), &job)
        .await
        .unwrap();
    assert_eq!(response.url, format!("{}/home", s.base));
    assert!(
        response.text().contains("cookie: sid=abc123"),
        "{}",
        response.text()
    );
    let session = NetworkContext::domain(s.base.trim_start_matches("http://"));
    let cookies = s
        .store
        .read(|conn| {
            let session = network::session_for(conn, &session)?;
            network::cookies(conn, &session)
        })
        .unwrap();
    assert_eq!(cookies.len(), 1);
    assert_eq!(cookies[0].name, "sid");
    assert_eq!(cookies[0].rest, vec![("HttpOnly".to_owned(), None)]);
    let again = s
        .engine
        .fetch(&Request::get(format!("{}/echo", s.base)), &Job::new())
        .await
        .unwrap();
    assert!(again.text().contains("cookie: sid=abc123"));
    assert_eq!(job.state().status, "done!");
}

#[tokio::test]
async fn statuses_become_errors_or_retries() {
    let s = setup(|_| Vec::new()).await;
    let fetch = |path: &str| {
        let request = Request::get(format!("{}/flaky/{path}", s.base));
        let engine = &s.engine;
        async move { engine.fetch(&request, &Job::new()).await }
    };
    match fetch("404").await {
        Err(NetError::Status {
            kind: StatusKind::NotFound,
            code: 404,
            message,
        }) => assert_eq!(message, "404: not here"),
        other => panic!("{other:?}"),
    }
    assert_eq!(fetch("503").await.unwrap().text(), "ok after 3");
    assert_eq!(fetch("429").await.unwrap().text(), "ok after 2");
    match fetch("always429").await {
        Err(NetError::Bandwidth(message)) => {
            assert_eq!(
                message,
                "Server reported very limited bandwidth: 429: slow down"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(s.server.hits.lock()["always429"], 5);
}

#[tokio::test]
async fn a_job_ends_as_the_references_do_and_has_a_speed() {
    let s = setup(|_| Vec::new()).await;
    // Speed counts the current integer-second bucket, not the entire
    // three-range transfer: CI can cross a second between its 400/400/200 reads.
    // Exact rollover/expiry is checked with a fixed clock in the Job unit test.
    let job = Job::new();
    let request = Request::get(format!("{}/file.png", s.base));
    s.engine.fetch(&request, &job).await.unwrap();
    let state = job.state();
    assert_eq!(state.status, "done!");
    assert!(state.done && !state.error);
    assert_eq!(state.bytes_read, 1000);
    assert!(state.speed <= state.bytes_read);
    // an error status as the server gave it
    let failing = Request::get(format!("{}/flaky/404", s.base));
    assert!(s.engine.fetch(&failing, &job).await.is_err());
    let state = job.state();
    assert_eq!(state.status, "404 - Not Found");
    assert!(state.done && state.error);
    // the same job working again isn't in error until it fails
    s.engine.fetch(&request, &job).await.unwrap();
    assert!(!job.state().error);
    // cancelled
    let job = Job::new();
    let slow = Request::get(format!("{}/slow", s.base));
    let cancelling = Arc::clone(&job);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        cancelling.cancel();
    });
    assert!(matches!(
        s.engine.fetch(&slow, &job).await,
        Err(NetError::Cancelled)
    ));
    let state = job.state();
    assert_eq!(state.status, "Cancelled!");
    assert!(state.done && state.error);
}

#[tokio::test]
async fn resumes_files_with_ranged_requests() {
    let s = setup(|_| Vec::new()).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("file.png");
    let mut request = Request::get(format!("{}/file.png", s.base));
    request.destination = Some(path.clone());
    let job = Job::new();
    let response = s.engine.fetch(&request, &job).await.unwrap();
    assert_eq!(response.bytes_read, 1000);
    assert_eq!(response.content_type.as_deref(), Some("image/png"));
    assert_eq!(std::fs::read(&path).unwrap(), FILE.to_vec());
    assert_eq!(s.server.hits.lock()["ranged"], 3);
    assert_eq!(job.state().bytes_total, Some(1000));
}

#[tokio::test]
async fn gives_up_on_redirect_loops_and_dead_servers() {
    let s = setup(|_| Vec::new()).await;
    match s
        .engine
        .fetch(&Request::get(format!("{}/loop/0", s.base)), &Job::new())
        .await
    {
        Err(NetError::Network(message)) => assert_eq!(message, "Exceeded 30 redirects."),
        other => panic!("{other:?}"),
    }
    // nothing listens on port 9 here
    match s
        .engine
        .fetch(&Request::get("http://127.0.0.1:9/x"), &Job::new())
        .await
    {
        Err(NetError::Connection(message)) => assert_eq!(message, "Could not connect!"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn cancels() {
    let s = setup(|_| Vec::new()).await;
    let job = Job::new();
    let request = Request::get(format!("{}/slow", s.base));
    let cancel = Arc::clone(&job);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        cancel.cancel();
    });
    assert_eq!(
        s.engine.fetch(&request, &job).await.unwrap_err(),
        NetError::Cancelled
    );
}

#[tokio::test]
async fn bandwidth_rules_space_out_requests_and_count_their_data() {
    use hydrus_core::bandwidth::{BandwidthType, Rule, Rules};
    use hydrus_store::bandwidth::BandwidthSettings;

    let s = setup(|_| Vec::new()).await;
    // the reference's default rules: one request a second to a domain
    let engine = NetEngine::new(Arc::clone(&s.store), NetOptions::default()).unwrap();
    let started = std::time::Instant::now();
    for _ in 0..3 {
        engine
            .fetch(&Request::get(format!("{}/echo", s.base)), &Job::new())
            .await
            .unwrap();
    }
    // three calendar seconds: at least one whole second between the first
    // and the third
    assert!(
        started.elapsed() >= std::time::Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    engine.save_bandwidth().unwrap();

    // a tiny daily data cap on this domain: used up already
    let url = format!("{}/echo", s.base);
    let site = NetEngine::contexts_for(&url)[1].clone();
    assert_eq!(site.kind, hydrus_core::network::CONTEXT_DOMAIN);
    let mut settings = BandwidthSettings::default();
    settings.rules.push((
        site.clone(),
        Rules::new([Rule::new(BandwidthType::Data, Some(86_400), 10)]),
    ));
    s.store
        .write_and_refresh(move |ctx| hydrus_store::settings::set(ctx.conn(), &settings))
        .unwrap();
    // (a new engine carries on from the usage kept)
    let engine = NetEngine::new(Arc::clone(&s.store), NetOptions::default()).unwrap();
    let job = Job::new();
    let request = Request::get(format!("{}/echo", s.base));
    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        engine.fetch(&request, &job),
    )
    .await;
    assert!(waited.is_err(), "it waited for tomorrow");
    let status = job.state().status;
    assert!(status.starts_with("bandwidth free in "), "{status}");
    assert!(status.contains(&site.to_human_string()), "{status}");
    // a request told to wait at most a second goes anyway
    let mut request = Request::get(format!("{}/echo", s.base));
    request.override_bandwidth_after = Some(1);
    engine.fetch(&request, &Job::new()).await.unwrap();
}

#[tokio::test]
async fn nothing_goes_out_while_all_new_network_traffic_is_paused() {
    use hydrus_store::settings::Pauses;
    let s = setup(|_| Vec::new()).await;
    let set = |network_traffic| {
        let pauses = Pauses {
            network_traffic,
            ..Pauses::default()
        };
        s.store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &pauses))
            .unwrap();
    };
    set(true);
    let job = Job::new();
    let request = Request::get(format!("{}/echo", s.base));
    let fetch = s.engine.fetch(&request, &job);
    tokio::pin!(fetch);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut fetch)
            .await
            .is_err(),
        "it waited"
    );
    assert_eq!(
        job.state().status,
        "all new network traffic is paused\u{2026}"
    );
    // switched off (as by the command line): it goes within a couple of
    // seconds
    set(false);
    tokio::time::timeout(std::time::Duration::from_secs(5), fetch)
        .await
        .expect("it went")
        .unwrap();
}

#[tokio::test]
async fn a_domain_with_several_serious_errors_waits() {
    let s = setup(|_| Vec::new()).await;
    let fetch = |path: &str| {
        let request = Request::get(format!("{}/flaky/{path}", s.base));
        let engine = &s.engine;
        async move { engine.fetch(&request, &Job::new()).await }
    };
    // a missing file is the file's problem, not the site's
    for _ in 0..5 {
        assert!(fetch("404").await.is_err());
    }
    assert!(s.engine.domain_ok(&s.base));
    for _ in 0..3 {
        match fetch("500").await {
            Err(NetError::Status {
                kind: StatusKind::Server,
                ..
            }) => {}
            other => panic!("{other:?}"),
        }
    }
    assert!(
        !s.engine.domain_ok(&s.base),
        "three server errors in ten minutes"
    );
    let job = Job::new();
    let request = Request::get(format!("{}/echo", s.base));
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            s.engine.fetch(&request, &job)
        )
        .await
        .is_err(),
        "it waited"
    );
    assert_eq!(
        job.state().status,
        "This domain has had several serious errors recently. Waiting a bit."
    );
    // a one-shot request goes anyway
    let mut request = Request::get(format!("{}/echo", s.base));
    request.one_shot = true;
    s.engine.fetch(&request, &Job::new()).await.unwrap();
}

#[tokio::test]
async fn requests_go_through_the_clients_proxy() {
    let s = setup(|_| Vec::new()).await;
    // the test server is the proxy too: a proxied request asks it for the
    // whole URL
    let engine = NetEngine::new(
        Arc::clone(&s.store),
        NetOptions {
            http_proxy: Some(s.base.clone()),
            no_proxy: Some("127.0.0.1".into()),
            obey_bandwidth: false,
            ..NetOptions::default()
        },
    )
    .unwrap();
    let fetch = |url: String| {
        let engine = &engine;
        async move {
            engine
                .fetch(&Request::get(url), &Job::new())
                .await
                .unwrap()
                .text()
        }
    };
    assert_eq!(
        fetch("http://booru.invalid/uri".into()).await,
        "http://booru.invalid/uri"
    );
    // hosts in no_proxy are asked directly
    assert_eq!(fetch(format!("{}/uri", s.base)).await, "/uri");
}

#[tokio::test]
async fn requests_wait_a_little_after_the_computer_wakes() {
    let s = setup(|_| Vec::new()).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    // checked a minute and more ago, and now: the computer slept
    s.engine.sleep_check_at(now - 61_000);
    s.engine.sleep_check_at(now);
    let job = Job::new();
    let request = Request::get(format!("{}/echo", s.base));
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            s.engine.fetch(&request, &job)
        )
        .await
        .is_err(),
        "it waited"
    );
    assert_eq!(
        job.state().status,
        "looks like computer just woke up, waiting a bit"
    );
    // the wake delay (15 s) passed
    s.engine.sleep_check_at(now + 16_000);
    s.engine.fetch(&request, &Job::new()).await.unwrap();
}

#[tokio::test]
async fn a_header_awaiting_approval_holds_its_requests() {
    let s = setup(|_| Vec::new()).await;
    let set = |approval| {
        s.store
            .write(move |ctx| {
                network::set_header(
                    ctx.conn(),
                    &NetworkContext::global(),
                    "X-New",
                    Some("yes"),
                    Some(approval),
                    None,
                )
            })
            .unwrap();
    };
    set(Approval::Pending);
    let job = Job::new();
    let request = Request::get(format!("{}/echo", s.base));
    let fetch = s.engine.fetch(&request, &job);
    tokio::pin!(fetch);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut fetch)
            .await
            .is_err(),
        "it waited"
    );
    assert_eq!(
        job.state().status,
        "waiting for the custom header \"X-New\" to be approved\u{2026}"
    );
    set(Approval::Approved);
    let text = tokio::time::timeout(std::time::Duration::from_secs(10), fetch)
        .await
        .expect("it went")
        .unwrap()
        .text();
    assert!(text.contains("x-new: yes"), "{text}");
}

#[tokio::test]
async fn deleting_history_discards_stale_flushes_resets_live_limits_and_wakes_waiters() {
    use hydrus_core::bandwidth::{BandwidthType, Rule, Rules};
    use hydrus_store::{
        bandwidth::{self, BandwidthSettings},
        settings,
    };
    let s = setup(|_| Vec::new()).await;
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &BandwidthSettings {
                    rules: vec![(
                        NetworkContext::global(),
                        Rules::new([Rule::new(BandwidthType::Requests, Some(3600), 1)]),
                    )],
                    ..BandwidthSettings::default()
                },
            )
        })
        .unwrap();
    let engine = NetEngine::new(
        s.store.clone(),
        NetOptions {
            obey_bandwidth: true,
            ..s.engine.options()
        },
    )
    .unwrap();
    let request = Request::get(format!("{}/echo", s.base));
    engine.fetch(&request, &Job::new()).await.unwrap();
    engine.publish_runtime().unwrap();
    let job = Job::new();
    let mut pending = Box::pin(engine.fetch(&request, &job));
    tokio::select! { _ = &mut pending => panic!("request must wait for bandwidth"), () = tokio::time::sleep(std::time::Duration::from_millis(30)) => {} }
    s.store
        .write(|ctx| bandwidth::delete_history(ctx.conn(), &[NetworkContext::global()]))
        .unwrap();
    engine.save_bandwidth().unwrap();
    let now = engine.runtime_snapshot().at;
    assert!(
        !s.store
            .read(|c| bandwidth::usage(c, now))
            .unwrap()
            .iter()
            .any(|(c, _)| c == &NetworkContext::global())
    );
    engine.publish_runtime().unwrap();
    assert!(
        !s.store
            .read(settings::get::<hydrus_store::network_runtime::Snapshot>)
            .unwrap()
            .usage
            .iter()
            .any(|(c, _)| c == &NetworkContext::global())
    );
    tokio::time::timeout(std::time::Duration::from_secs(1), pending)
        .await
        .unwrap()
        .unwrap();
    engine.save_bandwidth().unwrap();
    let mut tracker = s
        .store
        .read(|c| bandwidth::usage(c, now))
        .unwrap()
        .into_iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap()
        .1;
    assert_eq!(tracker.usage(BandwidthType::Requests, None, now), 1);
    assert_eq!(
        s.store
            .read(settings::get::<BandwidthSettings>)
            .unwrap()
            .rules[0]
            .1
            .rules()[0]
            .max_allowed,
        1
    );
    let reopened = NetEngine::new(
        s.store.clone(),
        NetOptions {
            obey_bandwidth: true,
            ..s.engine.options()
        },
    )
    .unwrap();
    let mut tracker = reopened
        .runtime_snapshot()
        .usage
        .into_iter()
        .find(|(c, _)| c == &NetworkContext::global())
        .unwrap()
        .1;
    assert_eq!(tracker.usage(BandwidthType::Requests, None, now), 1);
}

#[tokio::test]
async fn disabling_sleep_detection_reloads_and_releases_pending_requests() {
    use hydrus_store::network::NetworkSettings;
    use hydrus_store::settings;
    let s = setup(|_| Vec::new()).await;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    s.engine.sleep_check_at(now - 61_000);
    s.engine.sleep_check_at(now);
    let request = Request::get(format!("{}/echo", s.base));
    let job = Job::new();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            s.engine.fetch(&request, &job)
        )
        .await
        .is_err()
    );
    assert_eq!(
        job.state().status,
        "looks like computer just woke up, waiting a bit"
    );
    s.store
        .write(|ctx| {
            let mut options = settings::get::<NetworkSettings>(ctx.conn())?;
            options.detect_sleep = false;
            settings::set(ctx.conn(), &options)
        })
        .unwrap();
    assert!(s.engine.reload_settings().unwrap());
    s.engine.sleep_check_at(now + 1);
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        s.engine.fetch(&request, &Job::new()),
    )
    .await
    .unwrap()
    .unwrap();
    s.engine.sleep_check_at(now + 61_002);
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        s.engine.fetch(&request, &Job::new()),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn independent_engines_add_new_usage_without_replacing_each_other_or_double_counting() {
    use hydrus_core::bandwidth::{BandwidthType, Tracker};
    use hydrus_store::bandwidth;
    let setup = setup(|_| Vec::new()).await;
    let now = setup.engine.runtime_snapshot().at;
    let context = NetworkContext::global();
    let mut old = Tracker::new(now);
    old.report_data(1234, now);
    old.report_requests(7, now);
    setup
        .store
        .write({
            let context = context.clone();
            move |ctx| bandwidth::save_usage(ctx.conn(), &[(context, old)])
        })
        .unwrap();
    // setup disables bandwidth accounting for unrelated HTTP tests. This
    // regression needs counting enabled while explicitly bypassing its limits.
    let options = NetOptions {
        obey_bandwidth: true,
        ..setup.engine.options()
    };
    let first = NetEngine::new(setup.store.clone(), options.clone()).unwrap();
    let second = NetEngine::new(setup.store.clone(), options).unwrap();
    let mut request = Request::get(format!("{}/echo", setup.base));
    request.override_bandwidth_after = Some(0);
    let one = first.fetch(&request, &Job::new()).await.unwrap();
    let two = second.fetch(&request, &Job::new()).await.unwrap();
    first.save_bandwidth().unwrap();
    second.save_bandwidth().unwrap();
    let read = || {
        setup
            .store
            .read(|conn| bandwidth::usage(conn, now))
            .unwrap()
            .into_iter()
            .find(|(c, _)| c == &context)
            .unwrap()
            .1
    };
    let usage = read();
    assert_eq!(
        usage.all_usage(BandwidthType::Data),
        1234 + one.bytes_read + two.bytes_read
    );
    assert_eq!(usage.all_usage(BandwidthType::Requests), 9);
    first.save_bandwidth().unwrap();
    second.save_bandwidth().unwrap();
    assert_eq!(read().to_counters(), usage.to_counters());
    let three = first.fetch(&request, &Job::new()).await.unwrap();
    first.save_bandwidth().unwrap();
    let usage = read();
    assert_eq!(
        usage.all_usage(BandwidthType::Data),
        1234 + one.bytes_read + two.bytes_read + three.bytes_read
    );
    assert_eq!(usage.all_usage(BandwidthType::Requests), 10);
    // A generation change skips stale deltas and clears the old baseline before
    // the next request is accounted, even while the other engine still exists.
    setup
        .store
        .write({
            let context = context.clone();
            move |ctx| bandwidth::delete_history(ctx.conn(), &[context])
        })
        .unwrap();
    second.save_bandwidth().unwrap();
    assert!(
        !setup
            .store
            .read(|conn| bandwidth::usage(conn, now))
            .unwrap()
            .iter()
            .any(|(c, _)| c == &context)
    );
    first.publish_runtime().unwrap();
    let fresh = first.fetch(&request, &Job::new()).await.unwrap();
    first.save_bandwidth().unwrap();
    let usage = read();
    assert_eq!(usage.all_usage(BandwidthType::Data), fresh.bytes_read);
    assert_eq!(usage.all_usage(BandwidthType::Requests), 1);
}

#[tokio::test]
async fn runtime_cog_server_retry_domain_scrub_and_retained_error() {
    use hydrus_store::network_runtime::{Command, JobAction, WaitReason};
    let s = setup(|_| Vec::new()).await;
    let request = Request::get(format!("{}/flaky/429long", s.base));
    let job = Job::new();
    let mut fetch = Box::pin(s.engine.fetch(&request, &job));
    let start = std::time::Instant::now();
    loop {
        tokio::select! { _ = &mut fetch => panic!("server retry wait was skipped"), () = tokio::time::sleep(std::time::Duration::from_millis(10)) => {} }
        if job.state().wait == WaitReason::ServerBandwidth {
            break;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
    let snapshot = s.engine.runtime_snapshot();
    let command = |action| Command {
        epoch: snapshot.epoch.clone(),
        job: snapshot.jobs[0].id,
        action,
    };
    assert!(
        !s.engine
            .runtime_command(&command(JobAction::OverrideConnectionWait))
    );
    assert!(
        s.engine
            .runtime_command(&command(JobAction::OverrideServerBandwidthWait))
    );
    tokio::time::timeout(std::time::Duration::from_secs(1), fetch)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s.server.hits.lock().get("429long"), Some(&2));
    assert!(!s.engine.runtime_command(&command(JobAction::Cancel)));

    let engine = NetEngine::new(
        s.store.clone(),
        NetOptions {
            domain_error_number: 1,
            ..s.engine.options()
        },
    )
    .unwrap();
    let mut fail = Request::get(format!("{}/flaky/500", s.base));
    fail.one_shot = true;
    assert!(engine.fetch(&fail, &Job::new()).await.is_err());
    let snapshot = engine.runtime_snapshot();
    assert!(snapshot.jobs.is_empty());
    assert_eq!(snapshot.errors.len(), 1);
    assert_eq!(snapshot.errors[0].url, fail.url);
    assert_eq!(snapshot.errors[0].text, "broken");
    // Loopback IPs have no registrable domain. The reference's scrub action
    // does not clear that gate; registered-domain wake consumption is asserted
    // in the engine unit test with a synthetic domain and no external requests.
    assert!(!engine.domain_ok(&fail.url));
}

#[tokio::test]
async fn runtime_cog_gallery_override_does_not_release_other_gallery_waiters() {
    use hydrus_core::bandwidth::{GalleryTokenKind, Rules};
    use hydrus_net::BandwidthScope;
    use hydrus_store::{
        bandwidth::BandwidthSettings,
        network_runtime::{Command, JobAction, WaitReason},
        settings,
    };
    let s = setup(|_| Vec::new()).await;
    s.store
        .write(|ctx| {
            settings::set(
                ctx.conn(),
                &BandwidthSettings {
                    rules: vec![(NetworkContext::global(), Rules::new([]))],
                    gallery_page_wait_pages: 60,
                    ..BandwidthSettings::default()
                },
            )
        })
        .unwrap();
    let engine = NetEngine::new(
        s.store.clone(),
        NetOptions {
            obey_bandwidth: true,
            ..s.engine.options()
        },
    )
    .unwrap();
    let mut request = Request::get(format!("{}/echo", s.base));
    request.gallery_page = true;
    let scope = BandwidthScope {
        gallery_token: Some(GalleryTokenKind::DownloadPage),
        ..BandwidthScope::default()
    };
    engine
        .fetch(&request, &Job::scoped(scope.clone()))
        .await
        .unwrap();
    let job = Job::scoped(scope.clone());
    let other = Job::scoped(scope);
    let mut fetch = Box::pin(engine.fetch(&request, &job));
    let mut waiting = Box::pin(engine.fetch(&request, &other));
    tokio::select! { _ = &mut fetch => panic!("gallery delay was skipped"), _ = &mut waiting => panic!("gallery delay was skipped"), () = tokio::time::sleep(std::time::Duration::from_millis(30)) => {} }
    let snapshot = engine.runtime_snapshot();
    assert_eq!(snapshot.jobs.len(), 2);
    assert!(snapshot.jobs.iter().all(|j| j.wait == WaitReason::Gallery));
    assert!(snapshot.controls.iter().all(|c| c.gallery));
    assert!(engine.runtime_command(&Command {
        epoch: snapshot.epoch.clone(),
        job: snapshot.jobs[0].id,
        action: JobAction::OverrideGalleryWait
    }));
    // IDs are insertion-ordered in the registry. The override goes to one request.
    let completed = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        tokio::select! { result = &mut fetch => result, result = &mut waiting => result }
    })
    .await
    .unwrap();
    completed.unwrap();
    let snapshot = engine.runtime_snapshot();
    assert_eq!(snapshot.jobs.len(), 1);
    assert_eq!(snapshot.jobs[0].wait, WaitReason::Gallery);
    assert!(engine.runtime_command(&Command {
        epoch: snapshot.epoch,
        job: snapshot.jobs[0].id,
        action: JobAction::Cancel
    }));
    // Drop both futures: the completed one must not be polled a second time.
    drop(fetch);
    drop(waiting);
    assert!(engine.runtime_snapshot().jobs.is_empty());
}

#[tokio::test]
async fn runtime_cog_connection_override_releases_only_the_current_retry() {
    use hydrus_store::network_runtime::{Command, JobAction, WaitReason};
    let s = setup(|_| Vec::new()).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let request = Request::get(format!(
        "http://{}/unavailable",
        listener.local_addr().unwrap()
    ));
    drop(listener); // a real loopback connection refusal, without external traffic
    let engine = NetEngine::new(
        s.store.clone(),
        NetOptions {
            connection_error_wait_time: 60,
            max_connection_attempts: 2,
            domain_error_number: 0,
            ..s.engine.options()
        },
    )
    .unwrap();
    let job = Job::new();
    let mut fetch = Box::pin(engine.fetch(&request, &job));
    let start = std::time::Instant::now();
    loop {
        tokio::select! { _ = &mut fetch => panic!("connection delay was skipped"), () = tokio::time::sleep(std::time::Duration::from_millis(10)) => {} }
        if job.state().wait == WaitReason::Connection {
            break;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
    let snapshot = engine.runtime_snapshot();
    assert!(engine.runtime_command(&Command {
        epoch: snapshot.epoch,
        job: snapshot.jobs[0].id,
        action: JobAction::OverrideConnectionWait
    }));
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), fetch)
        .await
        .unwrap()
        .unwrap_err();
    assert!(matches!(error, NetError::Connection(_)));
    assert!(engine.runtime_snapshot().jobs.is_empty());
    assert_eq!(engine.runtime_snapshot().errors.len(), 1);
}

#[tokio::test]
async fn boot_pause_blocks_real_requests_and_resume_does_not_reset_the_preference() {
    use hydrus_store::{network_runtime::WaitReason, settings};
    let s = setup(|_| Vec::new()).await;
    s.store
        .write(|ctx| {
            settings::set(ctx.conn(), &settings::NetworkBootPause(true))?;
            settings::set(ctx.conn(), &settings::Pauses::default())
        })
        .unwrap();
    settings::apply_network_boot_pause(&s.store).unwrap();
    let engine = NetEngine::new(s.store.clone(), s.engine.options()).unwrap();
    let request = Request::get(format!("{}/echo", s.base));
    let job = Job::new();
    let mut fetch = Box::pin(engine.fetch(&request, &job));
    tokio::select! {
        _ = &mut fetch => panic!("boot-paused request reached the server"),
        () = tokio::time::sleep(std::time::Duration::from_millis(30)) => {}
    }
    assert_eq!(engine.runtime_snapshot().jobs[0].wait, WaitReason::Paused);
    s.store
        .write(|ctx| settings::set(ctx.conn(), &settings::Pauses::default()))
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), fetch)
        .await
        .unwrap()
        .unwrap();
    assert!(
        s.store
            .read(settings::get::<settings::NetworkBootPause>)
            .unwrap()
            .0
    );
    // Parser/login consumers create additional engines inside this boot.
    // They must honor live Resume rather than applying the preference again.
    let next = NetEngine::new(s.store.clone(), s.engine.options()).unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        next.fetch(&request, &Job::new()),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        s.store
            .read(settings::get::<settings::NetworkBootPause>)
            .unwrap()
            .0
    );
    settings::apply_network_boot_pause(&s.store).unwrap();
    assert!(
        s.store
            .read(settings::get::<settings::Pauses>)
            .unwrap()
            .network_traffic
    );
}
