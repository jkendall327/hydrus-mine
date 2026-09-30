//! `hydrus-gui <store>`: the desktop client over a hydrus-rs store.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, anyhow};
use slint::ComponentHandle as _;

use hydrus_gui::{MainWindow, Pages, bind};

fn main() -> Result<()> {
    let dir: PathBuf = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow!("usage: hydrus-gui <store directory>"))?
        .into();
    let store = hydrus_store::Store::open(&dir)
        .with_context(|| format!("opening the store at {}", dir.display()))?;
    let window = MainWindow::new()?;
    let pages = Pages::open(store).context("opening the last session")?;
    let bound = bind(&window, pages);
    // as the reference does: the last session every five minutes, and on exit
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_secs(300), {
        let pages = bound.pages.clone();
        move || {
            if let Err(e) = save(&mut pages.borrow_mut()) {
                eprintln!("saving the session failed: {e}");
            }
        }
    });
    window.run()?;
    save(&mut bound.pages.borrow_mut()).context("saving the session")?;
    Ok(())
}

fn save(pages: &mut Pages) -> hydrus_store::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    pages.save(now)
}
