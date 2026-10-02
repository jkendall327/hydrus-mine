//! A watcher page's watchers, made and worked as the page makes and works
//! them (the reference's `MultipleWatcherImport.AddURL` and the
//! `WatcherImport` controls its list and its highlighted watcher's box
//! give): one per thread, with the page's checker and import options;
//! checking now; pausing and resuming checking. The daemon is nudged
//! after each.

use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::subscriptions::{CheckerOptions, SeedTime};
use hydrus_core::watchers::{CheckerStatus, WatcherState};

use crate::error::Result;
use crate::queues::{self, FileSeed, Queue, QueueKind};
use crate::store::Store;

/// The name of a watcher page made without one (the reference's).
pub const DEFAULT_WATCHER_PAGE_NAME: &str = "watcher";

/// A watcher queue's state.
pub fn watcher_state(queue: &Queue) -> Option<WatcherState> {
    (queue.kind == QueueKind::Watcher)
        .then(|| serde_json::from_value(queue.extra.clone()).ok())
        .flatten()
}

/// The times its checker's velocity counts, of a watcher's files.
pub fn seed_times(seeds: &[FileSeed]) -> Vec<SeedTime> {
    seeds
        .iter()
        .map(|s| SeedTime {
            source_time: s.source_time,
            created: s.created,
        })
        .collect()
}

/// A watcher page: its name and key, its watchers, and what new ones get.
#[derive(Debug, Clone)]
pub struct WatcherPage<'a> {
    pub name: &'a str,
    pub key: Option<&'a [u8]>,
    pub queues: &'a [i64],
    pub checker: &'a CheckerOptions,
    pub options: &'a ImportOptionsSlice,
}

/// What adding a URL to a page did.
#[derive(Debug, Clone, PartialEq)]
pub enum Watched {
    /// A new watcher.
    New(Box<Queue>),
    /// The page watches the thread already: that watcher's queue.
    Already(i64),
    /// Not a URL (the reference skips it).
    NotAUrl,
}

/// Watch `url` on `page` (`AddURL`), as the server would be asked for it
/// (normalised), unless the page watches it already.
pub fn add_watcher(store: &Store, page: &WatcherPage<'_>, url: &str, now: i64) -> Result<Watched> {
    let url = url.trim();
    if url.is_empty() || hydrus_core::url::functions::check_full_url(url).is_err() {
        return Ok(Watched::NotAUrl);
    }
    let url = store
        .snapshot()
        .url_classes
        .normalise(url, true)
        .unwrap_or_else(|_| url.to_owned());
    let (queues, name, key) = (
        page.queues.to_vec(),
        page.name.to_owned(),
        page.key.map(<[u8]>::to_vec),
    );
    let (checker, options) = (page.checker.clone(), page.options.clone());
    store.write(move |ctx| {
        let conn = ctx.conn();
        for &id in &queues {
            let existing = queues::queue(conn, id)?;
            if existing
                .as_ref()
                .and_then(watcher_state)
                .is_some_and(|w| w.url == url)
            {
                return Ok(Watched::Already(id));
            }
        }
        let state = WatcherState::new(url, checker, now);
        let extra = serde_json::to_value(&state).expect("plain data serialises");
        let id = queues::create_queue(
            conn,
            QueueKind::Watcher,
            &name,
            key.as_deref(),
            &options,
            now,
        )?;
        queues::set_queue_extra(conn, id, &extra)?;
        queues::nudge(conn, id)?;
        Ok(Watched::New(Box::new(
            queues::queue(conn, id)?.expect("just made"),
        )))
    })
}

/// Check a watcher's thread again soon, alive again (`CheckNow`).
pub fn check_now(store: &Store, queue: i64, now: i64) -> Result<()> {
    change(store, queue, move |state, seeds| {
        state.check_now(&seed_times(seeds), now);
    })
}

/// Pause or resume a watcher's checking (`PausePlayChecking`): a dead or
/// 404 watcher stays paused until checked again.
pub fn pause_play_checking(store: &Store, queue: i64) -> Result<()> {
    change(store, queue, |state, _| {
        let dead = state.status != CheckerStatus::Ok;
        if !(state.checking_paused && dead) {
            state.checking_paused = !state.checking_paused;
        }
    })
}

