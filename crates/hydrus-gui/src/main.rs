//! `hydrus-gui <store>`: the desktop client over a hydrus-rs store.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result, anyhow};
use slint::ComponentHandle as _;

use hydrus_gui::{MainWindow, SearchPage, bind};

fn main() -> Result<()> {
    let dir: PathBuf = std::env::args_os()
        .nth(1)
        .ok_or_else(|| anyhow!("usage: hydrus-gui <store directory>"))?
        .into();
    let store = hydrus_store::Store::open(&dir)
        .with_context(|| format!("opening the store at {}", dir.display()))?;
    let window = MainWindow::new()?;
    bind(&window, Rc::new(RefCell::new(SearchPage::new(store))));
    window.run()?;
    Ok(())
}
