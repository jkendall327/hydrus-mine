//! Rendered dialogs, detached cancellation and local daemon command boundaries.
use hydrus_core::{
    bandwidth::{BandwidthType, Tracker},
    network::NetworkContext,
    time::TimestampMs,
};
use hydrus_gui::{headless, network_data_window as windows};
use hydrus_store::{
    Store,
    network_runtime::{self, NetworkJob, Snapshot, WaitReason},
    settings,
};
use slint::{ComponentHandle as _, Model as _};

fn until(mut condition: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !condition() {
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        slint::platform::update_timers_and_animations();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
fn capture(windows: &headless::Windows, index: usize, name: &str, width: u32, height: u32) {
    let pixels = headless::render(&windows.get(index).unwrap(), width, height);
    let directory = std::env::var_os("HYDRUS_NETWORK_SCREENSHOTS").map_or_else(
        || std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")),
        std::path::PathBuf::from,
    );
    std::fs::create_dir_all(&directory).unwrap();
    headless::save_png(&directory.join(name), &pixels, width, height).unwrap();
}

#[test]
fn bandwidth_rules_cancel_apply_reopen_and_render() {
    let headless = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    let mut tracker = Tracker::new(now);
    tracker.report_data(2048, now);
    tracker.report_requests(1, now);
    store
        .write(move |ctx| {
            hydrus_store::bandwidth::save_usage(
                ctx.conn(),
                &[(NetworkContext::domain("example.com"), tracker)],
            )
        })
        .unwrap();
    let review = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| review.get_rows().row_count() == 2);
    assert!(review.get_status().contains("offline"));
    review.set_domain("example.com".into());
    review.invoke_domain_clicked();
    let edit = windows::last_rules().unwrap();
    let before = store
        .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
        .unwrap();
    edit.set_amount("0".into());
    edit.invoke_add_rule();
    assert!(edit.get_error().contains("positive"));
    edit.set_requests(true);
    edit.set_amount("23".into());
    edit.set_seconds("60".into());
    edit.set_monthly(false);
    edit.invoke_add_rule();
    assert_eq!(edit.get_rows().row_count(), 3);
    edit.invoke_cancel_clicked();
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
            .unwrap(),
        before
    );
    review.invoke_domain_clicked();
    let edit = windows::last_rules().unwrap();
    edit.set_requests(true);
    edit.set_amount("23".into());
    edit.set_seconds("60".into());
    edit.invoke_add_rule();
    capture(
        &headless,
        headless.count() - 1,
        "network_bandwidth_rules.png",
        670,
        460,
    );
    edit.invoke_apply_clicked();
    until(|| windows::last_rules().is_none());
    let saved = store
        .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
        .unwrap();
    let rules = saved
        .rules
        .iter()
        .find(|(c, _)| c == &NetworkContext::domain("example.com"))
        .unwrap()
        .1
        .rules();
    assert!(
        rules
            .iter()
            .any(|r| r.kind == BandwidthType::Requests && r.max_allowed == 23)
    );
    let row = review
        .get_rows()
        .iter()
        .position(|r| r.cells.row_data(0).unwrap().contains("example.com"))
        .unwrap();
    review.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
    until(|| review.get_can_revert());
    capture(&headless, 0, "network_bandwidth_review.png", 1120, 720);
    review.invoke_revert_clicked();
    assert!(review.get_question().contains("default rules"));
    review.invoke_answer(false);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
            .unwrap(),
        saved
    );
    review.invoke_edit_clicked();
    let abandoned = windows::last_rules().unwrap();
    review.invoke_close_clicked();
    assert!(!abandoned.window().is_visible());
    abandoned.invoke_apply_clicked();
    let reopened = windows::open_bandwidth(store.clone(), &slots).unwrap();
    until(|| reopened.get_rows().row_count() == 2);
    assert_eq!(
        store
            .read(settings::get::<hydrus_store::bandwidth::BandwidthSettings>)
            .unwrap(),
        saved
    );
    reopened.set_domain("example.com".into());
    reopened.invoke_domain_clicked();
    let stale = windows::last_rules().unwrap();
    store
        .write(|ctx| {
            let mut current =
                settings::get::<hydrus_store::bandwidth::BandwidthSettings>(ctx.conn())?;
            current
                .rules
                .retain(|(c, _)| c != &NetworkContext::domain("example.com"));
            current.rules.push((
                NetworkContext::domain("example.com"),
                hydrus_core::bandwidth::Rules::new([hydrus_core::bandwidth::Rule::new(
                    BandwidthType::Requests,
                    Some(60),
                    31,
                )]),
            ));
            settings::set(ctx.conn(), &current)
        })
        .unwrap();
    stale.invoke_apply_clicked();
    until(|| !stale.get_busy());
    assert!(stale.get_error().contains("changed in another editor"));
    assert!(reopened.get_status().contains("changed in another editor"));
    reopened.invoke_close_clicked();
}