/// Change a watcher's state, and nudge the daemon.
fn change(
    store: &Store,
    queue: i64,
    change: impl FnOnce(&mut WatcherState, &[FileSeed]) + Send + 'static,
) -> Result<()> {
    store.write(move |ctx| {
        let conn = ctx.conn();
        let Some(mut state) = queues::queue(conn, queue)?.as_ref().and_then(watcher_state) else {
            return Ok(());
        };
        let seeds = queues::file_seeds(conn, queue)?;
        change(&mut state, &seeds);
        let extra = serde_json::to_value(&state).expect("plain data serialises");
        queues::set_queue_extra(conn, queue, &extra)?;
        queues::nudge(conn, queue)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_KEY: &[u8] = b"page";

    fn page<'a>(
        queues: &'a [i64],
        checker: &'a CheckerOptions,
        options: &'a ImportOptionsSlice,
    ) -> WatcherPage<'a> {
        WatcherPage {
            name: "threads",
            key: Some(PAGE_KEY),
            queues,
            checker,
            options,
        }
    }

    fn state(store: &Store, queue: i64) -> WatcherState {
        store
            .read(|c| queues::queue(c, queue))
            .unwrap()
            .as_ref()
            .and_then(watcher_state)
            .unwrap()
    }

    #[test]
    fn a_page_watches_each_thread_once_with_its_own_options() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let checker = CheckerOptions {
            never_faster_than: 123,
            ..CheckerOptions::default()
        };
        let options = ImportOptionsSlice::default();
        let url = "https://boards.example/thread/1";
        let Watched::New(made) =
            add_watcher(&store, &page(&[], &checker, &options), url, 1000).unwrap()
        else {
            panic!("a new watcher");
        };
        assert_eq!(made.name, "threads");
        assert_eq!(made.page_key.as_deref(), Some(PAGE_KEY));
        assert_eq!(made.created, 1000);
        let made_state = state(&store, made.id);
        assert_eq!(made_state.url, url);
        assert_eq!(made_state.checker, checker);
        assert_eq!(made_state.created, 1000);
        // the daemon is told
        assert!(store.read(queues::any_nudged).unwrap());
        // the same thread again is the same watcher; not a URL is nothing
        let watched = [made.id];
        assert_eq!(
            add_watcher(&store, &page(&watched, &checker, &options), url, 2000).unwrap(),
            Watched::Already(made.id)
        );
        for text in ["", "  ", "not a url"] {
            assert_eq!(
                add_watcher(&store, &page(&watched, &checker, &options), text, 2000).unwrap(),
                Watched::NotAUrl
            );
        }
        // (another page may watch it too)
        assert!(matches!(
            add_watcher(&store, &page(&[], &checker, &options), url, 2000).unwrap(),
            Watched::New(_)
        ));
    }

    #[test]
    fn checking_now_and_pausing_checking() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let (checker, options) = (CheckerOptions::default(), ImportOptionsSlice::default());
        let Watched::New(made) = add_watcher(
            &store,
            &page(&[], &checker, &options),
            "https://boards.example/thread/1",
            1000,
        )
        .unwrap() else {
            panic!("a new watcher");
        };
        let queue = made.id;
        pause_play_checking(&store, queue).unwrap();
        assert!(state(&store, queue).checking_paused);
        pause_play_checking(&store, queue).unwrap();
        assert!(!state(&store, queue).checking_paused);
        // a dead watcher, paused, stays so until checked now
        let mut dead = state(&store, queue);
        dead.status = CheckerStatus::Dead;
        dead.checking_paused = true;
        let extra = serde_json::to_value(&dead).unwrap();
        store
            .write(move |ctx| queues::set_queue_extra(ctx.conn(), queue, &extra))
            .unwrap();
        pause_play_checking(&store, queue).unwrap();
        assert!(state(&store, queue).checking_paused);
        check_now(&store, queue, 5000).unwrap();
        let checked = state(&store, queue);
        assert!(checked.check_now && !checked.checking_paused);
        assert_eq!(checked.status, CheckerStatus::Ok);
    }
}
