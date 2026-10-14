//! Help > debug's switches, shared by the desktop client and the daemon.
//!
//! The reference is one process, so a switch flipped in its menu reaches
//! every job. Here the work is split (DECISIONS.md, 2026-10-01): downloads,
//! subscriptions, imports and the Client API run in `hydrus serve`. The
//! client writes the switches it flips into the store, the daemon reads them
//! about once a second and flips its own to match, and what the daemon
//! reports goes to the popups the client shows.
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_core::profiling::{self, Mode};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::popups;
use crate::settings::Setting;
use crate::store::Store;

/// Which switches are on, and the profile and planner modes with the time
/// each began (so both processes write the same log).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugSwitches {
    /// Report-mode and other flags that are on, by their menu labels.
    pub flags: Vec<String>,
    pub profile_mode: Option<String>,
    pub profile_started: i64,
    pub query_planner: bool,
    pub planner_started: i64,
    /// How many times "subscription manager snapshot" has been chosen; the
    /// daemon, which has the manager, answers each.
    pub subscription_snapshots: u64,
    /// The same for "simulate a wake from sleep".
    pub wake_simulations: u64,
    /// Counts each change, so that a daemon started long after the client
    /// set them (or left them set, if it crashed) can tell they are old.
    pub generation: u64,
}

/// What the client has asked the daemon to do since it last looked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Requests {
    pub subscription_snapshot: bool,
    pub wake: bool,
}

impl Setting for DebugSwitches {
    const KEY: &'static str = "debug_switches";
}

/// The switches of this process.
pub fn current() -> DebugSwitches {
    let (mode, profile_started) = profiling::mode_and_start();
    let (query_planner, planner_started) = profiling::planner_and_start();
    DebugSwitches {
        flags: Flag::ALL
            .into_iter()
            .filter(|f| f.is_on())
            .map(|f| f.label().to_owned())
            .collect(),
        profile_mode: mode.map(|m| m.name().to_owned()),
        profile_started,
        query_planner,
        planner_started,
        subscription_snapshots: 0,
        wake_simulations: 0,
        generation: 0,
    }
}

/// Write this process's switches where the daemon looks (the client calls it
/// after each switch it flips, and with all off as it boots: the switches
/// are not kept between runs).
pub fn publish(store: &Store) -> Result<()> {
    store.write(move |ctx| {
        let stored = crate::settings::get::<DebugSwitches>(ctx.conn())?;
        let now = DebugSwitches {
            subscription_snapshots: stored.subscription_snapshots,
            wake_simulations: stored.wake_simulations,
            generation: stored.generation,
            ..current()
        };
        if stored != now {
            let now = DebugSwitches {
                generation: stored.generation + 1,
                ..now
            };
            crate::settings::set(ctx.conn(), &now)?;
        }
        Ok(())
    })
}

/// Ask the daemon to simulate a wake from sleep.
pub fn request_wake_simulation(store: &Store) -> Result<()> {
    store.write(move |ctx| {
        let mut stored = crate::settings::get::<DebugSwitches>(ctx.conn())?;
        stored.wake_simulations += 1;
        crate::settings::set(ctx.conn(), &stored)
    })
}

/// Ask the daemon for a subscription manager snapshot.
pub fn request_subscription_snapshot(store: &Store) -> Result<()> {
    store.write(move |ctx| {
        let mut stored = crate::settings::get::<DebugSwitches>(ctx.conn())?;
        stored.subscription_snapshots += 1;
        crate::settings::set(ctx.conn(), &stored)
    })
}

/// Take up the switches in the store (the daemon does this about every
/// second); the client's own are the ones it flips.
pub fn apply(store: &Store) -> Result<Requests> {
    let wanted = debug_flags::quiet(|| store.read(crate::settings::get::<DebugSwitches>))?;
    // (what was set before this process first looked is old: the client that
    // set it may be gone)
    let seen = SEEN_GENERATION.swap(wanted.generation, Ordering::Relaxed);
    if seen != u64::MAX && seen != wanted.generation {
        for flag in Flag::ALL {
            flag.set(wanted.flags.iter().any(|l| l == flag.label()));
        }
        profiling::adopt_mode(
            wanted.profile_mode.as_deref().and_then(Mode::from_name),
            wanted.profile_started,
        );
        profiling::adopt_planner(wanted.query_planner, wanted.planner_started);
    }
    // (what was asked before this process looked is not answered)
    let before = SEEN_SNAPSHOTS.swap(wanted.subscription_snapshots, Ordering::Relaxed);
    let woke = SEEN_WAKES.swap(wanted.wake_simulations, Ordering::Relaxed);
    Ok(Requests {
        subscription_snapshot: before != u64::MAX && wanted.subscription_snapshots > before,
        wake: woke != u64::MAX && wanted.wake_simulations > woke,
    })
}

static SEEN_SNAPSHOTS: AtomicU64 = AtomicU64::new(u64::MAX);
static SEEN_WAKES: AtomicU64 = AtomicU64::new(u64::MAX);
static SEEN_GENERATION: AtomicU64 = AtomicU64::new(u64::MAX);

/// Send reports (`HydrusData.ShowText`) to the console and the popups in
/// `store`, unless a sink is installed already. Returns the sink so that a
/// caller with its own console can add to it.
pub fn report_to_popups(store: &Arc<Store>) -> Box<dyn Fn(&str) + Send + Sync> {
    // (a report can be made from inside a database job, so the popup is
    // written by a thread of its own rather than waited for)
    let (lines, queue) = std::sync::mpsc::channel::<String>();
    let store = Arc::downgrade(store);
    std::thread::spawn(move || {
        for text in queue {
            let Some(store) = store.upgrade() else { break };
            let at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            #[allow(clippy::cast_precision_loss)] // (seconds)
            let job = popups::Job::text(text, at as f64);
            #[allow(clippy::cast_possible_wrap)] // (seconds)
            let at = at as i64;
            let done =
                debug_flags::quiet(|| store.write(move |ctx| popups::add(ctx.conn(), &job, at)));
            if let Err(e) = done {
                eprintln!("could not show the report: {e}");
            }
        }
    });
    let lines = std::sync::Mutex::new(lines);
    Box::new(move |text| {
        let _ = lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send(text.to_owned());
    })
}
