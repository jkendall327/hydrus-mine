//! The crash log, in a temp store. The panic hook is process-wide, so these
//! tests take turns.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use hydrus_core::crash_log;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn log(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join(crash_log::LOG_NAME)).unwrap_or_default()
}

#[test]
fn a_panic_on_the_main_path_is_logged_with_its_fields() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    crash_log::install(dir.path(), "hydrus-test");
    let _ = catch_unwind(AssertUnwindSafe(|| panic!("main path failure")));
    let text = log(dir.path());
    assert!(text.contains("==== panic ===="), "{text}");
    assert!(text.contains("time: 20"), "{text}");
    assert!(
        text.contains(&format!("version: {}", crash_log::version())),
        "{text}"
    );
    assert!(text.contains("program: hydrus-test"), "{text}");
    // (the test harness names its thread for the test)
    assert!(
        text.contains("thread: a_panic_on_the_main_path_is_logged_with_its_fields"),
        "{text}"
    );
    assert!(text.contains("main path failure"), "{text}");
    assert!(text.contains("crash_log.rs"), "the location: {text}");
    assert!(text.contains("backtrace:"), "{text}");
    crash_log::uninstall();
}

#[test]
fn a_panic_on_a_worker_thread_is_logged_and_the_process_carries_on() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    crash_log::install(dir.path(), "hydrus-test");
    let worker = std::thread::Builder::new()
        .name("test-worker".to_owned())
        .spawn(|| panic!("worker failure"))
        .unwrap();
    assert!(worker.join().is_err());
    let text = log(dir.path());
    assert!(text.contains("thread: test-worker"), "{text}");
    assert!(text.contains("worker failure"), "{text}");
    crash_log::uninstall();
}

#[test]
fn the_log_is_rotated_so_it_cannot_grow_without_bound() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let entry = "x".repeat(100 * 1024);
    for _ in 0..40 {
        crash_log::write_entry(dir.path(), &entry).unwrap();
    }
    let size = |name: &str| std::fs::metadata(dir.path().join(name)).map_or(0, |m| m.len());
    assert!(size(crash_log::LOG_NAME) <= crash_log::MAX_BYTES);
    assert!(size(crash_log::OLD_LOG_NAME) <= crash_log::MAX_BYTES);
    assert!(size(crash_log::OLD_LOG_NAME) > 0, "the earlier log is kept");
}

#[test]
fn without_a_directory_nothing_is_written_and_the_panic_still_unwinds() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    crash_log::uninstall();
    assert!(crash_log::path().is_none());
    assert!(catch_unwind(|| panic!("nowhere to log")).is_err());
}
