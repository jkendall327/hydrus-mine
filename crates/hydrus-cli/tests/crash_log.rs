//! A panic in a tokio task or blocking job, as the `serve` workers run, is
//! caught by the runtime (the worker sees a `JoinError`), and the crash log
//! has it anyway.
use hydrus_core::crash_log;

#[test]
fn a_panic_in_a_serve_worker_job_is_logged_with_its_thread() {
    let dir = tempfile::tempdir().unwrap();
    crash_log::install(dir.path(), "hydrus");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .unwrap();
    let (task, job) = runtime.block_on(async {
        (
            tokio::spawn(async { panic!("task failure") }).await,
            tokio::task::spawn_blocking(|| panic!("blocking job failure")).await,
        )
    });
    // (what the workers see: they carry on or stop on this)
    assert!(task.unwrap_err().is_panic());
    assert!(job.unwrap_err().is_panic());
    let text = std::fs::read_to_string(dir.path().join(crash_log::LOG_NAME)).unwrap();
    assert!(text.contains("task failure"), "{text}");
    assert!(text.contains("blocking job failure"), "{text}");
    assert!(text.contains("program: hydrus\n"), "{text}");
    assert!(text.contains("thread: "), "{text}");
    crash_log::uninstall();
}
