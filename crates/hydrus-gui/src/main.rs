//! `hydrus-gui <store>`: the desktop client over a hydrus-rs store.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use slint::ComponentHandle as _;

use hydrus_core::lock::LockPassword;
use hydrus_gui::{Bound, MainWindow, Pages, bind, unlock_window};
use hydrus_store::Store;

fn main() -> Result<()> {
    let dir: PathBuf = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow!("usage: hydrus-gui <store directory>"))?
        .into();
    let store =
        Store::open(&dir).with_context(|| format!("opening the store at {}", dir.display()))?;
    let lock: LockPassword = store
        .read(hydrus_store::settings::get)
        .context("reading the lock password")?;
    let client: Rc<RefCell<Option<Client>>> = Rc::default();
    let failed: Rc<RefCell<Option<anyhow::Error>>> = Rc::default();
    let open = {
        let (client, failed) = (client.clone(), failed.clone());
        move || match Client::open(store) {
            Ok(opened) => *client.borrow_mut() = Some(opened),
            Err(e) => {
                *failed.borrow_mut() = Some(e);
                let _ = slint::quit_event_loop();
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
        return Err(anyhow!(
            "the client is locked, and its password wasn't entered"
        ));
    };
    save(&mut client.bound.pages.borrow_mut()).context("saving the session")?;
    Ok(())
}

/// The open client: its window, its pages, and the timer saving them.
struct Client {
    _window: MainWindow,
    bound: Bound,
    _saving: slint::Timer,
}

impl Client {
    fn open(store: Arc<Store>) -> Result<Self> {
        let window = MainWindow::new()?;
        let pages = Pages::open(store).context("opening the last session")?;
        let bound = bind(&window, pages);
        // as the reference does: the last session every five minutes, and on exit
        let saving = slint::Timer::default();
        saving.start(slint::TimerMode::Repeated, Duration::from_secs(300), {
            let pages = bound.pages.clone();
            move || {
                if let Err(e) = save(&mut pages.borrow_mut()) {
                    eprintln!("saving the session failed: {e}");
                }
            }
        });
        window.show()?;
        Ok(Self {
            _window: window,
            bound,
            _saving: saving,
        })
    }
}

fn save(pages: &mut Pages) -> hydrus_store::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    pages.save(now)
}
