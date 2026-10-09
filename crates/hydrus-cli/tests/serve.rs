//! `hydrus serve`: attached, as the desktop client starts it, it stops when
//! its input closes, and without the flag it doesn't; its Client API is off
//! with no port, and one that can't start stops nothing else.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use hydrus_store::settings::{self, ClientApiState, ClientApiStatus};
use hydrus_store::store::lock_serving;

const HYDRUS: &str = env!("CARGO_BIN_EXE_hydrus");

/// A store imported from the basic legacy fixture.
fn store() -> (tempfile::TempDir, PathBuf) {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("store");
    let imported = Command::new(HYDRUS)
        .arg("import-legacy")
        .arg(legacy.path())
        .arg(&dir)
        .args(["--files", "copy"])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(imported.success());
    (parent, dir)
}

/// A daemon a test started, killed if the test ends with it running.
struct Serving(Child);

impl Drop for Serving {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// `hydrus serve` on `dir` with `args`, once it says a line starting with
/// `ready` (its output is read on, so its logging never blocks it).
fn start(dir: &Path, args: &[&str], input: Stdio, ready: &'static str) -> (Serving, String) {
    let mut child = Command::new(HYDRUS)
        .arg("serve")
        .arg(dir)
        .args(args)
        .stdin(input)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let child = Serving(child);
    let (said, on_said) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if line.starts_with(ready) {
                let _ = said.send(line);
            }
        }
    });
    let line = on_said
        .recv_timeout(Duration::from_secs(60))
        .unwrap_or_else(|_| panic!("never said {ready:?}"));
    (child, line)
}

/// `hydrus serve` on `dir` with `args`, once it is serving, on any port.
fn serve(dir: &Path, args: &[&str], input: Stdio) -> Serving {
    let args: Vec<&str> = ["--port", "0"].iter().chain(args).copied().collect();
    start(dir, &args, input, "Client API at").0
}

/// What the daemon last said of its Client API.
fn api_status(dir: &Path) -> ClientApiStatus {
    let store = hydrus_store::Store::open(dir).unwrap();
    store.read(settings::get::<ClientApiStatus>).unwrap()
}

