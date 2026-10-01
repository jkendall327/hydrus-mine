//! `hydrus pause` and `hydrus resume`: the global pause switches (the
//! reference's "network > pause" menu, and its import and export folder
//! switches). A running `serve` notices a change within half a minute.

use std::path::Path;

use anyhow::Result;
use clap::ValueEnum;

use hydrus_store::Store;
use hydrus_store::settings::{FolderSettings, Pauses};

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
    /// Every import folder.
    ImportFolders,
    /// Every export folder.
    ExportFolders,
}

fn field<'a>(pauses: &'a mut Pauses, folders: &'a mut FolderSettings, what: What) -> &'a mut bool {
    match what {
        What::Subscriptions => &mut pauses.subscriptions,
        What::Network => &mut pauses.network_traffic,
        What::Queues => &mut pauses.paged_importers,
        What::FileQueues => &mut pauses.file_queues,
        What::GallerySearches => &mut pauses.gallery_searches,
        What::Watchers => &mut pauses.watcher_checkers,
        What::ImportFolders => &mut folders.pause_import_folders,
        What::ExportFolders => &mut folders.pause_export_folders,
    }
}

/// Switch `what` on or off, or (with none) say what is paused.
pub fn run(dir: &Path, what: Option<What>, paused: bool) -> Result<()> {
    let store = Store::open(dir)?;
    let (mut pauses, mut folders): (Pauses, FolderSettings) = store.read(|conn| {
        Ok((
            hydrus_store::settings::get(conn)?,
            hydrus_store::settings::get(conn)?,
        ))
    })?;
    if let Some(what) = what {
        *field(&mut pauses, &mut folders, what) = paused;
        let (p, f) = (pauses, folders);
        store.write(move |ctx| {
            hydrus_store::settings::set(ctx.conn(), &p)?;
            hydrus_store::settings::set(ctx.conn(), &f)
        })?;
    }
    for what in What::value_variants() {
        let name = what
            .to_possible_value()
            .expect("named")
            .get_name()
            .to_owned();
        let state = if *field(&mut pauses, &mut folders, *what) {
            "paused"
        } else {
            "running"
        };
        println!("{name:>16}: {state}");
    }
    Ok(())
}
