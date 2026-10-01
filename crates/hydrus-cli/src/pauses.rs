//! `hydrus pause` and `hydrus resume`: the global pause switches (the
//! reference's "network > pause" menu). A running `serve` notices a change
//! within half a minute.

use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;

use hydrus_store::Store;
use hydrus_store::settings::Pauses;

/// What to pause or resume.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum What {
    /// Subscriptions.
    Subscriptions,
    /// Every new request (subscriptions wait too).
    Network,
    /// Every downloader queue: URL queues, gallery searches and watchers.
    Queues,
    /// The queues' file downloads.
    FileQueues,
    /// Gallery pages (gallery searches, and gallery URLs in URL queues).
    GallerySearches,
    /// Watchers' thread checks.
    Watchers,
}

fn field(pauses: &mut Pauses, what: What) -> &mut bool {
    match what {
        What::Subscriptions => &mut pauses.subscriptions,
        What::Network => &mut pauses.network_traffic,
        What::Queues => &mut pauses.paged_importers,
        What::FileQueues => &mut pauses.file_queues,
        What::GallerySearches => &mut pauses.gallery_searches,
        What::Watchers => &mut pauses.watcher_checkers,
    }
}

/// Switch `what` on or off, or (with none) say what is paused.
pub fn run(dir: &Path, what: Option<What>, paused: bool) -> Result<()> {
    let store = Store::open(dir)?;
    let mut pauses: Pauses = store.read(hydrus_store::settings::get)?;
    if let Some(what) = what {
        *field(&mut pauses, what) = paused;
        store.write(move |ctx| hydrus_store::settings::set(ctx.conn(), &pauses))?;
    }
    for what in What::value_variants() {
        let name = what
            .to_possible_value()
            .expect("named")
            .get_name()
            .to_owned();
        let state = if *field(&mut pauses, *what) {
            "paused"
        } else {
            "running"
        };
        println!("{name:>16}: {state}");
    }
    Ok(())
}
