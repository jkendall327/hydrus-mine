//! Opening the store the way the desktop client starts: its lease first (a
//! second client is refused), then any restore a File > restart left for
//! this start, then the store itself.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use hydrus_store::Store;

/// Acquire the desktop lease before restoring or opening the database.
/// A competing client cannot consume or execute a queued restore request.
pub fn open_store(dir: &std::path::Path) -> Result<(Arc<Store>, std::fs::File)> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating the store directory {}", dir.display()))?;
    let open = lock_client(dir)?;
    hydrus_store::backup::restore_pending(dir, &mut |text| eprintln!("{text}"))
        .with_context(|| format!("restoring the pending backup in {}", dir.display()))?;
    let store =
        Store::open(dir).with_context(|| format!("opening the store at {}", dir.display()))?;
    Ok((store, open))
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
