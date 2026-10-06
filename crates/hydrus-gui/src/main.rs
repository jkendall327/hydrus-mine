//! `hydrus-gui <store>`: the desktop client over a hydrus-rs store.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use slint::ComponentHandle as _;

use hydrus_core::lock::LockPassword;
use hydrus_gui::daemon::{self, Daemon};
use hydrus_gui::{Bound, MainWindow, Pages, bind, unlock_window};
use hydrus_store::Store;
use hydrus_store::settings::ClientApiStatus;

fn main() -> Result<()> {
    // (an option, such as --help, is not a store to make)
    let dir: PathBuf = std::env::args_os()
        .nth(1)
        .filter(|arg| !arg.to_string_lossy().starts_with('-'))
        .ok_or_else(|| anyhow!("usage: hydrus-gui <store directory>"))?
        .into();
    // a backup asked to be restored before this start (the reference's
    // restart after "restore from a database backup")
    if let Some((from, media)) = hydrus_store::backup::take_restore_request(&dir) {
        eprintln!("restoring the backup at {}", from.display());
        hydrus_store::backup::restore(&dir, &from, &media, &mut |text| eprintln!("{text}"))
            .with_context(|| format!("restoring the backup at {}", from.display()))?;
    }
    let store =
        Store::open(&dir).with_context(|| format!("opening the store at {}", dir.display()))?;
    // one client at a time on a store, as the reference allows one on its
    // database; held before the session is read, so what the Client API
    // asks of the pages from now on waits for this client
    let _open = lock_client(&dir)?;
    let lock: LockPassword = store
        .read(hydrus_store::settings::get)
        .context("reading the lock password")?;
    hydrus_gui::session_autosave::install_activity_backend()
        .context("initializing desktop input activity")?;
    let run = hydrus_gui::session_startup::Run::begin(store.dir())
        .context("recording the running client")?;
    let bad = run.bad();
    let recovery: Rc<RefCell<Option<hydrus_gui::session_startup::Recovery>>> = Rc::default();
    let client: Rc<RefCell<Option<Client>>> = Rc::default();
    let failed: Rc<RefCell<Option<anyhow::Error>>> = Rc::default();
    let open = {
        let (client, failed, recovery) = (client.clone(), failed.clone(), recovery.clone());
        move || {
            let slot = Rc::downgrade(&recovery);
            let ready = Rc::new({
                let store = store.clone();
                let failed = failed.clone();
                move |pages: hydrus_store::Result<Pages>| {
                    match pages
                        .map_err(anyhow::Error::from)
                        .and_then(|pages| Client::open(&store, pages))
                    {
                        Ok(opened) => *client.borrow_mut() = Some(opened),
                        Err(error) => {
                            *failed.borrow_mut() = Some(error);
                            let _ = slint::quit_event_loop();
                        }
                    }
                    if let Some(slot) = slot.upgrade() {
                        slot.borrow_mut().take();
                    }
                }
            });
            match hydrus_gui::session_startup::prepare(store, bad, ready) {
                Ok(window) => *recovery.borrow_mut() = window,
                Err(error) => {
                    *failed.borrow_mut() = Some(anyhow!(error));
                    let _ = slint::quit_event_loop();
                }
            }
        }
    };
    // as the reference does: with a lock password, nothing opens until it is
    // entered
    let _unlock = if lock.is_set() {
        let window = unlock_window(lock, open)?;
        window.show()?;
        Some(window)
    } else {
        open();
        None
    };
    if failed.borrow().is_none() {
        // (until the last window closes)
        slint::run_event_loop()?;
    }
    if let Some(e) = failed.take() {
        return Err(e);
    }
    let Some(client) = client.take() else {
        run.finish()
            .context("clearing the cancelled startup marker")?;
        return Err(anyhow!(
            "the client is locked, and its password wasn't entered"
        ));
    };
    save(&mut client.bound.pages.borrow_mut()).context("saving the session")?;
    // (closed pages don't outlive the client; their downloads go with them)
    client.bound.pages.borrow_mut().forget_closed();
    // (as hydrus stops its downloads on closing)
    client.daemon.borrow_mut().stop(daemon::GRACE);
    run.finish().context("recording a clean client shutdown")?;
    drop(client);
    drop(_open);
    // File > restart: start again, now this client has let go of the store
    if hydrus_gui::client_exit::RESTART.load(std::sync::atomic::Ordering::SeqCst) {
        let exe = std::env::current_exe().context("finding this program to restart it")?;
        std::process::Command::new(exe)
            .args(std::env::args_os().skip(1))
            .spawn()
            .context("restarting the client")?;
    }
    Ok(())
}

