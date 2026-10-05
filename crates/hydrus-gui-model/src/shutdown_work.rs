//! Exiting the client (`FrameGUI.TryToExit` and `Controller.Exit`): the exit
//! and restart questions, whether shutdown maintenance runs (Options >
//! maintenance and processing > shutdown, or File > exit/force
//! maintenance), what it asks first, and the work itself.

use hydrus_core::numbers::human_int;
use hydrus_store::Store;
use hydrus_store::settings::ShutdownWork;

/// How the client is exiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExitMode {
    #[default]
    Exit,
    /// File > restart: start again after exiting.
    Restart,
    /// File > exit/force maintenance: run shutdown work whether due or not.
    ForceMaintenance,
}

impl ExitMode {
    /// The exit confirmation.
    pub const fn question(self) -> &'static str {
        match self {
            Self::Restart => {
                "Are you sure you want to restart the client? (Will auto-yes in 15 seconds)"
            }
            _ => "Are you sure you want to exit the client? (Will auto-yes in 15 seconds)",
        }
    }
}

/// The run-jobs-on-shutdown choices, in the reference's order
/// (`idle_string_lookup`).
pub const ACTIONS: [&str; 3] = [
    "do not run jobs on shutdown",
    "run jobs on shutdown if needed",
    "run jobs on shutdown if needed, but ask first",
];

/// What to do about shutdown maintenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Skip,
    Run,
    /// Ask this ("Maintenance is due", auto-no in 15 seconds); no registers
    /// the work as done so it isn't asked again.
    Ask(String),
}

/// The "Maintenance is due" title.
pub const ASK_TITLE: &str = "Maintenance is due";

/// The outstanding work (`GetIdleShutdownWorkDue`).
pub fn work_due(store: &Store) -> Vec<String> {
    let due = store
        .read(hydrus_store::db_maintenance::tables_due_analysis)
        .unwrap_or_default();
    if due.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "analyze {} table_names",
            human_int(due.len() as u64)
        )]
    }
}

/// Whether shutdown work runs, at `now` (seconds), given the outstanding work.
pub fn decide(settings: &ShutdownWork, mode: ExitMode, now: i64, work: &[String]) -> Decision {
    if mode == ExitMode::ForceMaintenance {
        return Decision::Run;
    }
    let period = i64::try_from(settings.period_seconds).unwrap_or(i64::MAX);
    if now <= settings.last_done.saturating_add(period) {
        return Decision::Skip;
    }
    match settings.action {
        1 => Decision::Run,
        2 if !work.is_empty() => Decision::Ask(format!(
            "Is now a good time for the client to do up to {} minutes' maintenance work? (Will auto-no in 15 seconds)\n\nThe outstanding jobs appear to be:\n\n{}",
            human_int(u64::from(settings.max_minutes)),
            work.join("\n")
        )),
        _ => Decision::Skip,
    }
}

/// Record that shutdown work was done (or declined) at `now`.
pub fn register(store: &Store, now: i64) -> hydrus_store::Result<()> {
    store.write(move |ctx| {
        let mut work: ShutdownWork = hydrus_store::settings::get(ctx.conn())?;
        work.last_done = now;
        hydrus_store::settings::set(ctx.conn(), &work)
    })
}

/// Do the shutdown work for at most its minutes, then register it.
pub fn run(store: &Store, now: i64) -> hydrus_store::Result<usize> {
    let settings: ShutdownWork = store.read(hydrus_store::settings::get)?;
    let stop = std::time::Instant::now()
        + std::time::Duration::from_secs(u64::from(settings.max_minutes) * 60);
    let done = store.write(move |ctx| {
        let tables = hydrus_store::db_maintenance::tables_due_analysis(ctx.conn())?;
        hydrus_store::db_maintenance::analyze_tables(ctx.conn(), &tables, stop)
    })?;
    register(store, now)?;
    Ok(done)
}
