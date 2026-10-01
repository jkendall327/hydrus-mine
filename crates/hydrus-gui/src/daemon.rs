//! The daemon, `hydrus serve`, which does the client's work: downloads,
//! subscriptions, import and export folders, maintenance and the Client
//! API. While none runs on its store, the GUI runs one, stopping it on
//! closing; one started on its own (as a service, say) runs on
//! (DECISIONS.md, 2026-10-01).

use std::io::{BufRead as _, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long the daemon has to finish what it is doing when the GUI closes,
/// before it is killed.
pub const GRACE: Duration = Duration::from_secs(20);

/// The `hydrus` program: the one beside this program, else the one on the
/// path.
pub fn program() -> PathBuf {
    let name = format!("hydrus{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(&name)))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from(name))
}

/// The command starting the daemon on the store in `dir`, to stop when its
/// input closes (as it does when the GUI exits, or crashes).
pub fn command(dir: &Path) -> Command {
    let mut command = Command::new(program());
    command.arg("serve").arg(dir).arg("--attached");
    command
}

/// Whether a daemon runs on the store in `dir`: one holds its lock.
pub fn running(dir: &Path) -> bool {
    // (the lock is let go at once, if it was free)
    matches!(hydrus_store::store::lock_serving(dir), Ok(None))
}

/// What the GUI says of the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// One runs: the GUI's, or one started on its own.
    Running,
    /// The GUI's stopped (or couldn't start), saying why; another isn't
    /// started until asked for.
    Failed(String),
}

/// A daemon the GUI started.
struct Started {
    child: Child,
    input: Option<ChildStdin>,
    /// What it says on its standard error (passed on to ours as well).
    said: Arc<Mutex<Vec<String>>>,
    reading: Option<JoinHandle<()>>,
}

impl Started {
    fn start(mut command: Command) -> std::io::Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take();
        let said: Arc<Mutex<Vec<String>>> = Arc::default();
        let reading = child.stderr.take().map(|errors| {
            let said = said.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(errors).lines().map_while(Result::ok) {
                    eprintln!("{line}");
                    let mut said = said.lock().unwrap_or_else(PoisonError::into_inner);
                    said.push(line);
                    if said.len() > 100 {
                        said.remove(0);
                    }
                }
            })
        });
        Ok(Self {
            child,
            input,
            said,
            reading,
        })
    }

    /// How it exited, once it has.
    fn exited(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Why it stopped (it has), as it said: its last error, else its last
    /// words.
    fn why(&mut self, status: ExitStatus) -> String {
        // (all it said is read once its error output closes)
        if let Some(reading) = self.reading.take() {
            let _ = reading.join();
        }
        let said = self.said.lock().unwrap_or_else(PoisonError::into_inner);
        let error = said
            .iter()
            .rposition(|line| line.starts_with("Error: "))
            .map(|at| {
                let lines: Vec<&str> = said[at..].iter().map(String::as_str).collect();
                lines.join("\n")["Error: ".len()..].trim().to_owned()
            })
            .or_else(|| said.iter().rev().find(|l| !l.trim().is_empty()).cloned());
        match error {
            Some(error) => error,
            None if status.success() => "hydrus serve stopped".to_owned(),
            None => format!("hydrus serve stopped ({status})"),
        }
    }

    /// Stop it: its input closed, it has `grace` to finish before it is
    /// killed. Whether it stopped by itself.
    fn stop(&mut self, grace: Duration) -> bool {
        drop(self.input.take());
        let started = Instant::now();
        loop {
            if self.exited().is_some() {
                return true;
            }
            if started.elapsed() >= grace {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        false
    }
}

/// The daemon as the GUI looks after it: started when none runs.
pub struct Daemon {
    dir: PathBuf,
    launch: Box<dyn Fn(&Path) -> Command>,
    started: Option<Started>,
    failed: Option<String>,
}

impl std::fmt::Debug for Daemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daemon")
            .field("dir", &self.dir)
            .field("started", &self.started.as_ref().map(|s| s.child.id()))
            .field("failed", &self.failed)
            .finish_non_exhaustive()
    }
}

impl Daemon {
    /// For the store in `dir`, started as `hydrus serve` is.
    pub fn new(dir: &Path) -> Self {
        Self::launched_by(dir, Box::new(command))
    }

    /// For the store in `dir`, started by `launch`'s command.
    pub fn launched_by(dir: &Path, launch: Box<dyn Fn(&Path) -> Command>) -> Self {
        Self {
            dir: dir.to_owned(),
            launch,
            started: None,
            failed: None,
        }
    }

    /// See that one runs: the GUI's is started if none does (unless it
    /// failed before).
    pub fn check(&mut self) -> State {
        if let Some(started) = &mut self.started {
            let Some(status) = started.exited() else {
                return State::Running;
            };
            let why = started.why(status);
            self.started = None;
            // (one started on its own got the store first: no failure)
            if !running(&self.dir) {
                self.failed = Some(why);
            }
        }
        if let Some(why) = &self.failed {
            return State::Failed(why.clone());
        }
        if running(&self.dir) {
            return State::Running;
        }
        match Started::start((self.launch)(&self.dir)) {
            Ok(started) => {
                self.started = Some(started);
                State::Running
            }
            Err(e) => {
                let why = if e.kind() == std::io::ErrorKind::NotFound {
                    format!("the hydrus program wasn't found beside this one, or on the path ({e})")
                } else {
                    format!("hydrus serve couldn't be started: {e}")
                };
                self.failed = Some(why.clone());
                State::Failed(why)
            }
        }
    }

