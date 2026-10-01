//! `hydrus serve --attached`, as the desktop client starts it: it stops
//! when its input closes, and without the flag it doesn't.

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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

/// `hydrus serve` on `dir` with `args`, once it is serving (its output is
/// read on, so its logging never blocks it).
fn serve(dir: &Path, args: &[&str], input: Stdio) -> Serving {
    let mut child = Command::new(HYDRUS)
        .arg("serve")
        .arg(dir)
        .args(["--port", "0"])
        .args(args)
        .stdin(input)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let output = child.stdout.take().unwrap();
    let child = Serving(child);
    let (serving, on_serving) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            if line.starts_with("Client API at") {
                let _ = serving.send(());
            }
        }
    });
    on_serving
        .recv_timeout(Duration::from_secs(60))
        .expect("serving");
    child
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
