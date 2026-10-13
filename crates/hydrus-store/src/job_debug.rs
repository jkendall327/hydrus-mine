//! Help > debug's database hooks, around each read and write job (the
//! reference's `HydrusDB` job loop and `HydrusDBBase._Execute`): db report
//! mode, db profile mode and query planner mode.
use std::cell::{Cell, RefCell};

use hydrus_core::debug_flags::{self, Flag};
use hydrus_core::profiling::{self, Mode};
use rusqlite::trace::{TraceEvent, TraceEventCodes};
use rusqlite::{Connection, types::Null};

thread_local! {
    /// Queries run by this thread's job that the planner has yet to plan.
    static PENDING: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Whether this thread's job touched the popup tables (the popups'
    /// own traffic is not reported, or each report would make another).
    static TOUCHED_POPUPS: Cell<bool> = const { Cell::new(false) };
}

/// Have `conn` tell [`note`] about each statement it runs.
pub(crate) fn instrument(conn: &Connection) {
    conn.trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(on_trace));
}

// (a `fn` pointer, so it takes the event by value)
#[allow(clippy::needless_pass_by_value)]
fn on_trace(event: TraceEvent<'_>) {
    if let TraceEvent::Stmt(_, sql) = event {
        note(sql);
    }
}

fn note(sql: &str) {
    if Flag::DbReport.is_on() && sql.contains("popup") {
        TOUCHED_POPUPS.with(|t| t.set(true));
    }
    if !profiling::query_planner_on() {
        return;
    }
    let sql = sql.trim();
    let word = sql.split_whitespace().next().unwrap_or_default();
    // (transaction plumbing the reference does not plan, trigger comments and
    // the plans themselves)
    if sql.starts_with("--")
        || ["EXPLAIN", "BEGIN", "COMMIT"]
            .iter()
            .any(|w| word.eq_ignore_ascii_case(w))
    {
        return;
    }
    if !profiling::needs_plan(sql) {
        return;
    }
    PENDING.with(|p| {
        let mut p = p.borrow_mut();
        if !p.iter().any(|q| q == sql) {
            p.push(sql.to_owned());
        }
    });
}

/// `query planner mode`'s `EXPLAIN QUERY PLAN` of each query the job ran.
fn plan_pending(conn: &Connection) {
    let pending = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for query in pending {
        let plan = (|| -> rusqlite::Result<Vec<String>> {
            let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {query}"))?;
            let nulls = std::iter::repeat_n(Null, stmt.parameter_count());
            let rows = stmt.query_map(rusqlite::params_from_iter(nulls), |row| {
                Ok(profiling::plan_row(
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    &row.get::<_, String>(3)?,
                ))
            })?;
            rows.collect()
        })()
        .unwrap_or_default();
        if let Some(dir) = database_dir(conn) {
            profiling::print_query_plan(&dir, &query, &plan);
        }
    }
}

/// The directory of the database `conn` is open on, where the logs go.
fn database_dir(conn: &Connection) -> Option<std::path::PathBuf> {
    std::path::Path::new(conn.path()?).parent().map(Into::into)
}

/// Run one job (`f`, named `name`) with the debug modes' hooks: the report,
/// the profile and the plans. `quiet` jobs are not reported.
pub(crate) fn run<R>(conn: &Connection, name: &str, quiet: bool, f: impl FnOnce() -> R) -> R {
    TOUCHED_POPUPS.with(|t| t.set(false));
    let result = if let (true, Some(dir)) = (profiling::is_mode(Mode::Db), database_dir(conn)) {
        profiling::profile(
            &dir,
            &format!("Profiling db job: {name}"),
            profiling::DB_MIN_MS,
            f,
        )
    } else {
        f()
    };
    if !quiet && !TOUCHED_POPUPS.with(Cell::get) {
        debug_flags::report(Flag::DbReport, || format!("Running db job: {name}"));
    }
    plan_pending(conn);
    result
}
