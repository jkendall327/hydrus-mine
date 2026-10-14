//! The crash log: every panic, on any thread, is written to a file in the
//! database directory, with the time, the version, the thread, the message
//! and a backtrace.
//!
//! The reference logs crashes only while Help > debug > "use faulthandler to
//! log crashes" is on. hydrus-rs always does (a deliberate step beyond the
//! reference: see `docs/rust/DIFFERENCES.md`), so a problem on a real
//! collection can be diagnosed afterwards.
//!
//! The panic hook is process-wide, so a panic on a worker thread (or in a
//! tokio task, which the runtime catches) is logged the same as one on the
//! main thread, before the thread unwinds.
use std::io::Write as _;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

/// The log's file name in the database directory.
pub const LOG_NAME: &str = "client crash.log";
/// Where the log goes when it grows past [`MAX_BYTES`]: the earlier log is
/// replaced, so the two files together stay under about twice the cap.
pub const OLD_LOG_NAME: &str = "client crash.log.old";
/// The log is rotated before an entry that would take it past this.
pub const MAX_BYTES: u64 = 512 * 1024;

/// The version the logs name: `git describe` at build time, with the crate
/// version.
pub fn version() -> String {
    format!(
        "{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("HYDRUS_GIT_DESCRIBE")
    )
}

struct Target {
    dir: PathBuf,
    program: String,
}

static TARGET: Mutex<Option<Target>> = Mutex::new(None);
// (one entry is written at a time, so rotation and appends never interleave)
static WRITING: Mutex<()> = Mutex::new(());
static HOOKED: std::sync::Once = std::sync::Once::new();

/// Log panics to `dir` from now on, as `program` (`hydrus-gui`, `hydrus
/// serve`...). Calling it again points the log at the new directory. The
/// earlier panic hook still runs, so the message still reaches stderr, and
/// a line after it says where the log is.
///
/// The hook is set once per process (a `Once`), taking the hook in place
/// at the first call, so this must be the last thing that sets a panic hook:
/// a hook set after it replaces the log, and one set before it is chained.
pub fn install(dir: &Path, program: &str) {
    *TARGET.lock().unwrap_or_else(PoisonError::into_inner) = Some(Target {
        dir: dir.to_owned(),
        program: program.to_owned(),
    });
    HOOKED.call_once(|| {
        let earlier = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            earlier(info);
            if let Some(path) = log_panic(info) {
                // (the hook must not panic: a failed write is ignored)
                let _ = writeln!(std::io::stderr(), "hydrus: crash log: {}", path.display());
            }
        }));
    });
}

/// Stop logging (for tests that point the log somewhere short-lived).
pub fn uninstall() {
    *TARGET.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

/// Where the log is, once [`install`] has been called.
pub fn path() -> Option<PathBuf> {
    TARGET
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|t| t.dir.join(LOG_NAME))
}

fn log_panic(info: &PanicHookInfo<'_>) -> Option<PathBuf> {
    // (a panic inside a panic hook aborts the process: whatever goes wrong
    // here, the original panic must carry on)
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| log_panic_inner(info)))
        .ok()
        .flatten()
}

fn log_panic_inner(info: &PanicHookInfo<'_>) -> Option<PathBuf> {
    let (dir, program) = {
        let guard = TARGET.lock().unwrap_or_else(PoisonError::into_inner);
        let target = guard.as_ref()?;
        (target.dir.clone(), target.program.clone())
    };
    let thread = std::thread::current();
    let entry = format_entry(
        &jiff::Timestamp::now().to_string(),
        &program,
        thread.name().unwrap_or("<unnamed>"),
        &info.to_string(),
        &std::backtrace::Backtrace::force_capture().to_string(),
    );
    write_entry(&dir, &entry).ok()
}

/// One log entry.
pub fn format_entry(
    time: &str,
    program: &str,
    thread: &str,
    panic: &str,
    backtrace: &str,
) -> String {
    format!(
        "==== panic ====\ntime: {time}\nversion: {}\nprogram: {program}\nthread: {thread}\n{panic}\nbacktrace:\n{backtrace}\n\n",
        version()
    )
}

/// Append an entry to the log in `dir`, rotating first if it would pass
/// the cap. Returns the log's path.
pub fn write_entry(dir: &Path, entry: &str) -> std::io::Result<PathBuf> {
    let _one = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
    let path = dir.join(LOG_NAME);
    let size = std::fs::metadata(&path).map_or(0, |m| m.len());
    if size > 0 && size + entry.len() as u64 > MAX_BYTES {
        // (a rename over the old log replaces it; on Windows too)
        let _ = std::fs::remove_file(dir.join(OLD_LOG_NAME));
        // (if it can't be moved, the entry is still appended)
        let _ = std::fs::rename(&path, dir.join(OLD_LOG_NAME));
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(entry.as_bytes())?;
    Ok(path)
}
