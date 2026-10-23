//! The current network jobs review's right-click menu and its refresh control,
//! replayed against `oracle/fixtures/network_jobs_panel.json` (recorded from the
//! reference's real `ReviewNetworkJobs`, `oracle/record_network_jobs_panel.py`).

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use hydrus_core::network::NetworkContext;
use hydrus_gui::{Clip, NetworkJobsWindow, headless, network_data_window as windows, set_clipper};
use hydrus_store::{
    Store,
    network_runtime::{JobControl, NetworkJob, Snapshot, WaitReason},
    settings,
};
use slint::Model as _;

/// The daemon's heartbeat is fresh for five seconds, so snapshots are stamped
/// with the clock (other tests hold the windows' time still, process-wide).
fn stamp() -> i64 {
    hydrus_core::time::TimestampMs::now().millis() / 1000
}

fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// What the reference's menu is about: `plain` is a running job whose first
/// context (global) would make it wait a minute.
struct Job {
    wait: WaitReason,
    obeys: bool,
    domain_ok: bool,
    tokens_ok: bool,
    /// Its connection-error and serverside-bandwidth retry waits are on.
    retrying: bool,
}

fn publish(store: &Store, at: i64, jobs: &[Job]) {
    let snapshot = Snapshot {
        epoch: "test daemon".into(),
        at,
        jobs: jobs
            .iter()
            .zip(1_u64..)
            .map(|(job, id)| NetworkJob {
                id,
                url: "https://example.com/file".into(),
                status: "downloading…".into(),
                wait: job.wait,
                speed: 0,
                bytes_read: 0,
                bytes_total: None,
                contexts: vec![
                    NetworkContext::global(),
                    NetworkContext::domain("example.com"),
                    NetworkContext::domain("sub.example.com"),
                    NetworkContext::downloader_page("66697874757265"),
                ],
                obeys_bandwidth: job.obeys,
            })
            .collect(),
        controls: jobs
            .iter()
            .zip(1_u64..)
            .map(|(job, id)| JobControl {
                id,
                created: at,
                gallery: false,
                domain_ok: job.domain_ok,
                tokens_ok: job.tokens_ok,
                auto_override: false,
                waiting_on_connection_error: job.retrying,
                waiting_on_serverside_bandwidth: job.retrying,
                context_waits: vec![60, 0, 0, 0],
            })
            .collect(),
        ..Snapshot::default()
    };
    store
        .write(move |ctx| settings::set(ctx.conn(), &snapshot))
        .unwrap();
}

/// The open menus' labels, by pane.
fn menu(window: &NetworkJobsWindow) -> Vec<Vec<String>> {
    let panes = window.get_menu_panes();
    (0..panes.row_count())
        .map(|p| {
            let lines = panes.row_data(p).unwrap().lines;
            (0..lines.row_count())
                .map(|l| lines.row_data(l).unwrap().label.to_string())
                .collect()
        })
        .collect()
}

/// The reference's menu for a job: the contexts submenu, then the flags.
fn recorded(case: &serde_json::Value) -> (Vec<String>, Vec<String>) {
    let rows = case["rows"].as_array().unwrap();
    let label = |r: &serde_json::Value| r["label"].as_str().unwrap().to_owned();
    let children = rows[0]["children"].as_array().unwrap().iter().map(label);
    (children.collect(), rows.iter().map(label).collect())
}

/// Open the review with one job published, select it and right-click it.
fn right_click(
    job: Job,
) -> (
    NetworkJobsWindow,
    windows::Slots,
    std::sync::Arc<Store>,
    tempfile::TempDir,
) {
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    publish(&store, stamp(), &[job]);
    let window = windows::open_jobs(store.clone(), &slots).unwrap();
    until(|| window.get_rows().row_count() == 1);
    window.invoke_row_menu(0, 120.0, 80.0);
    (window, slots, store, dir)
}

