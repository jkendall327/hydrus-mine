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
    drop(serving.0.stdin.take());
    let stopped = exited_within(&mut serving.0, Duration::from_secs(30)).expect("stopped");
    assert!(stopped.success(), "{stopped}");
}
