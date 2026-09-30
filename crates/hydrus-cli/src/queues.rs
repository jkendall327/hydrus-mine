//! `hydrus queues`: the downloaders' queues (URL downloaders, gallery
//! searches, watchers) by the page they belong to, including those carried
//! over from the reference's session.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use hydrus_download::queue::{gallery_search, watcher_state};
use hydrus_store::Store;
use hydrus_store::queues::{self, Queue, QueueKind, SeedStatus};

#[derive(Subcommand, Clone, Copy)]
pub enum Action {
    /// Every URL downloader, gallery search and watcher, by page.
    List,
}

/// A queue with how many of its files have each status.
type Counted = (Queue, BTreeMap<SeedStatus, usize>);

pub fn run(dir: &Path, action: Action) -> Result<()> {
    let store = Store::open(dir)?;
    match action {
        Action::List => {
            let found = store.read(|conn| {
                let mut out = Vec::new();
                for queue in queues::queues(conn, None)? {
                    if matches!(
                        queue.kind,
                        QueueKind::Urls | QueueKind::Gallery | QueueKind::Watcher
                    ) {
                        let counts = queues::file_seed_counts(conn, queue.id)?;
                        out.push((queue, counts));
                    }
                }
                Ok(out)
            })?;
            // pages in the order their first queue was made
            let mut pages: Vec<(String, Vec<Counted>)> = Vec::new();
            for (queue, counts) in found {
                match pages.iter_mut().find(|(name, _)| *name == queue.name) {
                    Some((_, queues)) => queues.push((queue, counts)),
                    None => pages.push((queue.name.clone(), vec![(queue, counts)])),
                }
            }
            for (page, queues) in pages {
                println!("{page}");
                for (queue, counts) in queues {
                    println!("  {}", describe(&queue, &counts));
                }
            }
            Ok(())
        }
    }
}

fn describe(queue: &Queue, counts: &BTreeMap<SeedStatus, usize>) -> String {
    let (what, checking_paused) = match queue.kind {
        QueueKind::Gallery => match gallery_search(queue) {
            Some(search) => (
                format!("search \"{}\" ({})", search.query, search.source_name),
                false,
            ),
            None => ("search".to_owned(), false),
        },
        QueueKind::Watcher => match watcher_state(queue) {
            Some(state) => (
                format!("watcher {} ({})", state.url, state.subject),
                state.checking_paused,
            ),
            None => ("watcher".to_owned(), false),
        },
        _ => ("urls".to_owned(), false),
    };
    let total: usize = counts.values().sum();
    let to_do = counts.get(&SeedStatus::Unknown).copied().unwrap_or(0);
    let failed = counts.get(&SeedStatus::Error).copied().unwrap_or(0);
    let mut paused = Vec::new();
    if queue.files_paused {
        paused.push("files paused");
    }
    if queue.kind == QueueKind::Watcher {
        if checking_paused {
            paused.push("checking paused");
        }
    } else if queue.gallery_paused && queue.kind == QueueKind::Gallery {
        paused.push("search paused");
    }
    let paused = if paused.is_empty() {
        String::new()
    } else {
        format!(" [{}]", paused.join(", "))
    };
    format!("{what}: {total} files, {to_do} to do, {failed} failed{paused}")
}