// leaf: network-job-actions
#[test]
fn a_jobs_right_click_menu_is_the_reference_s_network_contexts_and_flags() {
    let recorded_menus = hydrus_testkit::fixture_json("network_jobs_panel.json");
    let _windows = headless::init();
    let cases = [
        ("plain", WaitReason::Downloading, true, true, true),
        (
            "gallery_token_needed",
            WaitReason::Downloading,
            true,
            true,
            true,
        ),
        ("post", WaitReason::Downloading, false, true, true),
        ("overridden", WaitReason::Downloading, false, true, true),
        (
            "gallery_token_blocked",
            WaitReason::Downloading,
            true,
            true,
            false,
        ),
        ("domain_errors", WaitReason::Downloading, true, false, true),
        // (the reference had a connection error wait and a serverside bandwidth
        // wait together; the engine keeps both wake times, as its job does)
        ("waiting", WaitReason::Connection, true, true, true),
    ];
    for (name, wait, obeys, domain_ok, tokens_ok) in cases {
        let case = &recorded_menus["menus"][name];
        let (contexts, flags) = recorded(case);
        let (window, _slots, _store, _dir) = right_click(Job {
            wait,
            obeys,
            domain_ok,
            tokens_ok,
            retrying: name == "waiting",
        });
        let panes = menu(&window);
        assert_eq!(panes[0], flags, "{name} {wait:?}");
        // the contexts open as a submenu of labels
        window.invoke_menu_line_hovered(0, 0, 300.0, 100.0, 10.0);
        assert_eq!(menu(&window)[1], contexts, "{name} {wait:?}");
        let kind = window
            .get_menu_panes()
            .row_data(0)
            .unwrap()
            .lines
            .row_data(0)
            .unwrap()
            .kind;
        assert_eq!(kind, 3, "the contexts line is a submenu");
        window.invoke_menu_dismissed();
        assert!(menu(&window).is_empty());
        window.invoke_close_clicked();
    }
    // no menu with two jobs selected (the reference raises DataMissing unless
    // exactly one is selected)
    assert_eq!(recorded_menus["no_selection"], "DataMissing");
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let job = || Job {
        wait: WaitReason::Downloading,
        obeys: true,
        domain_ok: true,
        tokens_ok: true,
        retrying: false,
    };
    publish(&store, stamp(), &[job(), job()]);
    let window = windows::open_jobs(store, &slots).unwrap();
    until(|| window.get_rows().row_count() == 2);
    window.invoke_row_clicked(0, false, false);
    window.invoke_row_clicked(1, true, false);
    window.invoke_row_menu(1, 10.0, 10.0);
    assert!(menu(&window).is_empty());
    // a right press on a row that is not selected selects it alone
    window.invoke_row_clicked(0, false, false);
    window.invoke_row_menu(1, 10.0, 10.0);
    assert_eq!(menu(&window)[0].len(), 6);
    assert!(!window.get_rows().row_data(0).unwrap().selected);
    assert!(window.get_rows().row_data(1).unwrap().selected);
    // each line is a label that copies itself
    let copied: Rc<RefCell<Vec<Clip>>> = Rc::default();
    set_clipper({
        let copied = copied.clone();
        move |clip| copied.borrow_mut().push(clip.clone())
    });
    window.invoke_menu_line_clicked(0, 1, 300.0, 100.0, 10.0);
    assert!(
        matches!(copied.borrow().as_slice(), [Clip::Text(t)] if t == "domain ok: True"),
        "{:?}",
        copied.borrow()
    );
    window.invoke_close_clicked();
}

// leaf: audit-network-jobs-refresh
#[test]
fn the_refresh_control_is_manual_to_begin_with_or_an_interval_as_the_reference_s_is() {
    let recorded = hydrus_testkit::fixture_json("network_jobs_panel.json");
    let _windows = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let job = || Job {
        wait: WaitReason::Downloading,
        obeys: true,
        domain_ok: true,
        tokens_ok: true,
        retrying: false,
    };
    publish(&store, stamp(), &[job()]);
    let window = windows::open_jobs(store.clone(), &slots).unwrap();
    until(|| window.get_rows().row_count() == 1);

    // the labels the panel carries
    assert_eq!(
        recorded["hint"][0],
        "right-click a job for more debug info!"
    );
    assert_eq!(recorded["checkbox"], "manual refresh");
    assert_eq!(recorded["buttons"][0], "refresh snapshot");

    // manual refresh to begin with, with no timer
    assert!(recorded["refresh"]["initial_value"].is_null());
    assert!(!recorded["refresh"]["initial_active"].as_bool().unwrap());
    assert!(window.get_manual_refresh());
    assert_eq!(slots.jobs_refresh_interval(), None);

    // in manual mode a new snapshot does not reach the list by itself
    publish(&store, stamp(), &[job(), job()]);
    for _ in 0..80 {
        slint::platform::update_timers_and_animations();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(window.get_rows().row_count(), 1);
    // ...but "refresh snapshot" takes one
    window.invoke_refresh();
    until(|| window.get_rows().row_count() == 2);

    // the recorded steps: an interval starts the timer at that many seconds
    // (never under 0.05), and manual refresh stops it
    for step in recorded["refresh"]["steps"].as_array().unwrap() {
        match step["set"].as_f64() {
            Some(seconds) => {
                window.set_manual_refresh(false);
                window.set_refresh_seconds(format!("{seconds}").into());
            }
            None => window.set_manual_refresh(true),
        }
        window.invoke_refresh_changed();
        let wanted = step["interval_ms"].as_u64().map(Duration::from_millis);
        assert_eq!(slots.jobs_refresh_interval(), wanted, "{step}");
    }

    // an interval refreshes by itself
    publish(&store, stamp(), &[job()]);
    window.set_manual_refresh(false);
    window.set_refresh_seconds("0.05".into());
    window.invoke_refresh_changed();
    until(|| window.get_rows().row_count() == 1);
    window.invoke_close_clicked();
}
