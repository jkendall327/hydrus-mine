//! One turn of the daemon's two duplicates workers, so that what `hydrus
//! serve` does and what the tests drive are the same code: the similar-files
//! search (finding potential pairs as files come in) and the auto-resolution
//! rules. Each turn reads its switches, asks the idle state the GUI
//! published, works if the switches allow it for that state, and says how
//! long the worker rests before its next turn.

use std::time::{Duration, Instant};

use hydrus_search::Clock;
use hydrus_store::Store;
use hydrus_store::duplicates::auto::AutoResolutionSettings;
use hydrus_store::idle_state::{self, Pace};
use hydrus_store::similar::{self, SimilarFilesSettings};

use crate::engine::{Orientation, WorkDone, work_rules};

/// What a turn did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Turn<T> {
    /// The switches do not allow work in this state (idle or normal time).
    Held,
    /// It worked, or found nothing to do.
    Worked(T),
    /// It failed, with why.
    Failed(String),
}

/// A turn and how long to rest after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tick<T> {
    pub turn: Turn<T>,
    pub rest: Duration,
}

/// Files the similar-files search looked at in a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Searched {
    pub files: usize,
    /// Whether there may be more to search.
    pub more: bool,
}

/// A turn of the similar-files search (`hydrus serve`'s searcher): packets
/// of work and rests, by the idle state published at `now_ms`. "Work hard"
/// (the preparation tab's button) works whatever the state.
pub fn similar_files_turn(store: &Store, now_ms: i64) -> Tick<Searched> {
    let settings: SimilarFilesSettings = store.read(hydrus_store::settings::get).unwrap_or_default();
    let idle = idle_state::is_idle(store.dir(), now_ms);
    let mut pace = settings.pace(idle);
    pace.allowed |= settings.work_hard;
    if !pace.allowed {
        return Tick {
            turn: Turn::Held,
            rest: Duration::from_secs(10),
        };
    }
    let started = Instant::now();
    let mut files = 0;
    let more = loop {
        match similar::run_search(store, 16) {
            Ok(n) => {
                files += n;
                if n == 0 {
                    break false;
                }
                if started.elapsed() >= pace.work {
                    break true;
                }
            }
            Err(e) => {
                return Tick {
                    turn: Turn::Failed(e.to_string()),
                    rest: Duration::from_secs(600),
                };
            }
        }
    };
    let rest = if files > 0 && more {
        pace.rest(started.elapsed()).max(Duration::from_millis(100))
    } else {
        Duration::from_secs(30)
    };
    Tick {
        turn: Turn::Worked(Searched { files, more }),
        rest,
    }
}

/// A turn of the auto-resolution rules (`hydrus serve`'s resolver): bursts
/// of work with rests between, by the idle state published at `now_ms`.
pub fn auto_resolution_turn(
    store: &Store,
    now_ms: i64,
    orientation: &mut dyn Orientation,
    clock: &Clock,
) -> Tick<WorkDone> {
    let settings: AutoResolutionSettings = match store.read(hydrus_store::settings::get) {
        Ok(s) => s,
        Err(e) => {
            return Tick {
                turn: Turn::Failed(e.to_string()),
                rest: Duration::from_secs(600),
            };
        }
    };
    let pace: Pace = settings.pace(idle_state::is_idle(store.dir(), now_ms));
    if !pace.allowed {
        return Tick {
            turn: Turn::Held,
            rest: Duration::from_secs(10),
        };
    }
    let started = Instant::now();
    match work_rules(store, pace.work, orientation, clock) {
        Ok(done) if done.more_to_do => Tick {
            rest: pace.rest(started.elapsed()).max(Duration::from_millis(100)),
            turn: Turn::Worked(done),
        },
        // the reference rests ten minutes unless woken by new pairs;
        // checking every minute stands in for that
        Ok(done) => Tick {
            turn: Turn::Worked(done),
            rest: Duration::from_secs(60),
        },
        Err(e) => Tick {
            turn: Turn::Failed(e.to_string()),
            rest: Duration::from_secs(600),
        },
    }
}