    /// Try again, after a failure.
    pub fn retry(&mut self) -> State {
        self.failed = None;
        self.check()
    }

    /// Whether the one running is the GUI's.
    pub fn started(&self) -> bool {
        self.started.is_some()
    }

    /// Stop the GUI's, giving it `grace` to finish what it is doing: whether
    /// it stopped by itself (`None` with none of the GUI's running).
    pub fn stop(&mut self, grace: Duration) -> Option<bool> {
        let mut started = self.started.take()?;
        Some(started.stop(grace))
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.stop(GRACE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This test program run as a stand-in daemon, behaving as `kind` says
    /// (see [`fake_daemon`]).
    fn fake(kind: &'static str) -> Box<dyn Fn(&Path) -> Command> {
        Box::new(move |dir| {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "daemon::tests::fake_daemon",
                    "--exact",
                    "--ignored",
                    "--nocapture",
                    "--quiet",
                ])
                .env("HYDRUS_FAKE_DAEMON", kind)
                .env("HYDRUS_FAKE_DAEMON_DIR", dir);
            command
        })
    }

    /// The stand-in daemon: "serve" holds the store until its input closes;
    /// "stubborn" holds it whatever happens; "fail" says why it can't run,
    /// as `hydrus serve` does, and stops.
    #[test]
    #[ignore = "run by the other tests, as a daemon"]
    fn fake_daemon() {
        let Ok(kind) = std::env::var("HYDRUS_FAKE_DAEMON") else {
            return;
        };
        let dir = PathBuf::from(std::env::var_os("HYDRUS_FAKE_DAEMON_DIR").unwrap());
        if kind == "fail" {
            eprintln!(
                "Error: these media locations are missing (is a drive not mounted?):\n  /gone"
            );
            std::process::exit(1);
        }
        let _held = hydrus_store::store::lock_serving(&dir).unwrap().unwrap();
        if kind == "stubborn" {
            std::thread::sleep(Duration::from_secs(120));
        }
        let mut buffer = [0; 64];
        while std::io::Read::read(&mut std::io::stdin(), &mut buffer).is_ok_and(|n| n > 0) {}
    }

    fn until(mut what: impl FnMut() -> bool) {
        let started = Instant::now();
        while !what() {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "waited too long"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn starts_one_while_none_runs_and_stops_it_on_closing() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::launched_by(dir.path(), fake("serve"));
        assert_eq!(daemon.check(), State::Running);
        assert!(daemon.started());
        until(|| running(dir.path()));
        assert_eq!(daemon.check(), State::Running);
        // (its input closed, it stops by itself, and lets the store go)
        assert_eq!(daemon.stop(Duration::from_secs(20)), Some(true));
        assert!(!running(dir.path()));
        assert_eq!(daemon.stop(Duration::from_secs(20)), None);
    }

    #[test]
    fn leaves_one_started_on_its_own_and_takes_over_when_it_stops() {
        let dir = tempfile::tempdir().unwrap();
        let theirs = hydrus_store::store::lock_serving(dir.path())
            .unwrap()
            .unwrap();
        let mut daemon = Daemon::launched_by(dir.path(), fake("serve"));
        assert_eq!(daemon.check(), State::Running);
        assert!(!daemon.started());
        drop(theirs);
        // (a process another test starts that moment holds a copy of the
        // lock until it is running)
        until(|| !running(dir.path()));
        assert_eq!(daemon.check(), State::Running);
        assert!(daemon.started());
        until(|| running(dir.path()));
        assert_eq!(daemon.stop(Duration::from_secs(20)), Some(true));
    }

    #[test]
    fn says_why_its_own_failed_and_waits_to_be_asked_again() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::launched_by(dir.path(), fake("fail"));
        assert_eq!(daemon.check(), State::Running);
        until(|| daemon.check() != State::Running);
        let failed = State::Failed(
            "these media locations are missing (is a drive not mounted?):\n  /gone".to_owned(),
        );
        assert_eq!(daemon.check(), failed);
        assert!(!daemon.started(), "started again by itself");
        assert_eq!(daemon.retry(), State::Running);
        assert!(daemon.started());
        until(|| daemon.check() != State::Running);
        assert_eq!(daemon.check(), failed);
    }

    #[test]
    fn kills_one_that_does_not_stop() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::launched_by(dir.path(), fake("stubborn"));
        daemon.check();
        until(|| running(dir.path()));
        let started = Instant::now();
        assert_eq!(daemon.stop(Duration::from_millis(300)), Some(false));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(!running(dir.path()));
    }

    #[test]
    fn says_so_when_there_is_no_program_to_start() {
        let dir = tempfile::tempdir().unwrap();
        let mut daemon = Daemon::launched_by(
            dir.path(),
            Box::new(|_| Command::new("hydrus-not-installed-anywhere")),
        );
        let State::Failed(why) = daemon.check() else {
            panic!("started");
        };
        assert!(why.starts_with("the hydrus program wasn't found"), "{why}");
    }
}