/// The open client: its window, its pages, the timer saving them, and the
/// daemon doing the work (with the timer watching it).
struct Client {
    _window: MainWindow,
    bound: Bound,
    _saving: slint::Timer,
    daemon: Rc<RefCell<Daemon>>,
    _watching: slint::Timer,
}

impl Client {
    fn open(store: &Arc<Store>, pages: Pages) -> Result<Self> {
        hydrus_store::settings::apply_network_boot_pause(store)
            .context("applying the network boot pause")?;
        let window = MainWindow::new()?;
        // the daemon, run while none does (and, as the reference's work
        // does, only once the client is unlocked)
        let daemon = Rc::new(RefCell::new(Daemon::new(store.dir())));
        // what to say of it: why it, or its Client API, isn't running
        let say = {
            let store = store.clone();
            let daemon = daemon.clone();
            let weak = window.as_weak();
            move |state: daemon::State| {
                let api = store
                    .read(hydrus_store::settings::get::<ClientApiStatus>)
                    .unwrap_or_default();
                let note = daemon::note(&state, &api, daemon.borrow().started_pid());
                if let Some(window) = weak.upgrade() {
                    let (said, retry) = note.unwrap_or_default();
                    window.set_daemon_note(said.into());
                    window.set_daemon_retry(retry);
                }
            }
        };
        let state = daemon.borrow_mut().check();
        say(state);
        let watching = slint::Timer::default();
        watching.start(slint::TimerMode::Repeated, Duration::from_secs(2), {
            let daemon = daemon.clone();
            let say = say.clone();
            move || {
                let state = daemon.borrow_mut().check();
                say(state);
            }
        });
        window.on_start_daemon_again({
            let daemon = daemon.clone();
            move || {
                let state = daemon.borrow_mut().retry();
                say(state);
            }
        });
        let bound = bind(&window, pages);
        // the last session kept as it changes (the reference saves it every
        // five minutes), and what the Client API asks of the pages done
        let saving = slint::Timer::default();
        saving.start(slint::TimerMode::Repeated, Duration::from_millis(500), {
            let sync = bound.sync.clone();
            move || sync()
        });
        // where hydrus had it, and how big (maximised, by its default); kept
        // as it closes, as the reference keeps it
        let store = bound.pages.borrow().store().clone();
        hydrus_gui::windows::place_named_geometry(window.window(), &store, "main_gui");
        window.show()?;
        Ok(Self {
            _window: window,
            bound,
            _saving: saving,
            daemon,
            _watching: watching,
        })
    }
}

/// Take the lock the client holds while it is open: a second client on the
/// store is refused. (Retried for a moment, as the daemon looks at it now
/// and then.)
fn lock_client(dir: &std::path::Path) -> Result<std::fs::File> {
    for _ in 0..20 {
        if let Some(lock) = hydrus_store::store::lock_gui(dir)
            .with_context(|| format!("opening the lock file in {}", dir.display()))?
        {
            return Ok(lock);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(anyhow!("hydrus-gui is already open on {}", dir.display()))
}

fn save(pages: &mut Pages) -> hydrus_store::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    pages.save(now)
}
