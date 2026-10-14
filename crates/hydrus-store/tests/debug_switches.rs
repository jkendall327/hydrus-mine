//! The debug switches the client sets reach the daemon through the store
//! (`debug_switches`), and the daemon's reports reach the client's popups.
//! The switches are process-wide, so this is tested alone here: the "client"
//! publishes, then the flags are put back as they would be in another
//! process and the "daemon" applies.
use hydrus_core::debug_flags::{self, Flag};
use hydrus_core::profiling::{self, Mode};
use hydrus_store::debug_switches::{
    apply, publish, report_to_popups, request_subscription_snapshot,
};
use hydrus_store::{Store, popups};

#[test]
fn the_daemon_follows_the_switches_the_client_set_and_ignores_old_ones() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    // switches set before the daemon first looked are old
    Flag::DbReport.set(true);
    publish(&store).unwrap();
    Flag::DbReport.set(false);
    assert!(!apply(&store).unwrap().subscription_snapshot);
    assert!(!Flag::DbReport.is_on(), "old switches are not taken up");
    // the client flips some
    Flag::DbReport.set(true);
    Flag::IdleReport.set(true);
    profiling::adopt_mode(Some(Mode::Db), 1_700_000_000);
    profiling::adopt_planner(true, 1_700_000_001);
    publish(&store).unwrap();
    // (the daemon is another process: its own are off)
    Flag::DbReport.set(false);
    Flag::IdleReport.set(false);
    profiling::reset();
    apply(&store).unwrap();
    assert!(Flag::DbReport.is_on() && Flag::IdleReport.is_on());
    assert_eq!(profiling::mode_and_start(), (Some(Mode::Db), 1_700_000_000));
    assert_eq!(profiling::planner_and_start(), (true, 1_700_000_001));
    // and turning them off
    Flag::DbReport.set(false);
    Flag::IdleReport.set(false);
    profiling::reset();
    publish(&store).unwrap();
    Flag::DbReport.set(true);
    apply(&store).unwrap();
    assert!(!Flag::DbReport.is_on());
    assert_eq!(profiling::mode_and_start().0, None);
    // a snapshot is asked for once
    request_subscription_snapshot(&store).unwrap();
    assert!(apply(&store).unwrap().subscription_snapshot);
    assert!(!apply(&store).unwrap().subscription_snapshot);
}

#[test]
fn the_daemons_reports_become_popups() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let sink = report_to_popups(&store);
    sink("a report from the daemon");
    let started = std::time::Instant::now();
    loop {
        let jobs = store
            .read(|c| popups::all(c, hydrus_core::TimestampMs::now().0 / 1000))
            .unwrap();
        if let Some(job) = jobs.first() {
            assert_eq!(
                job.status_text_1.as_deref(),
                Some("a report from the daemon")
            );
            break;
        }
        assert!(started.elapsed().as_secs() < 10, "the popup arrives");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    debug_flags::set_sink(None);
}