fn exited_within(child: &mut Child, time: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    while started.elapsed() < time {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn attached_it_stops_when_its_input_closes() {
    let (_parent, dir) = store();
    let mut serving = serve(&dir, &["--attached"], Stdio::piped());
    let child = &mut serving.0;
    assert!(lock_serving(&dir).unwrap().is_none(), "it holds the store");
    // (still running with its input open)
    assert!(exited_within(child, Duration::from_millis(500)).is_none());
    drop(child.stdin.take());
    let status = exited_within(child, Duration::from_secs(30)).expect("stopped");
    assert!(status.success(), "{status}");
    assert!(lock_serving(&dir).unwrap().is_some(), "it let the store go");
}

#[test]
fn unattached_it_runs_on_without_input() {
    let (_parent, dir) = store();
    // (as a service manager starts it: no input at all)
    let mut serving = serve(&dir, &[], Stdio::null());
    let exited = exited_within(&mut serving.0, Duration::from_secs(2));
    assert!(exited.is_none(), "stopped: {exited:?}");
}

#[test]
fn service_edits_reach_the_running_client_api_without_a_restart() {
    use std::io::{Read as _, Write as _};

    let (_parent, dir) = store();
    let (mut serving, said) = start(
        &dir,
        &["--port", "0", "--bind", "127.0.0.1", "--attached"],
        Stdio::piped(),
        "Client API at",
    );
    let address = said.strip_prefix("Client API at http://").unwrap();
    let fixture = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let key = fixture["access_keys"]["full"].as_str().unwrap();
    let services = || {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        write!(
            stream,
            "GET /get_services HTTP/1.1\r\nHost: {address}\r\nHydrus-Client-API-Access-Key: {key}\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        response
    };
    assert!(!services().contains("edited while serving"));
    let editor = hydrus_store::Store::open(&dir).unwrap();
    let id = editor.snapshot().services.by_name("my tags").unwrap().id;
    editor
        .write_and_refresh(move |ctx| {
            ctx.conn().execute(
                "UPDATE services SET name='edited while serving' WHERE service_id=?1",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !services().contains("edited while serving") {
        assert!(
            Instant::now() < deadline,
            "daemon kept its old service registry"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(serving.0.stdin.take());
    assert!(
        exited_within(&mut serving.0, Duration::from_secs(30))
            .unwrap()
            .success()
    );
}

#[test]
fn a_client_api_that_cant_listen_stops_nothing_else() {
    let (_parent, dir) = store();
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let (mut serving, said) = start(
        &dir,
        &["--port", &port, "--attached"],
        Stdio::piped(),
        "Client API",
    );
    // (it runs on, holding the store, and says why there is no API)
    assert!(said.starts_with("Client API couldn't start"), "{said}");
    assert!(exited_within(&mut serving.0, Duration::from_secs(2)).is_none());
    assert!(lock_serving(&dir).unwrap().is_none());
    let status = api_status(&dir);
    assert_eq!(status.pid, serving.0.id());
    let ClientApiState::Failed(why) = status.state else {
        panic!("{status:?}");
    };
    assert!(why.starts_with("Could not start \"client api\": "), "{why}");
    // (and stops as told)
    drop(serving.0.stdin.take());
    let stopped = exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
    assert!(stopped.success(), "{stopped}");
}

#[test]
fn with_no_port_there_is_no_client_api() {
    let (_parent, dir) = store();
    // (hydrus's Client API turned off: its service has no port)
    {
        let store = hydrus_store::Store::open(&dir).unwrap();
        let snapshot = store.snapshot();
        let api = snapshot
            .services
            .of_type(hydrus_core::ServiceType::ClientApiService)
            .next()
            .unwrap();
        let id = api.id;
        let hydrus_store::services::ServiceKind::ClientApi(mut config) = api.kind.clone() else {
            panic!("not the Client API");
        };
        config.port = None;
        let kind = hydrus_store::services::ServiceKind::ClientApi(config);
        store
            .write(move |ctx| hydrus_store::services::update_config(ctx.conn(), id, &kind))
            .unwrap();
    }
    let (mut serving, said) = start(
        &dir,
        &["--attached"],
        Stdio::piped(),
        "The Client API is off",
    );
    assert!(said.contains("\"client api\" has no port"), "{said}");
    assert!(exited_within(&mut serving.0, Duration::from_secs(2)).is_none());
    assert_eq!(api_status(&dir).state, ClientApiState::Off);
    let editor = hydrus_store::Store::open(&dir).unwrap();
    let runtime: hydrus_store::network_runtime::Snapshot = editor.read(settings::get).unwrap();
    assert!(runtime.fresh(hydrus_core::time::TimestampMs::now().millis() / 1000));
    assert!(runtime.jobs.is_empty());
    drop(serving.0.stdin.take());
    let stopped = exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
    assert!(stopped.success(), "{stopped}");
    assert_eq!(
        editor
            .read(settings::get::<hydrus_store::network_runtime::Snapshot>)
            .unwrap(),
        hydrus_store::network_runtime::Snapshot::default()
    );
}

#[test]
fn queues_another_process_nudges_are_worked_on_at_once() {
    use hydrus_store::queues::{self, FileSeedMeta, NewFileSeed, QueueKind, SeedStatus, SeedType};

    let (_parent, dir) = store();
    // (the fixture's client had all new network traffic paused)
    hydrus_store::Store::open(&dir)
        .unwrap()
        .write(|ctx| settings::set(ctx.conn(), &settings::Pauses::default()))
        .unwrap();
    let mut serving = serve(&dir, &["--attached"], Stdio::piped());
    // (a URL that fails at once, as a 404 does)
    let site = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/file.jpg", site.local_addr().unwrap());
    std::thread::spawn(move || {
        use std::io::{Read as _, Write as _};
        for mut stream in site.incoming().map_while(Result::ok) {
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let seed = move || NewFileSeed {
        seed_type: SeedType::Url,
        data: url.clone(),
        data_for_comparison: url.clone(),
        source_time: None,
        referral_url: None,
        meta: FileSeedMeta::default(),
    };
    let store = hydrus_store::Store::open(&dir).unwrap();
    let (urls, subscription) = store
        .write(move |ctx| {
            let conn = ctx.conn();
            let options = hydrus_core::import_options::ImportOptionsSlice::default();
            let urls =
                queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
            let subscription =
                queues::create_queue(conn, QueueKind::Subscription, "a query", None, &options, 0)?;
            queues::add_file_seeds(conn, urls, &[seed()], false, 0)?;
            queues::add_file_seeds(conn, subscription, &[seed()], false, 0)?;
            queues::nudge(conn, urls)?;
            queues::nudge(conn, subscription)?;
            Ok((urls, subscription))
        })
        .unwrap();
    let status_of = |queue: i64| {
        store
            .read(move |conn| Ok(queues::file_seeds(conn, queue)?[0].status))
            .unwrap()
    };
    // worked on within seconds (the daemon otherwise looks once a minute)
    let started = Instant::now();
    while status_of(urls) == SeedStatus::Unknown {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the nudged queue wasn't worked on"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    // (a 404 is "ignored", as hydrus has it)
    assert_eq!(status_of(urls), SeedStatus::Vetoed);
    // a subscription's queue is the subscriptions' to run, nudged or not
    assert_eq!(status_of(subscription), SeedStatus::Unknown);
    drop(serving.0.stdin.take());
    exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
}

#[test]
fn urls_typed_into_a_page_are_added_and_a_closed_pages_queue_waits() {
    use hydrus_store::queues::{self, QueueKind, SeedStatus};

    let (_parent, dir) = store();
    hydrus_store::Store::open(&dir)
        .unwrap()
        .write(|ctx| settings::set(ctx.conn(), &settings::Pauses::default()))
        .unwrap();
    let mut serving = serve(&dir, &["--attached"], Stdio::piped());
    let site = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = site.local_addr().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    // (another host for the closed page's, so its turn isn't after the
    // open page's)
    let other = format!("http://localhost:{port}");
    std::thread::spawn(move || {
        use std::io::{Read as _, Write as _};
        for mut stream in site.incoming().map_while(Result::ok) {
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    let store = hydrus_store::Store::open(&dir).unwrap();
    let (open, closed) = {
        let (base, other) = (base.clone(), other.clone());
        store
            .write(move |ctx| {
                let conn = ctx.conn();
                let options = hydrus_core::import_options::ImportOptionsSlice::default();
                let open =
                    queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
                let closed =
                    queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
                queues::set_page_closed(conn, closed, true)?;
                // (as the page hands them over: the daemon keeps full URLs)
                queues::request_urls(conn, open, &["not a url".into(), format!("{base}/post/1")])?;
                queues::request_urls(conn, closed, &[format!("{other}/post/2")])?;
                Ok((open, closed))
            })
            .unwrap()
    };
    let seeds_of = |queue: i64| {
        store
            .read(move |conn| {
                Ok(queues::file_seeds(conn, queue)?
                    .into_iter()
                    .map(|s| (s.data, s.status))
                    .collect::<Vec<_>>())
            })
            .unwrap()
    };
    let started = Instant::now();
    while seeds_of(open)
        .first()
        .is_none_or(|s| s.1 == SeedStatus::Unknown)
    {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{:?}",
            seeds_of(open)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        seeds_of(open),
        [(format!("{base}/post/1"), SeedStatus::Vetoed)]
    );
    // (the closed page's URL is added, but waits)
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        seeds_of(closed),
        [(format!("{other}/post/2"), SeedStatus::Unknown)]
    );
    drop(serving.0.stdin.take());
    exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
}

#[test]
fn a_download_is_published_as_it_goes_and_can_be_cancelled() {
    use hydrus_store::live::{self, JobKind};
    use hydrus_store::queues::{self, QueueKind, SeedStatus};

    let (_parent, dir) = store();
    hydrus_store::Store::open(&dir)
        .unwrap()
        .write(|ctx| settings::set(ctx.conn(), &settings::Pauses::default()))
        .unwrap();
    let mut serving = serve(&dir, &["--attached"], Stdio::piped());
    // a file that comes slowly: a kilobyte every 50ms of a megabyte
    let site = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/slow.png",
        site.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        use std::io::{Read as _, Write as _};
        for mut stream in site.incoming().map_while(Result::ok) {
            std::thread::spawn(move || {
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 1048576\r\n\r\n",
                );
                while stream.write_all(&[0; 1024]).is_ok() {
                    std::thread::sleep(Duration::from_millis(50));
                }
            });
        }
    });
    let store = hydrus_store::Store::open(&dir).unwrap();
    let queue = {
        let url = url.clone();
        store
            .write(move |ctx| {
                let conn = ctx.conn();
                let options = hydrus_core::import_options::ImportOptionsSlice::default();
                let queue =
                    queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
                queues::request_urls(conn, queue, &[url])?;
                Ok(queue)
            })
            .unwrap()
    };
    let live_of = |queue: i64| {
        store
            .read(move |conn| Ok(live::live(conn, &[queue])?.remove(&queue)))
            .unwrap()
    };
    // the download, as it goes
    let started = Instant::now();
    let job = loop {
        if let Some(job) = live_of(queue).and_then(|l| l.file_job)
            && job.bytes_read > 0
        {
            break job;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{:?}",
            live_of(queue)
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(job.status, "downloading\u{2026}");
    assert_eq!(job.bytes_to_read, Some(1_048_576));
    assert!(!job.done && !job.error);
    // ("5 KB/1 MB 5 KB/s")
    let right = job.line().right;
    assert!(right.contains("/1 MB ") && right.ends_with("/s"), "{right}");
    assert!(job.line().can_cancel);
    // the daemon says what its network has read, and is reading a second,
    // for the client's status bar
    let started = Instant::now();
    let said = loop {
        let said: hydrus_store::live::DaemonLive = store.read(settings::get).unwrap();
        // (at least what the job had read, as the download goes on)
        if said.bytes >= job.bytes_read && said.speed > 0 {
            break said;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{said:?} {job:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(said.started <= said.at);
    // cancelled as its page's button asks: the file ends vetoed, as the
    // reference notes it
    store
        .write(move |ctx| live::cancel(ctx.conn(), queue, JobKind::File))
        .unwrap();
    let seed = || {
        store
            .read(move |conn| Ok(queues::file_seeds(conn, queue)?.remove(0)))
            .unwrap()
    };
    let started = Instant::now();
    while seed().status == SeedStatus::Unknown {
        assert!(started.elapsed() < Duration::from_secs(10), "not cancelled");
        std::thread::sleep(Duration::from_millis(100));
    }
    let seed = seed();
    assert_eq!(seed.status, SeedStatus::Vetoed);
    assert_eq!(seed.note, "Download cancelled: Cancelled by user.");
    // nothing downloading, and nothing at all once the daemon stops
    let started = Instant::now();
    while live_of(queue).is_some_and(|l| l.file_job.is_some()) {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            live_of(queue)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    drop(serving.0.stdin.take());
    exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
    assert_eq!(live_of(queue), None);
}

#[test]
fn changed_options_apply_without_a_restart() {
    use hydrus_store::queues::{self, QueueKind};

    let (_parent, dir) = store();
    hydrus_store::Store::open(&dir)
        .unwrap()
        .write(|ctx| settings::set(ctx.conn(), &settings::Pauses::default()))
        .unwrap();
    // (the daemon's log, to see it notice)
    let mut child = Command::new(HYDRUS)
        .arg("serve")
        .arg(&dir)
        .args(["--port", "0", "--attached"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let mut serving = Serving(child);
    let (said, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            let _ = said.send(line);
        }
    });
    let wait_for = |text: &str| loop {
        let line = lines
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| panic!("never said {text:?}"));
        if line.contains(text) {
            break;
        }
    };
    wait_for("Client API at");
    // a proxy, which notes what it is asked for
    let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_url = format!("http://127.0.0.1:{}", proxy.local_addr().unwrap().port());
    let (asked, asks) = mpsc::channel();
    std::thread::spawn(move || {
        use std::io::{Read as _, Write as _};
        for mut stream in proxy.incoming().map_while(Result::ok) {
            let mut request = [0; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let first = String::from_utf8_lossy(&request[..read])
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            let _ = asked.send(first);
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        }
    });
    // set while the daemon runs: it says it noticed
    let store = hydrus_store::Store::open(&dir).unwrap();
    store
        .write(move |ctx| {
            let mut network: hydrus_store::network::NetworkSettings = settings::get(ctx.conn())?;
            network.http_proxy = Some(proxy_url);
            settings::set(ctx.conn(), &network)
        })
        .unwrap();
    wait_for("the network options changed");
    // and a file on a site that isn't there is asked of the proxy
    store
        .write(|ctx| {
            let conn = ctx.conn();
            let options = hydrus_core::import_options::ImportOptionsSlice::default();
            let queue =
                queues::create_queue(conn, QueueKind::Urls, "url import", None, &options, 0)?;
            queues::request_urls(
                conn,
                queue,
                &["http://hydrus-test.invalid/file.png".to_owned()],
            )
        })
        .unwrap();
    let first = asks
        .recv_timeout(Duration::from_secs(30))
        .expect("the proxy is asked");
    assert_eq!(first, "GET http://hydrus-test.invalid/file.png HTTP/1.1");
    // the thumbnail options, which imports make thumbnails by: noticed
    // once, through the shared store snapshot refresh (plain settings writes
    // remain supported without publishing a revision).
    store
        .write(|ctx| {
            let mut thumbnails: hydrus_core::thumbnail::ThumbnailSettings =
                settings::get(ctx.conn())?;
            thumbnails.bounding_width = 300;
            settings::set(ctx.conn(), &thumbnails)
        })
        .unwrap();
    wait_for("the store snapshot changed");
    std::thread::sleep(Duration::from_millis(2500));
    assert!(
        lines
            .try_iter()
            .all(|line| !line.contains("the store snapshot changed")),
        "said again: its snapshot was not read again"
    );
    drop(serving.0.stdin.take());
    exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
}

#[test]
fn client_api_listener_reconfigures_recovers_and_preserves_daemon_state() {
    use hydrus_store::services::{ServiceKind, update_config};
    use std::io::{Read as _, Write as _};
    let (_parent, dir) = store();
    let editor = hydrus_store::Store::open(&dir).unwrap();
    let api = editor
        .snapshot()
        .services
        .of_type(hydrus_core::ServiceType::ClientApiService)
        .next()
        .unwrap()
        .clone();
    let ServiceKind::ClientApi(mut config) = api.kind.clone() else {
        panic!("API")
    };
    config.port = None;
    let update = {
        let editor = editor.clone();
        move |config: hydrus_store::services::ServerConfig| {
            let id = api.id;
            editor
                .write(move |ctx| update_config(ctx.conn(), id, &ServiceKind::ClientApi(config)))
                .unwrap();
        }
    };
    update(config.clone());
    let logs = dir.join("listener-test.log");
    let output = std::fs::File::create(&logs).unwrap();
    let mut serving = Serving(
        Command::new(HYDRUS)
            .arg("serve")
            .arg(&dir)
            .arg("--attached")
            .stdin(Stdio::piped())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap(),
    );
    let pid = serving.0.id();
    let wait = {
        let editor = editor.clone();
        move |predicate: &dyn Fn(&ClientApiState) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(12);
            loop {
                let status: ClientApiStatus = editor.read(settings::get).unwrap();
                if status.pid == pid && predicate(&status.state) {
                    return status.state;
                }
                assert!(Instant::now() < deadline, "listener state: {status:?}");
                std::thread::sleep(Duration::from_millis(40));
            }
        }
    };
    wait(&|state| matches!(state, ClientApiState::Off));
    let queue = editor
        .write(|ctx| {
            hydrus_store::queues::create_queue(
                ctx.conn(),
                hydrus_store::queues::QueueKind::Urls,
                "keep this queue",
                None,
                &hydrus_core::import_options::ImportOptionsSlice::default(),
                0,
            )
        })
        .unwrap();
    let unused_port = || {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    };
    config.port = Some(unused_port());
    config.support_cors = true;
    config.log_requests = true;
    update(config.clone());
    let ClientApiState::Listening(first) =
        wait(&|state| matches!(state, ClientApiState::Listening(_)))
    else {
        panic!("listening")
    };
    let request = |address: &str, path: &str, credential: Option<(&str, &str)>| {
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let credential = credential
            .map(|(header, key)| format!("{header}: {key}\r\n"))
            .unwrap_or_default();
        write!(stream,"GET {path} HTTP/1.1\r\nHost: {address}\r\nOrigin: https://test.example\r\n{credential}Connection: close\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    };
    assert!(
        request(
            &first,
            "/api_version?private-query-value=never-log-this",
            None
        )
        .to_lowercase()
        .contains("access-control-allow-origin: *")
    );
    let fixture = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let access = fixture["access_keys"]["full"].as_str().unwrap();
    let response = request(
        &first,
        "/session_key",
        Some(("Hydrus-Client-API-Access-Key", access)),
    );
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body: serde_json::Value =
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    let session = body["session_key"].as_str().unwrap();
    assert!(
        request(
            &first,
            "/verify_access_key",
            Some(("Hydrus-Client-API-Session-Key", session))
        )
        .starts_with("HTTP/1.1 200")
    );
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    config.port = Some(occupied.local_addr().unwrap().port());
    update(config.clone());
    wait(&|state| matches!(state, ClientApiState::Failed(_)));
    assert!(
        std::net::TcpStream::connect(&first).is_err(),
        "previous listener was stopped"
    );
    assert!(
        serving.0.try_wait().unwrap().is_none(),
        "daemon jobs continue after bind failure"
    );
    config.port = Some(unused_port());
    config.support_cors = false;
    update(config.clone());
    let ClientApiState::Listening(second) =
        wait(&|state| matches!(state, ClientApiState::Listening(_)))
    else {
        panic!("listening")
    };
    assert!(
        !request(&second, "/api_version", None)
            .to_lowercase()
            .contains("access-control-allow-origin")
    );
    assert_eq!(
        editor
            .read(move |conn| Ok(hydrus_store::queues::queue(conn, queue)?.unwrap().name))
            .unwrap(),
        "keep this queue"
    );
    assert_eq!(
        api_status(&dir).pid,
        pid,
        "listener correction keeps daemon process"
    );
    assert!(
        request(
            &second,
            "/verify_access_key",
            Some(("Hydrus-Client-API-Session-Key", session))
        )
        .starts_with("HTTP/1.1 200"),
        "same session survives listener rebind and failed-bind correction"
    );
    config.use_https = true;
    update(config.clone());
    wait(&|state| matches!(state, ClientApiState::Listening(_)));
    config.use_https = false;
    update(config.clone());
    wait(&|state| matches!(state, ClientApiState::Listening(_)));
    config.port = None;
    update(config);
    wait(&|state| matches!(state, ClientApiState::Off));
    drop(serving.0.stdin.take());
    assert!(
        exited_within(&mut serving.0, Duration::from_secs(15))
            .unwrap()
            .success()
    );
    let logged = std::fs::read_to_string(logs).unwrap();
    assert!(logged.contains("Client API request"));
    assert!(
        !logged.contains("never-log-this"),
        "request logging omits query secrets"
    );
}