#[test]
fn jobs_live_progress_commands_and_offline_expiry() {
    let headless = headless::init();
    let slots = windows::Slots::default();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let now = TimestampMs::now().millis() / 1000;
    store
        .write(move |ctx| {
            settings::set(
                ctx.conn(),
                &Snapshot {
                    epoch: "test daemon".into(),
                    at: now,
                    usage: Vec::new(),
                    jobs: vec![NetworkJob {
                        id: 42,
                        url: "https://example.com/file".into(),
                        status: "downloading…".into(),
                        wait: WaitReason::Downloading,
                        speed: 2048,
                        bytes_read: 2048,
                        bytes_total: Some(4096),
                        contexts: vec![
                            NetworkContext::global(),
                            NetworkContext::domain("example.com"),
                        ],
                        obeys_bandwidth: true,
                    }],
                },
            )
        })
        .unwrap();
    let jobs = windows::open_jobs(store.clone(), &slots).unwrap();
    until(|| jobs.get_rows().row_count() == 1);
    jobs.invoke_row_clicked(0, false, false);
    assert!(jobs.get_can_act());
    assert!(jobs.get_detail().contains("example.com"));
    assert_eq!(
        jobs.get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(4)
            .unwrap(),
        "2 KB/4 KB"
    );
    capture(&headless, 0, "network_current_jobs.png", 1100, 590);
    jobs.invoke_override_jobs();
    jobs.invoke_cancel_jobs();
    let mut commands = Vec::new();
    until(|| {
        commands.extend(
            store
                .write(|ctx| network_runtime::take_commands(ctx.conn()))
                .unwrap(),
        );
        commands.len() == 2
    });
    assert!(
        commands
            .iter()
            .all(|c| c.epoch == "test daemon" && c.job == 42)
    );
    assert_eq!(
        commands[0].action,
        network_runtime::JobAction::OverrideBandwidth
    );
    assert_eq!(commands[1].action, network_runtime::JobAction::Cancel);
    // A replacement daemon can reuse the numeric job id; selection cannot.
    store
        .write(|ctx| {
            let mut snapshot = settings::get::<Snapshot>(ctx.conn())?;
            snapshot.epoch = "replacement daemon".into();
            settings::set(ctx.conn(), &snapshot)
        })
        .unwrap();
    jobs.invoke_refresh();
    until(|| !jobs.get_can_act());
    jobs.invoke_cancel_jobs();
    assert!(
        store
            .write(|ctx| network_runtime::take_commands(ctx.conn()))
            .unwrap()
            .is_empty()
    );
    store
        .write(|ctx| settings::set(ctx.conn(), &Snapshot::default()))
        .unwrap();
    jobs.invoke_refresh();
    until(|| jobs.get_rows().row_count() == 0);
    assert!(!jobs.get_can_act());
    assert!(jobs.get_status().contains("offline"));
    jobs.invoke_close_clicked();
    jobs.invoke_cancel_jobs();
    assert!(
        store
            .write(|ctx| network_runtime::take_commands(ctx.conn()))
            .unwrap()
            .is_empty()
    );
}
